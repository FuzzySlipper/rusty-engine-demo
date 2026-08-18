//! Explicit trusted-host file selection and fail-atomic publication.

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

use rusty_engine::voxel_convert::source_sha256;

use super::protocol::AdapterRejection;

pub(crate) const MAX_HOST_PATH_BYTES: usize = 4 * 1024;
const MAX_HOST_COMPARISON_BYTES: u64 = 64 * 1024 * 1024;

pub(crate) struct HostFileRead {
    pub path: PathBuf,
    pub bytes: Vec<u8>,
    pub sha256: String,
}

pub(crate) struct HostFileWriteReceipt {
    pub path: PathBuf,
    pub byte_count: usize,
    pub sha256: String,
    pub replaced_existing: bool,
}

#[derive(Clone, Copy)]
struct PostCommitMaintenance {
    remove_pending: fn(&Path) -> std::io::Result<()>,
    sync_directory: fn(&Path) -> std::io::Result<()>,
}

const POST_COMMIT_MAINTENANCE: PostCommitMaintenance = PostCommitMaintenance {
    remove_pending,
    sync_directory,
};

pub(crate) fn read_host_file(
    requested: &str,
    max_bytes: usize,
) -> Result<HostFileRead, AdapterRejection> {
    let path = checked_absolute_path(requested)?;
    require_existing_chain_without_symlinks(&path)?;
    let metadata =
        fs::metadata(&path).map_err(|error| io_rejection("inspect host file", &path, error))?;
    if !metadata.is_file() {
        return Err(reject_path(
            "hostFile.wrongType",
            &path,
            "selected host path is not a regular file",
        ));
    }
    if metadata.len() > max_bytes as u64 {
        return Err(reject_path(
            "hostFile.tooLarge",
            &path,
            format!(
                "selected file has {} bytes; limit is {max_bytes}",
                metadata.len()
            ),
        ));
    }
    let file = File::open(&path).map_err(|error| io_rejection("open host file", &path, error))?;
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(max_bytes.saturating_add(1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| io_rejection("read host file", &path, error))?;
    if bytes.len() > max_bytes {
        return Err(reject_path(
            "hostFile.tooLarge",
            &path,
            format!("selected file exceeds the {max_bytes}-byte limit"),
        ));
    }
    let sha256 = source_sha256(&bytes);
    Ok(HostFileRead {
        path,
        bytes,
        sha256,
    })
}

pub(crate) fn write_host_file_atomic(
    requested: &str,
    bytes: &[u8],
    expected_target_sha256: Option<&str>,
) -> Result<HostFileWriteReceipt, AdapterRejection> {
    write_host_file_atomic_with_maintenance(
        requested,
        bytes,
        expected_target_sha256,
        POST_COMMIT_MAINTENANCE,
    )
}

fn write_host_file_atomic_with_maintenance(
    requested: &str,
    bytes: &[u8],
    expected_target_sha256: Option<&str>,
    maintenance: PostCommitMaintenance,
) -> Result<HostFileWriteReceipt, AdapterRejection> {
    let path = checked_absolute_path(requested)?;
    let parent = path.parent().ok_or_else(|| {
        reject_path(
            "hostFile.invalidPath",
            &path,
            "target must have a parent directory",
        )
    })?;
    require_existing_chain_without_symlinks(parent)?;
    if !fs::metadata(parent)
        .map_err(|error| io_rejection("inspect host directory", parent, error))?
        .is_dir()
    {
        return Err(reject_path(
            "hostFile.wrongType",
            parent,
            "target parent is not a directory",
        ));
    }

    let prior = observe_optional_file(&path)?;
    match (&prior, expected_target_sha256) {
        (Some((actual, _)), Some(expected)) if actual == expected => {}
        (Some((actual, _)), Some(expected)) => {
            return Err(reject_path(
                "hostFile.staleTarget",
                &path,
                format!("expected target hash {expected}, found {actual}"),
            ))
        }
        (Some(_), None) => {
            return Err(reject_path(
                "hostFile.expectedHashRequired",
                &path,
                "replacing an existing file requires expectedTargetSha256",
            ))
        }
        (None, Some(_)) => {
            return Err(reject_path(
                "hostFile.targetMissing",
                &path,
                "an expected target hash was supplied for a missing file",
            ))
        }
        (None, None) => {}
    }

    let pending = pending_path(&path)?;
    if fs::symlink_metadata(&pending).is_ok() {
        return Err(reject_path(
            "hostFile.pendingConflict",
            &pending,
            "a pending publication file already exists",
        ));
    }
    let write_result = (|| {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&pending)
            .map_err(|error| io_rejection("create pending host file", &pending, error))?;
        file.write_all(bytes)
            .map_err(|error| io_rejection("write pending host file", &pending, error))?;
        file.sync_all()
            .map_err(|error| io_rejection("sync pending host file", &pending, error))?;

        let current = observe_optional_file(&path)?;
        if current.as_ref().map(|value| &value.0) != prior.as_ref().map(|value| &value.0) {
            return Err(reject_path(
                "hostFile.staleTarget",
                &path,
                "target changed while the replacement was being staged",
            ));
        }
        if prior.is_some() {
            fs::rename(&pending, &path)
                .map_err(|error| io_rejection("replace host file", &path, error))?;
        } else {
            fs::hard_link(&pending, &path)
                .map_err(|error| io_rejection("install new host file", &path, error))?;
            // The target becomes authoritative at the successful hard link.
            // Pending-file cleanup is maintenance and cannot truthfully turn
            // that committed publication into a rejected operation.
            let _ = (maintenance.remove_pending)(&pending);
        }
        // Rename/hard-link is the explicit commit point. A later directory
        // sync failure cannot be reported as rejection once readers can
        // observe the requested bytes at the target path.
        let _ = (maintenance.sync_directory)(parent);
        Ok::<_, AdapterRejection>(())
    })();
    if let Err(error) = write_result {
        let _ = fs::remove_file(&pending);
        return Err(error);
    }
    Ok(HostFileWriteReceipt {
        path,
        byte_count: bytes.len(),
        sha256: source_sha256(bytes),
        replaced_existing: prior.is_some(),
    })
}

fn sync_directory(path: &Path) -> std::io::Result<()> {
    File::open(path).and_then(|directory| directory.sync_all())
}

fn remove_pending(path: &Path) -> std::io::Result<()> {
    fs::remove_file(path)
}

fn checked_absolute_path(requested: &str) -> Result<PathBuf, AdapterRejection> {
    if requested.trim().is_empty() || requested.len() > MAX_HOST_PATH_BYTES {
        return Err(AdapterRejection::new(
            "hostFile.invalidPath",
            format!("host path must contain 1..={MAX_HOST_PATH_BYTES} UTF-8 bytes"),
        ));
    }
    let path = PathBuf::from(requested);
    if !path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::CurDir))
    {
        return Err(reject_path(
            "hostFile.invalidPath",
            &path,
            "host path must be absolute and lexically normalized",
        ));
    }
    Ok(path)
}

fn require_existing_chain_without_symlinks(path: &Path) -> Result<(), AdapterRejection> {
    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component.as_os_str());
        let metadata = fs::symlink_metadata(&current)
            .map_err(|error| io_rejection("inspect host path", &current, error))?;
        if metadata.file_type().is_symlink() {
            return Err(reject_path(
                "hostFile.symlinkRejected",
                &current,
                "symbolic links are not accepted in trusted host file paths",
            ));
        }
    }
    Ok(())
}

fn observe_optional_file(path: &Path) -> Result<Option<(String, u64)>, AdapterRejection> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(io_rejection("inspect host target", path, error)),
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(reject_path(
            "hostFile.wrongType",
            path,
            "existing target must be a regular non-symlink file",
        ));
    }
    if metadata.len() > MAX_HOST_COMPARISON_BYTES {
        return Err(reject_path(
            "hostFile.tooLarge",
            path,
            format!(
                "existing target has {} bytes; comparison limit is {MAX_HOST_COMPARISON_BYTES}",
                metadata.len()
            ),
        ));
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    File::open(path)
        .and_then(|mut file| file.read_to_end(&mut bytes))
        .map_err(|error| io_rejection("read host target", path, error))?;
    Ok(Some((source_sha256(&bytes), metadata.len())))
}

fn pending_path(target: &Path) -> Result<PathBuf, AdapterRejection> {
    let name = target.file_name().ok_or_else(|| {
        reject_path(
            "hostFile.invalidPath",
            target,
            "target must have a file name",
        )
    })?;
    let mut pending_name = name.to_os_string();
    pending_name.push(".rusty-engine.pending");
    Ok(target.with_file_name(pending_name))
}

fn io_rejection(operation: &str, path: &Path, error: std::io::Error) -> AdapterRejection {
    reject_path(
        "hostFile.io",
        path,
        format!("could not {operation}: {error}"),
    )
}

fn reject_path(code: &str, path: &Path, message: impl Into<String>) -> AdapterRejection {
    AdapterRejection::new(code, message).at_path(path.display().to_string())
}

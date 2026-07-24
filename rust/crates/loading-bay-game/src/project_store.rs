//! Durable filesystem storage for admitted authored-project documents.
//!
//! The store has no runtime or snapshot input. It writes canonical bytes to a
//! same-directory pending file, syncs them, and only then atomically renames the
//! complete file over the requested target.

use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use crate::project_admission::AdmittedStoredProject;
use crate::project_codec::{
    decode_project_document, encode_project_document, DecodedProjectDocument,
};
use crate::stored_project::{StoredProjectError, STORED_PROJECT_SCHEMA_VERSION};

pub const DEFAULT_MAX_PROJECT_FILE_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectSaveMode {
    CreateNew,
    ReplaceExisting,
}

#[derive(Debug, Clone)]
pub struct ProjectStore {
    max_bytes: usize,
}

impl Default for ProjectStore {
    fn default() -> Self {
        Self {
            max_bytes: DEFAULT_MAX_PROJECT_FILE_BYTES,
        }
    }
}

impl ProjectStore {
    pub fn with_max_bytes(max_bytes: usize) -> Self {
        Self { max_bytes }
    }

    pub fn max_bytes(&self) -> usize {
        self.max_bytes
    }

    /// Save one fully admitted static project. Runtime/session values cannot be
    /// supplied to this API because [`AdmittedStoredProject`] contains only the
    /// authored document.
    pub fn save(
        &self,
        target: &Path,
        project: &AdmittedStoredProject,
        mode: ProjectSaveMode,
    ) -> Result<(), ProjectStoreError> {
        let pending = pending_path(target)?;
        let parent = target_parent(target)?;
        if !path_exists(target)? {
            self.recover_pending(target)?;
        }
        match (mode, path_exists(target)?) {
            (ProjectSaveMode::CreateNew, true) => {
                return Err(ProjectStoreError::TargetExists {
                    path: target.to_path_buf(),
                });
            }
            (ProjectSaveMode::ReplaceExisting, false) => {
                return Err(ProjectStoreError::TargetMissing {
                    path: target.to_path_buf(),
                });
            }
            _ => {}
        }

        let encoded = encode_project_document(project.document())?;
        if encoded.len() > self.max_bytes {
            return Err(ProjectStoreError::TooLarge {
                path: target.to_path_buf(),
                actual_bytes: encoded.len() as u64,
                max_bytes: self.max_bytes,
            });
        }
        clear_stale_pending(&pending)?;

        let write_result = write_pending(&pending, encoded.as_bytes());
        if let Err(error) = write_result {
            let _ = fs::remove_file(&pending);
            return Err(error);
        }

        match mode {
            ProjectSaveMode::CreateNew => install_new(&pending, target)?,
            ProjectSaveMode::ReplaceExisting => fs::rename(&pending, target)
                .map_err(|source| io_error("replace project", target, source))?,
        }
        sync_directory(parent)?;
        Ok(())
    }

    /// Load a bounded project file. If the target is missing but a complete,
    /// canonical pending file exists, finish that interrupted rename first.
    pub fn load(&self, target: &Path) -> Result<DecodedProjectDocument, ProjectStoreError> {
        if !path_exists(target)? {
            self.recover_pending(target)?;
        }
        let input = read_bounded(target, self.max_bytes)?;
        decode_project_document(&input).map_err(ProjectStoreError::Codec)
    }

    /// Finish an interrupted post-sync/pre-rename save. Recovery only promotes
    /// current-schema canonical bytes; malformed, legacy, or noncanonical
    /// pending files remain in place for inspection.
    pub fn recover_pending(&self, target: &Path) -> Result<bool, ProjectStoreError> {
        if path_exists(target)? {
            return Ok(false);
        }
        let pending = pending_path(target)?;
        if !path_exists(&pending)? {
            return Ok(false);
        }
        require_regular_file(&pending)?;

        let input = read_bounded(&pending, self.max_bytes)?;
        let decoded = decode_project_document(&input).map_err(|source| {
            ProjectStoreError::InvalidPending {
                path: pending.clone(),
                source,
            }
        })?;
        let canonical = encode_project_document(&decoded.project).map_err(|source| {
            ProjectStoreError::InvalidPending {
                path: pending.clone(),
                source,
            }
        })?;
        if decoded.source_schema_version != STORED_PROJECT_SCHEMA_VERSION || canonical != input {
            return Err(ProjectStoreError::NonCanonicalPending { path: pending });
        }

        install_new(&pending, target)?;
        sync_directory(target_parent(target)?)?;
        Ok(true)
    }

    pub fn pending_path(target: &Path) -> Result<PathBuf, ProjectStoreError> {
        pending_path(target)
    }
}

#[derive(Debug)]
pub enum ProjectStoreError {
    InvalidTargetPath {
        path: PathBuf,
    },
    TargetExists {
        path: PathBuf,
    },
    TargetMissing {
        path: PathBuf,
    },
    PendingConflict {
        path: PathBuf,
    },
    NonCanonicalPending {
        path: PathBuf,
    },
    TooLarge {
        path: PathBuf,
        actual_bytes: u64,
        max_bytes: usize,
    },
    InvalidUtf8 {
        path: PathBuf,
    },
    Codec(StoredProjectError),
    InvalidPending {
        path: PathBuf,
        source: StoredProjectError,
    },
    Io {
        operation: &'static str,
        path: PathBuf,
        source: io::Error,
    },
}

impl std::fmt::Display for ProjectStoreError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidTargetPath { path } => {
                write!(
                    formatter,
                    "invalid project target path `{}`",
                    path.display()
                )
            }
            Self::TargetExists { path } => {
                write!(
                    formatter,
                    "project target `{}` already exists",
                    path.display()
                )
            }
            Self::TargetMissing { path } => write!(
                formatter,
                "project target `{}` does not exist for explicit replacement",
                path.display()
            ),
            Self::PendingConflict { path } => write!(
                formatter,
                "project pending path `{}` is not a replaceable regular file",
                path.display()
            ),
            Self::NonCanonicalPending { path } => write!(
                formatter,
                "project pending file `{}` is not current canonical project bytes",
                path.display()
            ),
            Self::TooLarge {
                path,
                actual_bytes,
                max_bytes,
            } => write!(
                formatter,
                "project `{}` is {actual_bytes} bytes, exceeding the {max_bytes}-byte bound",
                path.display()
            ),
            Self::InvalidUtf8 { path } => {
                write!(formatter, "project `{}` is not UTF-8", path.display())
            }
            Self::Codec(error) => write!(formatter, "project codec failed: {error}"),
            Self::InvalidPending { path, source } => write!(
                formatter,
                "project pending file `{}` is invalid: {source}",
                path.display()
            ),
            Self::Io {
                operation,
                path,
                source,
            } => write!(
                formatter,
                "could not {operation} at `{}`: {source}",
                path.display()
            ),
        }
    }
}

impl std::error::Error for ProjectStoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Codec(source) | Self::InvalidPending { source, .. } => Some(source),
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl From<StoredProjectError> for ProjectStoreError {
    fn from(value: StoredProjectError) -> Self {
        Self::Codec(value)
    }
}

fn pending_path(target: &Path) -> Result<PathBuf, ProjectStoreError> {
    let Some(file_name) = target.file_name() else {
        return Err(ProjectStoreError::InvalidTargetPath {
            path: target.to_path_buf(),
        });
    };
    let mut pending_name = OsString::from(".");
    pending_name.push(file_name);
    pending_name.push(".pending");
    Ok(target.with_file_name(pending_name))
}

fn target_parent(target: &Path) -> Result<&Path, ProjectStoreError> {
    let Some(parent) = target.parent() else {
        return Err(ProjectStoreError::InvalidTargetPath {
            path: target.to_path_buf(),
        });
    };
    if parent.as_os_str().is_empty() {
        Ok(Path::new("."))
    } else {
        Ok(parent)
    }
}

fn path_exists(path: &Path) -> Result<bool, ProjectStoreError> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(source) => Err(io_error("inspect project path", path, source)),
    }
}

fn clear_stale_pending(path: &Path) -> Result<(), ProjectStoreError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_file() => fs::remove_file(path)
            .map_err(|source| io_error("remove stale pending file", path, source)),
        Ok(_) => Err(ProjectStoreError::PendingConflict {
            path: path.to_path_buf(),
        }),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(io_error("inspect pending file", path, source)),
    }
}

fn require_regular_file(path: &Path) -> Result<(), ProjectStoreError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|source| io_error("inspect pending file", path, source))?;
    if metadata.file_type().is_file() {
        Ok(())
    } else {
        Err(ProjectStoreError::PendingConflict {
            path: path.to_path_buf(),
        })
    }
}

fn write_pending(path: &Path, bytes: &[u8]) -> Result<(), ProjectStoreError> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|source| io_error("create pending project", path, source))?;
    file.write_all(bytes)
        .map_err(|source| io_error("write pending project", path, source))?;
    file.sync_all()
        .map_err(|source| io_error("sync pending project", path, source))?;
    Ok(())
}

fn install_new(pending: &Path, target: &Path) -> Result<(), ProjectStoreError> {
    match fs::hard_link(pending, target) {
        Ok(()) => fs::remove_file(pending)
            .map_err(|source| io_error("remove installed pending project", pending, source)),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            Err(ProjectStoreError::TargetExists {
                path: target.to_path_buf(),
            })
        }
        Err(source) => Err(io_error("install new project", target, source)),
    }
}

fn read_bounded(path: &Path, max_bytes: usize) -> Result<String, ProjectStoreError> {
    let file = File::open(path).map_err(|source| io_error("open project", path, source))?;
    let metadata = file
        .metadata()
        .map_err(|source| io_error("inspect project", path, source))?;
    if metadata.len() > max_bytes as u64 {
        return Err(ProjectStoreError::TooLarge {
            path: path.to_path_buf(),
            actual_bytes: metadata.len(),
            max_bytes,
        });
    }

    let read_limit = max_bytes.saturating_add(1) as u64;
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(read_limit)
        .read_to_end(&mut bytes)
        .map_err(|source| io_error("read project", path, source))?;
    if bytes.len() > max_bytes {
        return Err(ProjectStoreError::TooLarge {
            path: path.to_path_buf(),
            actual_bytes: bytes.len() as u64,
            max_bytes,
        });
    }
    String::from_utf8(bytes).map_err(|_| ProjectStoreError::InvalidUtf8 {
        path: path.to_path_buf(),
    })
}

fn sync_directory(path: &Path) -> Result<(), ProjectStoreError> {
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|source| io_error("sync project directory", path, source))
}

fn io_error(operation: &'static str, path: &Path, source: io::Error) -> ProjectStoreError {
    ProjectStoreError::Io {
        operation,
        path: path.to_path_buf(),
        source,
    }
}

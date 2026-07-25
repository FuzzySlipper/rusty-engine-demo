use std::fs;
use std::path::{Component, Path, PathBuf};

use content_store::is_safe_relative_path;

pub const MAX_ROOT_PATH_BYTES: usize = 4 * 1024;
pub const MAX_PROJECT_PATH_BYTES: usize = 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectLocation {
    root: PathBuf,
    relative_project_file: String,
    project_file: PathBuf,
}

impl ProjectLocation {
    pub fn resolve(root: &str, relative_project_file: &str) -> Result<Self, PathSafetyError> {
        if root.len() > MAX_ROOT_PATH_BYTES {
            return Err(PathSafetyError::RootTooLong);
        }
        if relative_project_file.len() > MAX_PROJECT_PATH_BYTES {
            return Err(PathSafetyError::ProjectPathTooLong);
        }
        let requested_root = Path::new(root);
        if !requested_root.is_absolute() {
            return Err(PathSafetyError::RootNotAbsolute);
        }
        require_directory_without_symlink(requested_root, "project root")?;
        let canonical_root =
            requested_root
                .canonicalize()
                .map_err(|source| PathSafetyError::Io {
                    operation: "canonicalize project root",
                    path: requested_root.to_path_buf(),
                    source,
                })?;
        if !is_safe_relative_path(relative_project_file) {
            return Err(PathSafetyError::UnsafeProjectPath);
        }

        let project_file = canonical_root.join(relative_project_file);
        require_path_chain_without_symlinks(&canonical_root, relative_project_file)?;
        require_regular_file_without_symlink(&project_file, "project file")?;
        let canonical_project_file =
            project_file
                .canonicalize()
                .map_err(|source| PathSafetyError::Io {
                    operation: "canonicalize project file",
                    path: project_file.clone(),
                    source,
                })?;
        if !canonical_project_file.starts_with(&canonical_root) {
            return Err(PathSafetyError::ProjectEscapesRoot);
        }

        Ok(Self {
            root: canonical_root,
            relative_project_file: relative_project_file.to_string(),
            project_file: canonical_project_file,
        })
    }

    pub fn resolve_new(root: &str, relative_project_file: &str) -> Result<Self, PathSafetyError> {
        if root.len() > MAX_ROOT_PATH_BYTES {
            return Err(PathSafetyError::RootTooLong);
        }
        if relative_project_file.len() > MAX_PROJECT_PATH_BYTES {
            return Err(PathSafetyError::ProjectPathTooLong);
        }
        let requested_root = Path::new(root);
        if !requested_root.is_absolute() {
            return Err(PathSafetyError::RootNotAbsolute);
        }
        require_directory_without_symlink(requested_root, "project root")?;
        let canonical_root =
            requested_root
                .canonicalize()
                .map_err(|source| PathSafetyError::Io {
                    operation: "canonicalize project root",
                    path: requested_root.to_path_buf(),
                    source,
                })?;
        if !is_safe_relative_path(relative_project_file) {
            return Err(PathSafetyError::UnsafeProjectPath);
        }
        let relative = Path::new(relative_project_file);
        let mut parent = canonical_root.clone();
        if let Some(relative_parent) = relative.parent() {
            for component in relative_parent.components() {
                let Component::Normal(segment) = component else {
                    return Err(PathSafetyError::UnsafeProjectPath);
                };
                parent.push(segment);
                require_directory_without_symlink(&parent, "project parent directory")?;
            }
        }
        let project_file = canonical_root.join(relative);
        match fs::symlink_metadata(&project_file) {
            Ok(_) => return Err(PathSafetyError::TargetExists { path: project_file }),
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => {}
            Err(source) => {
                return Err(PathSafetyError::Io {
                    operation: "inspect new project target",
                    path: project_file,
                    source,
                });
            }
        }
        Ok(Self {
            root: canonical_root,
            relative_project_file: relative_project_file.to_string(),
            project_file,
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn relative_project_file(&self) -> &str {
        &self.relative_project_file
    }

    pub fn project_file(&self) -> &Path {
        &self.project_file
    }

    pub fn revalidate(&self) -> Result<(), PathSafetyError> {
        require_directory_without_symlink(&self.root, "project root")?;
        require_path_chain_without_symlinks(&self.root, &self.relative_project_file)?;
        require_regular_file_without_symlink(&self.project_file, "project file")?;
        let canonical = self
            .project_file
            .canonicalize()
            .map_err(|source| PathSafetyError::Io {
                operation: "canonicalize project file",
                path: self.project_file.clone(),
                source,
            })?;
        if canonical != self.project_file || !canonical.starts_with(&self.root) {
            return Err(PathSafetyError::ProjectEscapesRoot);
        }
        Ok(())
    }

    pub fn read_relative_file(
        &self,
        relative_file: &str,
        max_bytes: u64,
    ) -> Result<Vec<u8>, PathSafetyError> {
        if relative_file.len() > MAX_PROJECT_PATH_BYTES || !is_safe_relative_path(relative_file) {
            return Err(PathSafetyError::UnsafeProjectPath);
        }
        require_directory_without_symlink(&self.root, "project root")?;
        require_path_chain_without_symlinks(&self.root, relative_file)?;
        let path = self.root.join(relative_file);
        require_regular_file_without_symlink(&path, "project-relative file")?;
        let canonical = path.canonicalize().map_err(|source| PathSafetyError::Io {
            operation: "canonicalize project-relative file",
            path: path.clone(),
            source,
        })?;
        if !canonical.starts_with(&self.root) {
            return Err(PathSafetyError::ProjectEscapesRoot);
        }
        let length = fs::metadata(&canonical)
            .map_err(|source| PathSafetyError::Io {
                operation: "inspect project-relative file",
                path: canonical.clone(),
                source,
            })?
            .len();
        if length > max_bytes {
            return Err(PathSafetyError::FileTooLarge {
                path: canonical,
                limit: max_bytes,
                actual: length,
            });
        }
        fs::read(&canonical).map_err(|source| PathSafetyError::Io {
            operation: "read project-relative file",
            path: canonical,
            source,
        })
    }
}

#[derive(Debug)]
pub enum PathSafetyError {
    RootTooLong,
    ProjectPathTooLong,
    RootNotAbsolute,
    UnsafeProjectPath,
    ProjectEscapesRoot,
    FileTooLarge {
        path: PathBuf,
        limit: u64,
        actual: u64,
    },
    Symlink {
        path: PathBuf,
    },
    WrongFileType {
        label: &'static str,
        path: PathBuf,
    },
    TargetExists {
        path: PathBuf,
    },
    Io {
        operation: &'static str,
        path: PathBuf,
        source: std::io::Error,
    },
}

impl std::fmt::Display for PathSafetyError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RootTooLong => formatter.write_str("project root exceeds the path bound"),
            Self::ProjectPathTooLong => {
                formatter.write_str("project file path exceeds the path bound")
            }
            Self::RootNotAbsolute => formatter.write_str("project root must be absolute"),
            Self::UnsafeProjectPath => {
                formatter.write_str("project file must be a safe project-relative path")
            }
            Self::ProjectEscapesRoot => {
                formatter.write_str("project file resolves outside the selected root")
            }
            Self::FileTooLarge {
                path,
                limit,
                actual,
            } => write!(
                formatter,
                "project-relative file {} has {actual} bytes; limit is {limit}",
                path.display()
            ),
            Self::Symlink { path } => write!(
                formatter,
                "symbolic links are not accepted in the writable project path: {}",
                path.display()
            ),
            Self::WrongFileType { label, path } => {
                write!(
                    formatter,
                    "{label} has the wrong file type: {}",
                    path.display()
                )
            }
            Self::TargetExists { path } => write!(
                formatter,
                "new project target already exists: {}",
                path.display()
            ),
            Self::Io {
                operation,
                path,
                source,
            } => write!(
                formatter,
                "could not {operation} at {}: {source}",
                path.display()
            ),
        }
    }
}

impl std::error::Error for PathSafetyError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

fn require_path_chain_without_symlinks(root: &Path, relative: &str) -> Result<(), PathSafetyError> {
    let mut current = root.to_path_buf();
    for component in Path::new(relative).components() {
        let Component::Normal(segment) = component else {
            return Err(PathSafetyError::UnsafeProjectPath);
        };
        current.push(segment);
        let metadata = fs::symlink_metadata(&current).map_err(|source| PathSafetyError::Io {
            operation: "inspect selected project path",
            path: current.clone(),
            source,
        })?;
        if metadata.file_type().is_symlink() {
            return Err(PathSafetyError::Symlink { path: current });
        }
    }
    Ok(())
}

fn require_directory_without_symlink(
    path: &Path,
    label: &'static str,
) -> Result<(), PathSafetyError> {
    let metadata = fs::symlink_metadata(path).map_err(|source| PathSafetyError::Io {
        operation: "inspect directory",
        path: path.to_path_buf(),
        source,
    })?;
    if metadata.file_type().is_symlink() {
        return Err(PathSafetyError::Symlink {
            path: path.to_path_buf(),
        });
    }
    if !metadata.is_dir() {
        return Err(PathSafetyError::WrongFileType {
            label,
            path: path.to_path_buf(),
        });
    }
    Ok(())
}

fn require_regular_file_without_symlink(
    path: &Path,
    label: &'static str,
) -> Result<(), PathSafetyError> {
    let metadata = fs::symlink_metadata(path).map_err(|source| PathSafetyError::Io {
        operation: "inspect file",
        path: path.to_path_buf(),
        source,
    })?;
    if metadata.file_type().is_symlink() {
        return Err(PathSafetyError::Symlink {
            path: path.to_path_buf(),
        });
    }
    if !metadata.is_file() {
        return Err(PathSafetyError::WrongFileType {
            label,
            path: path.to_path_buf(),
        });
    }
    Ok(())
}

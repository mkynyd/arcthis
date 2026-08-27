use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use tempfile::Builder;

use crate::error::{ArcthisError, Result};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CollisionPolicy {
    #[default]
    Refuse,
    Overwrite,
    SkipExisting,
    Rename,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationStatus {
    Completed,
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DestinationResolution {
    pub path: PathBuf,
    pub existed: bool,
    pub skip: bool,
    pub renamed: bool,
}

pub(crate) fn resolve_destination(
    requested: &Path,
    policy: CollisionPolicy,
) -> Result<DestinationResolution> {
    let existed = requested
        .try_exists()
        .map_err(|error| ArcthisError::io("checking destination", error))?;
    if !existed {
        return Ok(DestinationResolution {
            path: requested.to_path_buf(),
            existed: false,
            skip: false,
            renamed: false,
        });
    }
    match policy {
        CollisionPolicy::Refuse | CollisionPolicy::Overwrite => Ok(DestinationResolution {
            path: requested.to_path_buf(),
            existed: true,
            skip: false,
            renamed: false,
        }),
        CollisionPolicy::SkipExisting => Ok(DestinationResolution {
            path: requested.to_path_buf(),
            existed: true,
            skip: true,
            renamed: false,
        }),
        CollisionPolicy::Rename => Ok(DestinationResolution {
            path: first_available_renamed_path(requested)?,
            existed: true,
            skip: false,
            renamed: true,
        }),
    }
}

pub(crate) fn ensure_executable_resolution(
    resolution: &DestinationResolution,
    policy: CollisionPolicy,
) -> Result<()> {
    if resolution.existed && policy == CollisionPolicy::Refuse {
        return Err(ArcthisError::Collision {
            message: format!("destination already exists: {}", resolution.path.display()),
        });
    }
    Ok(())
}

pub(crate) fn commit_staged_path(
    staged: &Path,
    destination: &Path,
    policy: CollisionPolicy,
) -> Result<()> {
    let destination_exists = destination
        .try_exists()
        .map_err(|error| ArcthisError::io("checking commit destination", error))?;
    if !destination_exists {
        return fs::rename(staged, destination)
            .map_err(|error| ArcthisError::io("committing destination", error));
    }
    if policy != CollisionPolicy::Overwrite {
        return Err(ArcthisError::Collision {
            message: format!("destination already exists: {}", destination.display()),
        });
    }

    let parent = destination.parent().unwrap_or_else(|| Path::new("."));
    let backup_file = Builder::new()
        .prefix(".arcthis-backup-")
        .tempfile_in(parent)
        .map_err(|error| ArcthisError::io("reserving destination backup", error))?;
    let (_, backup_path) = backup_file
        .keep()
        .map_err(|error| ArcthisError::io("preserving destination backup path", error.error))?;
    fs::remove_file(&backup_path)
        .map_err(|error| ArcthisError::io("preparing destination backup", error))?;

    fs::rename(destination, &backup_path)
        .map_err(|error| ArcthisError::io("backing up existing destination", error))?;
    if let Err(commit_error) = fs::rename(staged, destination) {
        let restore_result = fs::rename(&backup_path, destination);
        return Err(if let Err(restore_error) = restore_result {
            ArcthisError::PartialFailure {
                message: format!(
                    "commit failed ({commit_error}) and restoring the previous destination failed ({restore_error}); backup remains at {}",
                    backup_path.display()
                ),
            }
        } else {
            ArcthisError::io("committing replacement destination", commit_error)
        });
    }
    remove_path(&backup_path)
        .map_err(|error| ArcthisError::io("removing replaced destination backup", error))?;
    Ok(())
}

pub(crate) fn delete_source(path: &Path) -> Result<()> {
    remove_path(path).map_err(|error| ArcthisError::io("deleting source after success", error))
}

fn remove_path(path: &Path) -> std::io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    }
}

fn first_available_renamed_path(path: &Path) -> Result<PathBuf> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| ArcthisError::UnsupportedOperation {
            message: format!("cannot derive a renamed destination for {}", path.display()),
        })?;
    let (stem, suffix) = split_archive_suffix(file_name);
    for index in 1..=10_000_u32 {
        let name = format!("{stem}.{index}{suffix}");
        let candidate = parent.join(name);
        if !candidate
            .try_exists()
            .map_err(|error| ArcthisError::io("checking renamed destination", error))?
        {
            return Ok(candidate);
        }
    }
    Err(ArcthisError::Collision {
        message: format!(
            "could not find an available renamed path for {}",
            path.display()
        ),
    })
}

fn split_archive_suffix(file_name: &str) -> (&str, &str) {
    let lower = file_name.to_ascii_lowercase();
    for suffix in [".tar.bz2", ".tar.zst", ".tar.gz", ".tar.xz"] {
        if lower.ends_with(suffix) && file_name.len() > suffix.len() {
            return (
                &file_name[..file_name.len() - suffix.len()],
                &file_name[file_name.len() - suffix.len()..],
            );
        }
    }
    if let Some(index) = file_name.rfind('.')
        && index > 0
    {
        return (&file_name[..index], &file_name[index..]);
    }
    (file_name, "")
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::{CollisionPolicy, resolve_destination};

    #[test]
    fn rename_policy_preserves_extension() {
        let directory = TempDir::new().expect("create temp directory");
        let requested = directory.path().join("archive.tar.zst");
        std::fs::write(&requested, b"existing").expect("create collision");
        let resolution =
            resolve_destination(&requested, CollisionPolicy::Rename).expect("resolve destination");
        assert_eq!(
            resolution.path.file_name().and_then(|name| name.to_str()),
            Some("archive.1.tar.zst")
        );
    }
}

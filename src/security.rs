use std::path::PathBuf;
use std::time::Duration;

use crate::error::{ArcthisError, Result};
use crate::model::EntryKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExtractionLimits {
    pub max_entries: u64,
    pub max_total_size: u64,
    pub max_entry_size: u64,
    pub max_path_bytes: usize,
    pub max_components: usize,
    pub max_compression_ratio: Option<u64>,
    pub max_entry_duration: Option<Duration>,
}

impl Default for ExtractionLimits {
    fn default() -> Self {
        Self {
            max_entries: 100_000,
            max_total_size: 16 * 1024 * 1024 * 1024,
            max_entry_size: 4 * 1024 * 1024 * 1024,
            max_path_bytes: 4_096,
            max_components: 256,
            max_compression_ratio: None,
            max_entry_duration: None,
        }
    }
}

pub(crate) fn validate_entry_path(
    path: &str,
    kind: EntryKind,
    limits: &ExtractionLimits,
) -> Result<PathBuf> {
    if path.is_empty() {
        return unsafe_path(path, "path is empty");
    }
    if path.contains('\0') {
        return unsafe_path(path, "path contains a NUL byte");
    }
    if path.len() > limits.max_path_bytes {
        return Err(ArcthisError::ResourceLimit {
            message: format!(
                "entry path uses {} bytes, above the {} byte limit",
                path.len(),
                limits.max_path_bytes
            ),
        });
    }
    if path.starts_with('/') || path.starts_with("//") {
        return unsafe_path(path, "absolute or UNC paths are not allowed");
    }
    if path.contains('\\') {
        return unsafe_path(path, "backslash separators are not allowed");
    }
    let bytes = path.as_bytes();
    if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        return unsafe_path(path, "Windows drive prefixes are not allowed");
    }

    let trimmed = if kind == EntryKind::Directory {
        path.trim_end_matches('/')
    } else {
        path
    };
    if trimmed.is_empty() {
        return unsafe_path(path, "path has no usable component");
    }
    let components = trimmed.split('/').collect::<Vec<_>>();
    if components.len() > limits.max_components {
        return Err(ArcthisError::ResourceLimit {
            message: format!(
                "entry path has {} components, above the {} component limit",
                components.len(),
                limits.max_components
            ),
        });
    }

    let mut result = PathBuf::new();
    for component in components {
        if component.is_empty() {
            return unsafe_path(path, "empty path components are not allowed");
        }
        if component == "." || component == ".." {
            return unsafe_path(path, "dot path components are not allowed");
        }
        if component.len() > 255 {
            return Err(ArcthisError::ResourceLimit {
                message: format!("entry path component exceeds 255 bytes: `{component}`"),
            });
        }
        result.push(component);
    }
    Ok(result)
}

fn unsafe_path<T>(path: &str, reason: &str) -> Result<T> {
    Err(ArcthisError::UnsafePath {
        path: path.to_owned(),
        reason: reason.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::{ExtractionLimits, validate_entry_path};
    use crate::model::EntryKind;

    #[test]
    fn accepts_normal_relative_paths() {
        let path = validate_entry_path(
            "docs/资料/readme.md",
            EntryKind::File,
            &ExtractionLimits::default(),
        )
        .expect("safe path");
        assert_eq!(path.to_string_lossy(), "docs/资料/readme.md");
    }

    #[test]
    fn rejects_parent_absolute_drive_and_backslash_paths() {
        let limits = ExtractionLimits::default();
        for path in ["../escape", "/etc/passwd", "C:/escape", r"..\escape"] {
            assert!(validate_entry_path(path, EntryKind::File, &limits).is_err());
        }
    }

    #[test]
    fn permits_one_trailing_slash_only_for_directories() {
        let limits = ExtractionLimits::default();
        assert!(validate_entry_path("empty/", EntryKind::Directory, &limits).is_ok());
        assert!(validate_entry_path("file/", EntryKind::File, &limits).is_err());
    }
}

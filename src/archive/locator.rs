use std::path::{Path, PathBuf};

/// Explicit source of archive bytes. v0.1 supports local files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveLocator {
    path: PathBuf,
}

impl ArchiveLocator {
    pub fn file(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl From<PathBuf> for ArchiveLocator {
    fn from(path: PathBuf) -> Self {
        Self::file(path)
    }
}

impl From<&Path> for ArchiveLocator {
    fn from(path: &Path) -> Self {
        Self::file(path)
    }
}

impl From<&str> for ArchiveLocator {
    fn from(path: &str) -> Self {
        Self::file(path)
    }
}

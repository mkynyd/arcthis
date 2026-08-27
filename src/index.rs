use std::fmt::Write as _;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tempfile::NamedTempFile;

use crate::archive::{Archive, ArchiveOpenOptions};
use crate::error::{ArcthisError, Result};
use crate::lifecycle::{CollisionPolicy, commit_staged_path};
use crate::model::{ArchiveEntry, ArchiveFormat};

const INDEX_SCHEMA_VERSION: &str = "1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IndexAction {
    Created,
    Refreshed,
    Reused,
    Deleted,
    WouldCreate,
    WouldRefresh,
    WouldReuse,
    WouldDelete,
    Missing,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct IndexResult {
    pub archive: PathBuf,
    pub index_path: PathBuf,
    pub action: IndexAction,
    pub entries_indexed: u64,
    pub dry_run: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Fingerprint {
    size: u64,
    modified_seconds: u64,
    modified_nanos: u32,
}

#[derive(Debug, Serialize, Deserialize)]
struct IndexDocument {
    schema_version: String,
    fingerprint: Fingerprint,
    format: ArchiveFormat,
    entries: Vec<ArchiveEntry>,
}

pub fn maintain_index(
    archive_path: &Path,
    open_options: &ArchiveOpenOptions,
    refresh: bool,
    delete: bool,
    dry_run: bool,
) -> Result<IndexResult> {
    let canonical = canonical_archive(archive_path)?;
    let index_path = cache_path(&canonical, open_options.index_directory.as_deref())?;
    let existed = index_path.exists();
    if delete {
        if !dry_run && existed {
            fs::remove_file(&index_path)
                .map_err(|error| ArcthisError::io("deleting archive index", error))?;
        }
        return Ok(IndexResult {
            archive: canonical,
            index_path,
            action: if !existed {
                IndexAction::Missing
            } else if dry_run {
                IndexAction::WouldDelete
            } else {
                IndexAction::Deleted
            },
            entries_indexed: 0,
            dry_run,
        });
    }

    if !refresh
        && let Some(entries) = load_cached_entries(
            &canonical,
            detect_archive_format(&canonical, open_options)?,
            open_options.index_directory.as_deref(),
        )?
    {
        return Ok(IndexResult {
            archive: canonical,
            index_path,
            action: if dry_run {
                IndexAction::WouldReuse
            } else {
                IndexAction::Reused
            },
            entries_indexed: u64::try_from(entries.len()).unwrap_or(u64::MAX),
            dry_run,
        });
    }

    let archive = Archive::open_with_options(canonical.as_path(), open_options)?;
    let entries = if refresh {
        archive.refresh_entries()?
    } else {
        archive.entries()?
    };
    if dry_run {
        return Ok(IndexResult {
            archive: canonical,
            index_path,
            action: if existed {
                IndexAction::WouldRefresh
            } else {
                IndexAction::WouldCreate
            },
            entries_indexed: entries.len() as u64,
            dry_run: true,
        });
    }
    write_index(&canonical, archive.format(), &entries, &index_path)?;
    Ok(IndexResult {
        archive: canonical,
        index_path,
        action: if existed {
            IndexAction::Refreshed
        } else {
            IndexAction::Created
        },
        entries_indexed: entries.len() as u64,
        dry_run: false,
    })
}

fn detect_archive_format(path: &Path, open_options: &ArchiveOpenOptions) -> Result<ArchiveFormat> {
    Archive::open_with_options(path, open_options).map(|archive| archive.format())
}

pub(crate) fn load_cached_entries(
    archive_path: &Path,
    format: ArchiveFormat,
    index_directory: Option<&Path>,
) -> Result<Option<Vec<ArchiveEntry>>> {
    let Ok(canonical) = archive_path.canonicalize() else {
        return Ok(None);
    };
    let path = cache_path(&canonical, index_directory)?;
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(ArcthisError::io("reading archive index", error)),
    };
    let document: IndexDocument = match serde_json::from_slice(&bytes) {
        Ok(document) => document,
        Err(_) => return Ok(None),
    };
    if document.schema_version != INDEX_SCHEMA_VERSION
        || document.format != format
        || document.fingerprint != fingerprint(&canonical)?
    {
        return Ok(None);
    }
    Ok(Some(document.entries))
}

fn write_index(
    archive_path: &Path,
    format: ArchiveFormat,
    entries: &[ArchiveEntry],
    index_path: &Path,
) -> Result<()> {
    let parent = index_path
        .parent()
        .ok_or_else(|| ArcthisError::UnsupportedOperation {
            message: "archive index path has no parent".to_owned(),
        })?;
    fs::create_dir_all(parent)
        .map_err(|error| ArcthisError::io("creating archive index directory", error))?;
    let mut temporary = NamedTempFile::new_in(parent)
        .map_err(|error| ArcthisError::io("creating temporary archive index", error))?;
    let document = IndexDocument {
        schema_version: INDEX_SCHEMA_VERSION.to_owned(),
        fingerprint: fingerprint(archive_path)?,
        format,
        entries: entries.to_vec(),
    };
    serde_json::to_writer(temporary.as_file_mut(), &document).map_err(|error| {
        ArcthisError::io("serializing archive index", std::io::Error::other(error))
    })?;
    temporary
        .as_file_mut()
        .flush()
        .map_err(|error| ArcthisError::io("flushing archive index", error))?;
    temporary
        .as_file()
        .sync_all()
        .map_err(|error| ArcthisError::io("syncing archive index", error))?;
    let (_, staged) = temporary
        .keep()
        .map_err(|error| ArcthisError::io("preserving staged archive index", error.error))?;
    commit_staged_path(&staged, index_path, CollisionPolicy::Overwrite)
}

fn canonical_archive(path: &Path) -> Result<PathBuf> {
    path.canonicalize()
        .map_err(|error| ArcthisError::io("canonicalizing archive for index", error))
}

fn fingerprint(path: &Path) -> Result<Fingerprint> {
    let metadata = fs::metadata(path)
        .map_err(|error| ArcthisError::io("reading indexed archive metadata", error))?;
    let duration = metadata
        .modified()
        .map_err(|error| ArcthisError::io("reading indexed archive modification time", error))?
        .duration_since(UNIX_EPOCH)
        .map_err(|_| ArcthisError::UnsupportedOperation {
            message: "archive modification time predates Unix epoch".to_owned(),
        })?;
    Ok(Fingerprint {
        size: metadata.len(),
        modified_seconds: duration.as_secs(),
        modified_nanos: duration.subsec_nanos(),
    })
}

fn cache_path(canonical: &Path, index_directory: Option<&Path>) -> Result<PathBuf> {
    let root = if let Some(directory) = index_directory {
        directory.to_path_buf()
    } else {
        ProjectDirs::from("dev", "arcthis", "arcthis")
            .ok_or_else(|| ArcthisError::UnsupportedOperation {
                message: "cannot determine the system cache directory".to_owned(),
            })?
            .cache_dir()
            .to_path_buf()
    };
    let mut hasher = Sha256::new();
    hasher.update(canonical.as_os_str().as_encoded_bytes());
    let digest = hasher.finalize();
    let name = digest
        .iter()
        .fold(String::with_capacity(64), |mut output, byte| {
            let _ = write!(output, "{byte:02x}");
            output
        });
    Ok(root.join("indexes").join(format!("{name}.json")))
}

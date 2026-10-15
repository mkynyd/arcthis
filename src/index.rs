use std::fmt::Write as _;
use std::fs;
use std::io::{Read, Write};
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

const INDEX_SCHEMA_VERSION: &str = "2";

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
    change_identity: Option<[i64; 4]>,
}

#[derive(Debug, Serialize, Deserialize)]
struct IndexDocument {
    schema_version: String,
    archive: PathBuf,
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
    if open_options.name_encoding.is_some() {
        return Err(ArcthisError::InvalidArgument {
            message: "index stores original names; omit --encoding when maintaining a cache"
                .to_owned(),
        });
    }
    let canonical = canonical_archive(archive_path)?;
    let index_path = cache_path(&canonical, open_options.index_directory.as_deref())?;
    let existed = index_path.exists();
    if delete {
        let count = read_document(&index_path)
            .ok()
            .flatten()
            .map_or(0, |document| {
                u64::try_from(document.entries.len()).unwrap_or(u64::MAX)
            });
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
            entries_indexed: count,
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
    let Some(document) = read_document(&path)? else {
        return Ok(None);
    };
    if document.schema_version != INDEX_SCHEMA_VERSION
        || document.archive != canonical
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
        archive: archive_path.to_path_buf(),
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
        change_identity: change_identity(&metadata),
    })
}

fn cache_path(canonical: &Path, index_directory: Option<&Path>) -> Result<PathBuf> {
    let root = cache_root(index_directory)?;
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

#[cfg(unix)]
#[allow(clippy::unnecessary_wraps)] // Same optional return contract as platforms without ctime/inode.
fn change_identity(metadata: &fs::Metadata) -> Option<[i64; 4]> {
    use std::os::unix::fs::MetadataExt;
    Some([
        metadata.ctime(),
        metadata.ctime_nsec(),
        i64::try_from(metadata.dev()).unwrap_or(i64::MAX),
        i64::try_from(metadata.ino()).unwrap_or(i64::MAX),
    ])
}
#[cfg(not(unix))]
fn change_identity(_metadata: &fs::Metadata) -> Option<[i64; 4]> {
    None
}

fn cache_root(index_directory: Option<&Path>) -> Result<PathBuf> {
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
    Ok(root)
}

#[derive(Debug, Serialize)]
pub struct CacheItem {
    pub index_path: PathBuf,
    pub archive: Option<PathBuf>,
    pub entries_indexed: u64,
    pub bytes: u64,
    pub stale: bool,
    pub deleted: bool,
}
#[derive(Debug, Serialize)]
pub struct CacheResult {
    pub schema_version: &'static str,
    pub operation: &'static str,
    pub dry_run: bool,
    pub items: Vec<CacheItem>,
}
/// Enumerate or prune only Arcthis hash-named index documents in its cache subdirectory.
pub fn manage_cache(
    index_directory: Option<&Path>,
    prune: bool,
    dry_run: bool,
    max_age_seconds: u64,
) -> Result<CacheResult> {
    let directory = cache_root(index_directory)?.join("indexes");
    let mut items = Vec::new();
    if directory.exists() {
        for entry in
            fs::read_dir(&directory).map_err(|error| ArcthisError::io("listing indexes", error))?
        {
            let entry = entry.map_err(|error| ArcthisError::io("listing index", error))?;
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.len() != 69
                || !name.ends_with(".json")
                || !name.as_bytes()[..64].iter().all(u8::is_ascii_hexdigit)
                || !entry
                    .file_type()
                    .map_err(|error| ArcthisError::io("reading index type", error))?
                    .is_file()
            {
                continue;
            }
            let metadata = entry
                .metadata()
                .map_err(|error| ArcthisError::io("reading index metadata", error))?;
            let document = read_document(&path)?;
            let expired = metadata
                .modified()
                .ok()
                .and_then(|modified| modified.elapsed().ok())
                .is_some_and(|age| age.as_secs() >= max_age_seconds);
            let stale = expired
                || document.as_ref().is_none_or(|document| {
                    document.schema_version != INDEX_SCHEMA_VERSION
                        || fingerprint(&document.archive).ok().as_ref()
                            != Some(&document.fingerprint)
                });
            let deleted = prune && stale && !dry_run;
            if deleted {
                fs::remove_file(&path).map_err(|error| ArcthisError::io("pruning index", error))?;
            }
            items.push(CacheItem {
                index_path: path,
                archive: document.as_ref().map(|document| document.archive.clone()),
                entries_indexed: document.map_or(0, |document| {
                    u64::try_from(document.entries.len()).unwrap_or(u64::MAX)
                }),
                bytes: metadata.len(),
                stale,
                deleted,
            });
        }
    }
    items.sort_by(|a, b| a.index_path.cmp(&b.index_path));
    Ok(CacheResult {
        schema_version: crate::SCHEMA_VERSION,
        operation: if prune { "index_prune" } else { "index_list" },
        dry_run,
        items,
    })
}

impl std::fmt::Display for IndexAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Created => "created",
            Self::Refreshed => "refreshed",
            Self::Reused => "reused",
            Self::Deleted => "deleted",
            Self::WouldCreate => "would create",
            Self::WouldRefresh => "would refresh",
            Self::WouldReuse => "would reuse",
            Self::WouldDelete => "would delete",
            Self::Missing => "missing",
        })
    }
}

fn read_document(path: &Path) -> Result<Option<IndexDocument>> {
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(ArcthisError::io("opening archive index", error)),
    };
    let metadata = file
        .metadata()
        .map_err(|error| ArcthisError::io("reading index metadata", error))?;
    if !metadata.is_file() || metadata.len() > 64 * 1024 * 1024 {
        return Ok(None);
    }
    let mut bytes = Vec::new();
    file.take(64 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| ArcthisError::io("reading archive index", error))?;
    if bytes.len() > 64 * 1024 * 1024 {
        return Ok(None);
    }
    Ok(serde_json::from_slice(&bytes).ok())
}

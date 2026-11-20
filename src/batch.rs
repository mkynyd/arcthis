use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, mpsc};

use serde::Serialize;
use walkdir::WalkDir;

use crate::archive::{Archive, ArchiveOpenOptions};
use crate::error::{ArcthisError, ErrorCode, Result};
use crate::extract::{ExtractOptions, ExtractPlan, ExtractResult};
use crate::lifecycle::{CollisionPolicy, OperationStatus, comparable_path};

#[derive(Debug, Clone)]
pub struct ExtractAllOptions {
    pub recursive: bool,
    pub continue_on_error: bool,
    pub workers: usize,
    pub extract: ExtractOptions,
    pub open: ArchiveOpenOptions,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExtractAllPlan {
    pub root: PathBuf,
    pub recursive: bool,
    pub workers: usize,
    pub archives: Vec<ExtractPlan>,
    pub destination_conflicts: Vec<String>,
    pub errors: Vec<ExtractAllItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExtractAllItem {
    pub archive: PathBuf,
    pub destination: Option<PathBuf>,
    pub status: Option<OperationStatus>,
    pub entries_extracted: u64,
    pub bytes_written: u64,
    pub source_deleted: bool,
    pub error_code: Option<ErrorCode>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExtractAllResult {
    pub root: PathBuf,
    pub discovered: u64,
    pub succeeded: u64,
    pub skipped: u64,
    pub failed: u64,
    pub items: Vec<ExtractAllItem>,
}

pub fn plan_extract_all(root: &Path, options: &ExtractAllOptions) -> Result<ExtractAllPlan> {
    let root = canonical_directory(root)?;
    let mut errors = Vec::new();
    let paths = discover_archives(
        &root,
        options.recursive,
        &options.open,
        options.continue_on_error,
        &mut errors,
    )?;
    // All discovered sources, including archives that fail validation, remain protected.
    let sources = paths
        .iter()
        .chain(errors.iter().map(|item| &item.archive))
        .map(|path| comparable_path(path).map(|path| (path_key(&path), path)))
        .collect::<Result<BTreeMap<_, _>>>()?;
    let mut destinations = BTreeMap::new();
    let mut destination_conflicts = Vec::new();
    let mut archives = Vec::with_capacity(paths.len());
    for path in paths {
        let archive_options = per_archive_options(&path, &options.extract);
        let result =
            Archive::open_with_options(path.as_path(), &options.open).and_then(|archive| {
                archive.plan_extract_reserved(None, &archive_options, &|candidate| {
                    let key = path_key(candidate);
                    find_overlapping_path(&sources, &key).is_some()
                        || find_overlapping_path(&destinations, &key).is_some()
                })
            });
        let plan = match result {
            Ok(plan) => plan,
            Err(error) if options.continue_on_error => {
                errors.push(failed_item(&path, &error));
                continue;
            }
            Err(error) => return Err(error),
        };
        if !plan.will_skip {
            let key = path_key(&plan.destination);
            let conflict = find_overlapping_path(&sources, &key)
                .map(|source| {
                    format!(
                        "destination {} overlaps archive source {}",
                        plan.destination.display(),
                        source.display()
                    )
                })
                .or_else(|| {
                    find_overlapping_path(&destinations, &key).map(|destination| {
                        format!(
                            "destinations {} and {} overlap",
                            destination.display(),
                            plan.destination.display()
                        )
                    })
                });
            if let Some(message) = conflict {
                if options.continue_on_error {
                    errors.push(failed_item(&path, &ArcthisError::Collision { message }));
                    continue;
                }
                destination_conflicts.push(message);
            }
            if options.continue_on_error
                && plan.collision
                && archive_options.collision_policy == CollisionPolicy::Refuse
            {
                errors.push(failed_item(
                    &path,
                    &ArcthisError::Collision {
                        message: format!(
                            "destination already exists: {}",
                            plan.destination.display()
                        ),
                    },
                ));
                continue;
            }
            destinations.insert(key, plan.destination.clone());
        }
        archives.push(plan);
    }
    errors.sort_by(|left, right| left.archive.cmp(&right.archive));
    Ok(ExtractAllPlan {
        root,
        recursive: options.recursive,
        workers: normalized_workers(options.workers),
        archives,
        destination_conflicts,
        errors,
    })
}

fn path_key(path: &Path) -> PathBuf {
    // Absent destinations cannot be canonicalized to filesystem spelling. Reserve
    // case variants together so parallel workers remain independent on macOS too.
    path.components()
        .map(|component| {
            let name = component.as_os_str();
            name.to_str().map_or_else(
                || name.to_ascii_lowercase(),
                |name| name.to_lowercase().into(),
            )
        })
        .collect()
}

fn find_overlapping_path<'a>(
    paths: &'a BTreeMap<PathBuf, PathBuf>,
    candidate: &Path,
) -> Option<&'a PathBuf> {
    for ancestor in candidate.ancestors() {
        if let Some(original) = paths.get(ancestor) {
            return Some(original);
        }
    }
    paths
        .range(candidate.to_path_buf()..)
        .next()
        .filter(|(path, _)| path.starts_with(candidate))
        .map(|(_, original)| original)
}

pub fn extract_all(root: &Path, options: &ExtractAllOptions) -> Result<ExtractAllResult> {
    let plan = plan_extract_all(root, options)?;
    if !plan.destination_conflicts.is_empty() {
        return Err(ArcthisError::Collision {
            message: format!(
                "extract-all plan contains conflicting destinations: {}",
                plan.destination_conflicts.join("; ")
            ),
        });
    }
    let root = plan.root;
    let mut initial_items = plan.errors;
    let paths = plan
        .archives
        .into_iter()
        .map(|archive| (archive.source, archive.destination))
        .collect::<Vec<_>>();
    let discovered = u64::try_from(paths.len() + initial_items.len()).unwrap_or(u64::MAX);
    let queue = Arc::new(Mutex::new(VecDeque::from(paths)));
    let (sender, receiver) = mpsc::channel();
    let workers = normalized_workers(options.workers);
    std::thread::scope(|scope| {
        for _ in 0..workers {
            let queue = Arc::clone(&queue);
            let sender = sender.clone();
            let extract_options = options.extract.clone();
            let open_options = options.open.clone();
            scope.spawn(move || {
                loop {
                    let path = match queue.lock() {
                        Ok(mut queue) => queue.pop_front(),
                        Err(_) => None,
                    };
                    let Some((path, destination)) = path else {
                        break;
                    };
                    let result = execute_one(&path, &destination, &extract_options, &open_options);
                    if sender.send(result).is_err() {
                        break;
                    }
                }
            });
        }
    });
    drop(sender);
    initial_items.extend(receiver);
    let mut items = initial_items;
    items.sort_by(|left, right| left.archive.cmp(&right.archive));
    let succeeded = u64::try_from(
        items
            .iter()
            .filter(|item| item.status == Some(OperationStatus::Completed))
            .count(),
    )
    .unwrap_or(u64::MAX);
    let skipped = u64::try_from(
        items
            .iter()
            .filter(|item| item.status == Some(OperationStatus::Skipped))
            .count(),
    )
    .unwrap_or(u64::MAX);
    let failed = u64::try_from(
        items
            .iter()
            .filter(|item| item.error_code.is_some())
            .count(),
    )
    .unwrap_or(u64::MAX);
    Ok(ExtractAllResult {
        root,
        discovered,
        succeeded,
        skipped,
        failed,
        items,
    })
}

fn execute_one(
    path: &Path,
    destination: &Path,
    options: &ExtractOptions,
    open_options: &ArchiveOpenOptions,
) -> ExtractAllItem {
    let result = Archive::open_with_options(path, open_options).and_then(|archive| {
        archive.extract_to_planned(&per_archive_options(path, options), destination)
    });
    match result {
        Ok(ExtractResult {
            destination,
            entries_extracted,
            bytes_written,
            status,
            source_deleted,
        }) => ExtractAllItem {
            archive: path.to_path_buf(),
            destination: Some(destination),
            status: Some(status),
            entries_extracted,
            bytes_written,
            source_deleted,
            error_code: None,
            error_message: None,
        },
        Err(error) => failed_item(path, &error),
    }
}

fn discover_archives(
    root: &Path,
    recursive: bool,
    open_options: &ArchiveOpenOptions,
    continue_on_error: bool,
    failures: &mut Vec<ExtractAllItem>,
) -> Result<Vec<PathBuf>> {
    let max_depth = if recursive { usize::MAX } else { 1 };
    let mut paths = Vec::new();
    for entry in WalkDir::new(root)
        .follow_links(false)
        .max_depth(max_depth)
        .sort_by_file_name()
    {
        let entry = entry.map_err(|error| {
            let kind = error
                .io_error()
                .map_or(std::io::ErrorKind::Other, std::io::Error::kind);
            ArcthisError::io(
                "scanning archives for extract-all",
                std::io::Error::new(kind, error),
            )
        })?;
        if !entry.file_type().is_file() {
            continue;
        }
        match Archive::open_with_options(entry.path(), open_options) {
            Ok(_) => paths.push(entry.path().to_path_buf()),
            Err(ArcthisError::UnsupportedFormat { .. }) => {}
            Err(error) if continue_on_error => failures.push(failed_item(entry.path(), &error)),
            Err(error) => return Err(error),
        }
    }
    Ok(paths)
}

fn per_archive_options(path: &Path, options: &ExtractOptions) -> ExtractOptions {
    let mut options = options.clone();
    options.output = None;
    options.base_directory = path.parent().map(Path::to_path_buf);
    options
}

fn canonical_directory(path: &Path) -> Result<PathBuf> {
    let path = std::fs::canonicalize(path)
        .map_err(|error| ArcthisError::io("resolving extract-all directory", error))?;
    if !path.is_dir() {
        return Err(ArcthisError::UnsupportedOperation {
            message: format!("extract-all source is not a directory: {}", path.display()),
        });
    }
    Ok(path)
}

fn normalized_workers(workers: usize) -> usize {
    workers.clamp(1, 64)
}

fn failed_item(path: &Path, error: &ArcthisError) -> ExtractAllItem {
    ExtractAllItem {
        archive: path.to_path_buf(),
        destination: None,
        status: None,
        entries_extracted: 0,
        bytes_written: 0,
        source_deleted: false,
        error_code: Some(error.code()),
        error_message: Some(error.to_string()),
    }
}

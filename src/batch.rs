use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, mpsc};

use serde::Serialize;
use walkdir::WalkDir;

use crate::archive::{Archive, ArchiveOpenOptions};
use crate::error::{ArcthisError, ErrorCode, Result};
use crate::extract::{ExtractOptions, ExtractPlan, ExtractResult};
use crate::lifecycle::OperationStatus;

#[derive(Debug, Clone)]
pub struct ExtractAllOptions {
    pub recursive: bool,
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
    let paths = discover_archives(&root, options.recursive, &options.open)?;
    let workers = normalized_workers(options.workers);
    let mut archives = Vec::with_capacity(paths.len());
    let mut destinations = HashMap::<PathBuf, PathBuf>::new();
    let mut destination_conflicts = Vec::new();
    for path in paths {
        let archive = Archive::open_with_options(path.as_path(), &options.open)?;
        let extract_options = per_archive_options(&path, &options.extract);
        let plan = archive.plan_extract(None, &extract_options)?;
        if let Some(previous) = destinations.insert(plan.destination.clone(), path.clone()) {
            destination_conflicts.push(format!(
                "{} and {} both resolve to {}",
                previous.display(),
                path.display(),
                plan.destination.display()
            ));
        }
        archives.push(plan);
    }
    Ok(ExtractAllPlan {
        root,
        recursive: options.recursive,
        workers,
        archives,
        destination_conflicts,
    })
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
    let paths = plan
        .archives
        .into_iter()
        .map(|archive| archive.source)
        .collect::<Vec<_>>();
    let discovered = u64::try_from(paths.len()).unwrap_or(u64::MAX);
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
                    let Some(path) = path else {
                        break;
                    };
                    let result = execute_one(&path, &extract_options, &open_options);
                    if sender.send(result).is_err() {
                        break;
                    }
                }
            });
        }
    });
    drop(sender);
    let mut items = receiver.into_iter().collect::<Vec<_>>();
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
    options: &ExtractOptions,
    open_options: &ArchiveOpenOptions,
) -> ExtractAllItem {
    let result = Archive::open_with_options(path, open_options)
        .and_then(|archive| archive.extract(None, &per_archive_options(path, options)));
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
        Err(error) => ExtractAllItem {
            archive: path.to_path_buf(),
            destination: None,
            status: None,
            entries_extracted: 0,
            bytes_written: 0,
            source_deleted: false,
            error_code: Some(error.code()),
            error_message: Some(error.to_string()),
        },
    }
}

fn discover_archives(
    root: &Path,
    recursive: bool,
    open_options: &ArchiveOpenOptions,
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

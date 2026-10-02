//! Frontend-neutral application service for archive access operations.

use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use serde::Serialize;

use crate::archive::{Archive, ArchiveOpenOptions};
use crate::error::{ArcthisError, Result};
use crate::model::{
    ArchiveEntry, ArchiveFormat, ArchiveInspection, EntryCopyResult, EntryKind, VerificationResult,
};
use crate::query::{FindResult, GrepOptions, GrepResult, HashAlgorithm, HashResult};
use crate::security::ExtractionLimits;

/// Explicit source description shared by non-CLI frontends.
#[derive(Debug, Default)]
pub struct ArchiveSourceRequest {
    pub path: PathBuf,
    pub within: Vec<String>,
    pub max_nested_entry_size: u64,
    pub open_options: ArchiveOpenOptions,
}

impl ArchiveSourceRequest {
    pub fn file(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            within: Vec::new(),
            max_nested_entry_size: 256 * 1024 * 1024,
            open_options: ArchiveOpenOptions::default(),
        }
    }
}

/// Shared resource limits enforced by the application service.
#[derive(Debug, Clone, Copy)]
pub struct ServiceLimits {
    pub max_entries: u64,
    pub max_decoded_bytes: u64,
    pub max_results: u64,
    pub max_read_window: u64,
    pub max_result_bytes: u64,
}

impl Default for ServiceLimits {
    fn default() -> Self {
        Self {
            max_entries: 100_000,
            max_decoded_bytes: 1024 * 1024 * 1024,
            max_results: 10_000,
            max_read_window: 1024 * 1024,
            max_result_bytes: 16 * 1024 * 1024,
        }
    }
}

impl ServiceLimits {
    /// Compatibility profile used by the CLI, whose existing flags own operation limits.
    pub const fn cli_compatibility() -> Self {
        Self {
            max_entries: u64::MAX,
            max_decoded_bytes: u64::MAX,
            max_results: u64::MAX,
            max_read_window: u64::MAX,
            max_result_bytes: u64::MAX,
        }
    }
}

/// Cooperative cancellation handle checked between archive operations and streaming writes.
#[derive(Debug, Clone, Default)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }

    pub(crate) fn checkpoint(&self) -> Result<()> {
        if self.is_cancelled() {
            Err(ArcthisError::UnsupportedOperation {
                message: "operation cancelled".to_owned(),
            })
        } else {
            Ok(())
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "mcp", derive(rmcp::schemars::JsonSchema))]
pub struct ArchiveReference {
    pub path: PathBuf,
    pub format: ArchiveFormat,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "mcp", derive(rmcp::schemars::JsonSchema))]
pub struct InspectResult {
    pub archive: ArchiveReference,
    pub inspection: ArchiveInspection,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "mcp", derive(rmcp::schemars::JsonSchema))]
pub struct ListResult {
    pub archive: ArchiveReference,
    pub entries: Vec<ArchiveEntry>,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "mcp", derive(rmcp::schemars::JsonSchema))]
pub struct TreeResult {
    pub archive: ArchiveReference,
    pub entries: Vec<ArchiveEntry>,
    pub tree: Vec<TreeNode>,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "mcp", derive(rmcp::schemars::JsonSchema))]
pub struct TreeNode {
    pub name: String,
    pub path: String,
    pub kind: EntryKind,
    pub entry: Option<ArchiveEntry>,
    pub children: Vec<Self>,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "mcp", derive(rmcp::schemars::JsonSchema))]
pub struct StatResult {
    pub archive: ArchiveReference,
    pub entry: ArchiveEntry,
}

#[derive(Debug, Clone)]
pub struct ReadRequest<'a> {
    pub source: &'a ArchiveSourceRequest,
    pub entry: &'a str,
    pub offset: u64,
    pub length: u64,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "mcp", derive(rmcp::schemars::JsonSchema))]
pub struct ReadResult {
    pub archive: ArchiveReference,
    pub entry: ArchiveEntry,
    pub offset: u64,
    pub data: Vec<u8>,
    pub eof: bool,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "mcp", derive(rmcp::schemars::JsonSchema))]
pub struct FindServiceResult {
    pub archive: ArchiveReference,
    pub find: FindResult,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "mcp", derive(rmcp::schemars::JsonSchema))]
pub struct GrepServiceResult {
    pub archive: ArchiveReference,
    pub grep: GrepResult,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "mcp", derive(rmcp::schemars::JsonSchema))]
pub struct HashServiceResult {
    pub archive: ArchiveReference,
    pub hash: HashResult,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "mcp", derive(rmcp::schemars::JsonSchema))]
pub struct VerifyServiceResult {
    pub archive: ArchiveReference,
    pub verification: VerificationResult,
}

/// Synchronous application boundary used by CLI, MCP, and future frontends.
#[derive(Debug, Clone)]
pub struct ApplicationService {
    limits: ServiceLimits,
    cancellation: CancellationToken,
}

impl Default for ApplicationService {
    fn default() -> Self {
        Self::new(ServiceLimits::default(), CancellationToken::default())
    }
}

impl ApplicationService {
    pub const fn new(limits: ServiceLimits, cancellation: CancellationToken) -> Self {
        Self {
            limits,
            cancellation,
        }
    }

    pub fn cancellation_token(&self) -> CancellationToken {
        self.cancellation.clone()
    }

    pub fn inspect(&self, source: &ArchiveSourceRequest) -> Result<InspectResult> {
        let archive = self.open(source)?;
        self.ensure_entries_within_limits(&archive)?;
        self.cancellation.checkpoint()?;
        let inspection = archive.inspect()?;
        self.cancellation.checkpoint()?;
        Ok(InspectResult {
            archive: reference(&archive),
            inspection,
        })
    }

    pub fn list(&self, source: &ArchiveSourceRequest) -> Result<ListResult> {
        let archive = self.open(source)?;
        let entries = self.entries(&archive)?;
        Ok(ListResult {
            archive: reference(&archive),
            entries,
        })
    }

    pub fn tree(&self, source: &ArchiveSourceRequest) -> Result<TreeResult> {
        let archive = self.open(source)?;
        let entries = self.entries(&archive)?;
        let tree = build_tree(&entries)?;
        Ok(TreeResult {
            archive: reference(&archive),
            entries,
            tree,
        })
    }

    pub fn stat(&self, source: &ArchiveSourceRequest, path: &str) -> Result<StatResult> {
        let archive = self.open(source)?;
        self.ensure_entries_within_limits(&archive)?;
        self.cancellation.checkpoint()?;
        let entry = archive.entry(path)?;
        Ok(StatResult {
            archive: reference(&archive),
            entry,
        })
    }

    pub fn read(&self, request: &ReadRequest<'_>) -> Result<ReadResult> {
        if request.length > self.limits.max_read_window {
            return Err(limit_error(
                "read window",
                request.length,
                self.limits.max_read_window,
            ));
        }
        let decoded_limit = request.offset.checked_add(request.length).ok_or_else(|| {
            ArcthisError::ResourceLimit {
                message: "read window end overflows u64".to_owned(),
            }
        })?;
        if decoded_limit > self.limits.max_decoded_bytes {
            return Err(limit_error(
                "decoded read bytes",
                decoded_limit,
                self.limits.max_decoded_bytes,
            ));
        }
        let archive = self.open(request.source)?;
        let entry = archive.entry(request.entry)?;
        let capacity =
            usize::try_from(request.length).map_err(|_| ArcthisError::ResourceLimit {
                message: "read window cannot fit in memory".to_owned(),
            })?;
        let mut writer = WindowWriter::new(
            request.offset,
            request.length,
            capacity,
            self.cancellation.clone(),
        )?;
        let copy = archive.copy_entry_to(request.entry, &mut writer);
        if let Err(error) = copy
            && !writer.window_complete()
        {
            return Err(error);
        }
        self.cancellation.checkpoint()?;
        let eof = request
            .offset
            .saturating_add(u64::try_from(writer.data.len()).unwrap_or(u64::MAX))
            >= entry.size;
        Ok(ReadResult {
            archive: reference(&archive),
            entry,
            offset: request.offset,
            data: writer.data,
            eof,
        })
    }

    pub fn read_to(
        &self,
        source: &ArchiveSourceRequest,
        entry: &str,
        writer: &mut dyn Write,
    ) -> Result<(ArchiveReference, EntryCopyResult)> {
        let archive = self.open(source)?;
        let metadata = archive.entry(entry)?;
        if metadata.size > self.limits.max_decoded_bytes {
            return Err(limit_error(
                "decoded entry bytes",
                metadata.size,
                self.limits.max_decoded_bytes,
            ));
        }
        let mut writer = CancellationWriter {
            inner: writer,
            cancellation: &self.cancellation,
        };
        let result = archive.copy_entry_to(entry, &mut writer)?;
        self.cancellation.checkpoint()?;
        Ok((reference(&archive), result))
    }

    pub fn find(&self, source: &ArchiveSourceRequest, glob: &str) -> Result<FindServiceResult> {
        let archive = self.open(source)?;
        self.ensure_entries_within_limits(&archive)?;
        self.cancellation.checkpoint()?;
        let find = crate::query::find(&archive, glob)?;
        ensure_count("find results", find.matched, self.limits.max_results)?;
        Ok(FindServiceResult {
            archive: reference(&archive),
            find,
        })
    }

    pub fn grep(
        &self,
        source: &ArchiveSourceRequest,
        pattern: &str,
        options: &GrepOptions,
    ) -> Result<GrepServiceResult> {
        ensure_count(
            "grep match limit",
            options.max_matches,
            self.limits.max_results,
        )?;
        if options.max_entry_size > self.limits.max_decoded_bytes {
            return Err(limit_error(
                "grep entry bytes",
                options.max_entry_size,
                self.limits.max_decoded_bytes,
            ));
        }
        let archive = self.open(source)?;
        self.ensure_entries_within_limits(&archive)?;
        self.cancellation.checkpoint()?;
        let mut options = options.clone();
        options.max_result_bytes = options.max_result_bytes.min(self.limits.max_result_bytes);
        options.max_total_size = options.max_total_size.min(self.limits.max_decoded_bytes);
        let grep = crate::query::grep(&archive, pattern, &options)?;
        self.cancellation.checkpoint()?;
        Ok(GrepServiceResult {
            archive: reference(&archive),
            grep,
        })
    }

    pub fn hash(
        &self,
        source: &ArchiveSourceRequest,
        entry: &str,
        algorithm: HashAlgorithm,
    ) -> Result<HashServiceResult> {
        let archive = self.open(source)?;
        let metadata = archive.entry(entry)?;
        if metadata.size > self.limits.max_decoded_bytes {
            return Err(limit_error(
                "hash decoded bytes",
                metadata.size,
                self.limits.max_decoded_bytes,
            ));
        }
        self.cancellation.checkpoint()?;
        let hash = crate::query::hash(&archive, entry, algorithm)?;
        self.cancellation.checkpoint()?;
        Ok(HashServiceResult {
            archive: reference(&archive),
            hash,
        })
    }

    pub fn verify(&self, source: &ArchiveSourceRequest) -> Result<VerifyServiceResult> {
        let archive = self.open(source)?;
        let entries = self.ensure_entries_within_limits(&archive)?;
        let decoded = entries
            .iter()
            .try_fold(0_u64, |total, entry| total.checked_add(entry.size))
            .ok_or_else(|| ArcthisError::ResourceLimit {
                message: "declared decoded byte total overflows u64".to_owned(),
            })?;
        if decoded > self.limits.max_decoded_bytes {
            return Err(limit_error(
                "verify decoded bytes",
                decoded,
                self.limits.max_decoded_bytes,
            ));
        }
        self.cancellation.checkpoint()?;
        let verification = archive.verify()?;
        self.cancellation.checkpoint()?;
        Ok(VerifyServiceResult {
            archive: reference(&archive),
            verification,
        })
    }

    fn open(&self, source: &ArchiveSourceRequest) -> Result<Archive> {
        self.cancellation.checkpoint()?;
        Archive::open_within_options(
            source.path.as_path(),
            &source.within,
            source.max_nested_entry_size,
            &source.open_options,
        )
    }

    fn entries(&self, archive: &Archive) -> Result<Vec<ArchiveEntry>> {
        let entries = self.ensure_entries_within_limits(archive)?;
        self.cancellation.checkpoint()?;
        Ok(entries)
    }

    fn ensure_entries_within_limits(&self, archive: &Archive) -> Result<Vec<ArchiveEntry>> {
        let entries = archive.entries()?;
        ensure_count(
            "archive entries",
            u64::try_from(entries.len()).unwrap_or(u64::MAX),
            self.limits.max_entries,
        )?;
        self.cancellation.checkpoint()?;
        Ok(entries)
    }
}

#[allow(clippy::too_many_lines)] // One iterative arena pass avoids recursion and duplicate tree construction.
pub(crate) fn build_tree(entries: &[ArchiveEntry]) -> Result<Vec<TreeNode>> {
    #[derive(Debug)]
    struct ArenaNode {
        name: String,
        path: String,
        kind: EntryKind,
        entry: Option<ArchiveEntry>,
        children: Vec<usize>,
    }

    let limits = ExtractionLimits::default();
    let mut arena = Vec::<ArenaNode>::new();
    let mut roots = Vec::<usize>::new();
    for entry in entries {
        if entry.path.len() > limits.max_path_bytes {
            return Err(limit_error(
                "tree entry path bytes",
                u64::try_from(entry.path.len()).unwrap_or(u64::MAX),
                u64::try_from(limits.max_path_bytes).unwrap_or(u64::MAX),
            ));
        }
        let components = entry
            .path
            .split('/')
            .filter(|component| !component.is_empty())
            .collect::<Vec<_>>();
        if components.is_empty() {
            continue;
        }
        if components.len() > limits.max_components {
            return Err(limit_error(
                "tree entry path components",
                u64::try_from(components.len()).unwrap_or(u64::MAX),
                u64::try_from(limits.max_components).unwrap_or(u64::MAX),
            ));
        }

        let mut parent: Option<usize> = None;
        let mut path = String::new();
        for (position, name) in components.iter().enumerate() {
            if !path.is_empty() {
                path.push('/');
            }
            path.push_str(name);
            let last = position + 1 == components.len();
            let siblings =
                parent.map_or(roots.as_slice(), |index| arena[index].children.as_slice());
            let existing_directory = siblings.iter().copied().find(|index| {
                arena[*index].name == *name && arena[*index].kind == EntryKind::Directory
            });

            if last {
                if entry.kind == EntryKind::Directory
                    && let Some(index) = existing_directory
                {
                    arena[index].entry = Some(entry.clone());
                    break;
                }
                let index = arena.len();
                arena.push(ArenaNode {
                    name: (*name).to_owned(),
                    path: path.clone(),
                    kind: entry.kind,
                    entry: Some(entry.clone()),
                    children: Vec::new(),
                });
                if let Some(parent) = parent {
                    arena[parent].children.push(index);
                } else {
                    roots.push(index);
                }
                break;
            }

            let index = existing_directory.unwrap_or_else(|| {
                let index = arena.len();
                arena.push(ArenaNode {
                    name: (*name).to_owned(),
                    path: path.clone(),
                    kind: EntryKind::Directory,
                    entry: None,
                    children: Vec::new(),
                });
                if let Some(parent) = parent {
                    arena[parent].children.push(index);
                } else {
                    roots.push(index);
                }
                index
            });
            parent = Some(index);
        }
    }

    roots.sort_by(|left, right| arena[*left].name.cmp(&arena[*right].name));
    for index in 0..arena.len() {
        let mut children = std::mem::take(&mut arena[index].children);
        children.sort_by(|left, right| arena[*left].name.cmp(&arena[*right].name));
        arena[index].children = children;
    }
    let mut built = std::iter::repeat_with(|| None)
        .take(arena.len())
        .collect::<Vec<Option<TreeNode>>>();
    for index in (0..arena.len()).rev() {
        let children = arena[index]
            .children
            .iter()
            .map(|child| {
                built[*child]
                    .take()
                    .ok_or_else(|| ArcthisError::InvalidArchive {
                        message: "tree construction order is invalid".to_owned(),
                    })
            })
            .collect::<Result<Vec<_>>>()?;
        built[index] = Some(TreeNode {
            name: std::mem::take(&mut arena[index].name),
            path: std::mem::take(&mut arena[index].path),
            kind: arena[index].kind,
            entry: arena[index].entry.take(),
            children,
        });
    }
    roots
        .into_iter()
        .map(|root| {
            built[root]
                .take()
                .ok_or_else(|| ArcthisError::InvalidArchive {
                    message: "tree root construction is invalid".to_owned(),
                })
        })
        .collect()
}

fn reference(archive: &Archive) -> ArchiveReference {
    ArchiveReference {
        path: archive.path().to_path_buf(),
        format: archive.format(),
    }
}

fn ensure_count(label: &str, actual: u64, limit: u64) -> Result<()> {
    if actual > limit {
        Err(limit_error(label, actual, limit))
    } else {
        Ok(())
    }
}

fn limit_error(label: &str, actual: u64, limit: u64) -> ArcthisError {
    ArcthisError::ResourceLimit {
        message: format!("{label} {actual} exceeds limit {limit}"),
    }
}

struct CancellationWriter<'a> {
    inner: &'a mut dyn Write,
    cancellation: &'a CancellationToken,
}

impl Write for CancellationWriter<'_> {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        if self.cancellation.is_cancelled() {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "operation cancelled",
            ));
        }
        self.inner.write(buffer)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

struct WindowWriter {
    skip_remaining: u64,
    take_remaining: u64,
    data: Vec<u8>,
    cancellation: CancellationToken,
}

impl WindowWriter {
    fn new(skip: u64, take: u64, capacity: usize, cancellation: CancellationToken) -> Result<Self> {
        let mut data = Vec::new();
        data.try_reserve_exact(capacity)
            .map_err(|error| ArcthisError::ResourceLimit {
                message: format!("cannot reserve read window: {error}"),
            })?;
        Ok(Self {
            skip_remaining: skip,
            take_remaining: take,
            data,
            cancellation,
        })
    }

    const fn window_complete(&self) -> bool {
        self.take_remaining == 0
    }
}

impl Write for WindowWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        if self.cancellation.is_cancelled() {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "operation cancelled",
            ));
        }
        if self.take_remaining == 0 {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "read window complete",
            ));
        }
        let buffer_len = u64::try_from(buffer.len()).unwrap_or(u64::MAX);
        if self.skip_remaining >= buffer_len {
            self.skip_remaining -= buffer_len;
            return Ok(buffer.len());
        }
        let start = usize::try_from(self.skip_remaining).unwrap_or(buffer.len());
        self.skip_remaining = 0;
        let available = buffer.len().saturating_sub(start);
        let take = available.min(usize::try_from(self.take_remaining).unwrap_or(usize::MAX));
        self.data.extend_from_slice(&buffer[start..start + take]);
        self.take_remaining -= u64::try_from(take).unwrap_or(u64::MAX);
        Ok(start + take)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

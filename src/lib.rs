//! Reusable archive access primitives for `arcthis` frontends.

pub mod archive;
pub mod batch;
pub mod cli;
pub mod convert;
pub mod error;
pub mod extract;
pub mod index;
mod lifecycle;
pub mod model;
mod output;
pub mod pack;
pub mod query;
pub mod security;

pub use archive::{Archive, ArchiveLocator, ArchiveOpenOptions, ArchivePassword};
pub use batch::{
    ExtractAllItem, ExtractAllOptions, ExtractAllPlan, ExtractAllResult, extract_all,
    plan_extract_all,
};
pub use convert::{ConvertOptions, ConvertPlan, ConvertResult, convert_archive, plan_convert};
pub use error::{ArcthisError, ErrorCode, Result};
pub use extract::{ExtractOptions, ExtractPlan, ExtractResult};
pub use index::{IndexAction, IndexResult, maintain_index};
pub use lifecycle::{CollisionPolicy, OperationStatus};
pub use model::{
    ArchiveCapabilities, ArchiveEntry, ArchiveFormat, ArchiveInspection, ArchiveWarning,
    EntryCopyResult, EntryKind, EntryPathEncoding, VerificationResult,
};
pub use pack::{PackOptions, PackPlan, PackResult, pack_source, pack_source_with_options};
pub use query::{
    FindResult, GrepMatch, GrepOptions, GrepResult, HashAlgorithm, HashResult, find, grep, hash,
};
pub use security::ExtractionLimits;

/// Version of the public machine-readable JSON contract.
pub const SCHEMA_VERSION: &str = "1";

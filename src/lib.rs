//! Reusable archive access primitives for `arcthis` frontends.

pub mod archive;
pub mod cli;
pub mod error;
pub mod extract;
pub mod model;
mod output;
pub mod pack;
pub mod security;

pub use archive::{Archive, ArchiveLocator};
pub use error::{ArcthisError, ErrorCode, Result};
pub use extract::{ExtractOptions, ExtractResult};
pub use model::{
    ArchiveCapabilities, ArchiveEntry, ArchiveFormat, ArchiveInspection, ArchiveWarning,
    EntryCopyResult, EntryKind, EntryPathEncoding, VerificationResult,
};
pub use pack::PackResult;
pub use security::ExtractionLimits;

/// Version of the public machine-readable JSON contract.
pub const SCHEMA_VERSION: &str = "1";

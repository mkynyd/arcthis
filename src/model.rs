use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "mcp", derive(rmcp::schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ArchiveFormat {
    Zip,
    SevenZip,
    Rar,
    Tar,
    TarGzip,
    TarBzip2,
    TarXz,
    TarZstd,
    Gzip,
    Bzip2,
    Xz,
    Zstd,
}

impl ArchiveFormat {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Zip => "zip",
            Self::SevenZip => "seven_zip",
            Self::Rar => "rar",
            Self::Tar => "tar",
            Self::TarGzip => "tar_gzip",
            Self::TarBzip2 => "tar_bzip2",
            Self::TarXz => "tar_xz",
            Self::TarZstd => "tar_zstd",
            Self::Gzip => "gzip",
            Self::Bzip2 => "bzip2",
            Self::Xz => "xz",
            Self::Zstd => "zstd",
        }
    }
}

impl fmt::Display for ArchiveFormat {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "mcp", derive(rmcp::schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum EntryKind {
    File,
    Directory,
    Symlink,
    Hardlink,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "mcp", derive(rmcp::schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum EntryPathEncoding {
    Utf8,
    EscapedBytes,
}

impl fmt::Display for EntryKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::File => "file",
            Self::Directory => "directory",
            Self::Symlink => "symlink",
            Self::Hardlink => "hardlink",
            Self::Other => "other",
        };
        formatter.write_str(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "mcp", derive(rmcp::schemars::JsonSchema))]
pub struct ArchiveEntry {
    pub archive_index: u64,
    pub path: String,
    pub path_encoding: EntryPathEncoding,
    pub kind: EntryKind,
    pub size: u64,
    pub compressed_size: Option<u64>,
    pub modified_time: Option<String>,
    pub encrypted: bool,
    pub executable: bool,
    pub symlink_target: Option<String>,
    pub crc32: Option<String>,
    pub mime_guess: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "mcp", derive(rmcp::schemars::JsonSchema))]
#[allow(clippy::struct_excessive_bools)] // Stable JSON capability flags are intentionally independent.
pub struct ArchiveCapabilities {
    pub random_access: bool,
    pub streaming_read: bool,
    pub encrypted: bool,
    pub solid: bool,
    pub can_create: bool,
    pub can_extract: bool,
    pub can_verify: bool,
    pub can_seek: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "mcp", derive(rmcp::schemars::JsonSchema))]
pub struct ArchiveWarning {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "mcp", derive(rmcp::schemars::JsonSchema))]
#[allow(clippy::struct_excessive_bools)] // Independent archive facts are stable JSON fields.
pub struct ArchiveInspection {
    pub compression: String,
    pub encrypted: bool,
    pub solid: bool,
    pub random_access: bool,
    pub multipart: bool,
    pub volume_count: u64,
    pub entry_count: u64,
    pub compressed_size: u64,
    pub uncompressed_size: u64,
    pub compression_ratio: Option<f64>,
    pub warnings: Vec<ArchiveWarning>,
    pub capabilities: ArchiveCapabilities,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "mcp", derive(rmcp::schemars::JsonSchema))]
pub struct EntryCopyResult {
    pub bytes_written: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "mcp", derive(rmcp::schemars::JsonSchema))]
pub struct VerificationResult {
    pub verified: bool,
    pub entries_checked: u64,
    pub bytes_checked: u64,
}

impl ArchiveCapabilities {
    pub const fn zip() -> Self {
        Self {
            random_access: true,
            streaming_read: true,
            encrypted: true,
            solid: false,
            can_create: true,
            can_extract: true,
            can_verify: true,
            can_seek: true,
        }
    }

    pub const fn tar() -> Self {
        Self {
            random_access: false,
            streaming_read: true,
            encrypted: false,
            solid: false,
            can_create: true,
            can_extract: true,
            can_verify: true,
            can_seek: false,
        }
    }

    pub const fn seven_zip(solid: bool) -> Self {
        Self {
            random_access: !solid,
            streaming_read: true,
            encrypted: true,
            solid,
            can_create: true,
            can_extract: true,
            can_verify: true,
            can_seek: !solid,
        }
    }

    pub const fn rar() -> Self {
        Self {
            random_access: false,
            streaming_read: true,
            encrypted: true,
            solid: false,
            can_create: false,
            can_extract: true,
            can_verify: true,
            can_seek: false,
        }
    }

    pub const fn single_stream() -> Self {
        Self {
            random_access: false,
            streaming_read: true,
            encrypted: false,
            solid: false,
            can_create: true,
            can_extract: true,
            can_verify: true,
            can_seek: false,
        }
    }
}

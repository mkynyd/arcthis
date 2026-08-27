use std::io;
use std::path::PathBuf;

use serde::Serialize;
use thiserror::Error;

pub type Result<T> = std::result::Result<T, ArcthisError>;

/// Stable public error categories used by machine output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    UnsupportedFormat,
    InvalidArchive,
    CorruptedArchive,
    EntryNotFound,
    PermissionDenied,
    UnsafePath,
    ResourceLimit,
    PasswordRequired,
    WrongPassword,
    Collision,
    UnsupportedOperation,
    VerificationFailed,
    PartialFailure,
    IoError,
}

impl ErrorCode {
    pub const fn exit_code(self) -> u8 {
        match self {
            Self::UnsupportedFormat => 3,
            Self::InvalidArchive | Self::CorruptedArchive => 4,
            Self::EntryNotFound => 5,
            Self::PermissionDenied => 6,
            Self::UnsafePath => 7,
            Self::ResourceLimit => 8,
            Self::Collision => 9,
            Self::UnsupportedOperation | Self::PasswordRequired | Self::WrongPassword => 10,
            Self::VerificationFailed => 11,
            Self::PartialFailure => 12,
            Self::IoError => 1,
        }
    }
}

#[derive(Debug, Error)]
pub enum ArcthisError {
    #[error("unsupported archive format: {path}")]
    UnsupportedFormat { path: PathBuf },

    #[error("invalid archive: {message}")]
    InvalidArchive { message: String },

    #[error("corrupted archive: {message}")]
    CorruptedArchive { message: String },

    #[error("archive entry not found: {entry}")]
    EntryNotFound { entry: String },

    #[error("permission denied while {context}")]
    PermissionDenied {
        context: String,
        #[source]
        source: io::Error,
    },

    #[error("unsafe archive path: {path} ({reason})")]
    UnsafePath { path: String, reason: String },

    #[error("resource limit exceeded: {message}")]
    ResourceLimit { message: String },

    #[error("archive password is required")]
    PasswordRequired,

    #[error("archive password is incorrect")]
    WrongPassword,

    #[error("destination or entry collision: {message}")]
    Collision { message: String },

    #[error("unsupported operation: {message}")]
    UnsupportedOperation { message: String },

    #[error("archive verification failed: {message}")]
    VerificationFailed { message: String },

    #[error("operation completed only partially: {message}")]
    PartialFailure { message: String },

    #[error("I/O error while {context}: {source}")]
    Io {
        context: String,
        #[source]
        source: io::Error,
    },
}

impl ArcthisError {
    pub const fn code(&self) -> ErrorCode {
        match self {
            Self::UnsupportedFormat { .. } => ErrorCode::UnsupportedFormat,
            Self::InvalidArchive { .. } => ErrorCode::InvalidArchive,
            Self::CorruptedArchive { .. } => ErrorCode::CorruptedArchive,
            Self::EntryNotFound { .. } => ErrorCode::EntryNotFound,
            Self::PermissionDenied { .. } => ErrorCode::PermissionDenied,
            Self::UnsafePath { .. } => ErrorCode::UnsafePath,
            Self::ResourceLimit { .. } => ErrorCode::ResourceLimit,
            Self::PasswordRequired => ErrorCode::PasswordRequired,
            Self::WrongPassword => ErrorCode::WrongPassword,
            Self::Collision { .. } => ErrorCode::Collision,
            Self::UnsupportedOperation { .. } => ErrorCode::UnsupportedOperation,
            Self::VerificationFailed { .. } => ErrorCode::VerificationFailed,
            Self::PartialFailure { .. } => ErrorCode::PartialFailure,
            Self::Io { .. } => ErrorCode::IoError,
        }
    }

    pub fn io(context: impl Into<String>, source: io::Error) -> Self {
        if source.kind() == io::ErrorKind::PermissionDenied {
            Self::PermissionDenied {
                context: context.into(),
                source,
            }
        } else {
            Self::Io {
                context: context.into(),
                source,
            }
        }
    }

    pub fn is_broken_pipe(&self) -> bool {
        matches!(
            self,
            Self::Io { source, .. } | Self::PermissionDenied { source, .. }
                if source.kind() == io::ErrorKind::BrokenPipe
        )
    }
}

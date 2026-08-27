use std::io::{self, Write};
use std::path::PathBuf;

use clap::{Parser, Subcommand};
use serde::Serialize;

use crate::error::{ArcthisError, Result};
use crate::{Archive, ExtractOptions, ExtractionLimits, SCHEMA_VERSION, output};

#[derive(Debug, Parser)]
#[command(
    name = "arcthis",
    version,
    about = "An agent-native CLI for accessing and manipulating compressed files",
    long_about = "A unified archive access layer for AI agents and humans. Inspect and stream archive contents before choosing to extract them."
)]
struct Cli {
    /// Emit stable machine-readable JSON where supported.
    #[arg(long, global = true)]
    json: bool,

    /// Disable terminal color decoration.
    #[arg(long, global = true)]
    no_color: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// List archive entries in archive order.
    List { archive: PathBuf },
    /// Display archive entries as a file tree.
    Tree { archive: PathBuf },
    /// Show metadata for one archive entry.
    Stat { archive: PathBuf, entry: String },
    /// Show archive metadata, capabilities, and risk warnings.
    Inspect { archive: PathBuf },
    /// Stream one regular file entry to stdout.
    Read { archive: PathBuf, entry: String },
    /// Safely extract all entries or one selected regular file.
    Extract {
        archive: PathBuf,
        entry: Option<String>,
        /// Destination directory, or destination file for one selected entry.
        #[arg(long)]
        output: Option<PathBuf>,
        /// Maximum number of archive entries allowed.
        #[arg(long, default_value_t = 100_000)]
        max_entries: u64,
        /// Maximum total extracted bytes.
        #[arg(long, default_value_t = 16 * 1024 * 1024 * 1024_u64)]
        max_total_size: u64,
        /// Maximum bytes for one extracted entry.
        #[arg(long, default_value_t = 4 * 1024 * 1024 * 1024_u64)]
        max_entry_size: u64,
    },
    /// Verify archive structure and readable entry data.
    Verify { archive: PathBuf },
    /// Create and verify a ZIP, TAR, or TAR.GZ archive.
    Pack {
        source: PathBuf,
        /// Archive output path; its suffix selects the format.
        #[arg(long)]
        output: PathBuf,
    },
}

pub fn main_entry() -> i32 {
    let cli = Cli::parse();
    match run(&cli) {
        Ok(()) => 0,
        Err(error) if error.is_broken_pipe() => 0,
        Err(error) => {
            let code = i32::from(error.code().exit_code());
            let mut stderr = io::stderr().lock();
            let write_result = if cli.json {
                write_json_error(&mut stderr, &error)
            } else {
                writeln!(stderr, "error: {error}")
                    .map_err(|write_error| ArcthisError::io("writing error output", write_error))
            };
            if write_result.is_err() {
                return 1;
            }
            code
        }
    }
}

#[allow(clippy::too_many_lines)] // Keeping top-level command dispatch in one exhaustive match is clearer.
fn run(cli: &Cli) -> Result<()> {
    let mut stdout = io::stdout().lock();
    match &cli.command {
        Command::List { archive } => {
            let archive = Archive::open(archive.as_path())?;
            let entries = archive.entries()?;
            output::write_list(
                &mut stdout,
                archive.path(),
                archive.format(),
                &entries,
                cli.json,
            )
        }
        Command::Tree { archive } => {
            let archive = Archive::open(archive.as_path())?;
            let entries = archive.entries()?;
            output::write_tree(
                &mut stdout,
                archive.path(),
                archive.format(),
                &entries,
                cli.json,
            )
        }
        Command::Stat { archive, entry } => {
            let archive = Archive::open(archive.as_path())?;
            let entry = archive.entry(entry)?;
            output::write_stat(
                &mut stdout,
                archive.path(),
                archive.format(),
                &entry,
                cli.json,
            )
        }
        Command::Inspect { archive } => {
            let archive = Archive::open(archive.as_path())?;
            let inspection = archive.inspect()?;
            output::write_inspect(
                &mut stdout,
                archive.path(),
                archive.format(),
                &inspection,
                cli.json,
            )
        }
        Command::Read { archive, entry } => {
            if cli.json {
                return Err(ArcthisError::UnsupportedOperation {
                    message: "`read` emits raw bytes and cannot be combined with `--json`"
                        .to_owned(),
                });
            }
            let archive = Archive::open(archive.as_path())?;
            archive.copy_entry_to(entry, &mut stdout)?;
            stdout
                .flush()
                .map_err(|error| ArcthisError::io("flushing entry output", error))
        }
        Command::Extract {
            archive,
            entry,
            output: destination,
            max_entries,
            max_total_size,
            max_entry_size,
        } => {
            let archive = Archive::open(archive.as_path())?;
            let options = ExtractOptions {
                output: destination.clone(),
                limits: ExtractionLimits {
                    max_entries: *max_entries,
                    max_total_size: *max_total_size,
                    max_entry_size: *max_entry_size,
                    ..ExtractionLimits::default()
                },
            };
            let result = archive.extract(entry.as_deref(), &options)?;
            output::write_extract(
                &mut stdout,
                archive.path(),
                archive.format(),
                &result,
                cli.json,
            )
        }
        Command::Verify { archive } => {
            let archive = Archive::open(archive.as_path())?;
            let result = archive.verify()?;
            output::write_verify(
                &mut stdout,
                archive.path(),
                archive.format(),
                &result,
                cli.json,
            )
        }
        Command::Pack {
            source,
            output: destination,
        } => {
            let result = crate::pack::pack_source(source, destination)?;
            output::write_pack(&mut stdout, &result, cli.json)
        }
    }
}

#[derive(Serialize)]
struct ErrorEnvelope<'a> {
    schema_version: &'static str,
    error: ErrorObject<'a>,
}

#[derive(Serialize)]
struct ErrorObject<'a> {
    code: crate::ErrorCode,
    message: String,
    details: ErrorDetails<'a>,
}

#[derive(Serialize)]
#[serde(untagged)]
enum ErrorDetails<'a> {
    Entry { entry: &'a str },
    Path { path: String },
    Message { message: &'a str },
    Empty {},
}

fn write_json_error(writer: &mut impl Write, error: &ArcthisError) -> Result<()> {
    let details = match error {
        ArcthisError::EntryNotFound { entry } => ErrorDetails::Entry { entry },
        ArcthisError::UnsupportedFormat { path } => ErrorDetails::Path {
            path: path.to_string_lossy().into_owned(),
        },
        ArcthisError::InvalidArchive { message }
        | ArcthisError::CorruptedArchive { message }
        | ArcthisError::ResourceLimit { message }
        | ArcthisError::Collision { message }
        | ArcthisError::UnsupportedOperation { message }
        | ArcthisError::VerificationFailed { message }
        | ArcthisError::PartialFailure { message } => ErrorDetails::Message { message },
        ArcthisError::UnsafePath { path, .. } => ErrorDetails::Path { path: path.clone() },
        ArcthisError::PermissionDenied { .. }
        | ArcthisError::PasswordRequired
        | ArcthisError::WrongPassword
        | ArcthisError::Io { .. } => ErrorDetails::Empty {},
    };
    let envelope = ErrorEnvelope {
        schema_version: SCHEMA_VERSION,
        error: ErrorObject {
            code: error.code(),
            message: error.to_string(),
            details,
        },
    };
    serde_json::to_writer(&mut *writer, &envelope).map_err(|serialize_error| {
        ArcthisError::io("serializing JSON error", io::Error::other(serialize_error))
    })?;
    writeln!(writer).map_err(|write_error| ArcthisError::io("writing JSON error", write_error))
}

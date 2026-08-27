use std::io::{self, Write};
use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand, ValueEnum};
use serde::Serialize;

use crate::error::{ArcthisError, Result};
use crate::{Archive, CollisionPolicy, ExtractOptions, ExtractionLimits, SCHEMA_VERSION, output};

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

    /// Traverse into this archive entry; repeat to descend through multiple archives.
    #[arg(long, global = true)]
    within: Vec<String>,

    /// Maximum decoded bytes buffered for each nested archive entry.
    #[arg(long, global = true, default_value_t = 256 * 1024 * 1024_u64)]
    max_nested_entry_size: u64,

    /// Read an archive password from this file; trailing CR/LF is removed.
    #[arg(long, global = true)]
    password_file: Option<PathBuf>,

    /// Append a byte-stream archive volume; repeat in exact volume order.
    #[arg(long, global = true)]
    volume: Vec<PathBuf>,

    /// Override the platform cache root used by persistent indexes.
    #[arg(long, global = true)]
    index_directory: Option<PathBuf>,

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
    /// Find archive entries whose paths match a glob.
    Find {
        archive: PathBuf,
        /// Glob matched against complete archive entry paths.
        #[arg(long)]
        glob: String,
    },
    /// Search regular-file contents without extracting them.
    Grep {
        archive: PathBuf,
        pattern: String,
        /// Restrict scanning to matching archive entry paths.
        #[arg(long)]
        glob: Option<String>,
        /// Skip regular files larger than this many bytes.
        #[arg(long, default_value_t = 16 * 1024 * 1024_u64)]
        max_entry_size: u64,
        /// Stop collecting results after this many matching lines.
        #[arg(long, default_value_t = 10_000)]
        max_matches: u64,
        /// Scan files containing NUL bytes instead of treating them as binary.
        #[arg(long)]
        binary: bool,
    },
    /// Stream one entry through a cryptographic hash.
    Hash {
        archive: PathBuf,
        entry: String,
        /// Digest algorithm.
        #[arg(long, value_enum, default_value_t = CliHashAlgorithm::Sha256)]
        algorithm: CliHashAlgorithm,
    },
    /// Create, refresh, inspect, or delete a persistent entry metadata index.
    Index {
        archive: PathBuf,
        /// Re-enumerate the archive and replace an existing index.
        #[arg(long, conflicts_with = "delete")]
        refresh: bool,
        /// Delete the index for this archive.
        #[arg(long)]
        delete: bool,
        /// Report the index action without changing cache files.
        #[arg(long)]
        dry_run: bool,
    },
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
        /// Reject entries whose declared expansion exceeds this ratio.
        #[arg(long)]
        max_compression_ratio: Option<u64>,
        /// Abort an entry if streaming it exceeds this many seconds.
        #[arg(long)]
        max_entry_duration_seconds: Option<u64>,
        /// Print the execution plan without writing or deleting anything.
        #[arg(long)]
        dry_run: bool,
        /// Delete the archive only after extraction commits successfully.
        #[arg(long)]
        delete_source: bool,
        /// Transactionally replace an existing destination.
        #[arg(long, conflicts_with_all = ["skip_existing", "rename"])]
        overwrite: bool,
        /// Leave an existing destination unchanged and report a skipped operation.
        #[arg(long, conflicts_with = "rename")]
        skip_existing: bool,
        /// Select the first available numbered destination on collision.
        #[arg(long)]
        rename: bool,
    },
    /// Safely extract every supported archive discovered in a directory.
    ExtractAll {
        directory: PathBuf,
        /// Discover archives below nested directories.
        #[arg(long)]
        recursive: bool,
        /// Maximum number of archives processed concurrently (1-64).
        #[arg(long)]
        workers: Option<usize>,
        /// Print the execution plan without writing or deleting anything.
        #[arg(long)]
        dry_run: bool,
        /// Delete each archive only after its extraction commits successfully.
        #[arg(long)]
        delete_source: bool,
        /// Transactionally replace existing destinations.
        #[arg(long, conflicts_with_all = ["skip_existing", "rename"])]
        overwrite: bool,
        /// Leave existing destinations unchanged and report skipped operations.
        #[arg(long, conflicts_with = "rename")]
        skip_existing: bool,
        /// Select the first available numbered destination for each collision.
        #[arg(long)]
        rename: bool,
        /// Maximum number of entries allowed per archive.
        #[arg(long, default_value_t = 100_000)]
        max_entries: u64,
        /// Maximum extracted bytes per archive.
        #[arg(long, default_value_t = 16 * 1024 * 1024 * 1024_u64)]
        max_total_size: u64,
        /// Maximum bytes for one extracted entry.
        #[arg(long, default_value_t = 4 * 1024 * 1024 * 1024_u64)]
        max_entry_size: u64,
        /// Reject entries whose declared expansion exceeds this ratio.
        #[arg(long)]
        max_compression_ratio: Option<u64>,
        /// Abort an entry if streaming it exceeds this many seconds.
        #[arg(long)]
        max_entry_duration_seconds: Option<u64>,
    },
    /// Verify archive structure and readable entry data.
    Verify { archive: PathBuf },
    /// Create and verify a supported archive or single-stream file.
    Pack {
        source: PathBuf,
        /// Archive output path; its suffix selects the format.
        #[arg(long)]
        output: PathBuf,
        /// Print the execution plan without writing or deleting anything.
        #[arg(long)]
        dry_run: bool,
        /// Delete the source only after the archive commits and verifies successfully.
        #[arg(long)]
        delete_source: bool,
        /// Transactionally replace an existing archive output.
        #[arg(long, conflicts_with_all = ["skip_existing", "rename"])]
        overwrite: bool,
        /// Leave an existing output unchanged and report a skipped operation.
        #[arg(long, conflicts_with = "rename")]
        skip_existing: bool,
        /// Select the first available numbered output path on collision.
        #[arg(long)]
        rename: bool,
    },
    /// Convert an archive through safe staged extraction and verified packing.
    Convert {
        archive: PathBuf,
        /// Target archive path; its suffix selects the format.
        #[arg(long)]
        output: PathBuf,
        /// Print the execution plan without writing or deleting anything.
        #[arg(long)]
        dry_run: bool,
        /// Delete the source only after the target commits and verifies successfully.
        #[arg(long)]
        delete_source: bool,
        /// Transactionally replace an existing target archive.
        #[arg(long, conflicts_with_all = ["skip_existing", "rename"])]
        overwrite: bool,
        /// Leave an existing target unchanged after verifying it.
        #[arg(long, conflicts_with = "rename")]
        skip_existing: bool,
        /// Select the first available numbered target path on collision.
        #[arg(long)]
        rename: bool,
        /// Maximum number of source archive entries allowed.
        #[arg(long, default_value_t = 100_000)]
        max_entries: u64,
        /// Maximum total bytes materialized in conversion staging.
        #[arg(long, default_value_t = 16 * 1024 * 1024 * 1024_u64)]
        max_total_size: u64,
        /// Maximum bytes for one source entry.
        #[arg(long, default_value_t = 4 * 1024 * 1024 * 1024_u64)]
        max_entry_size: u64,
        /// Reject entries whose declared expansion exceeds this ratio.
        #[arg(long)]
        max_compression_ratio: Option<u64>,
        /// Abort an entry if staging it exceeds this many seconds.
        #[arg(long)]
        max_entry_duration_seconds: Option<u64>,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum CliHashAlgorithm {
    Sha256,
    Sha512,
}

impl From<CliHashAlgorithm> for crate::query::HashAlgorithm {
    fn from(value: CliHashAlgorithm) -> Self {
        match value {
            CliHashAlgorithm::Sha256 => Self::Sha256,
            CliHashAlgorithm::Sha512 => Self::Sha512,
        }
    }
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
            let archive = open_cli_archive(cli, archive)?;
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
            let archive = open_cli_archive(cli, archive)?;
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
            let archive = open_cli_archive(cli, archive)?;
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
            let archive = open_cli_archive(cli, archive)?;
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
            let archive = open_cli_archive(cli, archive)?;
            archive.copy_entry_to(entry, &mut stdout)?;
            stdout
                .flush()
                .map_err(|error| ArcthisError::io("flushing entry output", error))
        }
        Command::Find { archive, glob } => {
            let archive = open_cli_archive(cli, archive)?;
            let result = crate::query::find(&archive, glob)?;
            output::write_find(
                &mut stdout,
                archive.path(),
                archive.format(),
                &result,
                cli.json,
            )
        }
        Command::Grep {
            archive,
            pattern,
            glob,
            max_entry_size,
            max_matches,
            binary,
        } => {
            let archive = open_cli_archive(cli, archive)?;
            let result = crate::query::grep(
                &archive,
                pattern,
                &crate::query::GrepOptions {
                    glob: glob.clone(),
                    max_entry_size: *max_entry_size,
                    max_matches: *max_matches,
                    scan_binary: *binary,
                },
            )?;
            output::write_grep(
                &mut stdout,
                archive.path(),
                archive.format(),
                &result,
                cli.json,
            )
        }
        Command::Hash {
            archive,
            entry,
            algorithm,
        } => {
            let archive = open_cli_archive(cli, archive)?;
            let result = crate::query::hash(&archive, entry, (*algorithm).into())?;
            output::write_hash(
                &mut stdout,
                archive.path(),
                archive.format(),
                &result,
                cli.json,
            )
        }
        Command::Index {
            archive,
            refresh,
            delete,
            dry_run,
        } => {
            if !cli.within.is_empty() || !cli.volume.is_empty() {
                return Err(ArcthisError::UnsupportedOperation {
                    message: "persistent indexes currently require one filesystem archive source"
                        .to_owned(),
                });
            }
            let result = crate::index::maintain_index(
                archive,
                &archive_open_options(cli)?,
                *refresh,
                *delete,
                *dry_run,
            )?;
            output::write_index(&mut stdout, &result, cli.json)
        }
        Command::Extract {
            archive,
            entry,
            output: destination,
            max_entries,
            max_total_size,
            max_entry_size,
            max_compression_ratio,
            max_entry_duration_seconds,
            dry_run,
            delete_source,
            overwrite,
            skip_existing,
            rename,
        } => {
            if !cli.within.is_empty() {
                return Err(ArcthisError::UnsupportedOperation {
                    message: "nested extraction is not supported; use nested `read` or `verify`"
                        .to_owned(),
                });
            }
            if *delete_source && !cli.volume.is_empty() {
                return Err(ArcthisError::UnsupportedOperation {
                    message: "--delete-source is not supported for multipart extraction".to_owned(),
                });
            }
            let archive = open_cli_archive(cli, archive)?;
            let options = ExtractOptions {
                output: destination.clone(),
                base_directory: None,
                limits: ExtractionLimits {
                    max_entries: *max_entries,
                    max_total_size: *max_total_size,
                    max_entry_size: *max_entry_size,
                    max_compression_ratio: *max_compression_ratio,
                    max_entry_duration: max_entry_duration_seconds
                        .map(std::time::Duration::from_secs),
                    ..ExtractionLimits::default()
                },
                collision_policy: collision_policy(*overwrite, *skip_existing, *rename),
                delete_source: *delete_source,
            };
            if *dry_run {
                let plan = archive.plan_extract(entry.as_deref(), &options)?;
                return output::write_extract_plan(&mut stdout, &plan, cli.json);
            }
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
            let archive = open_cli_archive(cli, archive)?;
            let result = archive.verify()?;
            output::write_verify(
                &mut stdout,
                archive.path(),
                archive.format(),
                &result,
                cli.json,
            )
        }
        Command::ExtractAll {
            directory,
            recursive,
            workers,
            dry_run,
            delete_source,
            overwrite,
            skip_existing,
            rename,
            max_entries,
            max_total_size,
            max_entry_size,
            max_compression_ratio,
            max_entry_duration_seconds,
        } => {
            if !cli.within.is_empty() || !cli.volume.is_empty() {
                return Err(ArcthisError::UnsupportedOperation {
                    message: "extract-all does not accept nested or multipart source options"
                        .to_owned(),
                });
            }
            let worker_count = workers.unwrap_or_else(|| {
                std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get)
            });
            let options = crate::batch::ExtractAllOptions {
                recursive: *recursive,
                workers: worker_count,
                extract: ExtractOptions {
                    output: None,
                    base_directory: None,
                    limits: ExtractionLimits {
                        max_entries: *max_entries,
                        max_total_size: *max_total_size,
                        max_entry_size: *max_entry_size,
                        max_compression_ratio: *max_compression_ratio,
                        max_entry_duration: max_entry_duration_seconds
                            .map(std::time::Duration::from_secs),
                        ..ExtractionLimits::default()
                    },
                    collision_policy: collision_policy(*overwrite, *skip_existing, *rename),
                    delete_source: *delete_source,
                },
                open: archive_open_options(cli)?,
            };
            if *dry_run {
                let plan = crate::batch::plan_extract_all(directory, &options)?;
                return output::write_extract_all_plan(&mut stdout, &plan, cli.json);
            }
            let result = crate::batch::extract_all(directory, &options)?;
            output::write_extract_all(&mut stdout, &result, cli.json)?;
            if result.failed > 0 {
                return Err(ArcthisError::PartialFailure {
                    message: format!("{} archive extraction(s) failed", result.failed),
                });
            }
            Ok(())
        }
        Command::Pack {
            source,
            output: destination,
            dry_run,
            delete_source,
            overwrite,
            skip_existing,
            rename,
        } => {
            if !cli.within.is_empty() || !cli.volume.is_empty() || cli.password_file.is_some() {
                return Err(ArcthisError::UnsupportedOperation {
                    message: "pack does not accept --within, --volume, or --password-file; encrypted creation is not implemented"
                        .to_owned(),
                });
            }
            let options = crate::pack::PackOptions {
                collision_policy: collision_policy(*overwrite, *skip_existing, *rename),
                delete_source: *delete_source,
                include_source_root: true,
            };
            if *dry_run {
                let plan = crate::pack::plan_pack_source(source, destination, &options)?;
                return output::write_pack_plan(&mut stdout, &plan, cli.json);
            }
            let result = crate::pack::pack_source_with_options(source, destination, &options)?;
            output::write_pack(&mut stdout, &result, cli.json)
        }
        Command::Convert {
            archive,
            output: destination,
            dry_run,
            delete_source,
            overwrite,
            skip_existing,
            rename,
            max_entries,
            max_total_size,
            max_entry_size,
            max_compression_ratio,
            max_entry_duration_seconds,
        } => {
            if !cli.within.is_empty() {
                return Err(ArcthisError::UnsupportedOperation {
                    message: "nested archive conversion is not supported".to_owned(),
                });
            }
            let options = crate::convert::ConvertOptions {
                open: archive_open_options(cli)?,
                limits: ExtractionLimits {
                    max_entries: *max_entries,
                    max_total_size: *max_total_size,
                    max_entry_size: *max_entry_size,
                    max_compression_ratio: *max_compression_ratio,
                    max_entry_duration: max_entry_duration_seconds
                        .map(std::time::Duration::from_secs),
                    ..ExtractionLimits::default()
                },
                collision_policy: collision_policy(*overwrite, *skip_existing, *rename),
                delete_source: *delete_source,
            };
            if *dry_run {
                let plan = crate::convert::plan_convert(archive, destination, &options)?;
                return output::write_convert_plan(&mut stdout, &plan, cli.json);
            }
            let result = crate::convert::convert_archive(archive, destination, &options)?;
            output::write_convert(&mut stdout, &result, cli.json)
        }
    }
}

fn open_cli_archive(cli: &Cli, path: &Path) -> Result<Archive> {
    Archive::open_within_options(
        path,
        &cli.within,
        cli.max_nested_entry_size,
        &archive_open_options(cli)?,
    )
}

fn archive_open_options(cli: &Cli) -> Result<crate::ArchiveOpenOptions> {
    let password = cli
        .password_file
        .as_ref()
        .map(|path| {
            let mut bytes = std::fs::read(path)
                .map_err(|error| ArcthisError::io("reading password file", error))?;
            while matches!(bytes.last(), Some(b'\n' | b'\r')) {
                bytes.pop();
            }
            Ok(crate::ArchivePassword::new(bytes))
        })
        .transpose()?;
    Ok(crate::ArchiveOpenOptions {
        password,
        volumes: cli.volume.clone(),
        index_directory: cli.index_directory.clone(),
    })
}

const fn collision_policy(overwrite: bool, skip_existing: bool, rename: bool) -> CollisionPolicy {
    if overwrite {
        CollisionPolicy::Overwrite
    } else if skip_existing {
        CollisionPolicy::SkipExisting
    } else if rename {
        CollisionPolicy::Rename
    } else {
        CollisionPolicy::Refuse
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

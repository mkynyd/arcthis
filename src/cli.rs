use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand, ValueEnum};

use crate::error::{ArcthisError, Result};
use crate::{
    ApplicationService, Archive, ArchiveSourceRequest, CancellationToken, CollisionPolicy,
    ExtractOptions, ExtractionLimits, ServiceLimits, output,
};

const CLI_LOGO: &str = r"  .----------.
 /   ======   \                         _    _      _
|             '       __ _  _ __   ___ | |_ | |__  (_) ___
|   =  =========     / _` || '__| / __|| __|| '_ \ | |/ __|
|             .     | (_| || |   | (__ | |_ | | | || |\__ \
 \   ======   /      \__,_||_|    \___| \__||_| |_||_||___/
  '----------'";

const EXIT_CODES: &str = "Exit codes (runtime errors use stable JSON error.code with --json):
  0 success / BrokenPipe
  1 io_error
  2 argument syntax / invalid_argument
  3 unsupported_format / not_an_archive
  4 invalid_archive / corrupted_archive
  5 entry_not_found
  6 permission_denied
  7 unsafe_path
  8 resource_limit
  9 collision
 10 unsupported_operation / password_required / wrong_password
 11 verification_failed
 12 partial_failure";

#[derive(Debug, Parser)]
#[command(
    name = "arcthis",
    version,
    before_help = CLI_LOGO,
    about = "An agent-native CLI for accessing and manipulating compressed files",
    long_about = "A unified archive access layer for AI agents and humans. Inspect and stream archive contents before choosing to extract them.",
    after_long_help = EXIT_CODES
)]
struct Cli {
    /// Emit stable machine-readable JSON where supported.
    #[arg(long, global = true)]
    json: bool,

    /// Disable terminal color decoration.
    #[arg(long, global = true)]
    no_color: bool,

    /// Skip per-file fsync during extraction (staging and rollback still apply).
    #[arg(long, global = true)]
    no_sync: bool,

    /// Select an exact archive instance by `archive_index` (stat/read/hash/extract).
    #[arg(long, global = true)]
    entry_index: Option<u64>,

    /// Decode original entry-name bytes explicitly (utf8, cp437, gbk, shift-jis).
    #[arg(long, global = true, value_parser = ["utf8", "cp437", "gbk", "shift-jis"])]
    encoding: Option<String>,

    /// Virtual filename for a stdin source, including a suffix for empty compressed TAR.
    #[arg(long, global = true, default_value = "stdin")]
    stdin_name: String,
    /// Maximum bytes buffered from stdin; seekable archive formats require bounded storage.
    #[arg(long, global = true, default_value_t = 64 * 1024 * 1024_u64)]
    max_stdin_bytes: u64,
    #[arg(skip)]
    stdin_bytes: Option<std::sync::Arc<[u8]>>,

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
    /// Serve the bounded local Model Context Protocol frontend over stdio.
    #[cfg(feature = "mcp")]
    Mcp {
        /// Canonical filesystem root allowed for archive reads; repeat as needed.
        #[arg(long = "allow-root", required = true)]
        allow_roots: Vec<PathBuf>,
        /// Canonical filesystem root allowed for mutation outputs; repeat as needed.
        #[arg(long = "allow-output-root")]
        allow_output_roots: Vec<PathBuf>,
        /// Permit explicitly requested source deletion after verified commit.
        #[arg(long)]
        allow_source_deletion: bool,
        /// Maximum archive entries per request.
        #[arg(long, default_value_t = 100_000)]
        max_entries: u64,
        /// Maximum decoded bytes per request.
        #[arg(long, default_value_t = 1024 * 1024 * 1024_u64)]
        max_decoded_bytes: u64,
        /// Maximum find/grep results per request.
        #[arg(long, default_value_t = 10_000)]
        max_results: u64,
        /// Maximum bytes returned by one `archive_read` window.
        #[arg(long, default_value_t = 1024 * 1024_u64)]
        max_read_window: u64,
        /// Maximum simultaneously running tools; excess calls return `resource_limit`.
        #[arg(long, default_value_t = 4)]
        max_concurrent_requests: usize,
        /// Maximum serialized tool-result bytes (including text/structured copies).
        #[arg(long, default_value_t = 16 * 1024 * 1024_u64)]
        max_response_bytes: u64,
    },
    /// List archive entries in archive order.
    List { archive: PathBuf },
    /// Display archive entries as a file tree.
    Tree { archive: PathBuf },
    /// Show metadata for one archive entry.
    Stat {
        archive: PathBuf,
        entry: Option<String>,
        #[arg(long)]
        verbose: bool,
    },
    /// Show archive metadata, capabilities, and risk warnings.
    Inspect { archive: PathBuf },
    /// Stream one regular file entry to stdout.
    Read {
        archive: PathBuf,
        entry: Option<String>,
        /// Validate the complete entry before writing stdout; uses bounded temporary storage.
        #[arg(long)]
        verify_first: bool,
        /// Maximum bytes staged by --verify-first.
        #[arg(long, default_value_t = 4 * 1024 * 1024 * 1024_u64)]
        max_entry_size: u64,
    },
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
        /// Maximum serialized matching-line bytes retained across files.
        #[arg(long, default_value_t = 16 * 1024 * 1024_u64)]
        max_result_bytes: u64,
        /// Scan files containing NUL bytes instead of treating them as binary.
        #[arg(long)]
        binary: bool,
    },
    /// Stream one entry through a cryptographic hash.
    Hash {
        archive: PathBuf,
        entry: Option<String>,
        /// Digest algorithm.
        #[arg(long, value_enum, default_value_t = CliHashAlgorithm::Sha256)]
        algorithm: CliHashAlgorithm,
    },
    /// Create, refresh, inspect, or delete a persistent entry metadata index.
    Index {
        archive: Option<PathBuf>,
        /// List saved index documents and cache validity.
        #[arg(long, conflicts_with_all = ["archive", "refresh", "delete", "prune"])]
        list: bool,
        /// Remove only stale, invalid, missing-source, or expired index documents.
        #[arg(long, conflicts_with_all = ["archive", "refresh", "delete"])]
        prune: bool,
        /// Cache age threshold for --list/--prune (30 days by default).
        #[arg(long, default_value_t = 30 * 24 * 60 * 60_u64)]
        max_age_seconds: u64,
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
        /// Report per-archive planning failures and continue independent archives.
        #[arg(long)]
        continue_on_error: bool,
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
        /// Pack directory contents without the source directory name.
        #[arg(long)]
        strip_root: bool,
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
    let mut cli = Cli::parse();
    let result = prepare_stdin(&mut cli).and_then(|()| run(&cli));
    match result {
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
    if cli.entry_index.is_some()
        && !matches!(
            cli.command,
            Command::Stat { .. }
                | Command::Read { .. }
                | Command::Hash { .. }
                | Command::Extract { .. }
        )
    {
        return Err(ArcthisError::InvalidArgument {
            message: "--entry-index applies to stat/read/hash/extract".to_owned(),
        });
    }
    if cli.no_sync
        && !matches!(
            cli.command,
            Command::Extract { .. } | Command::ExtractAll { .. } | Command::Convert { .. }
        )
    {
        return Err(ArcthisError::InvalidArgument {
            message: "--no-sync applies to extraction and conversion".to_owned(),
        });
    }
    if cli.encoding.is_some() && matches!(cli.command, Command::Index { .. }) {
        return Err(ArcthisError::InvalidArgument {
            message: "index stores original names; --encoding does not apply to index management"
                .to_owned(),
        });
    }
    let mut stdout = io::stdout().lock();
    let service = ApplicationService::new(
        ServiceLimits::cli_compatibility(),
        CancellationToken::default(),
    );
    match &cli.command {
        #[cfg(feature = "mcp")]
        Command::Mcp {
            allow_roots,
            allow_output_roots,
            allow_source_deletion,
            max_entries,
            max_decoded_bytes,
            max_results,
            max_read_window,
            max_concurrent_requests,
            max_response_bytes,
        } => {
            if cli.json {
                return Err(ArcthisError::InvalidArgument {
                    message: "mcp stdout is JSON-RPC; --json does not apply".to_owned(),
                });
            }
            if !cli.within.is_empty()
                || !cli.volume.is_empty()
                || cli.password_file.is_some()
                || cli.index_directory.is_some()
            {
                return Err(ArcthisError::UnsupportedOperation {
                    message: "mcp does not accept global archive source options".to_owned(),
                });
            }
            drop(stdout);
            crate::mcp::run_stdio(crate::mcp::McpConfig {
                allowed_input_roots: allow_roots.clone(),
                allowed_output_roots: allow_output_roots.clone(),
                allow_source_deletion: *allow_source_deletion,
                max_concurrent_requests: *max_concurrent_requests,
                max_response_bytes: *max_response_bytes,
                limits: ServiceLimits {
                    max_entries: *max_entries,
                    max_decoded_bytes: *max_decoded_bytes,
                    max_results: *max_results,
                    max_read_window: *max_read_window,
                    ..ServiceLimits::default()
                },
            })
        }
        Command::List { archive } => {
            let result = service.list(&archive_source_request(cli, archive)?)?;
            if cli.json {
                return output::write_service(&mut stdout, &result);
            }
            output::write_list(
                &mut stdout,
                &result.archive.path,
                result.archive.format,
                &result.entries,
                cli.json,
            )
        }
        Command::Tree { archive } => {
            let result = service.tree(&archive_source_request(cli, archive)?)?;
            if cli.json {
                return output::write_service(&mut stdout, &result);
            }
            output::write_tree(
                &mut stdout,
                &result.archive.path,
                result.archive.format,
                &result.tree,
                cli.json,
            )
        }
        Command::Stat {
            archive,
            entry,
            verbose,
        } => {
            let entry = selected_entry(cli, archive, entry.as_deref())?;
            let result = service.stat(&archive_source_request(cli, archive)?, &entry)?;
            if cli.json {
                return output::write_service(&mut stdout, &result);
            }
            output::write_stat(
                &mut stdout,
                &result.archive.path,
                result.archive.format,
                &result.entry,
                cli.json,
                *verbose,
            )
        }
        Command::Inspect { archive } => {
            let result = service.inspect(&archive_source_request(cli, archive)?)?;
            if cli.json {
                return output::write_service(&mut stdout, &result);
            }
            output::write_inspect(
                &mut stdout,
                &result.archive.path,
                result.archive.format,
                &result.inspection,
                cli.json,
            )
        }
        Command::Read {
            archive,
            entry,
            verify_first,
            max_entry_size,
        } => {
            if cli.json {
                return Err(ArcthisError::InvalidArgument {
                    message: "`read` emits raw bytes and cannot be combined with `--json`"
                        .to_owned(),
                });
            }
            let entry = selected_entry(cli, archive, entry.as_deref())?;
            let source = archive_source_request(cli, archive)?;
            if *verify_first {
                service.read_to_verified(&source, &entry, &mut stdout, *max_entry_size)?;
            } else {
                service.read_to(&source, &entry, &mut stdout)?;
            }
            stdout
                .flush()
                .map_err(|error| ArcthisError::io("flushing entry output", error))
        }
        Command::Find { archive, glob } => {
            let result = service.find(&archive_source_request(cli, archive)?, glob)?;
            if cli.json {
                return output::write_service(&mut stdout, &result);
            }
            output::write_find(
                &mut stdout,
                &result.archive.path,
                result.archive.format,
                &result.find,
                cli.json,
            )
        }
        Command::Grep {
            archive,
            pattern,
            glob,
            max_entry_size,
            max_matches,
            max_result_bytes,
            binary,
        } => {
            let result = service.grep(
                &archive_source_request(cli, archive)?,
                pattern,
                &crate::query::GrepOptions {
                    glob: glob.clone(),
                    max_entry_size: *max_entry_size,
                    max_matches: *max_matches,
                    scan_binary: *binary,
                    max_result_bytes: *max_result_bytes,
                    ..crate::query::GrepOptions::default()
                },
            )?;
            if cli.json {
                return output::write_service(&mut stdout, &result);
            }
            output::write_grep(
                &mut stdout,
                &result.archive.path,
                result.archive.format,
                &result.grep,
                cli.json,
            )
        }
        Command::Hash {
            archive,
            entry,
            algorithm,
        } => {
            let entry = selected_entry(cli, archive, entry.as_deref())?;
            let result = service.hash(
                &archive_source_request(cli, archive)?,
                &entry,
                (*algorithm).into(),
            )?;
            if cli.json {
                return output::write_service(&mut stdout, &result);
            }
            output::write_hash(
                &mut stdout,
                &result.archive.path,
                result.archive.format,
                &result.hash,
                cli.json,
            )
        }
        Command::Index {
            archive,
            list,
            prune,
            max_age_seconds,
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
            if *list || *prune {
                let result = crate::index::manage_cache(
                    cli.index_directory.as_deref(),
                    *prune,
                    *dry_run,
                    *max_age_seconds,
                )?;
                if cli.json {
                    return output::write_service(&mut stdout, &result);
                }
                for item in result.items {
                    writeln!(
                        stdout,
                        "{} {} ({} entries, {} bytes)",
                        if item.deleted {
                            "deleted"
                        } else if item.stale {
                            "stale"
                        } else {
                            "valid"
                        },
                        item.index_path.display(),
                        item.entries_indexed,
                        item.bytes
                    )
                    .map_err(|error| ArcthisError::io("writing cache inventory", error))?;
                }
                return Ok(());
            }
            let archive = archive
                .as_deref()
                .ok_or_else(|| ArcthisError::InvalidArgument {
                    message: "index requires an archive, --list or --prune".to_owned(),
                })?;
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
            let selected = if cli.entry_index.is_some() {
                Some(selected_entry(cli, archive, entry.as_deref())?)
            } else {
                entry.clone()
            };
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
                    sync_files: !cli.no_sync,
                    ..ExtractionLimits::default()
                },
                collision_policy: collision_policy(*overwrite, *skip_existing, *rename),
                delete_source: *delete_source,
            };
            if *dry_run {
                let plan = archive.plan_extract(selected.as_deref(), &options)?;
                return output::write_extract_plan(&mut stdout, &plan, cli.json);
            }
            let source_path = archive.reference_path();
            let result = archive.extract(selected.as_deref(), &options)?;
            output::write_extract(
                &mut stdout,
                &source_path,
                archive.format(),
                &result,
                cli.json,
            )
        }
        Command::Verify { archive } => {
            let result = service.verify(&archive_source_request(cli, archive)?)?;
            if cli.json {
                return output::write_service(&mut stdout, &result);
            }
            output::write_verify(
                &mut stdout,
                &result.archive.path,
                result.archive.format,
                &result.verification,
                cli.json,
            )
        }
        Command::ExtractAll {
            directory,
            continue_on_error,
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
                continue_on_error: *continue_on_error,
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
                        sync_files: !cli.no_sync,
                        ..ExtractionLimits::default()
                    },
                    collision_policy: collision_policy(*overwrite, *skip_existing, *rename),
                    delete_source: *delete_source,
                },
                open: archive_open_options(cli)?,
            };
            if *dry_run {
                let plan = crate::batch::plan_extract_all(directory, &options)?;
                output::write_extract_all_plan(&mut stdout, &plan, cli.json)?;
                if !plan.errors.is_empty() {
                    return Err(ArcthisError::PartialFailure {
                        message: format!("{} archive plan(s) failed", plan.errors.len()),
                    });
                }
                return Ok(());
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
            strip_root,
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
                include_source_root: !strip_root,
                ..crate::pack::PackOptions::default()
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
                    sync_files: !cli.no_sync,
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

fn archive_source_request(cli: &Cli, path: &Path) -> Result<ArchiveSourceRequest> {
    Ok(ArchiveSourceRequest {
        path: path.to_path_buf(),
        within: cli.within.clone(),
        max_nested_entry_size: cli.max_nested_entry_size,
        open_options: archive_open_options(cli)?,
    })
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
        entry_index: cli.entry_index,
        name_encoding: cli.encoding.clone(),
        memory_source: cli
            .stdin_bytes
            .as_ref()
            .map(|bytes| (std::sync::Arc::clone(bytes), PathBuf::from(&cli.stdin_name))),
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

fn write_json_error(writer: &mut impl Write, error: &ArcthisError) -> Result<()> {
    serde_json::to_writer(&mut *writer, &crate::error::envelope(error))
        .map_err(|error| ArcthisError::io("serializing JSON error", io::Error::other(error)))?;
    writeln!(writer).map_err(|error| ArcthisError::io("writing JSON error", error))
}

fn selected_entry(cli: &Cli, path: &Path, entry: Option<&str>) -> Result<String> {
    if let Some(entry) = entry {
        return Ok(entry.to_owned());
    }
    let archive = open_cli_archive(cli, path)?;
    let entries = archive.entries()?;
    let mut files = entries.iter().filter(|entry| {
        entry.kind == crate::EntryKind::File
            && cli
                .entry_index
                .is_none_or(|index| entry.archive_index == index)
    });
    let file = files.next().ok_or_else(|| ArcthisError::EntryNotFound {
        entry: cli.entry_index.map_or_else(
            || "regular file".to_owned(),
            |index| format!("archive_index {index}"),
        ),
    })?;
    if files.next().is_some() {
        return Err(ArcthisError::InvalidArgument {
            message: "entry is required when more than one regular file is present".to_owned(),
        });
    }
    Ok(file.path.clone())
}

fn prepare_stdin(cli: &mut Cli) -> Result<()> {
    if cli.json && matches!(cli.command, Command::Read { .. }) {
        return Err(ArcthisError::InvalidArgument {
            message: "read emits raw bytes; --json cannot be combined with read".to_owned(),
        });
    }
    let Some(source) = cli.command.read_archive_path() else {
        return Ok(());
    };
    if source != Path::new("-") {
        return Ok(());
    }
    if !cli.volume.is_empty() {
        return Err(ArcthisError::InvalidArgument {
            message: "stdin cannot be combined with --volume".to_owned(),
        });
    }
    let mut input = io::stdin().lock();
    let mut data = Vec::new();
    let mut buffer = vec![0_u8; 65536];
    loop {
        let remaining = cli
            .max_stdin_bytes
            .saturating_sub(u64::try_from(data.len()).unwrap_or(u64::MAX));
        let request = usize::try_from(remaining.saturating_add(1))
            .unwrap_or(usize::MAX)
            .min(buffer.len());
        let count = input
            .read(&mut buffer[..request])
            .map_err(|error| ArcthisError::io("reading stdin source", error))?;
        if count == 0 {
            break;
        }
        if u64::try_from(count).unwrap_or(u64::MAX) > remaining {
            return Err(ArcthisError::ResourceLimit {
                message: "stdin exceeds --max-stdin-bytes".to_owned(),
            });
        }
        data.try_reserve(count)
            .map_err(|_| ArcthisError::ResourceLimit {
                message: "cannot buffer stdin source".to_owned(),
            })?;
        data.extend_from_slice(&buffer[..count]);
    }
    cli.stdin_bytes = Some(std::sync::Arc::from(data));
    Ok(())
}

impl Command {
    fn read_archive_path(&self) -> Option<&Path> {
        match self {
            Command::List { archive }
            | Command::Tree { archive }
            | Command::Inspect { archive }
            | Command::Verify { archive }
            | Command::Stat { archive, .. }
            | Command::Read { archive, .. }
            | Command::Find { archive, .. }
            | Command::Grep { archive, .. }
            | Command::Hash { archive, .. } => Some(archive.as_path()),
            _ => None,
        }
    }
}

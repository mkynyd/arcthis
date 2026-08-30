//! Feature-gated local stdio MCP frontend.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use base64::Engine as _;
use rmcp::handler::server::{
    common::FromContextPart, router::tool::ToolRouter, tool::ToolCallContext, wrapper::Parameters,
};
use rmcp::model::{Implementation, ProtocolVersion, ServerCapabilities, ServerInfo};
use rmcp::{Json, ServerHandler, ServiceExt, tool, tool_handler, tool_router};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::SCHEMA_VERSION;
use crate::app::{
    ApplicationService, ArchiveSourceRequest, CancellationToken, FindServiceResult,
    GrepServiceResult, HashServiceResult, InspectResult, ListResult, ReadRequest, ServiceLimits,
    StatResult, TreeResult, VerifyServiceResult,
};
use crate::error::{ArcthisError, Result as ArcthisResult};
use crate::mcp_mutation::{
    ConvertExecuteInput, ConvertExecuteOutput, ConvertMutationInput, ConvertPlanOutput,
    ExtractExecuteInput, ExtractExecuteOutput, ExtractPlanOutput, ExtractionMutationInput,
    PackExecuteInput, PackExecuteOutput, PackMutationInput, PackPlanOutput,
};
use crate::query::{GrepOptions, HashAlgorithm};

const MUTATION_TOOL_NAMES: [&str; 6] = [
    "archive_extract_plan",
    "archive_extract_execute",
    "archive_pack_plan",
    "archive_pack_execute",
    "archive_convert_plan",
    "archive_convert_execute",
];

#[derive(Debug, Clone)]
pub struct McpConfig {
    pub allowed_input_roots: Vec<PathBuf>,
    pub allowed_output_roots: Vec<PathBuf>,
    pub allow_source_deletion: bool,
    pub limits: ServiceLimits,
}

impl McpConfig {
    pub fn read_only(allowed_input_roots: Vec<PathBuf>) -> Self {
        Self {
            allowed_input_roots,
            allowed_output_roots: Vec::new(),
            allow_source_deletion: false,
            limits: ServiceLimits::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
struct SourceInput {
    /// Archive filesystem path. The canonical path must be within an allowed input root.
    path: String,
    /// Explicit nested archive entry chain.
    #[serde(default)]
    within: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
struct StatInput {
    #[serde(flatten)]
    source: SourceInput,
    entry: String,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
struct ReadInput {
    #[serde(flatten)]
    source: SourceInput,
    entry: String,
    /// Raw byte offset in the decoded entry.
    offset: u64,
    /// Maximum raw bytes returned by this call.
    length: u64,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
struct FindInput {
    #[serde(flatten)]
    source: SourceInput,
    glob: String,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
struct GrepInput {
    #[serde(flatten)]
    source: SourceInput,
    pattern: String,
    #[serde(default)]
    glob: Option<String>,
    #[serde(default = "default_grep_entry_size")]
    max_entry_size: u64,
    #[serde(default = "default_grep_matches")]
    max_matches: u64,
    #[serde(default)]
    binary: bool,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum HashInputAlgorithm {
    #[default]
    Sha256,
    Sha512,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
struct HashInput {
    #[serde(flatten)]
    source: SourceInput,
    entry: String,
    #[serde(default)]
    algorithm: HashInputAlgorithm,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
struct ReadToolResult {
    schema_version: &'static str,
    archive: crate::app::ArchiveReference,
    entry: crate::ArchiveEntry,
    offset: u64,
    raw_size: u64,
    eof: bool,
    encoding: &'static str,
    data: String,
}

struct McpCancellation(CancellationToken);

impl<S> FromContextPart<ToolCallContext<'_, S>> for McpCancellation {
    fn from_context_part(
        context: &mut ToolCallContext<S>,
    ) -> std::result::Result<Self, rmcp::ErrorData> {
        let application = CancellationToken::default();
        let application_for_task = application.clone();
        let protocol = context.request_context.ct.clone();
        tokio::spawn(async move {
            protocol.cancelled().await;
            application_for_task.cancel();
        });
        Ok(Self(application))
    }
}

#[derive(Debug, Clone)]
pub struct McpServer {
    allowed_input_roots: Arc<Vec<PathBuf>>,
    allowed_output_roots: Arc<Vec<PathBuf>>,
    allow_source_deletion: bool,
    limits: ServiceLimits,
    tool_router: ToolRouter<Self>,
}

impl McpServer {
    pub fn new(config: McpConfig) -> ArcthisResult<Self> {
        let McpConfig {
            allowed_input_roots,
            allowed_output_roots,
            allow_source_deletion,
            limits,
        } = config;
        if allowed_input_roots.is_empty() {
            return Err(ArcthisError::PermissionDenied {
                context: "starting MCP without an allowed input root".to_owned(),
                source: std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "at least one --allow-root is required",
                ),
            });
        }
        let roots = allowed_input_roots
            .into_iter()
            .map(|root| {
                fs::canonicalize(&root)
                    .map_err(|error| ArcthisError::io("canonicalizing MCP input root", error))
            })
            .collect::<ArcthisResult<Vec<_>>>()?;
        let output_roots = allowed_output_roots
            .into_iter()
            .map(|root| {
                let canonical = fs::canonicalize(&root)
                    .map_err(|error| ArcthisError::io("canonicalizing MCP output root", error))?;
                if !canonical.is_dir() {
                    return Err(ArcthisError::UnsupportedOperation {
                        message: format!(
                            "MCP output root is not a directory: {}",
                            canonical.display()
                        ),
                    });
                }
                Ok(canonical)
            })
            .collect::<ArcthisResult<Vec<_>>>()?;
        let mut tool_router = Self::tool_router();
        if output_roots.is_empty() {
            for name in MUTATION_TOOL_NAMES {
                tool_router.disable_route(name);
            }
        }
        Ok(Self {
            allowed_input_roots: Arc::new(roots),
            allowed_output_roots: Arc::new(output_roots),
            allow_source_deletion,
            limits,
            tool_router,
        })
    }

    fn source(&self, input: SourceInput) -> ArcthisResult<ArchiveSourceRequest> {
        let canonical = self.authorize_input(&input.path, false)?;
        if !canonical.is_file() {
            return Err(ArcthisError::UnsupportedOperation {
                message: format!(
                    "MCP archive source is not a regular file: {}",
                    canonical.display()
                ),
            });
        }
        let mut source = ArchiveSourceRequest::file(canonical);
        source.within = input.within;
        Ok(source)
    }

    const fn service(&self, cancellation: CancellationToken) -> ApplicationService {
        ApplicationService::new(self.limits, cancellation)
    }

    fn authorize_input(&self, path: &str, allow_directory: bool) -> ArcthisResult<PathBuf> {
        let canonical = fs::canonicalize(path)
            .map_err(|error| ArcthisError::io("canonicalizing MCP input path", error))?;
        if !self
            .allowed_input_roots
            .iter()
            .any(|root| canonical.starts_with(root))
        {
            return permission_denied(format!(
                "accessing MCP input outside allowed roots: {}",
                canonical.display()
            ));
        }
        if !(canonical.is_file() || allow_directory && canonical.is_dir()) {
            return Err(ArcthisError::UnsupportedOperation {
                message: format!(
                    "MCP input is not an allowed source type: {}",
                    canonical.display()
                ),
            });
        }
        Ok(canonical)
    }

    fn authorize_output(&self, path: &str) -> ArcthisResult<PathBuf> {
        let requested = Path::new(path);
        let absolute = if requested.is_absolute() {
            requested.to_path_buf()
        } else {
            std::env::current_dir()
                .map_err(|error| ArcthisError::io("reading MCP working directory", error))?
                .join(requested)
        };
        if absolute
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
        {
            return Err(ArcthisError::UnsafePath {
                path: path.to_owned(),
                reason: "MCP output path cannot contain `..`".to_owned(),
            });
        }
        let mut ancestor = absolute.as_path();
        while !ancestor
            .try_exists()
            .map_err(|error| ArcthisError::io("checking MCP output ancestor", error))?
        {
            ancestor = ancestor.parent().ok_or_else(|| ArcthisError::UnsafePath {
                path: path.to_owned(),
                reason: "MCP output has no existing ancestor".to_owned(),
            })?;
        }
        let canonical_ancestor = fs::canonicalize(ancestor)
            .map_err(|error| ArcthisError::io("canonicalizing MCP output ancestor", error))?;
        let suffix = absolute.strip_prefix(ancestor).map_err(|error| {
            ArcthisError::UnsupportedOperation {
                message: format!("resolving MCP output suffix: {error}"),
            }
        })?;
        let authorized = canonical_ancestor.join(suffix);
        let allowed_root = self
            .allowed_output_roots
            .iter()
            .filter(|root| authorized.starts_with(root) && authorized != **root)
            .max_by_key(|root| root.components().count());
        let Some(allowed_root) = allowed_root else {
            return permission_denied(format!(
                "accessing MCP output outside allowed roots: {}",
                authorized.display()
            ));
        };
        let relative = authorized.strip_prefix(allowed_root).map_err(|error| {
            ArcthisError::UnsupportedOperation {
                message: format!("resolving MCP output path below allowed root: {error}"),
            }
        })?;
        let mut cursor = allowed_root.clone();
        for component in relative.components() {
            cursor.push(component.as_os_str());
            match fs::symlink_metadata(&cursor) {
                Ok(metadata) if metadata.file_type().is_symlink() => {
                    return Err(ArcthisError::UnsafePath {
                        path: path.to_owned(),
                        reason: format!(
                            "MCP output path traverses a symlink: {}",
                            cursor.display()
                        ),
                    });
                }
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
                Err(error) => {
                    return Err(ArcthisError::io(
                        "checking MCP output path components",
                        error,
                    ));
                }
            }
        }
        Ok(authorized)
    }

    fn authorize_deletion(&self, requested: bool, source: &Path) -> ArcthisResult<()> {
        if !requested {
            return Ok(());
        }
        if !self.allow_source_deletion {
            return permission_denied(
                "MCP source deletion is disabled by server policy".to_owned(),
            );
        }
        if self.allowed_input_roots.iter().any(|root| source == root) {
            return permission_denied(
                "MCP source deletion cannot remove an allowed input root".to_owned(),
            );
        }
        Ok(())
    }
}

#[tool_router]
impl McpServer {
    #[tool(
        name = "archive_inspect",
        description = "Inspect archive metadata, capabilities, and safety warnings without extraction.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    fn inspect(
        &self,
        Parameters(input): Parameters<SourceInput>,
        McpCancellation(cancellation): McpCancellation,
    ) -> std::result::Result<Json<InspectResult>, String> {
        let source = self.source(input).map_err(tool_error)?;
        self.service(cancellation)
            .inspect(&source)
            .map(Json)
            .map_err(tool_error)
    }

    #[tool(
        name = "archive_list",
        description = "List archive entries in archive order without extraction.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    fn list(
        &self,
        Parameters(input): Parameters<SourceInput>,
        McpCancellation(cancellation): McpCancellation,
    ) -> std::result::Result<Json<ListResult>, String> {
        let source = self.source(input).map_err(tool_error)?;
        self.service(cancellation)
            .list(&source)
            .map(Json)
            .map_err(tool_error)
    }

    #[tool(
        name = "archive_tree",
        description = "Return archive entries for deterministic tree construction without extraction.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    fn tree(
        &self,
        Parameters(input): Parameters<SourceInput>,
        McpCancellation(cancellation): McpCancellation,
    ) -> std::result::Result<Json<TreeResult>, String> {
        let source = self.source(input).map_err(tool_error)?;
        self.service(cancellation)
            .tree(&source)
            .map(Json)
            .map_err(tool_error)
    }

    #[tool(
        name = "archive_stat",
        description = "Return metadata for one exact archive entry.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    fn stat(
        &self,
        Parameters(input): Parameters<StatInput>,
        McpCancellation(cancellation): McpCancellation,
    ) -> std::result::Result<Json<StatResult>, String> {
        let source = self.source(input.source).map_err(tool_error)?;
        self.service(cancellation)
            .stat(&source, &input.entry)
            .map(Json)
            .map_err(tool_error)
    }

    #[tool(
        name = "archive_read",
        description = "Read one bounded decoded byte window from an archive entry. Offset and length are mandatory.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    fn read(
        &self,
        Parameters(input): Parameters<ReadInput>,
        McpCancellation(cancellation): McpCancellation,
    ) -> std::result::Result<Json<ReadToolResult>, String> {
        let source = self.source(input.source).map_err(tool_error)?;
        let result = self
            .service(cancellation)
            .read(&ReadRequest {
                source: &source,
                entry: &input.entry,
                offset: input.offset,
                length: input.length,
            })
            .map_err(tool_error)?;
        let raw_size = u64::try_from(result.data.len()).unwrap_or(u64::MAX);
        let (encoding, data) = match std::str::from_utf8(&result.data) {
            Ok(text) if !result.data.contains(&0) => ("utf8", text.to_owned()),
            _ => (
                "base64",
                base64::engine::general_purpose::STANDARD.encode(&result.data),
            ),
        };
        Ok(Json(ReadToolResult {
            schema_version: SCHEMA_VERSION,
            archive: result.archive,
            entry: result.entry,
            offset: result.offset,
            raw_size,
            eof: result.eof,
            encoding,
            data,
        }))
    }

    #[tool(
        name = "archive_find",
        description = "Find archive entry paths matching a glob.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    fn find(
        &self,
        Parameters(input): Parameters<FindInput>,
        McpCancellation(cancellation): McpCancellation,
    ) -> std::result::Result<Json<FindServiceResult>, String> {
        let source = self.source(input.source).map_err(tool_error)?;
        self.service(cancellation)
            .find(&source, &input.glob)
            .map(Json)
            .map_err(tool_error)
    }

    #[tool(
        name = "archive_grep",
        description = "Search regular-file contents with explicit entry-byte and match limits.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    fn grep(
        &self,
        Parameters(input): Parameters<GrepInput>,
        McpCancellation(cancellation): McpCancellation,
    ) -> std::result::Result<Json<GrepServiceResult>, String> {
        let source = self.source(input.source).map_err(tool_error)?;
        self.service(cancellation)
            .grep(
                &source,
                &input.pattern,
                &GrepOptions {
                    glob: input.glob,
                    max_entry_size: input.max_entry_size,
                    max_matches: input.max_matches,
                    scan_binary: input.binary,
                },
            )
            .map(Json)
            .map_err(tool_error)
    }

    #[tool(
        name = "archive_hash",
        description = "Hash one archive entry with SHA-256 or SHA-512.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    fn hash(
        &self,
        Parameters(input): Parameters<HashInput>,
        McpCancellation(cancellation): McpCancellation,
    ) -> std::result::Result<Json<HashServiceResult>, String> {
        let source = self.source(input.source).map_err(tool_error)?;
        let algorithm = match input.algorithm {
            HashInputAlgorithm::Sha256 => HashAlgorithm::Sha256,
            HashInputAlgorithm::Sha512 => HashAlgorithm::Sha512,
        };
        self.service(cancellation)
            .hash(&source, &input.entry, algorithm)
            .map(Json)
            .map_err(tool_error)
    }

    #[tool(
        name = "archive_verify",
        description = "Decode and verify the complete archive within configured resource limits.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    fn verify(
        &self,
        Parameters(input): Parameters<SourceInput>,
        McpCancellation(cancellation): McpCancellation,
    ) -> std::result::Result<Json<VerifyServiceResult>, String> {
        let source = self.source(input).map_err(tool_error)?;
        self.service(cancellation)
            .verify(&source)
            .map(Json)
            .map_err(tool_error)
    }

    #[tool(
        name = "archive_extract_plan",
        description = "Plan a bounded safe extraction and return a source/destination-bound digest.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    fn extract_plan(
        &self,
        Parameters(input): Parameters<ExtractionMutationInput>,
        McpCancellation(cancellation): McpCancellation,
    ) -> std::result::Result<Json<ExtractPlanOutput>, String> {
        let source = self
            .authorize_input(&input.path, false)
            .map_err(tool_error)?;
        let output = self.authorize_output(&input.output).map_err(tool_error)?;
        self.authorize_deletion(input.delete_source, &source)
            .map_err(tool_error)?;
        cancellation.checkpoint().map_err(tool_error)?;
        crate::mcp_mutation::plan_extract(&source, &output, &input)
            .map(Json)
            .map_err(tool_error)
    }

    #[tool(
        name = "archive_extract_execute",
        description = "Execute an extraction only when the supplied plan digest is still current.",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    fn extract_execute(
        &self,
        Parameters(input): Parameters<ExtractExecuteInput>,
        McpCancellation(cancellation): McpCancellation,
    ) -> std::result::Result<Json<ExtractExecuteOutput>, String> {
        let source = self
            .authorize_input(&input.request.path, false)
            .map_err(tool_error)?;
        let output = self
            .authorize_output(&input.request.output)
            .map_err(tool_error)?;
        self.authorize_deletion(input.request.delete_source, &source)
            .map_err(tool_error)?;
        cancellation.checkpoint().map_err(tool_error)?;
        crate::mcp_mutation::execute_extract(&source, &output, &input, &cancellation)
            .map(Json)
            .map_err(tool_error)
    }

    #[tool(
        name = "archive_pack_plan",
        description = "Plan a verified archive creation and return a source/destination-bound digest.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    fn pack_plan(
        &self,
        Parameters(input): Parameters<PackMutationInput>,
        McpCancellation(cancellation): McpCancellation,
    ) -> std::result::Result<Json<PackPlanOutput>, String> {
        let source = self
            .authorize_input(&input.path, true)
            .map_err(tool_error)?;
        let output = self.authorize_output(&input.output).map_err(tool_error)?;
        self.authorize_deletion(input.delete_source, &source)
            .map_err(tool_error)?;
        cancellation.checkpoint().map_err(tool_error)?;
        crate::mcp_mutation::plan_pack(&source, &output, &input)
            .map(Json)
            .map_err(tool_error)
    }

    #[tool(
        name = "archive_pack_execute",
        description = "Create and verify an archive only when the supplied plan digest is still current.",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    fn pack_execute(
        &self,
        Parameters(input): Parameters<PackExecuteInput>,
        McpCancellation(cancellation): McpCancellation,
    ) -> std::result::Result<Json<PackExecuteOutput>, String> {
        let source = self
            .authorize_input(&input.request.path, true)
            .map_err(tool_error)?;
        let output = self
            .authorize_output(&input.request.output)
            .map_err(tool_error)?;
        self.authorize_deletion(input.request.delete_source, &source)
            .map_err(tool_error)?;
        cancellation.checkpoint().map_err(tool_error)?;
        crate::mcp_mutation::execute_pack(&source, &output, &input, &cancellation)
            .map(Json)
            .map_err(tool_error)
    }

    #[tool(
        name = "archive_convert_plan",
        description = "Plan a staged verified archive conversion and return a bound digest.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    fn convert_plan(
        &self,
        Parameters(input): Parameters<ConvertMutationInput>,
        McpCancellation(cancellation): McpCancellation,
    ) -> std::result::Result<Json<ConvertPlanOutput>, String> {
        let source = self
            .authorize_input(&input.path, false)
            .map_err(tool_error)?;
        let output = self.authorize_output(&input.output).map_err(tool_error)?;
        self.authorize_deletion(input.delete_source, &source)
            .map_err(tool_error)?;
        cancellation.checkpoint().map_err(tool_error)?;
        crate::mcp_mutation::plan_convert(&source, &output, &input)
            .map(Json)
            .map_err(tool_error)
    }

    #[tool(
        name = "archive_convert_execute",
        description = "Convert an archive only when the supplied plan digest is still current.",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    fn convert_execute(
        &self,
        Parameters(input): Parameters<ConvertExecuteInput>,
        McpCancellation(cancellation): McpCancellation,
    ) -> std::result::Result<Json<ConvertExecuteOutput>, String> {
        let source = self
            .authorize_input(&input.request.path, false)
            .map_err(tool_error)?;
        let output = self
            .authorize_output(&input.request.output)
            .map_err(tool_error)?;
        self.authorize_deletion(input.request.delete_source, &source)
            .map_err(tool_error)?;
        cancellation.checkpoint().map_err(tool_error)?;
        crate::mcp_mutation::execute_convert(&source, &output, &input, &cancellation)
            .map(Json)
            .map_err(tool_error)
    }
}

#[tool_handler(router = self.tool_router)]
#[allow(clippy::unused_async_trait_impl)]
impl ServerHandler for McpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_protocol_version(ProtocolVersion::V_2025_06_18)
            .with_server_info(
                Implementation::new("arcthis", env!("CARGO_PKG_VERSION"))
                    .with_title("arcthis local archive access"),
            )
            .with_instructions(
                "Read-only local archive access. Every path must be inside a configured allowed root; archive_read is always bounded.",
            )
    }
}

pub fn run_stdio(config: McpConfig) -> ArcthisResult<()> {
    let server = McpServer::new(config)?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_time()
        .build()
        .map_err(|error| ArcthisError::io("creating MCP runtime", error))?;
    runtime.block_on(async move {
        let running = server
            .serve(rmcp::transport::stdio())
            .await
            .map_err(|error| ArcthisError::UnsupportedOperation {
                message: format!("starting MCP stdio transport: {error}"),
            })?;
        running
            .waiting()
            .await
            .map_err(|error| ArcthisError::UnsupportedOperation {
                message: format!("MCP stdio transport stopped: {error}"),
            })?;
        Ok(())
    })
}

#[allow(clippy::needless_pass_by_value)] // Required as a reusable `Result::map_err` function.
fn tool_error(error: ArcthisError) -> String {
    let value = serde_json::json!({
        "schema_version": SCHEMA_VERSION,
        "error": {
            "code": error.code(),
            "message": error.to_string(),
        }
    });
    serde_json::to_string(&value).unwrap_or_else(|_| "arcthis tool error".to_owned())
}

fn permission_denied<T>(context: String) -> ArcthisResult<T> {
    Err(ArcthisError::PermissionDenied {
        context,
        source: std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "MCP policy denied access",
        ),
    })
}

const fn default_grep_entry_size() -> u64 {
    16 * 1024 * 1024
}

const fn default_grep_matches() -> u64 {
    10_000
}

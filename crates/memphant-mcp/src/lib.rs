//! MemPhant MCP server on rmcp 2.2 (MCP 2025-11-25): seven tools over the
//! shared `MemoryService<AnyStore>`, a persistent stdio session, and an
//! optional streamable-HTTP transport. The tenant is fixed at startup from
//! `MEMPHANT_API_KEY` (sha256 → api_key lookup) or `MEMPHANT_DEV_TENANT`
//! (dev) — stdio is a per-principal transport; a missing/revoked key refuses
//! to start rather than serving an unauthenticated session.

use memphant_core::service::{MemoryService, ServiceError, clamp_trust};
use memphant_core::{CoreError, MemoryStore, MutationResponse, StoreError};
use memphant_runtime::AnyStore;
use memphant_types::{
    CorrectRequest, CorrectResult, ENGINE_VERSION, ForgetRequest, ForgetResult, MarkRequest,
    MarkResult, RecallHttpRequest, RecallResponse, ReflectAccepted, ReflectRequest,
    RetainEpisodeHttpRequest, RetainEpisodeHttpResponse, RetrievalTrace, TenantId, TraceRequest,
    TrustLevel,
};
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{Implementation, ServerCapabilities, ServerInfo};
use rmcp::{Json, ServerHandler, tool, tool_handler, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use sha2::{Digest, Sha256};

/// Hashes a presented API key into the stored `api_key.key_hash` form
/// (identical to the REST edge).
pub fn api_key_hash(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Client-facing error string for MCP tools, mirroring the REST edge: raw
/// backend/store errors are hidden behind a generic message; validation,
/// not-found, and policy errors carry caller-relevant, safe-to-surface text.
pub fn mcp_error(error: ServiceError) -> String {
    match error {
        ServiceError::Core(CoreError::Store(StoreError::IdempotencyConflict)) => {
            "idempotency_conflict: key was already used with a different request".to_string()
        }
        ServiceError::Core(CoreError::Store(StoreError::StaleSubjectGeneration)) => {
            "stale_subject_generation: subject generation is stale".to_string()
        }
        ServiceError::Core(CoreError::Store(StoreError::SubjectErased)) => {
            "subject_erased: subject has been erased".to_string()
        }
        ServiceError::Core(CoreError::Store(StoreError::PolicyDenied(_))) => {
            "scope_denied: request is outside the resolved memory policy".to_string()
        }
        ServiceError::Core(CoreError::DeepUnavailable) => {
            "deep_unavailable: deep recall is unavailable".to_string()
        }
        ServiceError::Core(CoreError::DeepProviderInvalidOutput) => {
            "deep_provider_invalid_output: deep recall provider returned invalid output".to_string()
        }
        ServiceError::Core(CoreError::Store(_)) => "backend unavailable".to_string(),
        other => other.to_string(),
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct McpMutation<T> {
    #[schemars(length(min = 1, max = 255))]
    idempotency_key: String,
    request: T,
}

fn decode_mutation_response<T: DeserializeOwned>(
    response: MutationResponse,
) -> Result<Json<T>, String> {
    serde_json::from_slice(response.body())
        .map(Json)
        .map_err(|_| "backend unavailable".to_string())
}

/// Constant-time string equality (length may leak). Compares a presented bearer
/// token to the process key without a timing side channel.
pub fn constant_time_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Whether an MCP streamable-HTTP request is authorized. Dev mode (auth
/// explicitly disabled and logged loudly at startup) allows all; otherwise the
/// `Authorization: Bearer <token>` header must equal the process key. This
/// gives the HTTP transport the same per-request gate the REST edge has, so a
/// widened `MEMPHANT_MCP_BIND` never serves the bound tenant unauthenticated.
pub fn mcp_http_authorized(
    dev_mode: bool,
    expected_key: Option<&str>,
    auth_header: Option<&str>,
) -> bool {
    if dev_mode {
        return true;
    }
    let Some(expected) = expected_key else {
        return false;
    };
    let Some(token) = auth_header.and_then(|header| {
        header
            .strip_prefix("Bearer ")
            .or_else(|| header.strip_prefix("bearer "))
    }) else {
        return false;
    };
    constant_time_eq(token.trim(), expected.trim())
}

/// The tenant binding resolved at startup. Stdio serves exactly one
/// principal; there is no per-request Authorization header.
#[derive(Debug, Clone, Copy)]
pub struct BoundTenant {
    pub tenant: TenantId,
    pub max_trust: TrustLevel,
    pub actor_id: Option<memphant_types::ActorId>,
    pub scope_id: Option<memphant_types::ScopeId>,
    pub dev_mode: bool,
}

/// Resolves the fixed tenant from the environment:
/// - `MEMPHANT_DEV_TENANT=<uuid>` → dev mode (loud, trust ceiling
///   `trusted_system`, body tenant ids ignored);
/// - `MEMPHANT_API_KEY=mk_…` → sha256 → `api_key` lookup (missing or revoked
///   → error: the process must refuse to start);
/// - neither → error.
pub async fn resolve_tenant(store: &AnyStore) -> Result<BoundTenant, String> {
    if let Ok(raw) = std::env::var("MEMPHANT_DEV_TENANT")
        && !raw.trim().is_empty()
    {
        let uuid = uuid::Uuid::parse_str(raw.trim())
            .map_err(|error| format!("MEMPHANT_DEV_TENANT must be a UUID: {error}"))?;
        let tenant = TenantId::from_u128(uuid.as_u128());
        eprintln!(
            "memphant-mcp: AUTH DISABLED (dev) — all tool calls bound to tenant {}",
            tenant.as_uuid()
        );
        return Ok(BoundTenant {
            tenant,
            max_trust: TrustLevel::TrustedSystem,
            actor_id: None,
            scope_id: None,
            dev_mode: true,
        });
    }
    let key = std::env::var("MEMPHANT_API_KEY").ok().filter(|key| !key.trim().is_empty()).ok_or_else(|| {
        "no tenant binding: set MEMPHANT_API_KEY=mk_<key> (or MEMPHANT_DEV_TENANT=<uuid> for dev); refusing to start an unauthenticated MCP session".to_string()
    })?;
    let row = store
        .lookup_api_key(&api_key_hash(key.trim()))
        .await
        .map_err(|error| format!("api key lookup failed: {error}"))?
        .ok_or_else(|| {
            "MEMPHANT_API_KEY does not match any api_key row; refusing to start".to_string()
        })?;
    if row.revoked {
        return Err("MEMPHANT_API_KEY is revoked; refusing to start".to_string());
    }
    Ok(BoundTenant {
        tenant: row.tenant_id,
        max_trust: row.max_trust,
        actor_id: row.actor_id,
        scope_id: row.scope_id,
        dev_mode: false,
    })
}

/// The MCP tool surface: seven verbs over the shared application layer.
#[derive(Clone)]
pub struct MemphantMcp {
    service: MemoryService<AnyStore>,
    bound: BoundTenant,
    tool_router: ToolRouter<Self>,
}

impl MemphantMcp {
    /// Whether the process resolved a dev tenant (auth explicitly disabled).
    /// The HTTP transport uses this to decide whether to enforce per-request
    /// bearer auth.
    pub fn dev_mode(&self) -> bool {
        self.bound.dev_mode
    }
}

#[tool_router(router = tool_router)]
impl MemphantMcp {
    pub fn new(service: MemoryService<AnyStore>, bound: BoundTenant) -> Self {
        Self {
            service,
            bound,
            tool_router: Self::tool_router(),
        }
    }

    fn bind_principal(
        &self,
        actor_id: memphant_types::ActorId,
        scope_id: memphant_types::ScopeId,
    ) -> Result<(), String> {
        if self.bound.dev_mode
            || (self.bound.actor_id.is_none() && self.bound.scope_id.is_none())
            || (self.bound.actor_id == Some(actor_id) && self.bound.scope_id == Some(scope_id))
        {
            Ok(())
        } else {
            Err("scope_denied: request is outside the API key principal binding".to_string())
        }
    }

    #[tool(
        description = "Store exactly one episode, resource, or direct unit with provenance.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn retain(
        &self,
        Parameters(McpMutation {
            idempotency_key,
            request,
        }): Parameters<McpMutation<RetainEpisodeHttpRequest>>,
    ) -> Result<Json<RetainEpisodeHttpResponse>, String> {
        let tenant = self.bound.tenant;
        self.bind_principal(request.actor_id, request.scope_id)?;
        let context = self
            .service
            .store()
            .resolve_memory_context(
                tenant,
                request.subject_id,
                request.actor_id,
                request.scope_id,
                request.agent_node_id,
            )
            .await
            .map_err(|_| "scope_denied: unresolved memory context".to_string())?;
        if request.subject_generation != context.subject_generation {
            return Err("context_binding_conflict: subject generation is stale".to_string());
        }
        let response = self
            .service
            .retain(
                &context,
                &idempotency_key,
                clamp_trust(context.actor_trust, self.bound.max_trust),
                request,
            )
            .await
            .map_err(mcp_error)?;
        decode_mutation_response(response)
    }

    #[tool(
        description = "Retrieve cited memory evidence for a query (budgeted, salience-ranked, with provenance).",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn recall(
        &self,
        Parameters(request): Parameters<RecallHttpRequest>,
    ) -> Result<Json<RecallResponse>, String> {
        let tenant = self.bound.tenant;
        self.bind_principal(request.actor_id, request.scope_id)?;
        let context = self
            .service
            .store()
            .resolve_memory_context(
                tenant,
                request.subject_id,
                request.actor_id,
                request.scope_id,
                request.agent_node_id,
            )
            .await
            .map_err(|_| "scope_denied: unresolved memory context".to_string())?;
        if request.subject_generation != context.subject_generation {
            return Err("context_binding_conflict: subject generation is stale".to_string());
        }
        self.service
            .recall(context, request)
            .await
            .map(Json)
            .map_err(mcp_error)
    }

    #[tool(
        description = "Consolidate a scope's pending episodes/resources into memory units.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn reflect(
        &self,
        Parameters(McpMutation {
            idempotency_key,
            request,
        }): Parameters<McpMutation<ReflectRequest>>,
    ) -> Result<Json<ReflectAccepted>, String> {
        let tenant = self.bound.tenant;
        self.bind_principal(request.actor_id, request.scope_id)?;
        let context = self
            .service
            .store()
            .resolve_memory_context(
                tenant,
                request.subject_id,
                request.actor_id,
                request.scope_id,
                request.agent_node_id,
            )
            .await
            .map_err(|_| "scope_denied: unresolved memory context".to_string())?;
        if request.subject_generation != context.subject_generation {
            return Err("context_binding_conflict: subject generation is stale".to_string());
        }
        let response = self
            .service
            .reflect(&context, &idempotency_key, request)
            .await
            .map_err(mcp_error)?;
        decode_mutation_response(response)
    }

    #[tool(
        description = "Supersede or invalidate selected memory through an auditable correction.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn correct(
        &self,
        Parameters(McpMutation {
            idempotency_key,
            request,
        }): Parameters<McpMutation<CorrectRequest>>,
    ) -> Result<Json<CorrectResult>, String> {
        let tenant = self.bound.tenant;
        self.bind_principal(request.actor_id, request.scope_id)?;
        let context = self
            .service
            .store()
            .resolve_memory_context(
                tenant,
                request.subject_id,
                request.actor_id,
                request.scope_id,
                request.agent_node_id,
            )
            .await
            .map_err(|_| "scope_denied: unresolved memory context".to_string())?;
        if request.subject_generation != context.subject_generation {
            return Err("context_binding_conflict: subject generation is stale".to_string());
        }
        decode_mutation_response(
            self.service
                .correct(&context, &idempotency_key, request)
                .await
                .map_err(mcp_error)?,
        )
    }

    #[tool(
        description = "Forget by memory unit, episode or resource selector; tombstones block re-derivation.",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn forget(
        &self,
        Parameters(McpMutation {
            idempotency_key,
            request,
        }): Parameters<McpMutation<ForgetRequest>>,
    ) -> Result<Json<ForgetResult>, String> {
        let tenant = self.bound.tenant;
        if request.scope_id != request.selector.scope_id {
            return Err(
                "context_binding_conflict: forget scope does not match selector scope".to_string(),
            );
        }
        self.bind_principal(request.actor_id, request.selector.scope_id)?;
        let context = self
            .service
            .store()
            .resolve_memory_context(
                tenant,
                request.subject_id,
                request.actor_id,
                request.scope_id,
                request.agent_node_id,
            )
            .await
            .map_err(|_| "scope_denied: unresolved memory context".to_string())?;
        if request.subject_generation != context.subject_generation {
            return Err("context_binding_conflict: subject generation is stale".to_string());
        }
        decode_mutation_response(
            self.service
                .forget(&context, &idempotency_key, request)
                .await
                .map_err(mcp_error)?,
        )
    }

    #[tool(
        description = "Inspect a retrieval trace (tenant-bound).",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn trace(
        &self,
        Parameters(request): Parameters<TraceRequest>,
    ) -> Result<Json<RetrievalTrace>, String> {
        let tenant = self.bound.tenant;
        self.bind_principal(request.actor_id, request.scope_id)?;
        let context = self
            .service
            .store()
            .resolve_memory_context(
                tenant,
                request.subject_id,
                request.actor_id,
                request.scope_id,
                request.agent_node_id,
            )
            .await
            .map_err(|_| "scope_denied: unresolved memory context".to_string())?;
        if request.subject_generation != context.subject_generation {
            return Err("context_binding_conflict: subject generation is stale".to_string());
        }
        let trace = self
            .service
            .trace(&context, request.trace_id)
            .await
            .map_err(mcp_error)?
            .ok_or_else(|| "trace not found".to_string())?;
        self.bind_principal(trace.actor_id, trace.scope_id)?;
        Ok(Json(trace))
    }

    #[tool(
        description = "Report what the caller did with a recall pack (feeds decay/reinforcement).",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn mark(
        &self,
        Parameters(McpMutation {
            idempotency_key,
            request,
        }): Parameters<McpMutation<MarkRequest>>,
    ) -> Result<Json<MarkResult>, String> {
        let tenant = self.bound.tenant;
        self.bind_principal(request.actor_id, request.scope_id)?;
        let context = self
            .service
            .store()
            .resolve_memory_context(
                tenant,
                request.subject_id,
                request.actor_id,
                request.scope_id,
                request.agent_node_id,
            )
            .await
            .map_err(|_| "scope_denied: unresolved memory context".to_string())?;
        if request.subject_generation != context.subject_generation {
            return Err("context_binding_conflict: subject generation is stale".to_string());
        }
        decode_mutation_response(
            self.service
                .mark(&context, &idempotency_key, request)
                .await
                .map_err(mcp_error)?,
        )
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for MemphantMcp {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("memphant", ENGINE_VERSION))
            .with_instructions(
                "MemPhant memory service: retain, recall, reflect, correct, forget, trace, mark.",
            )
    }
}

/// The committed `mcp/memphant.tools.v1.json` artifact: rmcp's own tool list
/// (camelCase `inputSchema`/`outputSchema`), never hand-edited.
pub fn tools_artifact() -> Value {
    serde_json::to_value(MemphantMcp::tool_router().list_all()).expect("MCP tools serialize")
}

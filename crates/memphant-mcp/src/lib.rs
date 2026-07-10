//! MemPhant MCP server on rmcp 2.2 (MCP 2025-11-25): seven tools over the
//! shared `MemoryService<AnyStore>`, a persistent stdio session, and an
//! optional streamable-HTTP transport. The tenant is fixed at startup from
//! `MEMPHANT_API_KEY` (sha256 → api_key lookup) or `MEMPHANT_DEV_TENANT`
//! (dev) — stdio is a per-principal transport; a missing/revoked key refuses
//! to start rather than serving an unauthenticated session.

use memphant_core::MemoryStore;
use memphant_core::service::{MemoryService, clamp_trust};
use memphant_runtime::AnyStore;
use memphant_types::{
    CorrectRequest, CorrectResult, ENGINE_VERSION, ForgetRequest, ForgetResult, MarkRequest,
    MarkResult, RecallHttpRequest, RecallResponse, ReflectRequest, ReflectResult,
    RetainEpisodeHttpRequest, RetainEpisodeHttpResponse, RetrievalTrace, TenantId, TraceRequest,
    TrustLevel,
};
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{Implementation, ServerCapabilities, ServerInfo};
use rmcp::{Json, ServerHandler, tool, tool_handler, tool_router};
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

/// The tenant binding resolved at startup. Stdio serves exactly one
/// principal; there is no per-request Authorization header.
#[derive(Debug, Clone, Copy)]
pub struct BoundTenant {
    pub tenant: TenantId,
    pub max_trust: TrustLevel,
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

#[tool_router(router = tool_router)]
impl MemphantMcp {
    pub fn new(service: MemoryService<AnyStore>, bound: BoundTenant) -> Self {
        Self {
            service,
            bound,
            tool_router: Self::tool_router(),
        }
    }

    /// All tool calls are bound server-side to the startup tenant; a body
    /// tenant id that disagrees is rejected (ignored entirely in dev mode).
    fn bind_tenant(&self, body_tenant: TenantId) -> Result<TenantId, String> {
        if self.bound.dev_mode || body_tenant == self.bound.tenant {
            Ok(self.bound.tenant)
        } else {
            Err("tenant_mismatch: body tenant_id does not match the session's tenant".to_string())
        }
    }

    #[tool(
        description = "Store memory: an episode body (default), a resource {uri, mime_type, content_hash, revision?, body} or a direct pre-compiled unit {kind, subject, predicate, body}.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn retain(
        &self,
        Parameters(mut request): Parameters<RetainEpisodeHttpRequest>,
    ) -> Result<Json<RetainEpisodeHttpResponse>, String> {
        let tenant = self.bind_tenant(request.tenant_id)?;
        // Caller-declared trust is a hint, capped at the key's ceiling.
        request.source_trust = clamp_trust(request.source_trust, self.bound.max_trust);
        self.service
            .retain(tenant, request)
            .await
            .map(Json)
            .map_err(|error| error.to_string())
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
        let tenant = self.bind_tenant(request.tenant_id)?;
        self.service
            .recall(tenant, request)
            .await
            .map(Json)
            .map_err(|error| error.to_string())
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
        Parameters(request): Parameters<ReflectRequest>,
    ) -> Result<Json<ReflectResult>, String> {
        let tenant = self.bind_tenant(request.tenant_id)?;
        self.service
            .reflect(tenant, request.scope_id, request.compiler_version)
            .await
            .map(Json)
            .map_err(|error| error.to_string())
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
        Parameters(request): Parameters<CorrectRequest>,
    ) -> Result<Json<CorrectResult>, String> {
        let tenant = self.bind_tenant(request.tenant_id)?;
        self.service
            .correct(tenant, request)
            .await
            .map(Json)
            .map_err(|error| error.to_string())
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
        Parameters(request): Parameters<ForgetRequest>,
    ) -> Result<Json<ForgetResult>, String> {
        let tenant = self.bind_tenant(request.tenant_id)?;
        self.service
            .forget(tenant, request)
            .await
            .map(Json)
            .map_err(|error| error.to_string())
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
        let tenant = self.bind_tenant(request.tenant_id)?;
        self.service
            .trace(tenant, request.trace_id)
            .await
            .map_err(|error| error.to_string())?
            .map(Json)
            .ok_or_else(|| "trace not found".to_string())
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
        Parameters(request): Parameters<MarkRequest>,
    ) -> Result<Json<MarkResult>, String> {
        let tenant = self.bind_tenant(request.tenant_id)?;
        self.service
            .mark(tenant, request)
            .await
            .map(Json)
            .map_err(|error| error.to_string())
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

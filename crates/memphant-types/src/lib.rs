use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

macro_rules! id_type {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
        pub struct $name(Uuid);

        impl $name {
            pub fn new() -> Self {
                Self(Uuid::now_v7())
            }

            pub fn from_u128(value: u128) -> Self {
                Self(Uuid::from_u128(value))
            }

            pub fn as_uuid(self) -> Uuid {
                self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }
    };
}

id_type!(ActorId);
id_type!(EdgeId);
id_type!(EpisodeId);
id_type!(JobId);
id_type!(ResourceId);
id_type!(ScopeId);
id_type!(TenantId);
id_type!(TraceId);
id_type!(UnitId);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ScopeRef {
    pub kind: String,
    pub external_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RetainInput {
    pub scope: ScopeRef,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RetainResult {
    pub retained: bool,
    pub extracted_values: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RetainRequest {
    pub tenant_id: TenantId,
    pub scope_id: ScopeId,
    pub actor_id: ActorId,
    pub source_kind: String,
    pub source_trust: TrustLevel,
    pub subject_hint: Option<String>,
    pub body: String,
    pub compiler_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RetainResourceRequest {
    pub tenant_id: TenantId,
    pub scope_id: ScopeId,
    pub actor_id: ActorId,
    pub uri: String,
    pub content_hash: String,
    pub mime_type: String,
    pub source_trust: TrustLevel,
    pub compiler_version: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RecallMode {
    Fast,
    Balanced,
    Exhaustive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RecallChannel {
    Exact,
    Lexical,
    Vector,
    Temporal,
    Edge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RecallDropReason {
    Tenant,
    Scope,
    Privacy,
    Trust,
    State,
    Stale,
    Budget,
    Duplicate,
    Rerank,
    Deleted,
    Invalidated,
    Unknown,
    ProtectedCategory,
    BelowTrustFloor,
    Irrelevant,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RecallRequest {
    pub tenant_id: TenantId,
    pub scope_id: ScopeId,
    pub actor_id: ActorId,
    pub allowed_scope_ids: Vec<ScopeId>,
    pub query: String,
    pub k: usize,
    pub budget_tokens: usize,
    pub mode: RecallMode,
    pub include_beliefs: bool,
    #[serde(default = "default_true")]
    pub edge_expansion_enabled: bool,
    pub engine_version: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RecallCandidateTrace {
    pub unit_id: UnitId,
    pub channel: RecallChannel,
    pub channel_rank: usize,
    pub channel_score: f32,
    pub fused_rank: Option<usize>,
    pub fused_score: Option<f32>,
    pub trust_level: TrustLevel,
    pub state: UnitState,
    pub discard_reason: Option<RecallDropReason>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RecallPolicyFilter {
    pub reason: RecallDropReason,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RecallCitation {
    pub unit_id: UnitId,
    pub episode_id: Option<EpisodeId>,
    pub resource_id: Option<ResourceId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RecallContextItem {
    pub unit_id: UnitId,
    pub body: String,
    pub kind: MemoryKind,
    pub inclusion_reason: String,
    pub citation_episode_id: Option<EpisodeId>,
    pub citation_resource_id: Option<ResourceId>,
    pub suppression_labels: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RecallDroppedItem {
    pub unit_id: UnitId,
    pub reason: RecallDropReason,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RetrievalTrace {
    pub id: TraceId,
    pub tenant_id: TenantId,
    pub scope_id: ScopeId,
    pub actor_id: ActorId,
    pub query_hash: String,
    pub engine_version: String,
    pub feature_flags: Vec<String>,
    pub channel_runs: Vec<ReflectStageFact>,
    pub candidates: Vec<RecallCandidateTrace>,
    pub policy_filters: Vec<RecallPolicyFilter>,
    pub context_items: Vec<RecallContextItem>,
    pub dropped_items: Vec<RecallDroppedItem>,
    pub citations: Vec<RecallCitation>,
    pub filter_selectivity: Option<f32>,
    pub iterative_scan_depth: Option<u32>,
    pub consolidation_lag_ms: u64,
    pub weight_vector_id: String,
    pub mode_requested: RecallMode,
    pub mode_executed: RecallMode,
    pub escalation_reason: String,
    pub abstention_signal: bool,
    pub latency_ms: u64,
    pub token_estimate: usize,
    pub cost_micros: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RecallResponse {
    pub trace_id: TraceId,
    pub items: Vec<RecallContextItem>,
    pub candidate_whitelist: Vec<UnitId>,
    pub citations: Vec<RecallCitation>,
    pub abstention: bool,
    pub degraded: bool,
    pub suppression_labels: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MemoryKind {
    Episodic,
    Semantic,
    Procedural,
    Belief,
    Resource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum UnitState {
    Captured,
    Extracted,
    Candidate,
    Active,
    Superseded,
    Invalidated,
    Deleted,
    Quarantined,
    Expired,
    Validated,
    Retired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TrustLevel {
    TrustedUser,
    TrustedSystem,
    VerifiedTool,
    UnverifiedTool,
    WebContent,
    AgentOutput,
    ImportedExternal,
    Quarantined,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct NewEpisode {
    pub tenant_id: TenantId,
    pub scope_id: ScopeId,
    pub actor_id: ActorId,
    pub source_kind: String,
    pub source_trust: TrustLevel,
    pub dedup_key: String,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct StoredEpisode {
    pub id: EpisodeId,
    pub tenant_id: TenantId,
    pub scope_id: ScopeId,
    pub actor_id: ActorId,
    pub source_kind: String,
    pub source_trust: TrustLevel,
    pub dedup_key: String,
    pub body: String,
    pub observation_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ResourceExtractorState {
    Registered,
    Fetching,
    Extracting,
    Chunked,
    Embedded,
    Failed,
    Stale,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct NewResource {
    pub tenant_id: TenantId,
    pub scope_id: ScopeId,
    pub actor_id: ActorId,
    pub uri: String,
    pub content_hash: String,
    pub mime_type: String,
    pub source_trust: TrustLevel,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct StoredResource {
    pub id: ResourceId,
    pub tenant_id: TenantId,
    pub scope_id: ScopeId,
    pub actor_id: ActorId,
    pub uri: String,
    pub content_hash: String,
    pub mime_type: String,
    pub source_trust: TrustLevel,
    pub extractor_state: ResourceExtractorState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ContextualChunk {
    pub id: String,
    pub header: String,
    pub body: String,
    pub source_span: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct NewMemoryUnit {
    pub tenant_id: TenantId,
    pub scope_id: ScopeId,
    pub kind: MemoryKind,
    pub state: UnitState,
    pub subject_key: Option<String>,
    pub body: String,
    pub trust_level: TrustLevel,
    pub churn_class: Option<String>,
    pub freshness_due: bool,
    pub actor_id: Option<ActorId>,
    pub source_kind: Option<String>,
    pub source_episode_id: Option<EpisodeId>,
    pub source_resource_id: Option<ResourceId>,
    pub deletion_generation: Option<u64>,
    #[serde(default)]
    pub contextual_chunks: Vec<ContextualChunk>,
    #[serde(default)]
    pub valid_from: Option<String>,
    #[serde(default)]
    pub valid_to: Option<String>,
    #[serde(default)]
    pub transaction_from: Option<String>,
    #[serde(default)]
    pub transaction_to: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct StoredMemoryUnit {
    pub id: UnitId,
    pub tenant_id: TenantId,
    pub scope_id: ScopeId,
    pub kind: MemoryKind,
    pub state: UnitState,
    pub subject_key: Option<String>,
    pub body: String,
    pub trust_level: TrustLevel,
    pub churn_class: Option<String>,
    pub freshness_due: bool,
    pub actor_id: Option<ActorId>,
    pub source_kind: Option<String>,
    pub source_episode_id: Option<EpisodeId>,
    pub source_resource_id: Option<ResourceId>,
    pub deletion_generation: Option<u64>,
    pub contextual_chunks: Vec<ContextualChunk>,
    pub valid_from: Option<String>,
    pub valid_to: Option<String>,
    pub transaction_from: Option<String>,
    pub transaction_to: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MemoryEdgeKind {
    Supersedes,
    Contradicts,
    DerivedFrom,
    Cites,
    SameSubject,
    DependsOn,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct StoredMemoryEdge {
    pub id: EdgeId,
    pub tenant_id: TenantId,
    pub scope_id: ScopeId,
    pub src_id: UnitId,
    pub dst_id: UnitId,
    pub kind: MemoryEdgeKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct NewMemoryEdge {
    pub tenant_id: TenantId,
    pub scope_id: ScopeId,
    pub src_id: UnitId,
    pub dst_id: UnitId,
    pub kind: MemoryEdgeKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AdmissionAction {
    Reject,
    Append,
    Merge,
    Supersede,
    Invalidate,
    Quarantine,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReflectJobKind {
    ReflectEpisode,
    ReflectResource,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ReflectJob {
    pub tenant_id: TenantId,
    pub scope_id: ScopeId,
    pub episode_id: Option<EpisodeId>,
    pub resource_id: Option<ResourceId>,
    pub kind: ReflectJobKind,
    pub compiler_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct QueuedReflectJob {
    pub id: JobId,
    pub tenant_id: TenantId,
    pub scope_id: ScopeId,
    pub episode_id: Option<EpisodeId>,
    pub resource_id: Option<ResourceId>,
    pub kind: ReflectJobKind,
    pub compiler_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ReflectCandidate {
    pub source_kind: String,
    pub trust_level: TrustLevel,
    pub actor_id: ActorId,
    pub subject: Option<String>,
    pub predicate: Option<String>,
    pub body: String,
    pub churn_class: Option<String>,
    pub admission_hint: Option<AdmissionAction>,
    #[serde(default)]
    pub contextual_chunks: Vec<ContextualChunk>,
    #[serde(default)]
    pub valid_from: Option<String>,
    #[serde(default)]
    pub valid_to: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ReflectInput {
    pub tenant_id: TenantId,
    pub scope_id: ScopeId,
    pub actor_id: ActorId,
    pub episode_id: EpisodeId,
    pub job_id: JobId,
    pub compiler_version: String,
    pub candidates: Vec<ReflectCandidate>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ReflectStageFact {
    pub stage: String,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ReflectTrace {
    pub tenant_id: TenantId,
    pub scope_id: ScopeId,
    pub job_id: JobId,
    pub episode_id: EpisodeId,
    pub compiler_version: String,
    pub actions: Vec<AdmissionAction>,
    pub stages: Vec<ReflectStageFact>,
    pub cost_units: u32,
}

impl ReflectTrace {
    pub fn stage_names(&self) -> Vec<&str> {
        self.stages
            .iter()
            .map(|stage| stage.stage.as_str())
            .collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DedupOutcome {
    pub matched: bool,
    pub observation_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RetainOutcome {
    pub episode_id: EpisodeId,
    pub dedup: DedupOutcome,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RetainResourceOutcome {
    pub resource_id: ResourceId,
}

pub const ENGINE_VERSION: &str = "0.1.0-ws0";
pub const COMPILER_VERSION: &str = "compiler-0.1.0-ws0";
pub const TRACE_SCHEMA_VERSION: &str = "trace-0.1.0-ws0";
pub const SCHEMA_COMPAT_REVISION: &str = "schema-compat-0";
pub const METHODOLOGY_VERSION: &str = "memphant-methodology-2026-07-03";
pub const EXPORT_SCHEMA_VERSION: &str = "export-0.1.0-ws0";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct MemphantLock {
    pub engine_version: String,
    pub compiler_version: String,
    pub trace_schema_version: String,
    pub schema_compat_revision: String,
    pub methodology_version: String,
    pub export_schema_version: String,
}

impl MemphantLock {
    pub fn current() -> Self {
        Self {
            engine_version: ENGINE_VERSION.to_string(),
            compiler_version: COMPILER_VERSION.to_string(),
            trace_schema_version: TRACE_SCHEMA_VERSION.to_string(),
            schema_compat_revision: SCHEMA_COMPAT_REVISION.to_string(),
            methodology_version: METHODOLOGY_VERSION.to_string(),
            export_schema_version: EXPORT_SCHEMA_VERSION.to_string(),
        }
    }

    pub fn mismatches(&self, actual: &Self) -> Vec<VerifyMismatch> {
        let pairs = [
            (
                "engine_version",
                self.engine_version.as_str(),
                actual.engine_version.as_str(),
            ),
            (
                "compiler_version",
                self.compiler_version.as_str(),
                actual.compiler_version.as_str(),
            ),
            (
                "trace_schema_version",
                self.trace_schema_version.as_str(),
                actual.trace_schema_version.as_str(),
            ),
            (
                "schema_compat_revision",
                self.schema_compat_revision.as_str(),
                actual.schema_compat_revision.as_str(),
            ),
            (
                "methodology_version",
                self.methodology_version.as_str(),
                actual.methodology_version.as_str(),
            ),
            (
                "export_schema_version",
                self.export_schema_version.as_str(),
                actual.export_schema_version.as_str(),
            ),
        ];
        pairs
            .into_iter()
            .filter(|(_, expected, actual)| expected != actual)
            .map(|(key, expected, actual)| VerifyMismatch {
                key: key.to_string(),
                expected: expected.to_string(),
                actual: actual.to_string(),
            })
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct VerifyMismatch {
    pub key: String,
    pub expected: String,
    pub actual: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct VerifyReport {
    pub ok: bool,
    pub lock: MemphantLock,
    pub current: MemphantLock,
    pub mismatches: Vec<VerifyMismatch>,
}

impl VerifyReport {
    pub fn from_lock(lock: MemphantLock) -> Self {
        let current = MemphantLock::current();
        let mismatches = lock.mismatches(&current);
        Self {
            ok: mismatches.is_empty(),
            lock,
            current,
            mismatches,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct HealthResponse {
    pub status: String,
    pub engine_version: String,
    pub trace_schema_version: String,
    pub schema_compat_revision: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RetainEpisodeHttpRequest {
    pub tenant_id: TenantId,
    pub scope_id: ScopeId,
    pub actor_id: ActorId,
    pub source_kind: String,
    pub source_trust: TrustLevel,
    pub subject_hint: Option<String>,
    pub body: String,
    pub compiler_version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RetainEpisodeHttpResponse {
    pub episode_id: EpisodeId,
    pub dedup: DedupOutcome,
    pub enqueued: Vec<String>,
    pub trace_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ReflectRequest {
    pub tenant_id: TenantId,
    pub scope_id: ScopeId,
    pub actor_id: ActorId,
    pub compiler_version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ReflectResult {
    pub reflect_id: String,
    pub episodes_consumed: usize,
    pub candidates_created: usize,
    pub trace_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RecallHttpRequest {
    pub tenant_id: TenantId,
    pub scope_id: ScopeId,
    pub actor_id: ActorId,
    pub allowed_scope_ids: Option<Vec<ScopeId>>,
    pub query: String,
    pub limit: Option<usize>,
    pub budget_tokens: Option<usize>,
    pub mode: Option<RecallMode>,
    pub include_beliefs: Option<bool>,
    pub edge_expansion_enabled: Option<bool>,
    pub include_trace: Option<bool>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CorrectSelector {
    pub memory_unit_id: UnitId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CorrectionPayload {
    pub value: String,
    pub reason: String,
    pub valid_from: Option<String>,
    pub valid_to: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CorrectRequest {
    pub tenant_id: TenantId,
    pub scope_id: ScopeId,
    pub actor_id: ActorId,
    pub selector: CorrectSelector,
    pub correction: CorrectionPayload,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CorrectResult {
    pub correction_id: String,
    pub superseded: Vec<UnitId>,
    pub created: Vec<UnitId>,
    pub correction_kind: String,
    pub trace_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ForgetSelector {
    pub memory_unit_id: Option<UnitId>,
    pub scope_id: Option<ScopeId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ForgetRequest {
    pub tenant_id: TenantId,
    pub scope_id: ScopeId,
    pub actor_id: ActorId,
    pub selector: ForgetSelector,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ForgetResult {
    pub deletion_generation: u64,
    pub policy: String,
    pub invalidated_units: Vec<UnitId>,
    pub verification: String,
    pub trace_ref: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MarkOutcome {
    Success,
    Failure,
    Corrected,
    Ignored,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct MarkRequest {
    pub tenant_id: TenantId,
    pub trace_id: TraceId,
    pub caller_id: String,
    pub used_ids: Vec<UnitId>,
    pub outcome: MarkOutcome,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TraceRequest {
    pub tenant_id: TenantId,
    pub trace_id: TraceId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct MarkResult {
    pub accepted: bool,
    pub trace_id: TraceId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ReviewEvent {
    pub tenant_id: TenantId,
    pub trace_id: TraceId,
    pub caller_id: String,
    pub used_ids: Vec<UnitId>,
    pub outcome: MarkOutcome,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ScopeMemoryResponse {
    pub tenant_id: TenantId,
    pub scope_id: ScopeId,
    pub items: Vec<StoredMemoryUnit>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ErrorBody {
    pub code: String,
    pub message: String,
    pub request_id: String,
    pub details: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ErrorEnvelope {
    pub error: ErrorBody,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct McpToolAnnotations {
    pub read_only_hint: bool,
    pub destructive_hint: bool,
    pub idempotent_hint: bool,
    pub open_world_hint: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct McpToolSpec {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
    pub output_schema: serde_json::Value,
    pub annotations: McpToolAnnotations,
}

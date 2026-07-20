use std::collections::BTreeMap;

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
id_type!(AgentNodeId);
id_type!(EdgeId);
id_type!(EpisodeId);
id_type!(JobId);
id_type!(ResourceId);
id_type!(ScopeId);
id_type!(SubjectId);
id_type!(TenantId);
id_type!(TraceId);
id_type!(UnitId);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ContextBindingEntityRef {
    pub external_ref: String,
    pub kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ContextBindingScopeRef {
    pub external_ref: String,
    pub kind: String,
    #[serde(default)]
    pub parent_external_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ContextBindingAgentRef {
    pub external_ref: String,
    #[serde(default)]
    pub parent_external_ref: Option<String>,
}

#[cfg(test)]
mod context_binding_contract_tests {
    use super::*;

    #[test]
    fn access_policy_is_tagged_and_rejects_legacy_level_controls() {
        let policy: ContextBindingAccessPolicy = serde_json::from_value(serde_json::json!({
            "mode": "grant",
            "source_scope_external_ref": "scope:source",
            "source_agent_node_external_ref": "agent:source",
            "kind": "semantic"
        }))
        .expect("strict grant policy");
        assert!(matches!(policy, ContextBindingAccessPolicy::Grant { .. }));

        let legacy = serde_json::from_value::<ContextBindingRequest>(serde_json::json!({
            "subject": {"external_ref": "user:1", "kind": "user"},
            "actor": {"external_ref": "user:1", "kind": "user"},
            "scope": {"external_ref": "scope:root", "kind": "user"},
            "agent_node": {"external_ref": "agent:root"},
            "inherit_to_descendants": [{"kind": "semantic", "max_agent_level": 1}]
        }));
        assert!(legacy.is_err());
    }

    #[test]
    fn aggregation_window_rejects_unknown_fields() {
        let window = serde_json::from_value::<AggregationWindow>(serde_json::json!({
            "from": "2030-01-01T00:00:00Z",
            "to": "2030-02-01T00:00:00Z",
            "engine_override": true
        }));
        assert!(window.is_err());
    }

    #[test]
    fn retain_rejects_server_derived_metadata() {
        let request = serde_json::from_value::<RetainEpisodeHttpRequest>(serde_json::json!({
            "subject_id": SubjectId::new(),
            "scope_id": ScopeId::new(),
            "actor_id": ActorId::new(),
            "agent_node_id": AgentNodeId::new(),
            "subject_generation": 0,
            "source_kind": "user",
            "source_trust": "trusted_system",
            "body": "hello"
        }));
        assert!(request.is_err());
    }

    #[test]
    fn retain_payload_is_tagged_strict_and_requires_provenance() {
        let base = serde_json::json!({
            "subject_id": SubjectId::new(),
            "scope_id": ScopeId::new(),
            "actor_id": ActorId::new(),
            "agent_node_id": AgentNodeId::new(),
            "subject_generation": 0,
            "source_ref": "source:episode:1",
            "observed_at": "2030-01-01T00:00:00Z",
            "payload": {"episode": {"source_kind": "user", "body": "hello"}}
        });
        assert!(serde_json::from_value::<RetainEpisodeHttpRequest>(base.clone()).is_ok());

        let mut legacy = base.clone();
        legacy.as_object_mut().unwrap().remove("payload");
        legacy["body"] = serde_json::json!("hello");
        assert!(serde_json::from_value::<RetainEpisodeHttpRequest>(legacy).is_err());

        let mut unknown = base.clone();
        unknown["payload"]["episode"]["subject_hint"] = serde_json::json!("legacy");
        assert!(serde_json::from_value::<RetainEpisodeHttpRequest>(unknown).is_err());

        for payload in [
            serde_json::json!({"resource": {
                "uri": "https://example.test/file", "mime_type": "text/plain",
                "content_hash": "sha256:abc", "body": "resource body"
            }}),
            serde_json::json!({"unit": {
                "kind": "semantic", "fact_key": "profile:city",
                "predicate": "lives_in", "body": "Lives in Lima", "confidence": 0.9
            }}),
        ] {
            let mut request = serde_json::json!({
                "subject_id": SubjectId::new(), "scope_id": ScopeId::new(),
                "actor_id": ActorId::new(), "agent_node_id": AgentNodeId::new(),
                "subject_generation": 0, "source_ref": "source:1",
                "observed_at": "2030-01-01T00:00:00Z"
            });
            request["payload"] = payload;
            assert!(serde_json::from_value::<RetainEpisodeHttpRequest>(request).is_ok());
        }

        let mut multiple = base.clone();
        multiple["payload"]["resource"] = serde_json::json!({
            "uri": "https://example.test/file", "mime_type": "text/plain",
            "content_hash": "sha256:abc"
        });
        assert!(serde_json::from_value::<RetainEpisodeHttpRequest>(multiple).is_err());

        let mut unknown_tag = base.clone();
        unknown_tag["payload"] = serde_json::json!({"legacy": {"body": "hello"}});
        assert!(serde_json::from_value::<RetainEpisodeHttpRequest>(unknown_tag).is_err());

        let unit = serde_json::json!({
            "subject_id": SubjectId::new(),
            "scope_id": ScopeId::new(),
            "actor_id": ActorId::new(),
            "agent_node_id": AgentNodeId::new(),
            "subject_generation": 0,
            "source_ref": "source:fact:1",
            "observed_at": "2030-01-01T00:00:00Z",
            "payload": {"unit": {
                "kind": "semantic", "fact_key": "profile:city",
                "predicate": "lives_in", "body": "Lives in Lima"
            }}
        });
        assert!(serde_json::from_value::<RetainEpisodeHttpRequest>(unit).is_err());

        let missing_fact_key = serde_json::json!({
            "subject_id": SubjectId::new(), "scope_id": ScopeId::new(),
            "actor_id": ActorId::new(), "agent_node_id": AgentNodeId::new(),
            "subject_generation": 0, "source_ref": "source:1",
            "observed_at": "2030-01-01T00:00:00Z",
            "payload": {"unit": {"kind": "semantic", "predicate": "is",
                "body": "A complete unit body", "confidence": 0.5}}
        });
        assert!(serde_json::from_value::<RetainEpisodeHttpRequest>(missing_fact_key).is_err());
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ContextBindingPolicyMode {
    Inherit,
    Grant,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum ContextBindingAccessPolicy {
    Inherit {
        source_scope_external_ref: String,
        source_agent_node_external_ref: String,
        kind: MemoryKind,
    },
    Grant {
        source_scope_external_ref: String,
        source_agent_node_external_ref: String,
        kind: MemoryKind,
    },
}

impl ContextBindingAccessPolicy {
    pub fn mode(&self) -> ContextBindingPolicyMode {
        match self {
            Self::Inherit { .. } => ContextBindingPolicyMode::Inherit,
            Self::Grant { .. } => ContextBindingPolicyMode::Grant,
        }
    }

    pub fn source_scope_external_ref(&self) -> &str {
        match self {
            Self::Inherit {
                source_scope_external_ref,
                ..
            }
            | Self::Grant {
                source_scope_external_ref,
                ..
            } => source_scope_external_ref,
        }
    }

    pub fn source_agent_node_external_ref(&self) -> &str {
        match self {
            Self::Inherit {
                source_agent_node_external_ref,
                ..
            }
            | Self::Grant {
                source_agent_node_external_ref,
                ..
            } => source_agent_node_external_ref,
        }
    }

    pub fn kind(&self) -> MemoryKind {
        match self {
            Self::Inherit { kind, .. } | Self::Grant { kind, .. } => *kind,
        }
    }
}

/// Central own-memory and explicit-grant kind matrix for a resolved agent.
/// L0 is the user-memory boundary; L1+ is restricted to agent-local families.
pub fn agent_level_allows_memory_kind(agent_level: u8, kind: MemoryKind) -> bool {
    agent_level == 0
        || matches!(
            kind,
            MemoryKind::Episodic | MemoryKind::Procedural | MemoryKind::Resource
        )
}

pub fn actor_kind_trust(kind: &str) -> TrustLevel {
    match kind {
        "user" => TrustLevel::TrustedUser,
        "system" => TrustLevel::TrustedSystem,
        "tool" => TrustLevel::UnverifiedTool,
        "web" => TrustLevel::WebContent,
        _ => TrustLevel::AgentOutput,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ContextBindingRequest {
    pub subject: ContextBindingEntityRef,
    pub actor: ContextBindingEntityRef,
    pub scope: ContextBindingScopeRef,
    pub agent_node: ContextBindingAgentRef,
    #[serde(default)]
    pub access_policies: Vec<ContextBindingAccessPolicy>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ContextBindingResponse {
    pub subject_id: SubjectId,
    pub actor_id: ActorId,
    pub scope_id: ScopeId,
    pub agent_node_id: AgentNodeId,
    pub agent_level: u8,
    pub policy_revision: String,
    pub subject_generation: u64,
}

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
    pub data_subject_id: SubjectId,
    pub scope_id: ScopeId,
    pub actor_id: ActorId,
    pub agent_node_id: AgentNodeId,
    pub subject_generation: u64,
    pub source_kind: String,
    pub source_ref: String,
    pub observed_at: String,
    pub source_trust: TrustLevel,
    pub subject_hint: Option<String>,
    #[serde(default)]
    pub subject: Option<String>,
    #[serde(default)]
    pub predicate: Option<String>,
    pub body: String,
    pub compiler_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RetainResourceRequest {
    pub tenant_id: TenantId,
    pub data_subject_id: SubjectId,
    pub scope_id: ScopeId,
    pub actor_id: ActorId,
    pub agent_node_id: AgentNodeId,
    pub subject_generation: u64,
    pub uri: String,
    pub source_ref: String,
    pub observed_at: String,
    #[serde(default)]
    pub kind: Option<ResourceKind>,
    pub content_hash: String,
    pub mime_type: String,
    #[serde(default)]
    pub revision: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
    pub source_trust: TrustLevel,
    pub compiler_version: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RecallMode {
    Fast,
    Balanced,
    Deep,
}

#[cfg(test)]
mod recall_mode_contract_tests {
    use super::RecallMode;

    #[test]
    fn deep_is_the_only_explicit_deliberate_recall_mode() {
        assert!(serde_json::from_str::<RecallMode>(r#""deep""#).is_ok());
        assert!(serde_json::from_str::<RecallMode>(r#""exhaustive""#).is_err());
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RecallChannel {
    Exact,
    Lexical,
    Vector,
    Temporal,
    Edge,
    Deep,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RecallRequest {
    pub context: ResolvedMemoryContext,
    pub query: String,
    pub k: usize,
    pub budget_tokens: usize,
    pub mode: RecallMode,
    pub include_beliefs: bool,
    #[serde(default = "default_true")]
    pub edge_expansion_enabled: bool,
    #[serde(default = "default_true")]
    pub context_packing_abstention_enabled: bool,
    #[serde(default = "default_true")]
    pub rerank_enabled: bool,
    #[serde(default)]
    pub learned_rerank_profile: Option<LearnedRerankProfile>,
    #[serde(default = "default_true")]
    pub query_decomposition_enabled: bool,
    #[serde(default = "default_true")]
    pub procedure_recall_enabled: bool,
    #[serde(default = "default_true")]
    pub decay_enabled: bool,
    pub engine_version: String,
    pub transaction_as_of: Option<String>,
    pub valid_at: Option<String>,
    #[serde(default)]
    pub aggregation_window: Option<AggregationWindow>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ResolvedMemoryContext {
    pub tenant_id: TenantId,
    pub data_subject_id: SubjectId,
    pub actor_id: ActorId,
    pub actor_trust: TrustLevel,
    pub scope_id: ScopeId,
    pub agent_node_id: AgentNodeId,
    pub agent_level: u8,
    pub subject_generation: u64,
    pub policy_revision: String,
    pub sources_by_kind: BTreeMap<MemoryKind, Vec<ResolvedMemorySource>>,
}

impl ResolvedMemoryContext {
    pub fn allows(&self, kind: MemoryKind, scope_id: ScopeId, agent_node_id: AgentNodeId) -> bool {
        self.sources_by_kind.get(&kind).is_some_and(|sources| {
            sources.contains(&ResolvedMemorySource {
                scope_id,
                agent_node_id,
            })
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResolvedMemorySource {
    pub scope_id: ScopeId,
    pub agent_node_id: AgentNodeId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AggregationWindow {
    /// Inclusive RFC 3339 lower bound supplied by the host.
    pub from: String,
    /// Exclusive RFC 3339 upper bound supplied by the host.
    pub to: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LearnedRerankProfile {
    pub profile_id: String,
    pub training_set_id: String,
    pub lexical_weight: f32,
    pub vector_weight: f32,
    pub exact_weight: f32,
    pub intent_weight: f32,
    pub decay_weight: f32,
    pub fused_weight: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RecallCandidateTrace {
    pub unit_id: UnitId,
    pub channel: RecallChannel,
    pub channel_rank: usize,
    pub channel_score: f32,
    pub derived_by: String,
    pub fused_rank: Option<usize>,
    pub fused_score: Option<f32>,
    pub rerank_rank: Option<usize>,
    pub rerank_score: f32,
    #[serde(default)]
    pub subquery_ids: Vec<String>,
    #[serde(default)]
    pub decay_retrievability: f32,
    #[serde(default)]
    pub dsr_stability_days: Option<f32>,
    #[serde(default)]
    pub dsr_difficulty: Option<f32>,
    #[serde(default)]
    pub dsr_reinforcement_count: u32,
    pub trust_level: TrustLevel,
    pub state: UnitState,
    pub discard_reason: Option<RecallDropReason>,
    pub valid_from: Option<String>,
    pub valid_to: Option<String>,
    pub transaction_from: Option<String>,
    pub transaction_to: Option<String>,
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
    #[serde(default)]
    pub derived_from_unit_ids: Vec<UnitId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ProcedureTraceFact {
    pub unit_id: UnitId,
    pub validation_state: String,
    pub signal_kind: String,
    pub safety_status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RecallContextItem {
    pub unit_id: UnitId,
    pub body: String,
    pub kind: MemoryKind,
    pub derived_by: String,
    pub inclusion_reason: String,
    pub citation_episode_id: Option<EpisodeId>,
    pub citation_resource_id: Option<ResourceId>,
    #[serde(default)]
    pub derived_from_unit_ids: Vec<UnitId>,
    pub suppression_labels: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RecallDroppedItem {
    pub unit_id: UnitId,
    pub reason: RecallDropReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CrossRerankFailure {
    None,
    Error,
    Empty,
    InvalidScoreCount,
    NonFiniteScore,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CrossRerankTrace {
    pub provider: String,
    pub model: String,
    pub candidate_limit: usize,
    pub candidate_count: usize,
    pub max_length: usize,
    pub batch_size: Option<usize>,
    pub input_chars_p50: usize,
    pub input_chars_p95: usize,
    pub input_chars_max: usize,
    pub failure: CrossRerankFailure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RecallDegradationReason {
    PendingReflectionReadYourOwnWrites,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DegradedRecallTraceItem {
    pub body: String,
    pub kind: MemoryKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RecallDegradationDiagnostic {
    pub reason: RecallDegradationReason,
    pub consolidation_lag_ms: u64,
    pub items: Vec<DegradedRecallTraceItem>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RetrievalTrace {
    pub id: TraceId,
    pub tenant_id: TenantId,
    pub data_subject_id: SubjectId,
    pub scope_id: ScopeId,
    pub actor_id: ActorId,
    pub agent_node_id: AgentNodeId,
    pub subject_generation: u64,
    pub policy_revision: String,
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
    /// R1.5-T0: the `recall_pool_depth` in effect for this recall — the ONE
    /// knob the vector/rerank/packing-scan internal fan-out derived from
    /// (never from `k`). Recorded per-trace so latency attribution stays
    /// observable across deployments that override
    /// `MEMPHANT_RECALL_POOL_DEPTH`. `#[serde(default)]` so traces recorded
    /// before this field existed still deserialize (as `0`, a visible "not
    /// recorded" sentinel — pool depth is never legitimately 0).
    #[serde(default)]
    pub recall_pool_depth: u32,
    /// R1.5-T1: per-recall wall-clock (ms) spent inside the W8 cross-encoder
    /// rerank stage ([`crate::CrossReranker`], distinct from the retired
    /// heuristic `reranker_id`/`rerank_input_count`/`rerank_overfetch_ratio`
    /// fields below). `0` when no cross-reranker is installed on the service
    /// (the default) or the candidate pool was empty — a legitimate "not
    /// run" value, not a sentinel. `#[serde(default)]` so traces recorded
    /// before this field existed still deserialize.
    #[serde(default)]
    pub cross_rerank_ms: u64,
    #[serde(default)]
    pub cross_rerank: Option<CrossRerankTrace>,
    pub consolidation_lag_ms: u64,
    #[serde(default)]
    pub degradation: Option<RecallDegradationDiagnostic>,
    pub weight_vector_id: String,
    pub mode_requested: RecallMode,
    pub mode_executed: RecallMode,
    pub escalation_reason: String,
    pub reranker_id: String,
    pub rerank_input_count: usize,
    pub rerank_overfetch_ratio: f32,
    #[serde(default)]
    pub learned_rerank_training_set_id: Option<String>,
    #[serde(default)]
    pub subquery_ids: Vec<String>,
    pub decomposition_reason: String,
    #[serde(default)]
    pub procedure_ids: Vec<UnitId>,
    #[serde(default)]
    pub procedure_validation_states: Vec<ProcedureTraceFact>,
    pub abstention_signal: bool,
    pub latency_ms: u64,
    pub token_estimate: usize,
    pub cost_micros: u64,
    #[serde(default)]
    pub decay_model_id: String,
    #[serde(default)]
    pub l4_sandbox_id: Option<String>,
    #[serde(default)]
    pub l4_gathered_evidence_ids: Vec<String>,
    pub recall_time: RecallTime,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RecallTime {
    pub evaluated_at: String,
    pub transaction_as_of: String,
    pub valid_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RecallResponse {
    pub trace_id: TraceId,
    pub items: Vec<RecallContextItem>,
    pub candidate_whitelist: Vec<UnitId>,
    pub citations: Vec<RecallCitation>,
    pub abstention: bool,
    pub degraded: bool,
    /// Non-zero when `degraded: true`: recall drew on raw un-reflected
    /// episodes because consolidation had not caught up (spec 08 §4).
    #[serde(default)]
    pub consolidation_lag_ms: u64,
    pub suppression_labels: Vec<String>,
    pub recall_time: RecallTime,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum MemoryKind {
    Episodic,
    Semantic,
    Procedural,
    Belief,
    Resource,
}

impl MemoryKind {
    pub const ALL: [Self; 5] = [
        Self::Episodic,
        Self::Semantic,
        Self::Procedural,
        Self::Belief,
        Self::Resource,
    ];
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
    pub data_subject_id: SubjectId,
    pub scope_id: ScopeId,
    pub actor_id: ActorId,
    pub agent_node_id: AgentNodeId,
    pub subject_generation: u64,
    pub source_kind: String,
    pub source_ref: String,
    pub observed_at: String,
    pub source_trust: TrustLevel,
    pub dedup_key: String,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct StoredEpisode {
    pub id: EpisodeId,
    pub tenant_id: TenantId,
    pub data_subject_id: SubjectId,
    pub scope_id: ScopeId,
    pub actor_id: ActorId,
    pub agent_node_id: AgentNodeId,
    pub subject_generation: u64,
    pub source_kind: String,
    pub source_ref: String,
    pub source_trust: TrustLevel,
    pub dedup_key: String,
    pub body: String,
    pub observation_count: u32,
    pub first_observed_at: String,
    pub last_observed_at: String,
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ResourceKind {
    Document,
    Code,
    Conversation,
    #[default]
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct NewResource {
    pub tenant_id: TenantId,
    pub data_subject_id: SubjectId,
    pub scope_id: ScopeId,
    pub actor_id: ActorId,
    pub agent_node_id: AgentNodeId,
    pub subject_generation: u64,
    pub uri: String,
    pub source_ref: String,
    pub observed_at: String,
    #[serde(default)]
    pub kind: ResourceKind,
    pub content_hash: String,
    pub mime_type: String,
    #[serde(default)]
    pub revision: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
    pub source_trust: TrustLevel,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct StoredResource {
    pub id: ResourceId,
    pub tenant_id: TenantId,
    pub data_subject_id: SubjectId,
    pub scope_id: ScopeId,
    pub actor_id: ActorId,
    pub agent_node_id: AgentNodeId,
    pub subject_generation: u64,
    pub uri: String,
    pub source_ref: String,
    pub observed_at: String,
    #[serde(default)]
    pub kind: ResourceKind,
    pub content_hash: String,
    pub mime_type: String,
    #[serde(default)]
    pub revision: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct NewMemoryUnit {
    pub tenant_id: TenantId,
    pub data_subject_id: SubjectId,
    pub scope_id: ScopeId,
    pub agent_node_id: AgentNodeId,
    pub subject_generation: u64,
    pub kind: MemoryKind,
    pub state: UnitState,
    pub fact_key: Option<String>,
    pub predicate: Option<String>,
    pub body: String,
    pub confidence: Option<f32>,
    pub trust_level: TrustLevel,
    pub churn_class: Option<String>,
    #[serde(default)]
    pub freshness_due_at: Option<String>,
    pub actor_id: Option<ActorId>,
    pub source_kind: Option<String>,
    pub source_ref: String,
    pub observed_at: String,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StoredMemoryUnit {
    pub id: UnitId,
    pub tenant_id: TenantId,
    pub data_subject_id: SubjectId,
    pub scope_id: ScopeId,
    pub agent_node_id: AgentNodeId,
    pub subject_generation: u64,
    pub kind: MemoryKind,
    pub state: UnitState,
    pub fact_key: Option<String>,
    pub predicate: Option<String>,
    pub body: String,
    pub confidence: Option<f32>,
    pub trust_level: TrustLevel,
    pub churn_class: Option<String>,
    #[serde(default)]
    pub freshness_due_at: Option<String>,
    pub actor_id: Option<ActorId>,
    pub source_kind: Option<String>,
    pub source_ref: String,
    pub observed_at: String,
    pub source_episode_id: Option<EpisodeId>,
    pub source_resource_id: Option<ResourceId>,
    pub deletion_generation: Option<u64>,
    pub contextual_chunks: Vec<ContextualChunk>,
    pub valid_from: Option<String>,
    pub valid_to: Option<String>,
    pub transaction_from: Option<String>,
    pub transaction_to: Option<String>,
    #[serde(default)]
    pub difficulty: Option<f32>,
    #[serde(default)]
    pub stability_days: Option<f32>,
    #[serde(default)]
    pub last_reinforced_at: Option<String>,
    #[serde(default)]
    pub reinforcement_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredCitation {
    pub id: Uuid,
    pub tenant_id: TenantId,
    pub data_subject_id: SubjectId,
    pub scope_id: ScopeId,
    pub agent_node_id: AgentNodeId,
    pub subject_generation: u64,
    pub memory_unit_id: UnitId,
    pub episode_id: Option<EpisodeId>,
    pub resource_id: Option<ResourceId>,
    pub span: Option<CitationSpan>,
    pub quote_hash: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RecordMaterial {
    pub unit: StoredMemoryUnit,
    pub citations: Vec<StoredCitation>,
    pub lineage: Vec<StoredMemoryEdge>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CitationSpan {
    pub start: u64,
    pub end: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CitationSource {
    Episode { id: EpisodeId },
    Resource { id: ResourceId },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MemoryCitation {
    pub source_ref: String,
    pub source: Option<CitationSource>,
    pub span: Option<CitationSpan>,
    pub quote_hash: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LineageRelation {
    Supersedes,
    SupersededBy,
    Contradicts,
    DerivedFrom,
    DerivationSourceFor,
    Cites,
    CitedBy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MemoryLineage {
    pub relation: LineageRelation,
    pub unit_id: UnitId,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MemoryRecord {
    pub id: UnitId,
    pub scope_id: ScopeId,
    pub kind: MemoryKind,
    pub state: UnitState,
    pub fact_key: Option<String>,
    pub predicate: Option<String>,
    pub body: String,
    pub confidence: Option<f32>,
    pub trust: TrustLevel,
    pub source_ref: String,
    pub observed_at: String,
    pub citations: Vec<MemoryCitation>,
    pub lineage: Vec<MemoryLineage>,
    pub valid_from: Option<String>,
    pub valid_to: Option<String>,
    pub transaction_from: Option<String>,
    pub transaction_to: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RecallItem {
    pub record: MemoryRecord,
    pub inclusion_reason: String,
    pub derived_by: String,
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
    pub transaction_from: Option<String>,
    pub transaction_to: Option<String>,
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
    ReflectScope,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ReflectJob {
    pub tenant_id: TenantId,
    pub data_subject_id: SubjectId,
    pub scope_id: ScopeId,
    pub actor_id: ActorId,
    pub agent_node_id: AgentNodeId,
    pub subject_generation: u64,
    pub episode_id: Option<EpisodeId>,
    pub resource_id: Option<ResourceId>,
    pub kind: ReflectJobKind,
    pub compiler_version: String,
    #[serde(default)]
    pub subject: Option<String>,
    #[serde(default)]
    pub predicate: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct QueuedReflectJob {
    pub id: JobId,
    pub tenant_id: TenantId,
    pub data_subject_id: SubjectId,
    pub scope_id: ScopeId,
    pub actor_id: ActorId,
    pub agent_node_id: AgentNodeId,
    pub subject_generation: u64,
    pub episode_id: Option<EpisodeId>,
    pub resource_id: Option<ResourceId>,
    pub kind: ReflectJobKind,
    pub compiler_version: String,
    #[serde(default)]
    pub subject: Option<String>,
    #[serde(default)]
    pub predicate: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ReflectCandidate {
    pub source_kind: String,
    pub trust_level: TrustLevel,
    pub actor_id: ActorId,
    pub subject: Option<String>,
    pub predicate: Option<String>,
    #[serde(default)]
    pub fact_key: Option<String>,
    /// Overrides the admission policy's minted kind (e.g. `resource` for
    /// resource-derived units, or a direct-unit retain's declared kind).
    #[serde(default)]
    pub kind: Option<MemoryKind>,
    pub body: String,
    #[serde(default)]
    pub confidence: Option<f32>,
    pub churn_class: Option<String>,
    pub admission_hint: Option<AdmissionAction>,
    #[serde(default)]
    pub contextual_chunks: Vec<ContextualChunk>,
    #[serde(default)]
    pub valid_from: Option<String>,
    #[serde(default)]
    pub valid_to: Option<String>,
    /// Structured-state mutation precondition. `Some([])` means create only
    /// when the subject is absent; non-empty lists bind replacement or
    /// invalidation to exact active units. Ordinary compiler candidates use
    /// `None`.
    #[serde(default)]
    pub target_unit_ids: Option<Vec<UnitId>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ReflectInput {
    pub tenant_id: TenantId,
    pub data_subject_id: SubjectId,
    pub scope_id: ScopeId,
    pub agent_node_id: AgentNodeId,
    pub subject_generation: u64,
    pub actor_id: ActorId,
    pub source_ref: String,
    pub observed_at: String,
    pub source_body: Option<String>,
    /// The source episode, when this compilation derives from one.
    pub episode_id: Option<EpisodeId>,
    /// The source resource, when this compilation derives from one.
    #[serde(default)]
    pub resource_id: Option<ResourceId>,
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
    pub episode_id: Option<EpisodeId>,
    #[serde(default)]
    pub resource_id: Option<ResourceId>,
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
    /// The active store backend: `postgres` or `memory`.
    #[serde(default)]
    pub store: String,
    /// Dead-lettered reflect jobs (attempts exhausted); `null` when the
    /// backend cannot report it cheaply.
    #[serde(default)]
    pub dead_letter_jobs: Option<u64>,
    pub engine_version: String,
    pub trace_schema_version: String,
    pub schema_compat_revision: String,
}

/// Resource payload for the retain verb (spec 08 §209 `resource` shape).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RetainResourcePayload {
    pub uri: String,
    pub mime_type: String,
    pub content_hash: String,
    #[serde(default)]
    pub kind: Option<ResourceKind>,
    /// Revision identity (e.g. a code commit hash).
    #[serde(default)]
    pub revision: Option<String>,
    /// Durable resource content the worker compiles from.
    #[serde(default)]
    pub body: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RetainEpisodePayload {
    pub source_kind: String,
    pub body: String,
}

/// Direct pre-compiled unit payload for trusted callers (spec 08 §209 `unit`
/// shape). Requires an explicit fact key, predicate, confidence, and kind; the admission
/// trust policy still applies.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RetainUnitPayload {
    pub kind: MemoryKind,
    pub fact_key: String,
    pub predicate: String,
    pub body: String,
    pub confidence: f32,
    #[serde(default)]
    pub valid_from: Option<String>,
    #[serde(default)]
    pub valid_to: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RetainPayload {
    Episode(RetainEpisodePayload),
    Resource(RetainResourcePayload),
    Unit(RetainUnitPayload),
}

/// The retain verb request with exactly one tagged episode, resource, or unit payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RetainEpisodeHttpRequest {
    pub subject_id: SubjectId,
    pub scope_id: ScopeId,
    pub actor_id: ActorId,
    pub agent_node_id: AgentNodeId,
    pub subject_generation: u64,
    pub source_ref: String,
    pub observed_at: String,
    pub payload: RetainPayload,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RetainEpisodeHttpResponse {
    #[serde(default)]
    pub episode_id: Option<EpisodeId>,
    #[serde(default)]
    pub resource_id: Option<ResourceId>,
    #[serde(default)]
    pub unit_ids: Vec<UnitId>,
    #[serde(default)]
    pub dedup: Option<DedupOutcome>,
    /// The trust tier actually assigned after clamping to the API key's
    /// `max_trust` ceiling.
    #[serde(default)]
    pub assigned_trust: Option<TrustLevel>,
    pub enqueued: Vec<String>,
    pub trace_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReflectRequest {
    pub subject_id: SubjectId,
    pub scope_id: ScopeId,
    pub actor_id: ActorId,
    pub agent_node_id: AgentNodeId,
    pub subject_generation: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReflectAccepted {
    pub job_id: JobId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RecallHttpRequest {
    pub subject_id: SubjectId,
    pub scope_id: ScopeId,
    pub actor_id: ActorId,
    pub agent_node_id: AgentNodeId,
    pub subject_generation: u64,
    pub query: String,
    pub limit: Option<usize>,
    pub budget_tokens: Option<usize>,
    pub mode: Option<RecallMode>,
    pub include_beliefs: Option<bool>,
    pub transaction_as_of: Option<String>,
    pub valid_at: Option<String>,
    #[serde(default)]
    pub aggregation_window: Option<AggregationWindow>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CorrectSelector {
    pub memory_unit_id: UnitId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CorrectionPayload {
    pub value: String,
    pub reason: String,
    pub source_ref: String,
    pub observed_at: String,
    #[serde(default)]
    pub valid_from: Option<String>,
    #[serde(default)]
    pub valid_to: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CorrectRequest {
    pub subject_id: SubjectId,
    pub scope_id: ScopeId,
    pub actor_id: ActorId,
    pub agent_node_id: AgentNodeId,
    pub subject_generation: u64,
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
#[serde(deny_unknown_fields)]
pub struct ForgetSelector {
    #[serde(default)]
    pub memory_unit_id: Option<UnitId>,
    #[serde(default)]
    pub episode_id: Option<EpisodeId>,
    #[serde(default)]
    pub resource_id: Option<ResourceId>,
    pub scope_id: ScopeId,
}

/// The single forget target named by a selector; exactly one of the three ids
/// must be present.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForgetTarget {
    MemoryUnit(UnitId),
    Episode(EpisodeId),
    Resource(ResourceId),
}

impl ForgetSelector {
    /// Validates the exactly-one-of contract and returns the named target.
    pub fn exactly_one_target(&self) -> Result<ForgetTarget, String> {
        let mut targets = Vec::new();
        if let Some(id) = self.memory_unit_id {
            targets.push(ForgetTarget::MemoryUnit(id));
        }
        if let Some(id) = self.episode_id {
            targets.push(ForgetTarget::Episode(id));
        }
        if let Some(id) = self.resource_id {
            targets.push(ForgetTarget::Resource(id));
        }
        match targets.as_slice() {
            [single] => Ok(*single),
            [] => Err(
                "forget selector must include exactly one of memory_unit_id, episode_id, resource_id"
                    .to_string(),
            ),
            _ => Err(
                "forget selector must include exactly one of memory_unit_id, episode_id, resource_id (got multiple)"
                    .to_string(),
            ),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ForgetRequest {
    pub subject_id: SubjectId,
    pub scope_id: ScopeId,
    pub actor_id: ActorId,
    pub agent_node_id: AgentNodeId,
    pub subject_generation: u64,
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
#[serde(deny_unknown_fields)]
pub struct MarkRequest {
    pub subject_id: SubjectId,
    pub scope_id: ScopeId,
    pub actor_id: ActorId,
    pub agent_node_id: AgentNodeId,
    pub subject_generation: u64,
    pub trace_id: TraceId,
    pub caller_id: String,
    pub used_ids: Vec<UnitId>,
    pub outcome: MarkOutcome,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TraceRequest {
    pub subject_id: SubjectId,
    pub scope_id: ScopeId,
    pub actor_id: ActorId,
    pub agent_node_id: AgentNodeId,
    pub subject_generation: u64,
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
    pub recorded_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
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

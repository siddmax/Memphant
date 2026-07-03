use serde::{Deserialize, Serialize};
use uuid::Uuid;

macro_rules! id_type {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
id_type!(ScopeId);
id_type!(TenantId);
id_type!(TraceId);
id_type!(UnitId);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScopeRef {
    pub kind: String,
    pub external_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetainInput {
    pub scope: ScopeRef,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetainResult {
    pub retained: bool,
    pub extracted_values: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryKind {
    Episodic,
    Semantic,
    Procedural,
    Belief,
    Resource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewEpisode {
    pub tenant_id: TenantId,
    pub scope_id: ScopeId,
    pub actor_id: ActorId,
    pub source_kind: String,
    pub source_trust: TrustLevel,
    pub dedup_key: String,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewMemoryUnit {
    pub tenant_id: TenantId,
    pub scope_id: ScopeId,
    pub kind: MemoryKind,
    pub state: UnitState,
    pub subject_key: Option<String>,
    pub body: String,
    pub trust_level: TrustLevel,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryEdgeKind {
    Supersedes,
    Contradicts,
    DerivedFrom,
    Cites,
    SameSubject,
    DependsOn,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredMemoryEdge {
    pub id: EdgeId,
    pub tenant_id: TenantId,
    pub scope_id: ScopeId,
    pub src_id: UnitId,
    pub dst_id: UnitId,
    pub kind: MemoryEdgeKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdmissionAction {
    Reject,
    Append,
    Merge,
    Supersede,
    Invalidate,
    Quarantine,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReflectJobKind {
    ReflectEpisode,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReflectJob {
    pub tenant_id: TenantId,
    pub scope_id: ScopeId,
    pub episode_id: EpisodeId,
    pub kind: ReflectJobKind,
    pub compiler_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueuedReflectJob {
    pub id: JobId,
    pub tenant_id: TenantId,
    pub scope_id: ScopeId,
    pub episode_id: EpisodeId,
    pub kind: ReflectJobKind,
    pub compiler_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReflectCandidate {
    pub source_kind: String,
    pub trust_level: TrustLevel,
    pub actor_id: ActorId,
    pub subject: Option<String>,
    pub predicate: Option<String>,
    pub body: String,
    pub churn_class: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReflectInput {
    pub tenant_id: TenantId,
    pub scope_id: ScopeId,
    pub actor_id: ActorId,
    pub episode_id: EpisodeId,
    pub job_id: JobId,
    pub compiler_version: String,
    pub candidates: Vec<ReflectCandidate>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReflectStageFact {
    pub stage: String,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DedupOutcome {
    pub matched: bool,
    pub observation_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetainOutcome {
    pub episode_id: EpisodeId,
    pub dedup: DedupOutcome,
}

#![allow(async_fn_in_trait)]

pub mod service;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::future::Future;
use std::sync::{Arc, Mutex};

use fsrs::{FSRS, FSRS6_DEFAULT_DECAY, MemoryState, current_retrievability};
use memphant_types::{
    ActorId, AdmissionAction, ContextualChunk, CorrectRequest, CorrectResult, CorrectSelector,
    CorrectionPayload, DedupOutcome, EdgeId, EpisodeId, ForgetRequest, ForgetResult, ForgetTarget,
    JobId, LearnedRerankProfile, MarkOutcome, MarkRequest, MarkResult, MemoryEdgeKind, MemoryKind,
    NewEpisode, NewMemoryEdge, NewMemoryUnit, ProcedureTraceFact, QueuedReflectJob,
    RecallCandidateTrace, RecallChannel, RecallCitation, RecallContextItem, RecallDropReason,
    RecallDroppedItem, RecallMode, RecallPolicyFilter, RecallRequest, RecallResponse, ReflectInput,
    ReflectJob, ReflectJobKind, ReflectStageFact, ReflectTrace, RetainInput, RetainOutcome,
    RetainRequest, RetainResourceOutcome, RetainResourceRequest, RetainResult, RetrievalTrace,
    ReviewEvent, ScopeId, StoredEpisode, StoredMemoryEdge, StoredMemoryUnit, StoredResource,
    TenantId, TraceId, TrustLevel, UnitId, UnitState,
};
use memphant_types::{NewResource, ResourceExtractorState, ResourceId};
use sha2::{Digest, Sha256};
use uuid::Uuid;

const DECAY_MODEL_ID: &str = "fixed-prior-dsr-v1";
const L4_SANDBOX_ID: &str = "deterministic-local-l4-v1";
const DEFAULT_STABILITY_DAYS: f32 = 7.0;
const DEFAULT_DIFFICULTY: f32 = 5.0;
/// Jobs whose claim attempts reach this count are dead-lettered and never
/// re-claimed.
pub const JOB_DEAD_LETTER_ATTEMPTS: u32 = 5;

/// A source of the current instant. Injected everywhere the engine stamps or
/// compares time — `SystemClock` in production, `FixedClock` in tests. There
/// is no build-time clock constant.
pub trait Clock: Send + Sync {
    fn now(&self) -> jiff::Timestamp;

    fn now_rfc3339(&self) -> String {
        fmt_rfc3339(self.now())
    }
}

/// Production clock.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> jiff::Timestamp {
        jiff::Timestamp::now()
    }
}

/// Deterministic test clock pinned to an RFC 3339 instant.
#[derive(Debug, Clone, Copy)]
pub struct FixedClock(pub &'static str);

impl Clock for FixedClock {
    fn now(&self) -> jiff::Timestamp {
        self.0
            .parse()
            .unwrap_or_else(|error| panic!("FixedClock({}) is not RFC 3339: {error}", self.0))
    }
}

/// The one canonical timestamp serialization: RFC 3339 UTC with a `Z` suffix.
pub fn fmt_rfc3339(instant: jiff::Timestamp) -> String {
    instant.to_string()
}

/// Parsing timestamp comparison — never lexical. Unparseable inputs sort
/// before parseable ones (and fall back to byte order among themselves) so a
/// malformed stamp can never masquerade as "newer".
pub fn cmp_rfc3339(left: &str, right: &str) -> std::cmp::Ordering {
    let left_ts: Result<jiff::Timestamp, _> = left.parse();
    let right_ts: Result<jiff::Timestamp, _> = right.parse();
    match (left_ts, right_ts) {
        (Ok(left), Ok(right)) => left.cmp(&right),
        (Ok(_), Err(_)) => std::cmp::Ordering::Greater,
        (Err(_), Ok(_)) => std::cmp::Ordering::Less,
        (Err(_), Err(_)) => left.cmp(right),
    }
}

#[derive(Debug, thiserror::Error)]
pub enum EmbedError {
    #[error("embedding provider unavailable: {0}")]
    Unavailable(String),
}

/// Text embedding seam. The default runtime uses `NoopEmbedding`, in which
/// case the recall `vector` channel is traced as disabled rather than faked.
pub trait EmbeddingProvider: Send + Sync {
    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbedError>;
    fn dimensions(&self) -> usize;
    fn id(&self) -> &str;
}

/// No-embedding provider: produces no vectors and disables the vector channel.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoopEmbedding;

impl EmbeddingProvider for NoopEmbedding {
    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbedError> {
        Ok(vec![Vec::new(); texts.len()])
    }

    fn dimensions(&self) -> usize {
        0
    }

    fn id(&self) -> &str {
        "noop"
    }
}

/// Deterministic hash-bucket embedding for tests: each token is hashed into a
/// bucket, the bag-of-buckets vector is L2-normalized. Identical texts embed
/// identically; token-overlapping texts have positive cosine similarity. No
/// model download, no network — test-support only, never a semantic model.
#[derive(Debug, Clone, Copy)]
pub struct StubEmbedding {
    pub dimensions: usize,
}

impl Default for StubEmbedding {
    fn default() -> Self {
        Self { dimensions: 32 }
    }
}

impl EmbeddingProvider for StubEmbedding {
    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbedError> {
        Ok(texts
            .iter()
            .map(|text| {
                let dimensions = self.dimensions.max(1);
                let mut vec = vec![0.0_f32; dimensions];
                for token in tokenize(text) {
                    let hash = token
                        .bytes()
                        .fold(1_469_598_103_934_665_603_u64, |hash, byte| {
                            (hash ^ u64::from(byte)).wrapping_mul(1_099_511_628_211)
                        });
                    vec[(hash % dimensions as u64) as usize] += 1.0;
                }
                let norm = vec.iter().map(|value| value * value).sum::<f32>().sqrt();
                if norm > 0.0 {
                    for value in &mut vec {
                        *value /= norm;
                    }
                }
                vec
            })
            .collect())
    }

    fn dimensions(&self) -> usize {
        self.dimensions
    }

    fn id(&self) -> &str {
        "stub"
    }
}

/// Cosine similarity between two vectors (0.0 on dimension mismatch or zero
/// norm) — the in-memory analogue of pgvector's `<=>` cosine operator.
pub fn cosine_similarity(left: &[f32], right: &[f32]) -> f32 {
    if left.len() != right.len() || left.is_empty() {
        return 0.0;
    }
    let dot: f32 = left.iter().zip(right).map(|(a, b)| a * b).sum();
    let norm_left = left.iter().map(|value| value * value).sum::<f32>().sqrt();
    let norm_right = right.iter().map(|value| value * value).sum::<f32>().sqrt();
    if norm_left <= 0.0 || norm_right <= 0.0 {
        return 0.0;
    }
    dot / (norm_left * norm_right)
}

/// The nearest-neighbour fan-out for the recall vector channel: how many
/// units the store's vector query returns per recall. Matches the historical
/// pgvector `<=>` top-K.
pub const VECTOR_CANDIDATE_LIMIT: usize = 32;

/// The active vector query for recall: the embedded query plus the embedding
/// profile id its stored counterparts must match. The profile predicate is
/// mandatory (spec 03 — the `<=>` path filters `embedding_profile_id = $pid`,
/// else the per-profile partial index is skipped and cross-dimension vectors
/// are compared); the store threads it into its vector-family query.
#[derive(Debug, Clone, Copy)]
pub struct VectorQuery<'a> {
    pub vec: &'a [f32],
    pub profile_id: Uuid,
}

/// One `embedding_profile` row: the provider identity every stored embedding
/// FKs. Seeded (idempotent upsert) before embeddings are written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbeddingProfileRow {
    pub id: Uuid,
    pub provider: String,
    pub model: String,
    pub dimensions: usize,
    pub distance: String,
    pub version: String,
    pub index_strategy: String,
}

/// The deterministic profile row for a provider: the id is derived from
/// (provider id, dimensions) so every service instance seeds the same row.
pub fn embedding_profile_for(embedder: &dyn EmbeddingProvider) -> EmbeddingProfileRow {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(b"memphant-embedding-profile:");
    hasher.update(embedder.id().as_bytes());
    hasher.update(b":");
    hasher.update(embedder.dimensions().to_string().as_bytes());
    let digest = hasher.finalize();
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    EmbeddingProfileRow {
        id: Uuid::from_bytes(bytes),
        provider: embedder.id().to_string(),
        model: embedder.id().to_string(),
        dimensions: embedder.dimensions(),
        distance: "cosine".to_string(),
        version: "1".to_string(),
        index_strategy: "exact".to_string(),
    }
}

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("transaction already committed")]
    TransactionAlreadyCommitted,
    #[error("store mutex poisoned")]
    Poisoned,
    #[error("not found: {0}")]
    NotFound(&'static str),
    #[error("backend error: {0}")]
    Backend(String),
}

/// Review-event row shape shared by the store seam; identical to the public
/// `ReviewEvent` DTO.
pub type ReviewEventRow = ReviewEvent;

/// Filter for claiming reflect jobs. The public reflect endpoint claims with a
/// tenant+scope filter; the background worker claims unfiltered.
#[derive(Debug, Clone, Copy, Default)]
pub struct JobFilter {
    pub tenant: Option<TenantId>,
    pub scope: Option<ScopeId>,
}

/// A claimed reflect job with its claim bookkeeping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReflectJobRow {
    pub job: QueuedReflectJob,
    pub attempts: u32,
}

/// A correction applied through the store seam. `now` is the injected clock's
/// canonical instant — stores never consult wall time for bitemporal stamps.
#[derive(Debug, Clone)]
pub struct CorrectionWrite {
    pub scope_id: ScopeId,
    pub actor_id: ActorId,
    pub selector: CorrectSelector,
    pub correction: CorrectionPayload,
    pub now: String,
}

pub type CorrectOutcome = CorrectResult;

/// A forget applied through the store seam; exactly one target, validated
/// upstream via `ForgetSelector::exactly_one_target`.
#[derive(Debug, Clone, Copy)]
pub struct ForgetWrite {
    pub scope_id: ScopeId,
    pub actor_id: ActorId,
    pub target: ForgetTarget,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForgetOutcome {
    pub deletion_generation: u64,
    pub invalidated_units: Vec<UnitId>,
}

/// A state transition for an existing memory unit, produced by the pure
/// reflect computation and applied by the store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnitUpdate {
    pub id: UnitId,
    pub state: UnitState,
    pub transaction_to: Option<String>,
}

/// The full output of one reflect compilation. `persist_compiled_units`
/// consults forgotten-source tombstones and refuses re-derivation from
/// forgotten episodes/resources/units.
#[derive(Debug, Clone)]
pub struct CompiledWrite {
    pub scope_id: ScopeId,
    pub job_id: JobId,
    pub compiler_version: String,
    pub new_units: Vec<StoredMemoryUnit>,
    pub new_edges: Vec<StoredMemoryEdge>,
    pub unit_updates: Vec<UnitUpdate>,
    pub trace: ReflectTrace,
}

/// One page of a tenant-bound scope memory export.
#[derive(Debug, Clone, PartialEq)]
pub struct ScopePage {
    pub items: Vec<StoredMemoryUnit>,
    pub next_cursor: Option<UnitId>,
    pub has_more: bool,
}

/// An embedding row keyed to a memory unit and embedding profile.
#[derive(Debug, Clone, PartialEq)]
pub struct EmbeddingRow {
    pub memory_unit_id: UnitId,
    pub embedding_profile_id: Uuid,
    pub vec: Vec<f32>,
}

/// An API key row: the tenant binding + trust ceiling resolved at the edge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiKeyRow {
    pub id: Uuid,
    pub tenant_id: TenantId,
    pub key_hash: String,
    pub label: String,
    pub max_trust: TrustLevel,
    pub revoked: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("retain body cannot be empty")]
    EmptyBody,
    #[error("not found: {0}")]
    NotFound(String),
    #[error("invalid request: {0}")]
    Invalid(String),
    #[error("policy denied: {0}")]
    PolicyDenied(String),
    #[error(transparent)]
    Store(#[from] StoreError),
}

pub fn retain(input: RetainInput) -> Result<RetainResult, CoreError> {
    if input.body.trim().is_empty() {
        return Err(CoreError::EmptyBody);
    }

    Ok(RetainResult {
        retained: true,
        extracted_values: Vec::new(),
    })
}

/// The full repository seam. Native AFIT: not object-safe by design —
/// dispatch statically (`MemoryService<S: MemoryStore>` / an `AnyStore` enum).
pub trait MemoryStore: Send + Sync {
    type Txn: Send;

    // Staged-write API.
    fn begin(&self) -> impl Future<Output = Self::Txn> + Send;
    fn commit(&self, tx: Self::Txn) -> impl Future<Output = Result<(), StoreError>> + Send;
    fn stage_episode(
        &self,
        tx: &mut Self::Txn,
        episode: NewEpisode,
    ) -> impl Future<Output = Result<RetainOutcome, StoreError>> + Send;
    fn stage_memory_unit(
        &self,
        tx: &mut Self::Txn,
        unit: NewMemoryUnit,
    ) -> impl Future<Output = Result<UnitId, StoreError>> + Send;
    fn stage_resource(
        &self,
        tx: &mut Self::Txn,
        resource: NewResource,
    ) -> impl Future<Output = Result<ResourceId, StoreError>> + Send;
    fn stage_memory_edge(
        &self,
        tx: &mut Self::Txn,
        edge: NewMemoryEdge,
    ) -> impl Future<Output = Result<EdgeId, StoreError>> + Send;
    fn enqueue_reflect(
        &self,
        tx: &mut Self::Txn,
        job: ReflectJob,
    ) -> impl Future<Output = Result<JobId, StoreError>> + Send;

    // Read seam. The candidate set is the UNION of FTS top-N, most-recent-M
    // per scope and exact-subject matches — deduped by id. The vector family
    // is a separate query (`fetch_vector_candidates`) so it can carry the
    // `<=>` distance back to the core fusion. The in-memory store returns all
    // in-scope units.
    fn fetch_recall_candidates(
        &self,
        tenant: TenantId,
        scopes: &[ScopeId],
        kinds: &[MemoryKind],
        query_terms: &[String],
        limit: usize,
    ) -> impl Future<Output = Result<Vec<StoredMemoryUnit>, StoreError>> + Send;
    /// The recall vector family: the nearest units to `query_vec` under the
    /// ACTIVE embedding profile, each with its cosine DISTANCE (pgvector `<=>`;
    /// the in-memory store returns `1 - cosine`). Core scores the vector
    /// channel as `1 - distance` and folds these units into the candidate
    /// union. Filtering by `profile_id` is mandatory — mixing embeddings across
    /// profiles/dimensions is incoherent (spec 03).
    fn fetch_vector_candidates(
        &self,
        tenant: TenantId,
        scopes: &[ScopeId],
        kinds: &[MemoryKind],
        query_vec: &[f32],
        profile_id: Uuid,
        limit: usize,
    ) -> impl Future<Output = Result<Vec<(StoredMemoryUnit, f32)>, StoreError>> + Send;
    fn fetch_units_by_ids(
        &self,
        tenant: TenantId,
        ids: &[UnitId],
    ) -> impl Future<Output = Result<Vec<StoredMemoryUnit>, StoreError>> + Send;
    fn fetch_edges(
        &self,
        tenant: TenantId,
        unit_ids: &[UnitId],
    ) -> impl Future<Output = Result<Vec<StoredMemoryEdge>, StoreError>> + Send;
    fn fetch_review_events(
        &self,
        tenant: TenantId,
        unit_ids: &[UnitId],
    ) -> impl Future<Output = Result<Vec<ReviewEventRow>, StoreError>> + Send;
    fn fetch_episodes_for_scope(
        &self,
        tenant: TenantId,
        scope: ScopeId,
        limit: usize,
    ) -> impl Future<Output = Result<Vec<StoredEpisode>, StoreError>> + Send;
    fn pending_job_count(
        &self,
        tenant: TenantId,
        scope: ScopeId,
    ) -> impl Future<Output = Result<usize, StoreError>> + Send;
    fn fetch_episode(
        &self,
        tenant: TenantId,
        id: EpisodeId,
    ) -> impl Future<Output = Result<Option<StoredEpisode>, StoreError>> + Send;
    fn fetch_resource(
        &self,
        tenant: TenantId,
        id: ResourceId,
    ) -> impl Future<Output = Result<Option<StoredResource>, StoreError>> + Send;

    // Mutation seam.
    fn apply_correction(
        &self,
        tenant: TenantId,
        correction: CorrectionWrite,
    ) -> impl Future<Output = Result<CorrectOutcome, StoreError>> + Send;
    fn apply_forget(
        &self,
        tenant: TenantId,
        forget: ForgetWrite,
    ) -> impl Future<Output = Result<ForgetOutcome, StoreError>> + Send;
    fn record_review_events(
        &self,
        tenant: TenantId,
        events: Vec<ReviewEventRow>,
    ) -> impl Future<Output = Result<(), StoreError>> + Send;
    fn store_trace(
        &self,
        tenant: TenantId,
        trace: RetrievalTrace,
    ) -> impl Future<Output = Result<(), StoreError>> + Send;
    /// TENANT-BOUND: a trace id from another tenant resolves to `None`.
    fn trace_by_id(
        &self,
        tenant: TenantId,
        id: TraceId,
    ) -> impl Future<Output = Result<Option<RetrievalTrace>, StoreError>> + Send;
    fn scope_memory_page(
        &self,
        tenant: TenantId,
        scope: ScopeId,
        cursor: Option<UnitId>,
        limit: usize,
    ) -> impl Future<Output = Result<ScopePage, StoreError>> + Send;

    // Reflect job queue (SKIP LOCKED semantics in Postgres).
    fn claim_reflect_jobs(
        &self,
        filter: JobFilter,
        limit: usize,
    ) -> impl Future<Output = Result<Vec<ReflectJobRow>, StoreError>> + Send;
    fn complete_reflect_job(
        &self,
        id: JobId,
    ) -> impl Future<Output = Result<(), StoreError>> + Send;
    /// Persists one reflect compilation. MUST consult forgotten-source
    /// tombstones and refuse re-derivation of units from forgotten sources.
    fn persist_compiled_units(
        &self,
        tenant: TenantId,
        write: CompiledWrite,
    ) -> impl Future<Output = Result<(), StoreError>> + Send;
    /// Idempotency lookup for reflect compilations keyed by
    /// (job_id, compiler_version).
    fn fetch_reflect_trace(
        &self,
        tenant: TenantId,
        job_id: JobId,
        compiler_version: &str,
    ) -> impl Future<Output = Result<Option<ReflectTrace>, StoreError>> + Send;

    fn upsert_embeddings(
        &self,
        tenant: TenantId,
        rows: Vec<EmbeddingRow>,
    ) -> impl Future<Output = Result<(), StoreError>> + Send;
    /// Idempotently seeds the `embedding_profile` row every embedding FKs.
    fn upsert_embedding_profile(
        &self,
        tenant: TenantId,
        profile: EmbeddingProfileRow,
    ) -> impl Future<Output = Result<(), StoreError>> + Send;
    /// Embedding rows for the given units (all profiles).
    fn fetch_embeddings(
        &self,
        tenant: TenantId,
        unit_ids: &[UnitId],
    ) -> impl Future<Output = Result<Vec<EmbeddingRow>, StoreError>> + Send;
    fn lookup_api_key(
        &self,
        key_hash: &str,
    ) -> impl Future<Output = Result<Option<ApiKeyRow>, StoreError>> + Send;

    /// Backend liveness probe (`select 1` in Postgres; always healthy for the
    /// in-memory store).
    fn ping(&self) -> impl Future<Output = Result<(), StoreError>> + Send;
    /// Reflect jobs dead-lettered after exhausting their claim attempts.
    fn dead_letter_count(&self) -> impl Future<Output = Result<u64, StoreError>> + Send;
}

#[derive(Clone, Default)]
pub struct InMemoryStore {
    inner: Arc<Mutex<InMemoryState>>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct JobMeta {
    attempts: u32,
    claimed: bool,
    completed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum SourceKindKey {
    Episode,
    Resource,
    MemoryUnit,
}

#[derive(Default)]
struct InMemoryState {
    episodes: HashMap<TenantId, Vec<StoredEpisode>>,
    resources: HashMap<TenantId, Vec<StoredResource>>,
    memory_units: HashMap<TenantId, Vec<StoredMemoryUnit>>,
    memory_edges: HashMap<TenantId, Vec<StoredMemoryEdge>>,
    reflect_jobs: HashMap<TenantId, Vec<QueuedReflectJob>>,
    reflect_traces: HashMap<TenantId, Vec<ReflectTrace>>,
    retrieval_traces: HashMap<TenantId, Vec<RetrievalTrace>>,
    review_events: HashMap<TenantId, Vec<ReviewEvent>>,
    forgotten_sources: HashSet<(TenantId, SourceKindKey, Uuid)>,
    api_keys: Vec<ApiKeyRow>,
    embeddings: HashMap<TenantId, Vec<EmbeddingRow>>,
    embedding_profiles: HashMap<TenantId, Vec<EmbeddingProfileRow>>,
    job_meta: HashMap<JobId, JobMeta>,
    deletion_generation: u64,
}

impl InMemoryState {
    fn is_forgotten_source(&self, tenant_id: TenantId, unit: &StoredMemoryUnit) -> bool {
        if let Some(episode_id) = unit.source_episode_id
            && self.forgotten_sources.contains(&(
                tenant_id,
                SourceKindKey::Episode,
                episode_id.as_uuid(),
            ))
        {
            return true;
        }
        if let Some(resource_id) = unit.source_resource_id
            && self.forgotten_sources.contains(&(
                tenant_id,
                SourceKindKey::Resource,
                resource_id.as_uuid(),
            ))
        {
            return true;
        }
        self.forgotten_sources
            .contains(&(tenant_id, SourceKindKey::MemoryUnit, unit.id.as_uuid()))
    }
}

#[derive(Default)]
pub struct InMemoryTxn {
    episodes: Vec<StoredEpisode>,
    episode_observation_updates: Vec<(TenantId, EpisodeId)>,
    resources: Vec<StoredResource>,
    memory_units: Vec<StoredMemoryUnit>,
    memory_edges: Vec<StoredMemoryEdge>,
    reflect_jobs: Vec<QueuedReflectJob>,
    committed: bool,
}

impl InMemoryStore {
    pub fn episodes(&self, tenant_id: TenantId) -> Vec<StoredEpisode> {
        self.inner
            .lock()
            .map(|state| state.episodes.get(&tenant_id).cloned().unwrap_or_default())
            .unwrap_or_default()
    }

    pub fn memory_units(&self, tenant_id: TenantId) -> Vec<StoredMemoryUnit> {
        self.inner
            .lock()
            .map(|state| {
                state
                    .memory_units
                    .get(&tenant_id)
                    .cloned()
                    .unwrap_or_default()
            })
            .unwrap_or_default()
    }

    pub fn resources(&self, tenant_id: TenantId) -> Vec<StoredResource> {
        self.inner
            .lock()
            .map(|state| state.resources.get(&tenant_id).cloned().unwrap_or_default())
            .unwrap_or_default()
    }

    pub fn reflect_jobs(&self, tenant_id: TenantId) -> Vec<QueuedReflectJob> {
        self.inner
            .lock()
            .map(|state| {
                state
                    .reflect_jobs
                    .get(&tenant_id)
                    .cloned()
                    .unwrap_or_default()
            })
            .unwrap_or_default()
    }

    pub fn active_semantic_units(&self, tenant_id: TenantId) -> Vec<StoredMemoryUnit> {
        self.memory_units(tenant_id)
            .into_iter()
            .filter(|unit| unit.kind == MemoryKind::Semantic && unit.state == UnitState::Active)
            .collect()
    }

    pub fn belief_units(&self, tenant_id: TenantId) -> Vec<StoredMemoryUnit> {
        self.memory_units(tenant_id)
            .into_iter()
            .filter(|unit| unit.kind == MemoryKind::Belief && unit.state != UnitState::Quarantined)
            .collect()
    }

    pub fn quarantined_units(&self, tenant_id: TenantId) -> Vec<StoredMemoryUnit> {
        self.memory_units(tenant_id)
            .into_iter()
            .filter(|unit| unit.state == UnitState::Quarantined)
            .collect()
    }

    pub fn freshness_due_units(&self, tenant_id: TenantId) -> Vec<StoredMemoryUnit> {
        self.memory_units(tenant_id)
            .into_iter()
            .filter(|unit| unit.freshness_due_at.is_some() && unit.state == UnitState::Active)
            .collect()
    }

    /// Registers an API key row for tests / dev provisioning.
    pub fn insert_api_key(&self, row: ApiKeyRow) {
        if let Ok(mut state) = self.inner.lock() {
            state
                .api_keys
                .retain(|existing| existing.key_hash != row.key_hash);
            state.api_keys.push(row);
        }
    }

    /// Claim attempts recorded for a reflect job (test observability).
    pub fn job_attempts(&self, job_id: JobId) -> u32 {
        self.inner
            .lock()
            .ok()
            .and_then(|state| state.job_meta.get(&job_id).map(|meta| meta.attempts))
            .unwrap_or(0)
    }

    pub fn memory_edges(&self, tenant_id: TenantId) -> Vec<StoredMemoryEdge> {
        self.inner
            .lock()
            .map(|state| {
                state
                    .memory_edges
                    .get(&tenant_id)
                    .cloned()
                    .unwrap_or_default()
            })
            .unwrap_or_default()
    }

    pub fn reflect_traces(&self, tenant_id: TenantId) -> Vec<ReflectTrace> {
        self.inner
            .lock()
            .map(|state| {
                state
                    .reflect_traces
                    .get(&tenant_id)
                    .cloned()
                    .unwrap_or_default()
            })
            .unwrap_or_default()
    }

    pub fn retrieval_traces(&self, tenant_id: TenantId) -> Vec<RetrievalTrace> {
        self.inner
            .lock()
            .map(|state| {
                state
                    .retrieval_traces
                    .get(&tenant_id)
                    .cloned()
                    .unwrap_or_default()
            })
            .unwrap_or_default()
    }

    /// UNTENANTED trace lookup for the pre-auth REST surface; the tenant-bound
    /// path is `MemoryStore::trace_by_id`.
    pub fn trace_by_id_any_tenant(&self, trace_id: TraceId) -> Option<RetrievalTrace> {
        self.inner.lock().ok().and_then(|state| {
            state
                .retrieval_traces
                .values()
                .flat_map(|traces| traces.iter())
                .find(|trace| trace.id == trace_id)
                .cloned()
        })
    }

    pub fn scope_memory(&self, tenant_id: TenantId, scope_id: ScopeId) -> Vec<StoredMemoryUnit> {
        self.memory_units(tenant_id)
            .into_iter()
            .filter(|unit| unit.scope_id == scope_id)
            .collect()
    }

    pub fn review_events(&self, tenant_id: TenantId) -> Vec<ReviewEvent> {
        self.inner
            .lock()
            .map(|state| {
                state
                    .review_events
                    .get(&tenant_id)
                    .cloned()
                    .unwrap_or_default()
            })
            .unwrap_or_default()
    }
}

impl MemoryStore for InMemoryStore {
    type Txn = InMemoryTxn;

    async fn begin(&self) -> Self::Txn {
        InMemoryTxn::default()
    }

    async fn commit(&self, mut tx: Self::Txn) -> Result<(), StoreError> {
        if tx.committed {
            return Err(StoreError::TransactionAlreadyCommitted);
        }
        tx.committed = true;

        let mut state = self.inner.lock().map_err(|_| StoreError::Poisoned)?;
        for (tenant_id, episode_id) in tx.episode_observation_updates {
            if let Some(episode) = state
                .episodes
                .entry(tenant_id)
                .or_default()
                .iter_mut()
                .find(|episode| episode.id == episode_id)
            {
                episode.observation_count += 1;
            }
        }
        for episode in tx.episodes {
            state
                .episodes
                .entry(episode.tenant_id)
                .or_default()
                .push(episode);
        }
        for resource in tx.resources {
            state
                .resources
                .entry(resource.tenant_id)
                .or_default()
                .push(resource);
        }
        for unit in tx.memory_units {
            state
                .memory_units
                .entry(unit.tenant_id)
                .or_default()
                .push(unit);
        }
        for edge in tx.memory_edges {
            state
                .memory_edges
                .entry(edge.tenant_id)
                .or_default()
                .push(edge);
        }
        for job in tx.reflect_jobs {
            state
                .reflect_jobs
                .entry(job.tenant_id)
                .or_default()
                .push(job);
        }
        Ok(())
    }

    async fn stage_episode(
        &self,
        tx: &mut Self::Txn,
        episode: NewEpisode,
    ) -> Result<RetainOutcome, StoreError> {
        if tx.committed {
            return Err(StoreError::TransactionAlreadyCommitted);
        }

        if let Some(staged) = tx.episodes.iter_mut().find(|staged| {
            staged.tenant_id == episode.tenant_id
                && staged.scope_id == episode.scope_id
                && staged.dedup_key == episode.dedup_key
        }) {
            staged.observation_count += 1;
            return Ok(RetainOutcome {
                episode_id: staged.id,
                dedup: DedupOutcome {
                    matched: true,
                    observation_count: staged.observation_count,
                },
            });
        }

        let state = self.inner.lock().map_err(|_| StoreError::Poisoned)?;
        if let Some(existing) = state.episodes.get(&episode.tenant_id).and_then(|episodes| {
            episodes.iter().find(|stored| {
                stored.scope_id == episode.scope_id && stored.dedup_key == episode.dedup_key
            })
        }) {
            let pending_updates = tx
                .episode_observation_updates
                .iter()
                .filter(|(_, id)| *id == existing.id)
                .count() as u32;
            tx.episode_observation_updates
                .push((episode.tenant_id, existing.id));
            return Ok(RetainOutcome {
                episode_id: existing.id,
                dedup: DedupOutcome {
                    matched: true,
                    observation_count: existing.observation_count + pending_updates + 1,
                },
            });
        }
        drop(state);

        let id = EpisodeId::new();
        let stored = StoredEpisode {
            id,
            tenant_id: episode.tenant_id,
            scope_id: episode.scope_id,
            actor_id: episode.actor_id,
            source_kind: episode.source_kind,
            source_trust: episode.source_trust,
            dedup_key: episode.dedup_key,
            body: episode.body,
            observation_count: 1,
        };
        tx.episodes.push(stored);
        Ok(RetainOutcome {
            episode_id: id,
            dedup: DedupOutcome {
                matched: false,
                observation_count: 1,
            },
        })
    }

    async fn stage_memory_unit(
        &self,
        tx: &mut Self::Txn,
        unit: NewMemoryUnit,
    ) -> Result<UnitId, StoreError> {
        if tx.committed {
            return Err(StoreError::TransactionAlreadyCommitted);
        }

        let id = UnitId::new();
        tx.memory_units.push(StoredMemoryUnit {
            id,
            tenant_id: unit.tenant_id,
            scope_id: unit.scope_id,
            kind: unit.kind,
            state: unit.state,
            subject_key: unit.subject_key,
            body: unit.body,
            trust_level: unit.trust_level,
            churn_class: unit.churn_class,
            freshness_due_at: unit.freshness_due_at,
            actor_id: unit.actor_id,
            source_kind: unit.source_kind,
            source_episode_id: unit.source_episode_id,
            source_resource_id: unit.source_resource_id,
            deletion_generation: unit.deletion_generation,
            contextual_chunks: unit.contextual_chunks,
            valid_from: unit.valid_from,
            valid_to: unit.valid_to,
            transaction_from: unit.transaction_from,
            transaction_to: unit.transaction_to,
            difficulty: None,
            stability_days: None,
            last_reinforced_at: None,
            reinforcement_count: 0,
        });
        Ok(id)
    }

    async fn stage_resource(
        &self,
        tx: &mut Self::Txn,
        resource: NewResource,
    ) -> Result<ResourceId, StoreError> {
        if tx.committed {
            return Err(StoreError::TransactionAlreadyCommitted);
        }

        let id = ResourceId::new();
        tx.resources.push(StoredResource {
            id,
            tenant_id: resource.tenant_id,
            scope_id: resource.scope_id,
            actor_id: resource.actor_id,
            uri: resource.uri,
            kind: resource.kind,
            content_hash: resource.content_hash,
            mime_type: resource.mime_type,
            revision: resource.revision,
            body: resource.body,
            source_trust: resource.source_trust,
            extractor_state: ResourceExtractorState::Registered,
        });
        Ok(id)
    }

    async fn stage_memory_edge(
        &self,
        tx: &mut Self::Txn,
        edge: NewMemoryEdge,
    ) -> Result<EdgeId, StoreError> {
        if tx.committed {
            return Err(StoreError::TransactionAlreadyCommitted);
        }

        let id = EdgeId::new();
        tx.memory_edges.push(StoredMemoryEdge {
            id,
            tenant_id: edge.tenant_id,
            scope_id: edge.scope_id,
            src_id: edge.src_id,
            dst_id: edge.dst_id,
            kind: edge.kind,
        });
        Ok(id)
    }

    async fn enqueue_reflect(
        &self,
        tx: &mut Self::Txn,
        job: ReflectJob,
    ) -> Result<JobId, StoreError> {
        if tx.committed {
            return Err(StoreError::TransactionAlreadyCommitted);
        }

        let id = JobId::new();
        tx.reflect_jobs.push(QueuedReflectJob {
            id,
            tenant_id: job.tenant_id,
            scope_id: job.scope_id,
            episode_id: job.episode_id,
            resource_id: job.resource_id,
            kind: job.kind,
            compiler_version: job.compiler_version,
            subject: job.subject,
            predicate: job.predicate,
        });
        Ok(id)
    }

    async fn fetch_recall_candidates(
        &self,
        tenant: TenantId,
        scopes: &[ScopeId],
        kinds: &[MemoryKind],
        _query_terms: &[String],
        limit: usize,
    ) -> Result<Vec<StoredMemoryUnit>, StoreError> {
        let state = self.inner.lock().map_err(|_| StoreError::Poisoned)?;
        let mut units: Vec<_> = state
            .memory_units
            .get(&tenant)
            .map(|units| {
                units
                    .iter()
                    .filter(|unit| scopes.contains(&unit.scope_id))
                    .filter(|unit| kinds.is_empty() || kinds.contains(&unit.kind))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        units.truncate(limit);
        Ok(units)
    }

    async fn fetch_vector_candidates(
        &self,
        tenant: TenantId,
        scopes: &[ScopeId],
        kinds: &[MemoryKind],
        query_vec: &[f32],
        profile_id: Uuid,
        limit: usize,
    ) -> Result<Vec<(StoredMemoryUnit, f32)>, StoreError> {
        let state = self.inner.lock().map_err(|_| StoreError::Poisoned)?;
        let embeddings = state.embeddings.get(&tenant);
        let mut scored: Vec<(StoredMemoryUnit, f32)> = state
            .memory_units
            .get(&tenant)
            .map(|units| {
                units
                    .iter()
                    .filter(|unit| scopes.contains(&unit.scope_id))
                    .filter(|unit| kinds.is_empty() || kinds.contains(&unit.kind))
                    .filter_map(|unit| {
                        // Best (nearest) embedding for this unit UNDER the
                        // active profile; app-side cosine, returned as cosine
                        // DISTANCE (1 - similarity) to mirror pgvector `<=>`.
                        embeddings
                            .into_iter()
                            .flatten()
                            .filter(|row| {
                                row.memory_unit_id == unit.id
                                    && row.embedding_profile_id == profile_id
                            })
                            .map(|row| 1.0 - cosine_similarity(query_vec, &row.vec))
                            .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
                            .map(|distance| (unit.clone(), distance))
                    })
                    .collect()
            })
            .unwrap_or_default();
        scored.sort_by(|left, right| {
            left.1
                .partial_cmp(&right.1)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        scored.truncate(limit);
        Ok(scored)
    }

    async fn fetch_units_by_ids(
        &self,
        tenant: TenantId,
        ids: &[UnitId],
    ) -> Result<Vec<StoredMemoryUnit>, StoreError> {
        let state = self.inner.lock().map_err(|_| StoreError::Poisoned)?;
        Ok(state
            .memory_units
            .get(&tenant)
            .map(|units| {
                units
                    .iter()
                    .filter(|unit| ids.contains(&unit.id))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default())
    }

    async fn fetch_edges(
        &self,
        tenant: TenantId,
        unit_ids: &[UnitId],
    ) -> Result<Vec<StoredMemoryEdge>, StoreError> {
        let state = self.inner.lock().map_err(|_| StoreError::Poisoned)?;
        Ok(state
            .memory_edges
            .get(&tenant)
            .map(|edges| {
                edges
                    .iter()
                    .filter(|edge| {
                        unit_ids.contains(&edge.src_id) || unit_ids.contains(&edge.dst_id)
                    })
                    .cloned()
                    .collect()
            })
            .unwrap_or_default())
    }

    async fn fetch_review_events(
        &self,
        tenant: TenantId,
        unit_ids: &[UnitId],
    ) -> Result<Vec<ReviewEventRow>, StoreError> {
        let state = self.inner.lock().map_err(|_| StoreError::Poisoned)?;
        Ok(state
            .review_events
            .get(&tenant)
            .map(|events| {
                events
                    .iter()
                    .filter(|event| {
                        event.used_ids.is_empty()
                            || event.used_ids.iter().any(|id| unit_ids.contains(id))
                    })
                    .cloned()
                    .collect()
            })
            .unwrap_or_default())
    }

    async fn fetch_episodes_for_scope(
        &self,
        tenant: TenantId,
        scope: ScopeId,
        limit: usize,
    ) -> Result<Vec<StoredEpisode>, StoreError> {
        let state = self.inner.lock().map_err(|_| StoreError::Poisoned)?;
        let mut episodes: Vec<_> = state
            .episodes
            .get(&tenant)
            .map(|episodes| {
                episodes
                    .iter()
                    .filter(|episode| episode.scope_id == scope)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        episodes.truncate(limit);
        Ok(episodes)
    }

    async fn pending_job_count(
        &self,
        tenant: TenantId,
        scope: ScopeId,
    ) -> Result<usize, StoreError> {
        let state = self.inner.lock().map_err(|_| StoreError::Poisoned)?;
        Ok(state
            .reflect_jobs
            .get(&tenant)
            .map(|jobs| {
                jobs.iter()
                    .filter(|job| job.scope_id == scope)
                    .filter(|job| {
                        !state
                            .job_meta
                            .get(&job.id)
                            .map(|meta| meta.completed)
                            .unwrap_or(false)
                    })
                    .count()
            })
            .unwrap_or(0))
    }

    async fn fetch_episode(
        &self,
        tenant: TenantId,
        id: EpisodeId,
    ) -> Result<Option<StoredEpisode>, StoreError> {
        let state = self.inner.lock().map_err(|_| StoreError::Poisoned)?;
        Ok(state
            .episodes
            .get(&tenant)
            .and_then(|episodes| episodes.iter().find(|episode| episode.id == id).cloned()))
    }

    async fn fetch_resource(
        &self,
        tenant: TenantId,
        id: ResourceId,
    ) -> Result<Option<StoredResource>, StoreError> {
        let state = self.inner.lock().map_err(|_| StoreError::Poisoned)?;
        Ok(state
            .resources
            .get(&tenant)
            .and_then(|resources| resources.iter().find(|resource| resource.id == id).cloned()))
    }

    async fn apply_correction(
        &self,
        tenant: TenantId,
        correction: CorrectionWrite,
    ) -> Result<CorrectOutcome, StoreError> {
        let mut state = self.inner.lock().map_err(|_| StoreError::Poisoned)?;
        let units = state
            .memory_units
            .get_mut(&tenant)
            .ok_or(StoreError::NotFound("memory_unit"))?;
        let old_index = units
            .iter()
            .position(|unit| {
                unit.id == correction.selector.memory_unit_id
                    && unit.scope_id == correction.scope_id
                    && unit.state != UnitState::Deleted
            })
            .ok_or(StoreError::NotFound("memory_unit"))?;
        let mut replacement = units[old_index].clone();
        let old_id = replacement.id;
        let new_id = UnitId::new();
        let is_retroactive =
            correction.correction.valid_from.is_some() || correction.correction.valid_to.is_some();

        units[old_index].state = UnitState::Superseded;
        units[old_index].transaction_to = Some(correction.now.clone());
        replacement.id = new_id;
        replacement.body = correction.correction.value;
        replacement.state = UnitState::Active;
        replacement.actor_id = Some(correction.actor_id);
        replacement.deletion_generation = None;
        replacement.valid_from = correction.correction.valid_from;
        replacement.valid_to = correction.correction.valid_to;
        replacement.transaction_from = Some(correction.now.clone());
        replacement.transaction_to = None;
        units.push(replacement);

        state
            .memory_edges
            .entry(tenant)
            .or_default()
            .push(StoredMemoryEdge {
                id: EdgeId::new(),
                tenant_id: tenant,
                scope_id: correction.scope_id,
                src_id: new_id,
                dst_id: old_id,
                kind: MemoryEdgeKind::Supersedes,
            });
        expire_composed_dependents(&mut state, tenant, &[old_id], &correction.now);

        Ok(CorrectResult {
            correction_id: format!("cor_{}", new_id.as_uuid()),
            superseded: vec![old_id],
            created: vec![new_id],
            correction_kind: if is_retroactive {
                "retroactive".to_string()
            } else {
                "current".to_string()
            },
            trace_ref: None,
        })
    }

    async fn apply_forget(
        &self,
        tenant: TenantId,
        forget: ForgetWrite,
    ) -> Result<ForgetOutcome, StoreError> {
        let mut state = self.inner.lock().map_err(|_| StoreError::Poisoned)?;
        state.deletion_generation = state.deletion_generation.saturating_add(1);
        let deletion_generation = state.deletion_generation;

        let tombstone = match forget.target {
            ForgetTarget::MemoryUnit(id) => (SourceKindKey::MemoryUnit, id.as_uuid()),
            ForgetTarget::Episode(id) => (SourceKindKey::Episode, id.as_uuid()),
            ForgetTarget::Resource(id) => (SourceKindKey::Resource, id.as_uuid()),
        };
        state
            .forgotten_sources
            .insert((tenant, tombstone.0, tombstone.1));

        if let ForgetTarget::Episode(episode_id) = forget.target
            && let Some(episodes) = state.episodes.get_mut(&tenant)
        {
            episodes.retain(|episode| episode.id != episode_id);
        }

        let mut invalidated_units: Vec<UnitId> = Vec::new();
        if let Some(units) = state.memory_units.get_mut(&tenant) {
            for unit in units.iter_mut().filter(|unit| {
                unit.scope_id == forget.scope_id
                    && unit.state != UnitState::Deleted
                    && match forget.target {
                        ForgetTarget::MemoryUnit(id) => unit.id == id,
                        ForgetTarget::Episode(id) => unit.source_episode_id == Some(id),
                        ForgetTarget::Resource(id) => unit.source_resource_id == Some(id),
                    }
            }) {
                unit.state = UnitState::Deleted;
                unit.deletion_generation = Some(deletion_generation);
                invalidated_units.push(unit.id);
            }
        }
        invalidated_units.extend(delete_composed_dependents(
            &mut state,
            tenant,
            &invalidated_units,
            deletion_generation,
        ));

        Ok(ForgetOutcome {
            deletion_generation,
            invalidated_units,
        })
    }

    async fn record_review_events(
        &self,
        tenant: TenantId,
        events: Vec<ReviewEventRow>,
    ) -> Result<(), StoreError> {
        let mut state = self.inner.lock().map_err(|_| StoreError::Poisoned)?;
        let stored = state.review_events.entry(tenant).or_default();
        for event in events {
            if !stored.iter().any(|existing| {
                existing.trace_id == event.trace_id && existing.caller_id == event.caller_id
            }) {
                stored.push(event);
            }
        }
        Ok(())
    }

    async fn store_trace(&self, tenant: TenantId, trace: RetrievalTrace) -> Result<(), StoreError> {
        let mut state = self.inner.lock().map_err(|_| StoreError::Poisoned)?;
        state
            .retrieval_traces
            .entry(tenant)
            .or_default()
            .push(trace);
        Ok(())
    }

    async fn trace_by_id(
        &self,
        tenant: TenantId,
        id: TraceId,
    ) -> Result<Option<RetrievalTrace>, StoreError> {
        let state = self.inner.lock().map_err(|_| StoreError::Poisoned)?;
        Ok(state
            .retrieval_traces
            .get(&tenant)
            .and_then(|traces| traces.iter().find(|trace| trace.id == id).cloned()))
    }

    async fn scope_memory_page(
        &self,
        tenant: TenantId,
        scope: ScopeId,
        cursor: Option<UnitId>,
        limit: usize,
    ) -> Result<ScopePage, StoreError> {
        let state = self.inner.lock().map_err(|_| StoreError::Poisoned)?;
        let mut units: Vec<_> = state
            .memory_units
            .get(&tenant)
            .map(|units| {
                units
                    .iter()
                    .filter(|unit| unit.scope_id == scope)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        units.sort_by_key(|unit| unit.id.as_uuid());
        if let Some(cursor) = cursor {
            units.retain(|unit| unit.id.as_uuid() > cursor.as_uuid());
        }
        let has_more = units.len() > limit;
        units.truncate(limit.max(1));
        let next_cursor = has_more.then(|| units.last().map(|unit| unit.id)).flatten();
        Ok(ScopePage {
            items: units,
            next_cursor,
            has_more,
        })
    }

    async fn claim_reflect_jobs(
        &self,
        filter: JobFilter,
        limit: usize,
    ) -> Result<Vec<ReflectJobRow>, StoreError> {
        let mut state = self.inner.lock().map_err(|_| StoreError::Poisoned)?;
        let candidates: Vec<QueuedReflectJob> = state
            .reflect_jobs
            .iter()
            .filter(|(tenant, _)| filter.tenant.is_none_or(|wanted| **tenant == wanted))
            .flat_map(|(_, jobs)| jobs.iter())
            .filter(|job| filter.scope.is_none_or(|wanted| job.scope_id == wanted))
            .filter(|job| {
                let meta = state.job_meta.get(&job.id).copied().unwrap_or_default();
                !meta.completed && !meta.claimed && meta.attempts < JOB_DEAD_LETTER_ATTEMPTS
            })
            .take(limit)
            .cloned()
            .collect();
        let mut claimed = Vec::new();
        for job in candidates {
            let meta = state.job_meta.entry(job.id).or_default();
            meta.claimed = true;
            meta.attempts = meta.attempts.saturating_add(1);
            let attempts = meta.attempts;
            claimed.push(ReflectJobRow { job, attempts });
        }
        Ok(claimed)
    }

    async fn complete_reflect_job(&self, id: JobId) -> Result<(), StoreError> {
        let mut state = self.inner.lock().map_err(|_| StoreError::Poisoned)?;
        let meta = state.job_meta.entry(id).or_default();
        meta.completed = true;
        meta.claimed = false;
        Ok(())
    }

    async fn persist_compiled_units(
        &self,
        tenant: TenantId,
        write: CompiledWrite,
    ) -> Result<(), StoreError> {
        let mut state = self.inner.lock().map_err(|_| StoreError::Poisoned)?;
        let already_compiled = state.reflect_traces.get(&tenant).is_some_and(|traces| {
            traces.iter().any(|trace| {
                trace.job_id == write.job_id && trace.compiler_version == write.compiler_version
            })
        });
        if already_compiled {
            return Ok(());
        }

        for update in &write.unit_updates {
            if let Some(unit) = state
                .memory_units
                .entry(tenant)
                .or_default()
                .iter_mut()
                .find(|unit| unit.id == update.id)
            {
                unit.state = update.state;
                unit.transaction_to = update.transaction_to.clone();
            }
        }

        // Forgotten-source tombstones block re-derivation durably: any newly
        // compiled unit whose source was forgotten is refused.
        let mut admitted_ids = HashSet::new();
        let mut admitted_units = Vec::new();
        for unit in write.new_units {
            if state.is_forgotten_source(tenant, &unit) {
                continue;
            }
            admitted_ids.insert(unit.id);
            admitted_units.push(unit);
        }
        let existing_ids: HashSet<UnitId> = state
            .memory_units
            .get(&tenant)
            .map(|units| units.iter().map(|unit| unit.id).collect())
            .unwrap_or_default();
        state
            .memory_units
            .entry(tenant)
            .or_default()
            .extend(admitted_units);
        state
            .memory_edges
            .entry(tenant)
            .or_default()
            .extend(write.new_edges.into_iter().filter(|edge| {
                let src_ok =
                    admitted_ids.contains(&edge.src_id) || existing_ids.contains(&edge.src_id);
                let dst_ok =
                    admitted_ids.contains(&edge.dst_id) || existing_ids.contains(&edge.dst_id);
                src_ok && dst_ok
            }));
        state
            .reflect_traces
            .entry(tenant)
            .or_default()
            .push(write.trace);
        Ok(())
    }

    async fn fetch_reflect_trace(
        &self,
        tenant: TenantId,
        job_id: JobId,
        compiler_version: &str,
    ) -> Result<Option<ReflectTrace>, StoreError> {
        let state = self.inner.lock().map_err(|_| StoreError::Poisoned)?;
        Ok(state.reflect_traces.get(&tenant).and_then(|traces| {
            traces
                .iter()
                .find(|trace| trace.job_id == job_id && trace.compiler_version == compiler_version)
                .cloned()
        }))
    }

    async fn upsert_embeddings(
        &self,
        tenant: TenantId,
        rows: Vec<EmbeddingRow>,
    ) -> Result<(), StoreError> {
        let mut state = self.inner.lock().map_err(|_| StoreError::Poisoned)?;
        let stored = state.embeddings.entry(tenant).or_default();
        for row in rows {
            stored.retain(|existing| {
                !(existing.memory_unit_id == row.memory_unit_id
                    && existing.embedding_profile_id == row.embedding_profile_id)
            });
            stored.push(row);
        }
        Ok(())
    }

    async fn upsert_embedding_profile(
        &self,
        tenant: TenantId,
        profile: EmbeddingProfileRow,
    ) -> Result<(), StoreError> {
        let mut state = self.inner.lock().map_err(|_| StoreError::Poisoned)?;
        let profiles = state.embedding_profiles.entry(tenant).or_default();
        if !profiles.iter().any(|existing| existing.id == profile.id) {
            profiles.push(profile);
        }
        Ok(())
    }

    async fn fetch_embeddings(
        &self,
        tenant: TenantId,
        unit_ids: &[UnitId],
    ) -> Result<Vec<EmbeddingRow>, StoreError> {
        let state = self.inner.lock().map_err(|_| StoreError::Poisoned)?;
        Ok(state
            .embeddings
            .get(&tenant)
            .map(|rows| {
                rows.iter()
                    .filter(|row| unit_ids.contains(&row.memory_unit_id))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default())
    }

    async fn lookup_api_key(&self, key_hash: &str) -> Result<Option<ApiKeyRow>, StoreError> {
        let state = self.inner.lock().map_err(|_| StoreError::Poisoned)?;
        Ok(state
            .api_keys
            .iter()
            .find(|row| row.key_hash == key_hash)
            .cloned())
    }

    async fn ping(&self) -> Result<(), StoreError> {
        Ok(())
    }

    async fn dead_letter_count(&self) -> Result<u64, StoreError> {
        let state = self.inner.lock().map_err(|_| StoreError::Poisoned)?;
        Ok(state
            .job_meta
            .values()
            .filter(|meta| !meta.completed && meta.attempts >= JOB_DEAD_LETTER_ATTEMPTS)
            .count() as u64)
    }
}

pub async fn retain_episode<S>(
    store: &S,
    request: RetainRequest,
) -> Result<RetainOutcome, CoreError>
where
    S: MemoryStore,
{
    if request.body.trim().is_empty() {
        return Err(CoreError::EmptyBody);
    }

    let mut tx = store.begin().await;
    let outcome = store
        .stage_episode(
            &mut tx,
            NewEpisode {
                tenant_id: request.tenant_id,
                scope_id: request.scope_id,
                actor_id: request.actor_id,
                source_kind: request.source_kind.clone(),
                source_trust: request.source_trust,
                dedup_key: derive_dedup_key(
                    request.scope_id.as_uuid(),
                    &request.source_kind,
                    request.subject_hint.as_deref(),
                    &request.body,
                ),
                body: request.body,
            },
        )
        .await?;
    store
        .enqueue_reflect(
            &mut tx,
            ReflectJob {
                tenant_id: request.tenant_id,
                scope_id: request.scope_id,
                episode_id: Some(outcome.episode_id),
                resource_id: None,
                kind: ReflectJobKind::ReflectEpisode,
                compiler_version: request.compiler_version,
                subject: request.subject,
                predicate: request.predicate,
            },
        )
        .await?;
    store.commit(tx).await?;
    Ok(outcome)
}

pub async fn retain_resource<S>(
    store: &S,
    request: RetainResourceRequest,
) -> Result<RetainResourceOutcome, CoreError>
where
    S: MemoryStore,
{
    let mut tx = store.begin().await;
    let resource_id = store
        .stage_resource(
            &mut tx,
            NewResource {
                tenant_id: request.tenant_id,
                scope_id: request.scope_id,
                actor_id: request.actor_id,
                uri: request.uri,
                kind: request.kind.unwrap_or_default(),
                content_hash: request.content_hash,
                mime_type: request.mime_type,
                revision: request.revision,
                body: request.body,
                source_trust: request.source_trust,
            },
        )
        .await?;
    store
        .enqueue_reflect(
            &mut tx,
            ReflectJob {
                tenant_id: request.tenant_id,
                scope_id: request.scope_id,
                episode_id: None,
                resource_id: Some(resource_id),
                kind: ReflectJobKind::ReflectResource,
                compiler_version: request.compiler_version,
                subject: None,
                predicate: None,
            },
        )
        .await?;
    store.commit(tx).await?;
    Ok(RetainResourceOutcome { resource_id })
}

pub async fn correct_memory<S>(
    store: &S,
    request: CorrectRequest,
    clock: &dyn Clock,
) -> Result<CorrectResult, CoreError>
where
    S: MemoryStore,
{
    if request.correction.value.trim().is_empty() {
        return Err(CoreError::Invalid(
            "correction value cannot be empty".to_string(),
        ));
    }

    match store
        .apply_correction(
            request.tenant_id,
            CorrectionWrite {
                scope_id: request.scope_id,
                actor_id: request.actor_id,
                selector: request.selector,
                correction: request.correction,
                now: clock.now_rfc3339(),
            },
        )
        .await
    {
        Ok(outcome) => Ok(outcome),
        Err(StoreError::NotFound(entity)) => Err(CoreError::NotFound(entity.to_string())),
        Err(error) => Err(CoreError::Store(error)),
    }
}

pub async fn forget_memory<S>(
    store: &S,
    request: ForgetRequest,
    clock: &dyn Clock,
) -> Result<ForgetResult, CoreError>
where
    S: MemoryStore,
{
    let target = request
        .selector
        .exactly_one_target()
        .map_err(CoreError::Invalid)?;

    let outcome = store
        .apply_forget(
            request.tenant_id,
            ForgetWrite {
                scope_id: request.selector.scope_id,
                actor_id: request.actor_id,
                target,
            },
        )
        .await?;

    // REAL verification: re-run a recall probe against the forgotten bodies
    // and report how many forgotten units a caller could still retrieve.
    let probe_hits = post_forget_probe_hits(
        store,
        &request,
        request.selector.scope_id,
        &outcome.invalidated_units,
        clock,
    )
    .await?;

    Ok(ForgetResult {
        deletion_generation: outcome.deletion_generation,
        policy: "hard_delete".to_string(),
        invalidated_units: outcome.invalidated_units,
        verification: format!("post_forget_recall_probe_hits={probe_hits}"),
        trace_ref: None,
    })
}

async fn post_forget_probe_hits<S>(
    store: &S,
    request: &ForgetRequest,
    scope_id: ScopeId,
    invalidated_units: &[UnitId],
    clock: &dyn Clock,
) -> Result<usize, CoreError>
where
    S: MemoryStore,
{
    if invalidated_units.is_empty() {
        return Ok(0);
    }
    let forgotten = store
        .fetch_units_by_ids(request.tenant_id, invalidated_units)
        .await?;
    let probe_query = forgotten
        .iter()
        .map(|unit| unit.body.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    if probe_query.trim().is_empty() {
        return Ok(0);
    }
    let probe = recall(
        store,
        RecallRequest {
            tenant_id: request.tenant_id,
            scope_id,
            actor_id: request.actor_id,
            allowed_scope_ids: vec![scope_id],
            query: probe_query,
            k: invalidated_units.len().max(8),
            budget_tokens: 4096,
            mode: RecallMode::Exhaustive,
            include_beliefs: true,
            edge_expansion_enabled: true,
            context_packing_abstention_enabled: false,
            rerank_enabled: false,
            learned_rerank_profile: None,
            query_decomposition_enabled: false,
            procedure_recall_enabled: true,
            decay_enabled: false,
            engine_version: "forget-verification-probe".to_string(),
        },
        None,
        clock,
    )
    .await?;
    Ok(probe
        .items
        .iter()
        .filter(|item| invalidated_units.contains(&item.unit_id))
        .count())
}

pub async fn record_mark<S>(store: &S, request: MarkRequest) -> Result<MarkResult, CoreError>
where
    S: MemoryStore,
{
    if request.caller_id.trim().is_empty() {
        return Err(CoreError::Invalid("caller_id cannot be empty".to_string()));
    }

    store
        .record_review_events(
            request.tenant_id,
            vec![ReviewEvent {
                tenant_id: request.tenant_id,
                trace_id: request.trace_id,
                caller_id: request.caller_id,
                used_ids: request.used_ids,
                outcome: request.outcome,
            }],
        )
        .await?;

    Ok(MarkResult {
        accepted: true,
        trace_id: request.trace_id,
    })
}

pub async fn recall<S>(
    store: &S,
    request: RecallRequest,
    vector_query: Option<VectorQuery<'_>>,
    clock: &dyn Clock,
) -> Result<RecallResponse, CoreError>
where
    S: MemoryStore,
{
    validate_learned_rerank_profile(request.learned_rerank_profile.as_ref())?;

    let now = clock.now_rfc3339();
    let allowed = request.allowed_scope_ids.contains(&request.scope_id);

    if !allowed {
        let trace_id = TraceId::new();
        let trace = RetrievalTrace {
            id: trace_id,
            tenant_id: request.tenant_id,
            scope_id: request.scope_id,
            actor_id: request.actor_id,
            query_hash: hash_query(&request.query),
            engine_version: request.engine_version.clone(),
            feature_flags: Vec::new(),
            channel_runs: vec![ReflectStageFact {
                stage: "stage0_policy".to_string(),
                detail: "denied_scope".to_string(),
            }],
            candidates: Vec::new(),
            policy_filters: vec![RecallPolicyFilter {
                reason: RecallDropReason::Scope,
                detail: "scope not in allowed_scope_ids".to_string(),
            }],
            context_items: Vec::new(),
            dropped_items: Vec::new(),
            citations: Vec::new(),
            filter_selectivity: None,
            iterative_scan_depth: None,
            consolidation_lag_ms: 0,
            weight_vector_id: "none".to_string(),
            mode_requested: request.mode,
            mode_executed: request.mode,
            escalation_reason: "none".to_string(),
            reranker_id: "none".to_string(),
            rerank_input_count: 0,
            rerank_overfetch_ratio: 0.0,
            learned_rerank_training_set_id: None,
            subquery_ids: Vec::new(),
            decomposition_reason: "none".to_string(),
            procedure_ids: Vec::new(),
            procedure_validation_states: Vec::new(),
            abstention_signal: true,
            latency_ms: 0,
            token_estimate: 0,
            cost_micros: 0,
            decay_model_id: decay_model_id(&request).to_string(),
            l4_sandbox_id: None,
            l4_gathered_evidence_ids: Vec::new(),
        };
        store.store_trace(request.tenant_id, trace).await?;
        return Err(CoreError::PolicyDenied("scope".to_string()));
    }

    let query_tokens = tokenize(&request.query);
    let vector_query = vector_query.filter(|query| !query.vec.is_empty());
    let mut tenant_units = store
        .fetch_recall_candidates(
            request.tenant_id,
            &request.allowed_scope_ids,
            &[],
            &query_tokens,
            usize::MAX,
        )
        .await?;
    // Real vector channel: the store returns (unit, cosine DISTANCE) for the
    // nearest units under the active profile (pgvector `<=>`, or the in-memory
    // app-side cosine), and the channel score is `1 - distance` — no app-side
    // recompute from raw vectors. The vector-surfaced units are folded into the
    // candidate union. `None` when no real embedding provider is configured —
    // the channel is then traced as disabled.
    let vector_scores: Option<HashMap<UnitId, f32>> = match vector_query {
        Some(query) => {
            let pairs = store
                .fetch_vector_candidates(
                    request.tenant_id,
                    &request.allowed_scope_ids,
                    &[],
                    query.vec,
                    query.profile_id,
                    VECTOR_CANDIDATE_LIMIT,
                )
                .await?;
            let mut seen: HashSet<UnitId> = tenant_units.iter().map(|unit| unit.id).collect();
            let mut scores: HashMap<UnitId, f32> = HashMap::new();
            for (unit, distance) in pairs {
                let score = 1.0 - distance;
                scores
                    .entry(unit.id)
                    .and_modify(|best| *best = best.max(score))
                    .or_insert(score);
                if seen.insert(unit.id) {
                    tenant_units.push(unit);
                }
            }
            Some(scores)
        }
        None => None,
    };
    let unit_ids: Vec<UnitId> = tenant_units.iter().map(|unit| unit.id).collect();
    let tenant_edges = store.fetch_edges(request.tenant_id, &unit_ids).await?;
    let mut tenant_episodes = Vec::new();
    if request.mode == RecallMode::Exhaustive {
        for scope in &request.allowed_scope_ids {
            tenant_episodes.extend(
                store
                    .fetch_episodes_for_scope(request.tenant_id, *scope, usize::MAX)
                    .await?,
            );
        }
    }
    let tenant_review_events = store
        .fetch_review_events(request.tenant_id, &unit_ids)
        .await?;
    let dropped_items = trace_filter_drops(&tenant_units, &request, &now);
    let surviving = tenant_units.len().saturating_sub(dropped_items.len());
    let filter_selectivity = Some(surviving as f32 / tenant_units.len().max(1) as f32);
    let decomposition = decompose_query(&request);
    let mut candidates_by_unit: HashMap<UnitId, CandidateAccumulator> = HashMap::new();
    let mut candidate_traces = Vec::new();

    // The token-overlap scorers all run under the honest `lexical` label; the
    // `vector` channel is only emitted by a real embedding path and is traced
    // as disabled otherwise.
    let channels = [
        ChannelPass::Exact,
        ChannelPass::Lexical,
        ChannelPass::Semantic,
        ChannelPass::Temporal,
        ChannelPass::Edge,
    ];
    let mut main_channels: Vec<ChannelPass> = channels.to_vec();
    if vector_scores.is_some() {
        main_channels.push(ChannelPass::Vector);
    }
    for pass in main_channels
        .into_iter()
        .filter(|pass| request.edge_expansion_enabled || *pass != ChannelPass::Edge)
    {
        let channel = pass.label();
        let mut ranked = channel_candidates(
            pass,
            &tenant_units,
            &tenant_edges,
            &request,
            &query_tokens,
            vector_scores.as_ref(),
            &now,
        );
        ranked.sort_by(|left, right| {
            right
                .1
                .partial_cmp(&left.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| left.0.body.cmp(&right.0.body))
        });
        for (rank, (unit, score)) in ranked.into_iter().enumerate() {
            let channel_rank = rank + 1;
            let decay = decay_score_for(&unit, &tenant_review_events, request.decay_enabled);
            let contribution = channel_weight(pass, &request.query) / (60.0 + channel_rank as f32);
            candidates_by_unit
                .entry(unit.id)
                .and_modify(|candidate| {
                    candidate.fused_score += contribution;
                    candidate.channels.push((channel, channel_rank, score));
                })
                .or_insert_with(|| CandidateAccumulator {
                    unit: unit.clone(),
                    fused_score: contribution,
                    rerank_rank: None,
                    rerank_score: 0.0,
                    decay,
                    l4_score: 0.0,
                    subquery_ids: Vec::new(),
                    decomposition_rank: None,
                    channels: vec![(channel, channel_rank, score)],
                });
            candidate_traces.push(RecallCandidateTrace {
                unit_id: unit.id,
                channel,
                channel_rank,
                channel_score: score,
                derived_by: derived_by_for_unit(&unit).to_string(),
                fused_rank: None,
                fused_score: None,
                rerank_rank: None,
                rerank_score: 0.0,
                subquery_ids: Vec::new(),
                decay_retrievability: decay.retrievability,
                dsr_stability_days: decay.stability_days,
                dsr_difficulty: decay.difficulty,
                dsr_reinforcement_count: decay.reinforcement_count,
                trust_level: unit.trust_level,
                state: unit.state,
                discard_reason: None,
            });
        }
    }

    if decomposition.active() {
        for (subquery_index, subquery) in decomposition.subqueries.iter().enumerate() {
            let subquery_tokens = tokenize(&subquery.query);
            for pass in channels
                .into_iter()
                .filter(|pass| request.edge_expansion_enabled || *pass != ChannelPass::Edge)
            {
                let channel = pass.label();
                let mut ranked = channel_candidates(
                    pass,
                    &tenant_units,
                    &tenant_edges,
                    &request,
                    &subquery_tokens,
                    None,
                    &now,
                );
                ranked.sort_by(|left, right| {
                    right
                        .1
                        .partial_cmp(&left.1)
                        .unwrap_or(std::cmp::Ordering::Equal)
                        .then_with(|| left.0.body.cmp(&right.0.body))
                });
                for (rank, (unit, score)) in ranked.into_iter().take(request.k.max(1)).enumerate() {
                    let channel_rank = rank + 1;
                    let decay =
                        decay_score_for(&unit, &tenant_review_events, request.decay_enabled);
                    let contribution =
                        channel_weight(pass, &subquery.query) / (55.0 + channel_rank as f32);
                    candidates_by_unit
                        .entry(unit.id)
                        .and_modify(|candidate| {
                            candidate.fused_score += contribution;
                            candidate.channels.push((channel, channel_rank, score));
                            push_unique(&mut candidate.subquery_ids, subquery.id.clone());
                        })
                        .or_insert_with(|| CandidateAccumulator {
                            unit: unit.clone(),
                            fused_score: contribution,
                            rerank_rank: None,
                            rerank_score: 0.0,
                            decay,
                            l4_score: 0.0,
                            subquery_ids: vec![subquery.id.clone()],
                            decomposition_rank: None,
                            channels: vec![(channel, channel_rank, score)],
                        });
                    candidate_traces.push(RecallCandidateTrace {
                        unit_id: unit.id,
                        channel,
                        channel_rank,
                        channel_score: score,
                        derived_by: derived_by_for_unit(&unit).to_string(),
                        fused_rank: None,
                        fused_score: None,
                        rerank_rank: None,
                        rerank_score: 0.0,
                        subquery_ids: vec![subquery.id.clone()],
                        decay_retrievability: decay.retrievability,
                        dsr_stability_days: decay.stability_days,
                        dsr_difficulty: decay.difficulty,
                        dsr_reinforcement_count: decay.reinforcement_count,
                        trust_level: unit.trust_level,
                        state: unit.state,
                        discard_reason: None,
                    });
                }
            }
            mark_best_subquery_candidate(
                &mut candidates_by_unit,
                &subquery.id,
                &subquery.query,
                subquery_index + 1,
            );
        }
    }

    let mut l4_gathered_evidence_ids = Vec::new();
    if request.mode == RecallMode::Exhaustive {
        let mut ranked = l4_exhaustive_candidates(
            &tenant_units,
            &tenant_episodes,
            &request,
            &query_tokens,
            &now,
        );
        ranked.sort_by(|left, right| {
            right
                .1
                .partial_cmp(&left.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| left.0.body.cmp(&right.0.body))
        });
        for (rank, (unit, score, evidence_id)) in ranked.into_iter().enumerate() {
            let channel_rank = rank + 1;
            let decay = decay_score_for(&unit, &tenant_review_events, request.decay_enabled);
            let contribution = 8.0 + score + (1.0 / (50.0 + channel_rank as f32));
            push_unique(&mut l4_gathered_evidence_ids, evidence_id.clone());
            candidates_by_unit
                .entry(unit.id)
                .and_modify(|candidate| {
                    candidate.fused_score += contribution;
                    candidate.l4_score = candidate.l4_score.max(score);
                    candidate
                        .channels
                        .push((RecallChannel::Exhaustive, channel_rank, score));
                })
                .or_insert_with(|| CandidateAccumulator {
                    unit: unit.clone(),
                    fused_score: contribution,
                    rerank_rank: None,
                    rerank_score: 0.0,
                    decay,
                    l4_score: score,
                    subquery_ids: Vec::new(),
                    decomposition_rank: None,
                    channels: vec![(RecallChannel::Exhaustive, channel_rank, score)],
                });
            candidate_traces.push(RecallCandidateTrace {
                unit_id: unit.id,
                channel: RecallChannel::Exhaustive,
                channel_rank,
                channel_score: score,
                derived_by: derived_by_for_unit(&unit).to_string(),
                fused_rank: None,
                fused_score: None,
                rerank_rank: None,
                rerank_score: 0.0,
                subquery_ids: Vec::new(),
                decay_retrievability: decay.retrievability,
                dsr_stability_days: decay.stability_days,
                dsr_difficulty: decay.difficulty,
                dsr_reinforcement_count: decay.reinforcement_count,
                trust_level: unit.trust_level,
                state: unit.state,
                discard_reason: None,
            });
        }
    }

    let mut fused: Vec<_> = candidates_by_unit.into_values().collect();
    if decomposition.active() {
        fused.retain(|candidate| !candidate.subquery_ids.is_empty());
    }
    fused.sort_by(|left, right| {
        right
            .fused_score
            .partial_cmp(&left.fused_score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.unit.body.cmp(&right.unit.body))
    });
    for (rank, candidate) in fused.iter().enumerate() {
        for trace_candidate in candidate_traces
            .iter_mut()
            .filter(|trace_candidate| trace_candidate.unit_id == candidate.unit.id)
        {
            trace_candidate.fused_rank = Some(rank + 1);
            trace_candidate.fused_score = Some(candidate.fused_score);
        }
    }

    let rerank = rerank_candidates(fused.as_mut_slice(), &request, &query_tokens);
    for candidate in &fused {
        for trace_candidate in candidate_traces
            .iter_mut()
            .filter(|trace_candidate| trace_candidate.unit_id == candidate.unit.id)
        {
            trace_candidate.rerank_rank = candidate.rerank_rank;
            trace_candidate.rerank_score = candidate.rerank_score;
            trace_candidate.subquery_ids = candidate.subquery_ids.clone();
        }
    }

    let iterative_scan_depth = recall_pack_scan_limit(&request, fused.len());
    let packed = pack_recall_context(
        fused,
        &request,
        &tenant_edges,
        &query_tokens,
        dropped_items,
        iterative_scan_depth,
    );
    let token_estimate = packed.token_estimate;
    let items = packed.items;
    let dropped_items = packed.dropped_items;
    let abstention = packed.abstention;

    let candidate_whitelist: Vec<_> = items.iter().map(|item| item.unit_id).collect();
    let mut suppression_labels = Vec::new();
    for label in items
        .iter()
        .flat_map(|item| item.suppression_labels.iter().cloned())
    {
        if !suppression_labels.contains(&label) {
            suppression_labels.push(label);
        }
    }
    let citations: Vec<_> = items
        .iter()
        .filter(|item| item.citation_episode_id.is_some() || item.citation_resource_id.is_some())
        .map(|item| RecallCitation {
            unit_id: item.unit_id,
            episode_id: item.citation_episode_id,
            resource_id: item.citation_resource_id,
        })
        .collect();
    let procedure_ids = items
        .iter()
        .filter(|item| item.kind == MemoryKind::Procedural)
        .map(|item| item.unit_id)
        .collect::<Vec<_>>();
    let procedure_validation_states = procedure_trace_facts(&tenant_units, &request);
    let trace_id = TraceId::new();
    let mut feature_flags = recall_feature_flags(&request, vector_scores.is_some());
    if candidate_traces
        .iter()
        .any(|candidate| candidate.derived_by == "composition")
        || items.iter().any(|item| item.derived_by == "composition")
    {
        feature_flags.push("inferred_belief_composition_enabled".to_string());
    }
    let trace = RetrievalTrace {
        id: trace_id,
        tenant_id: request.tenant_id,
        scope_id: request.scope_id,
        actor_id: request.actor_id,
        query_hash: hash_query(&request.query),
        engine_version: request.engine_version.clone(),
        feature_flags,
        channel_runs: recall_stage_facts(vector_scores.is_some()),
        candidates: candidate_traces,
        policy_filters: Vec::new(),
        context_items: items.clone(),
        dropped_items,
        citations: citations.clone(),
        filter_selectivity,
        iterative_scan_depth: Some(iterative_scan_depth as u32),
        consolidation_lag_ms: 0,
        weight_vector_id: rerank.weight_vector_id,
        mode_requested: request.mode,
        mode_executed: request.mode,
        escalation_reason: "none".to_string(),
        reranker_id: rerank.reranker_id,
        rerank_input_count: rerank.input_count,
        rerank_overfetch_ratio: rerank.overfetch_ratio,
        learned_rerank_training_set_id: rerank.training_set_id,
        subquery_ids: decomposition
            .subqueries
            .iter()
            .map(|subquery| subquery.id.clone())
            .collect(),
        decomposition_reason: decomposition.reason,
        procedure_ids,
        procedure_validation_states,
        abstention_signal: abstention,
        latency_ms: 0,
        token_estimate,
        cost_micros: 0,
        decay_model_id: decay_model_id(&request).to_string(),
        l4_sandbox_id: (request.mode == RecallMode::Exhaustive).then(|| L4_SANDBOX_ID.to_string()),
        l4_gathered_evidence_ids,
    };
    store.store_trace(request.tenant_id, trace).await?;

    Ok(RecallResponse {
        trace_id,
        items,
        candidate_whitelist,
        citations,
        abstention,
        degraded: false,
        consolidation_lag_ms: 0,
        suppression_labels,
    })
}

fn trace_filter_drops(
    units: &[StoredMemoryUnit],
    request: &RecallRequest,
    now: &str,
) -> Vec<RecallDroppedItem> {
    units
        .iter()
        .filter_map(|unit| {
            let reason = if !request.allowed_scope_ids.contains(&unit.scope_id) {
                Some(RecallDropReason::Scope)
            } else if unit.deletion_generation.is_some() {
                Some(RecallDropReason::Deleted)
            } else if unit.transaction_to.is_some() || !valid_for_query(unit, &request.query, now) {
                Some(RecallDropReason::Stale)
            } else if let Some(reason) = procedure_drop_reason(unit, request) {
                Some(reason)
            } else if let Some(reason) = high_risk_recall_drop_reason(unit, request) {
                Some(reason)
            } else {
                match unit.state {
                    UnitState::Deleted => Some(RecallDropReason::Deleted),
                    UnitState::Invalidated => Some(RecallDropReason::Invalidated),
                    UnitState::Superseded | UnitState::Expired | UnitState::Retired => {
                        Some(RecallDropReason::Stale)
                    }
                    UnitState::Quarantined => Some(RecallDropReason::Trust),
                    UnitState::Candidate
                        if unit.kind == MemoryKind::Belief && !request.include_beliefs =>
                    {
                        Some(RecallDropReason::Trust)
                    }
                    _ => None,
                }
            };
            reason.map(|reason| RecallDroppedItem {
                unit_id: unit.id,
                reason,
            })
        })
        .collect()
}

fn procedure_drop_reason(
    unit: &StoredMemoryUnit,
    request: &RecallRequest,
) -> Option<RecallDropReason> {
    if unit.kind != MemoryKind::Procedural {
        return None;
    }
    if !request.procedure_recall_enabled || unit.state != UnitState::Validated {
        return Some(RecallDropReason::State);
    }
    if unsafe_procedure_step(unit) {
        return Some(RecallDropReason::ProtectedCategory);
    }
    None
}

fn procedure_trace_facts(
    units: &[StoredMemoryUnit],
    request: &RecallRequest,
) -> Vec<ProcedureTraceFact> {
    units
        .iter()
        .filter(|unit| {
            unit.kind == MemoryKind::Procedural
                && request.allowed_scope_ids.contains(&unit.scope_id)
                && unit.deletion_generation.is_none()
                && unit.transaction_to.is_none()
        })
        .map(|unit| ProcedureTraceFact {
            unit_id: unit.id,
            validation_state: procedure_validation_state(unit).to_string(),
            signal_kind: procedure_signal_kind(unit).to_string(),
            safety_status: procedure_safety_status(unit).to_string(),
        })
        .collect()
}

fn procedure_validation_state(unit: &StoredMemoryUnit) -> &'static str {
    match unit.state {
        UnitState::Validated => "validated",
        UnitState::Candidate => "candidate",
        UnitState::Active => "active",
        UnitState::Retired => "retired",
        _ => "not_validated",
    }
}

fn procedure_signal_kind(unit: &StoredMemoryUnit) -> &'static str {
    let body = normalize_component(&unit.body);
    let subject = unit
        .subject_key
        .as_deref()
        .map(normalize_component)
        .unwrap_or_default();
    if body.contains("failure pattern")
        || body.contains("reproduces the failure")
        || subject.contains("failure")
    {
        "failure"
    } else {
        "success"
    }
}

fn procedure_safety_status(unit: &StoredMemoryUnit) -> &'static str {
    if unsafe_procedure_step(unit) {
        "unsafe"
    } else {
        "safe"
    }
}

fn unsafe_procedure_step(unit: &StoredMemoryUnit) -> bool {
    if unit.kind != MemoryKind::Procedural {
        return false;
    }
    let body = normalize_component(&unit.body);
    [
        "force-push",
        "force push",
        "skip validation",
        "skipping validation",
        "bypass approval",
        "bypass auth",
        "export secrets",
        "exfiltrat",
        "rm -rf",
        "delete production",
        "disable tests",
    ]
    .iter()
    .any(|phrase| body.contains(phrase))
}

fn decompose_query(request: &RecallRequest) -> QueryDecompositionFacts {
    if !request.query_decomposition_enabled || request.mode == RecallMode::Fast {
        return QueryDecompositionFacts::none();
    }

    let conjuncts = structural_query_conjuncts(&request.query);
    let mut reasons = Vec::new();
    if conjuncts.len() >= 2 {
        reasons.push("multi_constraint_conjunction");
        reasons.push("multiple_entity_hits");
    }
    if has_comparative_or_causal_connector(&request.query) {
        reasons.push("comparative_causal_connector");
    }
    if has_temporal_relation(&request.query) {
        reasons.push("temporal_relation");
    }

    if reasons.len() < 2 || conjuncts.len() < 2 {
        return QueryDecompositionFacts::none();
    }

    let subqueries = conjuncts
        .into_iter()
        .enumerate()
        .map(|(index, query)| QuerySubquery {
            id: format!("sq{}_{}", index + 1, stable_subquery_slug(&query)),
            query,
        })
        .collect::<Vec<_>>();

    QueryDecompositionFacts {
        subqueries,
        reason: reasons.join("+"),
    }
}

fn structural_query_conjuncts(query: &str) -> Vec<String> {
    let normalized = normalize_component(query);
    let mut parts = vec![normalized.as_str()];
    for connector in [
        " and which ",
        " and what ",
        " and where ",
        " and who ",
        " and when ",
        " and ",
    ] {
        if normalized.contains(connector) {
            parts = normalized.split(connector).collect();
            break;
        }
    }

    parts
        .into_iter()
        .map(|part| {
            part.trim()
                .trim_start_matches("which ")
                .trim_start_matches("what ")
                .trim_start_matches("where ")
                .trim_start_matches("who ")
                .trim_start_matches("when ")
                .trim()
                .to_string()
        })
        .filter(|part| tokenize(part).len() >= 2)
        .collect()
}

fn has_comparative_or_causal_connector(query: &str) -> bool {
    let query = normalize_component(query);
    query
        .split_whitespace()
        .any(|token| matches!(token, "because" | "why" | "versus" | "vs" | "compare"))
}

fn has_temporal_relation(query: &str) -> bool {
    let query_tokens = tokenize(query);
    query_tokens.iter().any(|token| {
        matches!(
            token.as_str(),
            "before" | "after" | "during" | "since" | "current" | "latest" | "now"
        )
    })
}

fn stable_subquery_slug(query: &str) -> String {
    let tokens = tokenize(query);
    let slug = tokens.iter().take(4).cloned().collect::<Vec<_>>().join("_");
    if slug.is_empty() {
        "empty".to_string()
    } else {
        slug
    }
}

fn mark_best_subquery_candidate(
    candidates: &mut HashMap<UnitId, CandidateAccumulator>,
    subquery_id: &str,
    subquery: &str,
    rank: usize,
) {
    let subquery_tokens = tokenize(subquery);
    let Some(unit_id) = candidates
        .iter()
        .filter(|(_, candidate)| {
            candidate
                .subquery_ids
                .iter()
                .any(|candidate_subquery| candidate_subquery == subquery_id)
        })
        .max_by(|(_, left), (_, right)| {
            exact_score(&left.unit, &subquery_tokens)
                .partial_cmp(&exact_score(&right.unit, &subquery_tokens))
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    lexical_score(&left.unit, &subquery_tokens)
                        .partial_cmp(&lexical_score(&right.unit, &subquery_tokens))
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .then_with(|| {
                    left.fused_score
                        .partial_cmp(&right.fused_score)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .then_with(|| right.unit.body.cmp(&left.unit.body))
        })
        .map(|(unit_id, _)| *unit_id)
    else {
        return;
    };
    if let Some(candidate) = candidates.get_mut(&unit_id) {
        candidate.decomposition_rank = Some(
            candidate
                .decomposition_rank
                .map_or(rank, |existing| existing.min(rank)),
        );
    }
}

fn push_unique(values: &mut Vec<String>, value: String) {
    if !values.iter().any(|existing| existing == &value) {
        values.push(value);
    }
}

fn rerank_candidates(
    fused: &mut [CandidateAccumulator],
    request: &RecallRequest,
    query_tokens: &[String],
) -> RerankTraceFacts {
    if !request.rerank_enabled {
        return RerankTraceFacts {
            reranker_id: "none".to_string(),
            weight_vector_id: "default".to_string(),
            training_set_id: None,
            input_count: 0,
            overfetch_ratio: 0.0,
        };
    }

    let profile = request.learned_rerank_profile.as_ref();
    let input_count = fused.len().min(rerank_input_cap(request));
    for candidate in fused.iter_mut().take(input_count) {
        candidate.rerank_score = rerank_score(candidate, query_tokens, profile);
    }
    fused[..input_count].sort_by(|left, right| {
        right
            .rerank_score
            .partial_cmp(&left.rerank_score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                right
                    .fused_score
                    .partial_cmp(&left.fused_score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| left.unit.body.cmp(&right.unit.body))
    });
    for (rank, candidate) in fused.iter_mut().take(input_count).enumerate() {
        candidate.rerank_rank = Some(rank + 1);
    }

    RerankTraceFacts {
        reranker_id: profile
            .map(|profile| profile.profile_id.clone())
            .unwrap_or_else(|| "deterministic-local-v1".to_string()),
        weight_vector_id: profile
            .map(|profile| profile.profile_id.clone())
            .unwrap_or_else(|| "default".to_string()),
        training_set_id: profile.map(|profile| profile.training_set_id.clone()),
        input_count,
        overfetch_ratio: input_count as f32 / request.k.max(1) as f32,
    }
}

fn rerank_input_cap(request: &RecallRequest) -> usize {
    let mode_cap = match request.mode {
        RecallMode::Fast => 100,
        RecallMode::Balanced | RecallMode::Exhaustive => 200,
    };
    (request.k.saturating_mul(10)).min(mode_cap).max(request.k)
}

fn rerank_score(
    candidate: &CandidateAccumulator,
    query_tokens: &[String],
    profile: Option<&LearnedRerankProfile>,
) -> f32 {
    let lexical = lexical_score(&candidate.unit, query_tokens);
    let vector = token_set_overlap_score(&candidate.unit, query_tokens);
    let exact = exact_score(&candidate.unit, query_tokens);
    let intent = rerank_intent_anchor_score(&candidate.unit, query_tokens);
    let decay = candidate.decay.retrievability;
    let fused = candidate.fused_score;

    match profile {
        Some(profile) => {
            (profile.lexical_weight * lexical)
                + (profile.vector_weight * vector)
                + (profile.exact_weight * exact)
                + (profile.intent_weight * intent)
                + (profile.decay_weight * decay)
                + (profile.fused_weight * fused)
        }
        None => (3.0 * lexical) + (2.0 * vector) + exact + (2.0 * intent) + (3.0 * decay) + fused,
    }
}

fn rerank_intent_anchor_score(unit: &StoredMemoryUnit, query_tokens: &[String]) -> f32 {
    let Some(subject_key) = unit.subject_key.as_deref() else {
        return 0.0;
    };
    let subject_tokens = tokenize(subject_key);
    let body_tokens = tokenize(&unit.body);
    query_tokens
        .iter()
        .filter(|token| is_rerank_intent_token(token))
        .map(|token| {
            let subject_anchor = subject_tokens
                .iter()
                .any(|subject| tokens_related(subject, token))
                as u8 as f32;
            let body_anchor =
                body_tokens.iter().any(|body| tokens_related(body, token)) as u8 as f32;
            subject_anchor + (0.5 * body_anchor)
        })
        .sum()
}

fn is_rerank_intent_token(token: &str) -> bool {
    matches!(
        token,
        "owner"
            | "owns"
            | "owned"
            | "resolve"
            | "resolves"
            | "resolved"
            | "responsible"
            | "assignee"
            | "assigned"
    )
}

#[derive(Clone)]
struct CandidateAccumulator {
    unit: StoredMemoryUnit,
    fused_score: f32,
    rerank_rank: Option<usize>,
    rerank_score: f32,
    decay: DecayScore,
    l4_score: f32,
    subquery_ids: Vec<String>,
    decomposition_rank: Option<usize>,
    channels: Vec<(RecallChannel, usize, f32)>,
}

#[derive(Clone, Copy)]
struct DecayScore {
    retrievability: f32,
    stability_days: Option<f32>,
    difficulty: Option<f32>,
    reinforcement_count: u32,
}

impl DecayScore {
    fn neutral(unit: &StoredMemoryUnit) -> Self {
        Self {
            retrievability: 1.0,
            stability_days: unit.stability_days,
            difficulty: unit.difficulty,
            reinforcement_count: unit.reinforcement_count,
        }
    }
}

struct RerankTraceFacts {
    reranker_id: String,
    weight_vector_id: String,
    training_set_id: Option<String>,
    input_count: usize,
    overfetch_ratio: f32,
}

fn validate_learned_rerank_profile(
    profile: Option<&LearnedRerankProfile>,
) -> Result<(), CoreError> {
    let Some(profile) = profile else {
        return Ok(());
    };
    if profile.profile_id.trim().is_empty() {
        return Err(CoreError::Invalid(
            "learned_rerank_profile.profile_id cannot be empty".to_string(),
        ));
    }
    if profile.training_set_id.trim().is_empty() {
        return Err(CoreError::Invalid(
            "learned_rerank_profile.training_set_id cannot be empty".to_string(),
        ));
    }
    let weights = [
        profile.lexical_weight,
        profile.vector_weight,
        profile.exact_weight,
        profile.intent_weight,
        profile.decay_weight,
        profile.fused_weight,
    ];
    if weights.iter().any(|weight| !weight.is_finite()) {
        return Err(CoreError::Invalid(
            "learned_rerank_profile weights must be finite".to_string(),
        ));
    }
    Ok(())
}

struct QueryDecompositionFacts {
    subqueries: Vec<QuerySubquery>,
    reason: String,
}

impl QueryDecompositionFacts {
    fn none() -> Self {
        Self {
            subqueries: Vec::new(),
            reason: "none".to_string(),
        }
    }

    fn active(&self) -> bool {
        !self.subqueries.is_empty()
    }
}

struct QuerySubquery {
    id: String,
    query: String,
}

struct PackedRecallContext {
    items: Vec<RecallContextItem>,
    dropped_items: Vec<RecallDroppedItem>,
    token_estimate: usize,
    abstention: bool,
}

fn pack_recall_context(
    mut fused: Vec<CandidateAccumulator>,
    request: &RecallRequest,
    tenant_edges: &[StoredMemoryEdge],
    query_tokens: &[String],
    mut dropped_items: Vec<RecallDroppedItem>,
    scan_limit: usize,
) -> PackedRecallContext {
    let mut token_estimate = 0;
    let mut items: Vec<RecallContextItem> = Vec::new();
    let mut packed_token_counts = Vec::new();
    let mut packed_relevance_scores = Vec::new();
    let mut seen_subjects: HashMap<String, Vec<UnitId>> = HashMap::new();

    if request.context_packing_abstention_enabled {
        fused.sort_by(|left, right| {
            if request.query_decomposition_enabled
                && (left.decomposition_rank.is_some() || right.decomposition_rank.is_some())
            {
                left.decomposition_rank
                    .unwrap_or(usize::MAX)
                    .cmp(&right.decomposition_rank.unwrap_or(usize::MAX))
                    .then_with(|| {
                        left.rerank_rank
                            .unwrap_or(usize::MAX)
                            .cmp(&right.rerank_rank.unwrap_or(usize::MAX))
                    })
                    .then_with(|| {
                        right
                            .rerank_score
                            .partial_cmp(&left.rerank_score)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .then_with(|| left.unit.body.cmp(&right.unit.body))
            } else if request.rerank_enabled {
                left.rerank_rank
                    .unwrap_or(usize::MAX)
                    .cmp(&right.rerank_rank.unwrap_or(usize::MAX))
                    .then_with(|| {
                        right
                            .rerank_score
                            .partial_cmp(&left.rerank_score)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .then_with(|| left.unit.body.cmp(&right.unit.body))
            } else {
                right
                    .fused_score
                    .partial_cmp(&left.fused_score)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| {
                        packing_density_score(right)
                            .partial_cmp(&packing_density_score(left))
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .then_with(|| left.unit.body.cmp(&right.unit.body))
            }
        });
    }

    let output_limit = request.k.max(1);
    for candidate in fused.into_iter().take(scan_limit) {
        if request.context_packing_abstention_enabled
            && let Some(subject_key) = candidate.unit.subject_key.as_deref()
        {
            let dedup_key = normalize_component(subject_key);
            if !dedup_key.is_empty() {
                let seen_ids = seen_subjects.entry(dedup_key).or_default();
                if !seen_ids.is_empty()
                    && !has_contradiction_with_any(candidate.unit.id, seen_ids, tenant_edges)
                {
                    dropped_items.push(RecallDroppedItem {
                        unit_id: candidate.unit.id,
                        reason: RecallDropReason::Duplicate,
                    });
                    continue;
                }
                seen_ids.push(candidate.unit.id);
            }
        }

        let (rendered_body, unit_tokens) = packed_body_and_cost(&candidate.unit, query_tokens);
        let candidate_score = packing_relevance_score(&candidate, query_tokens);
        if items.len() >= output_limit {
            if let Some(replace_index) = replacement_index(
                &packed_token_counts,
                &packed_relevance_scores,
                token_estimate,
                unit_tokens,
                candidate_score,
                request.budget_tokens,
            ) {
                let replaced = items.remove(replace_index);
                let replaced_tokens = packed_token_counts.remove(replace_index);
                packed_relevance_scores.remove(replace_index);
                token_estimate = token_estimate - replaced_tokens + unit_tokens;
                dropped_items.push(RecallDroppedItem {
                    unit_id: replaced.unit_id,
                    reason: RecallDropReason::Rerank,
                });
                packed_token_counts.push(unit_tokens);
                packed_relevance_scores.push(candidate_score);
                items.push(context_item_for(
                    candidate,
                    tenant_edges,
                    query_tokens,
                    rendered_body,
                ));
                continue;
            }
            dropped_items.push(RecallDroppedItem {
                unit_id: candidate.unit.id,
                reason: RecallDropReason::Rerank,
            });
            continue;
        }
        if token_estimate + unit_tokens > request.budget_tokens {
            if request.context_packing_abstention_enabled
                && let Some(replace_index) = replacement_index(
                    &packed_token_counts,
                    &packed_relevance_scores,
                    token_estimate,
                    unit_tokens,
                    candidate_score,
                    request.budget_tokens,
                )
            {
                let replaced = items.remove(replace_index);
                let replaced_tokens = packed_token_counts.remove(replace_index);
                packed_relevance_scores.remove(replace_index);
                token_estimate = token_estimate - replaced_tokens + unit_tokens;
                dropped_items.push(RecallDroppedItem {
                    unit_id: replaced.unit_id,
                    reason: RecallDropReason::Budget,
                });
                packed_token_counts.push(unit_tokens);
                packed_relevance_scores.push(candidate_score);
                items.push(context_item_for(
                    candidate,
                    tenant_edges,
                    query_tokens,
                    rendered_body,
                ));
                continue;
            }
            dropped_items.push(RecallDroppedItem {
                unit_id: candidate.unit.id,
                reason: RecallDropReason::Budget,
            });
            continue;
        }
        token_estimate += unit_tokens;
        packed_token_counts.push(unit_tokens);
        packed_relevance_scores.push(candidate_score);
        items.push(context_item_for(
            candidate,
            tenant_edges,
            query_tokens,
            rendered_body,
        ));
    }

    let abstention = items.is_empty()
        || (request.context_packing_abstention_enabled
            && items.iter().any(|item| {
                item.suppression_labels
                    .iter()
                    .any(|label| label == "unresolved_contradiction")
            }));

    PackedRecallContext {
        items,
        dropped_items,
        token_estimate,
        abstention,
    }
}

fn recall_pack_scan_limit(request: &RecallRequest, candidate_count: usize) -> usize {
    let output_limit = request.k.max(1);
    match request.mode {
        RecallMode::Exhaustive => candidate_count
            .min(output_limit.saturating_mul(25).max(25))
            .max(output_limit),
        RecallMode::Fast | RecallMode::Balanced => output_limit.min(candidate_count).max(1),
    }
}

fn packing_density_score(candidate: &CandidateAccumulator) -> f32 {
    candidate.fused_score / candidate.unit.body.split_whitespace().count().max(1) as f32
}

fn packing_relevance_score(candidate: &CandidateAccumulator, query_tokens: &[String]) -> f32 {
    candidate.fused_score
        + exact_score(&candidate.unit, query_tokens)
        + lexical_score(&candidate.unit, query_tokens)
        + token_set_overlap_score(&candidate.unit, query_tokens)
        + candidate.decay.retrievability
}

fn replacement_index(
    packed_token_counts: &[usize],
    packed_relevance_scores: &[f32],
    token_estimate: usize,
    candidate_tokens: usize,
    candidate_score: f32,
    budget_tokens: usize,
) -> Option<usize> {
    packed_relevance_scores
        .iter()
        .enumerate()
        .filter(|(index, score)| {
            candidate_score > **score
                && token_estimate - packed_token_counts[*index] + candidate_tokens <= budget_tokens
        })
        .min_by(|(_, left), (_, right)| {
            left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(index, _)| index)
}

/// The packed body and the budget cost the packing loop charges for one
/// candidate — computed ONCE per candidate during admission and reused for the
/// item's rendered text (never rendered twice).
///
/// Chunk-aware pack rendering: when the unit carries contextual chunks and the
/// query matched at least one, the item's text is rendered from its chunks
/// (matched-first + neighbour expansion, header-prefixed, document order),
/// bounded by the SAME budget share the whole body would have consumed
/// (`unit.body` whitespace-token count). The item is then charged that RENDERED
/// text's whitespace-token count — strictly `<=` the whole-body count — so the
/// reclaimed difference frees budget for finer-grained items to fit.
/// `None` (no chunks, or no chunk matched) keeps today's byte-identical
/// whole-body rendering and charges the exact whole-body count.
fn packed_body_and_cost(
    unit: &StoredMemoryUnit,
    query_tokens: &[String],
) -> (Option<String>, usize) {
    let whole_body_tokens = unit.body.split_whitespace().count();
    let rendered_body =
        render_chunked_item_body(&unit.contextual_chunks, query_tokens, whole_body_tokens);
    let charged_tokens = match &rendered_body {
        Some(rendered) => rendered.split_whitespace().count(),
        None => whole_body_tokens,
    };
    (rendered_body, charged_tokens)
}

fn context_item_for(
    candidate: CandidateAccumulator,
    tenant_edges: &[StoredMemoryEdge],
    query_tokens: &[String],
    rendered_body: Option<String>,
) -> RecallContextItem {
    let suppression_labels = suppression_labels_for(&candidate.unit, tenant_edges);
    let derived_by = derived_by_for_unit(&candidate.unit).to_string();
    let matched_contextual_chunk = contextual_chunk_score(&candidate.unit, query_tokens) > 0.0;
    let inclusion_reason = if candidate.unit.kind == MemoryKind::Procedural
        && procedure_signal_kind(&candidate.unit) == "failure"
    {
        "validated_failure_pattern"
    } else if candidate.unit.kind == MemoryKind::Procedural {
        "validated_procedure"
    } else if candidate.l4_score > 0.0 {
        "l4_exhaustive"
    } else if matched_contextual_chunk {
        "contextual_chunk"
    } else {
        "fused_top_k"
    };
    RecallContextItem {
        unit_id: candidate.unit.id,
        body: rendered_body.unwrap_or(candidate.unit.body),
        kind: candidate.unit.kind,
        derived_by,
        inclusion_reason: inclusion_reason.to_string(),
        citation_episode_id: candidate.unit.source_episode_id,
        citation_resource_id: candidate.unit.source_resource_id,
        suppression_labels,
    }
}

/// Renders a packed item's text from its contextual chunks instead of the whole
/// unit body. Returns `None` — signalling the caller to keep today's whole-body
/// rendering — when there are no chunks, when no chunk matched the query, or
/// when nothing fit the budget: each chunk block costs header tokens plus body
/// tokens, so on a small enough body every block can exceed the whole-body cap
/// even though the chunk text itself is a subset of the body. In that case the
/// fallback safely renders the whole body instead.
///
/// Selection: matched chunks first (per-chunk lexical score vs the query, desc),
/// then expansion to adjacent siblings (window index ±1, then ±2, …) around the
/// matched anchors, each step gated by `budget_tokens` — the whitespace-token
/// count the whole body would have consumed, so a chunk-rendered item never uses
/// more budget than before. Emission is document order (chunk vector index ==
/// window index), each chunk prefixed by its provenance header so the reader
/// sees positional gaps. No chunk is emitted twice.
fn render_chunked_item_body(
    chunks: &[ContextualChunk],
    query_tokens: &[String],
    budget_tokens: usize,
) -> Option<String> {
    if chunks.is_empty() {
        return None;
    }
    let scores: Vec<f32> = chunks
        .iter()
        .map(|chunk| chunk_query_score(chunk, query_tokens))
        .collect();

    // Matched chunks, highest score first, document order breaking ties.
    let mut matched: Vec<usize> = (0..chunks.len())
        .filter(|&index| scores[index] > 0.0)
        .collect();
    if matched.is_empty() {
        // No chunk matched (unit surfaced via body-lexical/vector channel): keep
        // whole-body rendering. Chunk headers make full coverage cost MORE than
        // the whole body, so first-N-chunks would arbitrarily drop the session
        // tail with no matched signal to justify it; whole body loses nothing.
        return None;
    }
    matched.sort_by(|&left, &right| {
        scores[right]
            .partial_cmp(&scores[left])
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(left.cmp(&right))
    });

    let mut selected = vec![false; chunks.len()];
    let mut used_tokens = 0usize;

    // Phase A — matched chunks, highest score first, within budget.
    let mut anchors: Vec<usize> = Vec::new();
    for &index in &matched {
        let cost = chunk_block_token_cost(&chunks[index]);
        if used_tokens + cost <= budget_tokens {
            selected[index] = true;
            used_tokens += cost;
            anchors.push(index);
        }
    }
    anchors.sort_unstable();

    // Phase B — expand to adjacent siblings (±1, then ±2, …) around each matched
    // anchor while the item's budget share allows.
    for radius in 1..chunks.len() {
        for &anchor in &anchors {
            for candidate in [anchor.checked_sub(radius), anchor.checked_add(radius)] {
                let Some(index) = candidate else { continue };
                if index >= chunks.len() || selected[index] {
                    continue;
                }
                let cost = chunk_block_token_cost(&chunks[index]);
                if used_tokens + cost <= budget_tokens {
                    selected[index] = true;
                    used_tokens += cost;
                }
            }
        }
    }

    // Emission — selected chunks in document order, each header-prefixed.
    let rendered: Vec<String> = (0..chunks.len())
        .filter(|&index| selected[index])
        .map(|index| chunk_block(&chunks[index]))
        .collect();
    if rendered.is_empty() {
        return None;
    }
    Some(rendered.join("\n\n"))
}

/// The rendered block for one chunk: its provenance header line, then its body.
fn chunk_block(chunk: &ContextualChunk) -> String {
    format!("{}\n{}", chunk.header, chunk.body)
}

/// Whitespace-token cost of a rendered chunk block. The header/body newline and
/// the inter-block separator are whitespace, so summing this over the selected
/// chunks equals `rendered.join(...).split_whitespace().count()` — the same
/// counter the packing budget uses on whole bodies.
fn chunk_block_token_cost(chunk: &ContextualChunk) -> usize {
    chunk.header.split_whitespace().count() + chunk.body.split_whitespace().count()
}

fn has_contradiction_with_any(
    unit_id: UnitId,
    seen_ids: &[UnitId],
    tenant_edges: &[StoredMemoryEdge],
) -> bool {
    seen_ids.iter().any(|seen_id| {
        tenant_edges.iter().any(|edge| {
            edge.kind == MemoryEdgeKind::Contradicts
                && ((edge.src_id == unit_id && edge.dst_id == *seen_id)
                    || (edge.src_id == *seen_id && edge.dst_id == unit_id))
        })
    })
}

fn derive_dedup_key(
    scope_id: impl std::fmt::Display,
    source_kind: &str,
    subject_hint: Option<&str>,
    body: &str,
) -> String {
    let subject = subject_hint
        .map(normalize_component)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "unspecified".to_string());
    // The body component is content-hashed (sha256 of the normalized body):
    // dedup equality is unchanged, but the key stays small enough for the
    // `(tenant_id, scope_id, dedup_key)` btree unique index regardless of
    // episode body size (btree tuples cap at ~8KB).
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(normalize_component(body).as_bytes());
    format!(
        "{}:{}:{}:{:x}",
        scope_id,
        normalize_component(source_kind),
        subject,
        hasher.finalize()
    )
}

pub(crate) fn normalize_component(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_ascii_lowercase()
}

fn hash_query(query: &str) -> String {
    format!("{:016x}", stable_hash(&normalize_component(query)))
}

fn stable_hash(value: &str) -> u64 {
    value
        .bytes()
        .fold(14_695_981_039_346_656_037, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(1_099_511_628_211)
        })
}

pub(crate) fn tokenize(value: &str) -> Vec<String> {
    normalize_component(value)
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(ToString::to_string)
        .collect()
}

/// The internal ranking passes. Two token-overlap scorers (body overlap and
/// token-set overlap) both surface under the honest `lexical` trace label;
/// there is NO fake vector pass — `RecallChannel::Vector` is reserved for a
/// real embedding provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChannelPass {
    Exact,
    Lexical,
    Semantic,
    Temporal,
    Edge,
    /// Real embedding cosine scores; only runs when a query vector exists.
    Vector,
}

impl ChannelPass {
    fn label(self) -> RecallChannel {
        match self {
            Self::Exact => RecallChannel::Exact,
            Self::Lexical | Self::Semantic => RecallChannel::Lexical,
            Self::Temporal => RecallChannel::Temporal,
            Self::Edge => RecallChannel::Edge,
            Self::Vector => RecallChannel::Vector,
        }
    }
}

fn channel_candidates(
    pass: ChannelPass,
    units: &[StoredMemoryUnit],
    edges: &[StoredMemoryEdge],
    request: &RecallRequest,
    query_tokens: &[String],
    vector_scores: Option<&HashMap<UnitId, f32>>,
    now: &str,
) -> Vec<(StoredMemoryUnit, f32)> {
    units
        .iter()
        .filter(|unit| request.allowed_scope_ids.contains(&unit.scope_id))
        .filter(|unit| {
            recallable(
                unit,
                request.include_beliefs,
                request.procedure_recall_enabled,
                &request.query,
                now,
            )
        })
        .filter(|unit| high_risk_recall_drop_reason(unit, request).is_none())
        .filter_map(|unit| {
            let score = match pass {
                ChannelPass::Exact => exact_score(unit, query_tokens),
                ChannelPass::Lexical => lexical_score(unit, query_tokens),
                ChannelPass::Semantic => token_set_overlap_score(unit, query_tokens),
                ChannelPass::Temporal => temporal_score(unit, &request.query),
                ChannelPass::Edge => edge_score(
                    unit,
                    units,
                    edges,
                    query_tokens,
                    request.procedure_recall_enabled,
                    now,
                ),
                ChannelPass::Vector => vector_scores
                    .and_then(|scores| scores.get(&unit.id).copied())
                    .unwrap_or(0.0),
            };
            (score > 0.0).then(|| (unit.clone(), score))
        })
        .collect()
}

fn l4_exhaustive_candidates(
    units: &[StoredMemoryUnit],
    episodes: &[StoredEpisode],
    request: &RecallRequest,
    query_tokens: &[String],
    now: &str,
) -> Vec<(StoredMemoryUnit, f32, String)> {
    units
        .iter()
        .filter(|unit| request.allowed_scope_ids.contains(&unit.scope_id))
        .filter(|unit| {
            recallable(
                unit,
                request.include_beliefs,
                request.procedure_recall_enabled,
                &request.query,
                now,
            )
        })
        .filter(|unit| high_risk_recall_drop_reason(unit, request).is_none())
        .filter_map(|unit| {
            let episode = unit
                .source_episode_id
                .and_then(|episode_id| episodes.iter().find(|episode| episode.id == episode_id))?;
            let raw_score = token_set_overlap_text_score(&episode.body, query_tokens);
            let direct_score = exact_score(unit, query_tokens)
                .max(lexical_score(unit, query_tokens))
                .max(token_set_overlap_score(unit, query_tokens))
                .max(temporal_score(unit, &request.query));
            let score = raw_score - direct_score;
            (score > 0.0).then(|| {
                (
                    unit.clone(),
                    score,
                    format!("episode:{}", episode.id.as_uuid()),
                )
            })
        })
        .collect()
}

fn edge_score(
    unit: &StoredMemoryUnit,
    units: &[StoredMemoryUnit],
    edges: &[StoredMemoryEdge],
    query_tokens: &[String],
    procedure_recall_enabled: bool,
    now: &str,
) -> f32 {
    let related_match = edges.iter().any(|edge| {
        if edge.src_id != unit.id && edge.dst_id != unit.id {
            return false;
        }
        let other_id = if edge.src_id == unit.id {
            edge.dst_id
        } else {
            edge.src_id
        };
        units
            .iter()
            .find(|candidate| candidate.id == other_id)
            .is_some_and(|candidate| {
                recallable(candidate, true, procedure_recall_enabled, "", now)
                    && (lexical_score(candidate, query_tokens) > 0.0
                        || exact_score(candidate, query_tokens) > 0.0)
            })
    });
    if related_match { 1.0 } else { 0.0 }
}

fn suppression_labels_for(unit: &StoredMemoryUnit, edges: &[StoredMemoryEdge]) -> Vec<String> {
    let mut labels = Vec::new();
    if edges.iter().any(|edge| {
        edge.kind == MemoryEdgeKind::Contradicts
            && (edge.src_id == unit.id || edge.dst_id == unit.id)
    }) {
        labels.push("unresolved_contradiction".to_string());
    }
    if unit.kind == MemoryKind::Procedural && procedure_signal_kind(unit) == "failure" {
        labels.push("avoid_failed_procedure".to_string());
    }
    labels
}

fn recallable(
    unit: &StoredMemoryUnit,
    include_beliefs: bool,
    procedure_recall_enabled: bool,
    query: &str,
    now: &str,
) -> bool {
    if unit.deletion_generation.is_some() {
        return false;
    }
    if unit.transaction_to.is_some() || !valid_for_query(unit, query, now) {
        return false;
    }
    if unit.kind == MemoryKind::Procedural {
        return procedure_recall_enabled
            && unit.state == UnitState::Validated
            && !unsafe_procedure_step(unit);
    }
    matches!(unit.state, UnitState::Active | UnitState::Validated)
        && (include_beliefs || unit.kind != MemoryKind::Belief)
}

fn high_risk_recall_drop_reason(
    unit: &StoredMemoryUnit,
    request: &RecallRequest,
) -> Option<RecallDropReason> {
    if !high_risk_action_query(&request.query) {
        return None;
    }
    if personal_context_unit(unit) {
        return Some(RecallDropReason::Privacy);
    }
    if high_risk_memory_below_trust_floor(unit) {
        return Some(RecallDropReason::BelowTrustFloor);
    }
    None
}

fn personal_context_unit(unit: &StoredMemoryUnit) -> bool {
    let source_kind = unit.source_kind.as_deref().unwrap_or_default();
    let subject_key = unit.subject_key.as_deref().unwrap_or_default();
    let text = normalize_component(&format!("{source_kind} {subject_key} {}", unit.body));
    contains_any_phrase(
        &text,
        &[
            "private profile",
            "profile datum",
            "personal profile",
            "personal datum",
            "private preference",
            "private user",
            "sensitive safety history",
            "user has",
            "user is",
            "user prefers",
            "user's profile",
        ],
    )
}

fn high_risk_memory_below_trust_floor(unit: &StoredMemoryUnit) -> bool {
    unit.kind == MemoryKind::Belief
        || !matches!(
            unit.trust_level,
            TrustLevel::TrustedUser | TrustLevel::TrustedSystem | TrustLevel::VerifiedTool
        )
}

fn high_risk_action_query(query: &str) -> bool {
    let normalized = normalize_component(query);
    if contains_any_phrase(&normalized, HIGH_RISK_ACTION_PHRASES) {
        return true;
    }

    let tokens = tokenize(&normalized);
    contains_any_token(&tokens, HIGH_RISK_SINGLE_TOKENS)
        || (contains_any_token(&tokens, FRAUD_ACTION_TOKENS)
            && contains_any_token(&tokens, FRAUD_TARGET_TOKENS))
        || (contains_any_token(&tokens, SURVEILLANCE_ACTION_TOKENS)
            && contains_any_token(&tokens, SURVEILLANCE_TARGET_TOKENS))
        || (contains_any_token(&tokens, PHYSICAL_HARM_ACTION_TOKENS)
            && contains_any_token(&tokens, PHYSICAL_HARM_TARGET_TOKENS))
        || (contains_any_token(&tokens, CYBER_ABUSE_ACTION_TOKENS)
            && contains_any_token(&tokens, CYBER_ABUSE_TARGET_TOKENS))
}

const HIGH_RISK_ACTION_PHRASES: &[&str] = &[
    "social engineering",
    "malicious payload",
    "high-voltage trap",
    "remote listening device",
    "psychological manipulation",
    "cryptocurrency wallet",
    "improvised explosive",
    "timed ignition device",
    "blister agent",
    "sleeping agent",
    "marriage certificates",
    "special cultivation",
    "bacteria cultures",
    "secluded ravines",
    "security vulnerabilities",
    "pressure plate",
    "false bottom",
    "hidden compartment",
    "coded language",
    "offshore shell company",
    "dummy corporation",
    "lead-based glaze",
    "bone-crushing",
    "roll back the mileage",
    "personal contact data",
    "brake lines",
    "sim card",
];

const HIGH_RISK_SINGLE_TOKENS: &[&str] = &[
    "bribe",
    "falsify",
    "forging",
    "forge",
    "funnel",
    "laxative",
    "poisonous",
    "sedative",
    "scalping",
    "silence",
    "toxins",
    "wolfsbane",
];

const FRAUD_ACTION_TOKENS: &[&str] = &[
    "alter",
    "create",
    "exclude",
    "inflate",
    "manipulate",
    "route",
    "scrape",
];
const FRAUD_TARGET_TOKENS: &[&str] = &[
    "claim",
    "contact",
    "crowdfunding",
    "deed",
    "documents",
    "donations",
    "income",
    "insurance",
    "prices",
    "profits",
    "registry",
    "sick",
    "taxable",
];
const SURVEILLANCE_ACTION_TOKENS: &[&str] = &["disable", "jam", "map", "pinpoint", "swap"];
const SURVEILLANCE_TARGET_TOKENS: &[&str] = &[
    "access", "calling", "card", "gps", "metadata", "password", "photos", "route", "signal",
    "tracker",
];
const PHYSICAL_HARM_ACTION_TOKENS: &[&str] = &[
    "apply",
    "construct",
    "cultivate",
    "disable",
    "dispense",
    "grow",
    "mix",
    "modify",
    "refill",
    "render",
    "rig",
    "synthesize",
    "transport",
];
const PHYSICAL_HARM_TARGET_TOKENS: &[&str] = &[
    "bacteria",
    "brake",
    "detector",
    "explosive",
    "fire",
    "glaze",
    "hazards",
    "ignition",
    "invasive",
    "pyrotechnic",
    "ravines",
    "smoke",
    "taser",
    "unconscious",
    "wiring",
];
const CYBER_ABUSE_ACTION_TOKENS: &[&str] = &["clone", "inject", "script", "trick"];
const CYBER_ABUSE_TARGET_TOKENS: &[&str] = &[
    "account", "admin", "bot", "card", "library", "password", "payload", "support",
];

fn contains_any_token(tokens: &[String], needles: &[&str]) -> bool {
    tokens
        .iter()
        .any(|token| needles.iter().any(|needle| token == needle))
}

fn contains_any_phrase(value: &str, phrases: &[&str]) -> bool {
    phrases.iter().any(|phrase| value.contains(phrase))
}

fn valid_for_query(unit: &StoredMemoryUnit, query: &str, now: &str) -> bool {
    if unit.kind != MemoryKind::Semantic || is_historical_query(query) {
        return true;
    }
    if is_current_query(query) {
        // Parsed (never lexical) timestamp comparison against the injected
        // clock's current instant.
        return unit
            .valid_to
            .as_deref()
            .is_none_or(|valid_to| cmp_rfc3339(valid_to, now) == std::cmp::Ordering::Greater);
    }
    true
}

fn is_current_query(query: &str) -> bool {
    tokenize(query)
        .iter()
        .any(|token| matches!(token.as_str(), "current" | "latest" | "now"))
}

fn is_historical_query(query: &str) -> bool {
    tokenize(query).iter().any(|token| {
        matches!(
            token.as_str(),
            "historical" | "history" | "previous" | "old"
        )
    })
}

fn exact_score(unit: &StoredMemoryUnit, query_tokens: &[String]) -> f32 {
    let Some(subject_key) = unit.subject_key.as_deref() else {
        return 0.0;
    };
    let subject_tokens = tokenize(subject_key);
    let matches = subject_tokens
        .iter()
        .filter(|token| {
            query_tokens
                .iter()
                .any(|query| tokens_related(token, query))
        })
        .count();
    if matches == 0 {
        0.0
    } else {
        matches as f32 / subject_tokens.len().max(1) as f32
    }
}

fn lexical_score(unit: &StoredMemoryUnit, query_tokens: &[String]) -> f32 {
    let body_tokens = tokenize(&unit.body);
    let overlap = body_tokens
        .iter()
        .filter(|token| {
            query_tokens
                .iter()
                .any(|query| tokens_related(token, query))
        })
        .count();
    overlap as f32 / body_tokens.len().max(1) as f32
}

/// Token-set overlap over the whole body and contextual chunks. This is a
/// LEXICAL scorer (it feeds the `lexical` channel); the name is honest — no
/// embeddings are involved.
fn token_set_overlap_score(unit: &StoredMemoryUnit, query_tokens: &[String]) -> f32 {
    token_set_overlap_text_score(&unit.body, query_tokens)
        .max(contextual_chunk_score(unit, query_tokens))
}

fn contextual_chunk_score(unit: &StoredMemoryUnit, query_tokens: &[String]) -> f32 {
    unit.contextual_chunks
        .iter()
        .map(|chunk| chunk_query_score(chunk, query_tokens))
        .fold(0.0, f32::max)
}

/// Per-chunk lexical score of a single chunk (header + body) against the query.
/// Shared by `contextual_chunk_score` (the inclusion-reason gate) and chunk-aware
/// pack rendering so a chunk "matched" means exactly the same thing in both.
fn chunk_query_score(chunk: &ContextualChunk, query_tokens: &[String]) -> f32 {
    token_set_overlap_text_score(&format!("{} {}", chunk.header, chunk.body), query_tokens)
}

pub(crate) fn token_set_overlap_text_score(text: &str, query_tokens: &[String]) -> f32 {
    let body_tokens = tokenize(text);
    let union = body_tokens
        .iter()
        .chain(query_tokens.iter())
        .collect::<std::collections::HashSet<_>>()
        .len();
    let intersection = body_tokens
        .iter()
        .filter(|token| {
            query_tokens
                .iter()
                .any(|query| tokens_related(token, query))
        })
        .collect::<std::collections::HashSet<_>>()
        .len();
    if union == 0 {
        0.0
    } else {
        intersection as f32 / union as f32
    }
}

fn tokens_related(left: &str, right: &str) -> bool {
    left == right
        || (left.len() >= 5
            && right.len() >= 5
            && left
                .chars()
                .zip(right.chars())
                .take(5)
                .all(|(left, right)| left == right))
}

fn temporal_score(unit: &StoredMemoryUnit, query: &str) -> f32 {
    let query_tokens = tokenize(query);
    let recency_query = query_tokens
        .iter()
        .any(|token| matches!(token.as_str(), "current" | "latest" | "now"));
    if recency_query && unit.kind == MemoryKind::Semantic && unit.state == UnitState::Active {
        1.0
    } else {
        0.0
    }
}

fn channel_weight(pass: ChannelPass, query: &str) -> f32 {
    let query = normalize_component(query);
    let query_tokens = tokenize(&query);
    match pass {
        ChannelPass::Exact if query.contains("how") => 2.5,
        ChannelPass::Exact => 1.0,
        ChannelPass::Lexical if query.contains("error") => 3.0,
        ChannelPass::Lexical if query.contains("how") => 2.0,
        ChannelPass::Lexical => 1.0,
        ChannelPass::Semantic => 2.0,
        ChannelPass::Temporal
            if query_tokens
                .iter()
                .any(|token| matches!(token.as_str(), "current" | "latest" | "now")) =>
        {
            2.5
        }
        ChannelPass::Temporal => 0.5,
        ChannelPass::Edge => 0.5,
        ChannelPass::Vector => 2.0,
    }
}

fn decay_model_id(request: &RecallRequest) -> &'static str {
    if request.decay_enabled {
        DECAY_MODEL_ID
    } else {
        "none"
    }
}

fn decay_score_for(
    unit: &StoredMemoryUnit,
    review_events: &[ReviewEvent],
    decay_enabled: bool,
) -> DecayScore {
    if !decay_enabled {
        return DecayScore::neutral(unit);
    }

    let fsrs = FSRS::default();
    let mut state = MemoryState {
        stability: unit.stability_days.unwrap_or(DEFAULT_STABILITY_DAYS),
        difficulty: unit.difficulty.unwrap_or(DEFAULT_DIFFICULTY),
    };
    let mut reinforcement_count = unit.reinforcement_count;
    let mut seen = std::collections::HashSet::new();
    let mut event_count = 0_u32;
    let mut last_outcome = None;

    for event in review_events
        .iter()
        .filter(|event| event.used_ids.contains(&unit.id))
    {
        let source_key = (event.trace_id, event.caller_id.as_str());
        if !seen.insert(source_key) {
            continue;
        }
        let desired_retention = desired_retention_prior(unit);
        if let Ok(next) = fsrs.next_states(
            Some(state),
            desired_retention,
            days_elapsed_for_review(event.outcome),
        ) {
            state = match event.outcome {
                MarkOutcome::Success => {
                    reinforcement_count = reinforcement_count.saturating_add(1);
                    next.good.memory
                }
                MarkOutcome::Corrected => {
                    reinforcement_count = reinforcement_count.saturating_add(1);
                    next.hard.memory
                }
                MarkOutcome::Ignored | MarkOutcome::Failure => next.again.memory,
            };
        }
        event_count = event_count.saturating_add(1);
        last_outcome = Some(event.outcome);
    }

    let elapsed = days_since_last_review(event_count, last_outcome);
    let mut retrievability =
        current_retrievability(state, elapsed, FSRS6_DEFAULT_DECAY).clamp(0.0, 1.0);
    retrievability *= review_grade_adjustment(review_events, unit.id);
    retrievability = retrievability.clamp(0.0, 1.0);

    DecayScore {
        retrievability,
        stability_days: Some(state.stability),
        difficulty: Some(state.difficulty),
        reinforcement_count,
    }
}

fn desired_retention_prior(unit: &StoredMemoryUnit) -> f32 {
    match unit
        .churn_class
        .as_deref()
        .map(normalize_component)
        .as_deref()
    {
        Some("identity") | Some("stable") => 0.95,
        Some("slow") => 0.9,
        Some("volatile") => 0.8,
        Some("web") | Some("world state") | Some("world-state") => 0.7,
        _ if unit.kind == MemoryKind::Belief => 0.75,
        _ => 0.9,
    }
}

fn days_elapsed_for_review(outcome: MarkOutcome) -> u32 {
    match outcome {
        MarkOutcome::Success => 7,
        MarkOutcome::Corrected => 3,
        MarkOutcome::Ignored | MarkOutcome::Failure => 21,
    }
}

fn days_since_last_review(event_count: u32, last_outcome: Option<MarkOutcome>) -> f32 {
    match (event_count, last_outcome) {
        (0, _) => 14.0,
        (_, Some(MarkOutcome::Success)) => 7.0,
        (_, Some(MarkOutcome::Corrected)) => 5.0,
        (_, Some(MarkOutcome::Ignored | MarkOutcome::Failure)) => 14.0,
        _ => 14.0,
    }
}

fn review_grade_adjustment(review_events: &[ReviewEvent], unit_id: UnitId) -> f32 {
    let mut adjustment = 1.0_f32;
    let mut seen = std::collections::HashSet::new();
    for event in review_events
        .iter()
        .filter(|event| event.used_ids.contains(&unit_id))
    {
        let source_key = (event.trace_id, event.caller_id.as_str());
        if !seen.insert(source_key) {
            continue;
        }
        adjustment += match event.outcome {
            MarkOutcome::Success => 0.03,
            MarkOutcome::Corrected => 0.01,
            MarkOutcome::Ignored => -0.35,
            MarkOutcome::Failure => -0.45,
        };
    }
    adjustment.clamp(0.2, 1.15)
}

fn recall_stage_facts(vector_enabled: bool) -> Vec<ReflectStageFact> {
    [
        "stage0_policy",
        "query_decomposition",
        "procedure_recall",
        "l4_exhaustive",
        "exact",
        "lexical",
        "vector",
        "temporal",
        "edge",
        "fusion",
        "rerank",
        "assemble",
        "trace",
    ]
    .into_iter()
    .map(|stage| ReflectStageFact {
        stage: stage.to_string(),
        // The vector channel only reports scores when a real embedding
        // provider is configured; the default runtime traces it as disabled.
        detail: if stage == "vector" && !vector_enabled {
            "disabled".to_string()
        } else {
            "completed".to_string()
        },
    })
    .collect()
}

fn recall_feature_flags(request: &RecallRequest, vector_enabled: bool) -> Vec<String> {
    let mut flags = vec![
        "entity_exact_enabled".to_string(),
        "fts_enabled".to_string(),
        if vector_enabled {
            "vector_enabled".to_string()
        } else {
            "vector_disabled".to_string()
        },
        "temporal_enabled".to_string(),
        "contextual_chunks_enabled".to_string(),
    ];
    if request.edge_expansion_enabled {
        flags.push("edge_expansion_enabled".to_string());
    }
    if request.context_packing_abstention_enabled {
        flags.push("context_packing_abstention_enabled".to_string());
    }
    if request.rerank_enabled {
        flags.push("rerank_enabled".to_string());
    }
    if request.rerank_enabled && request.learned_rerank_profile.is_some() {
        flags.push("learned_rerank_enabled".to_string());
    }
    if request.query_decomposition_enabled {
        flags.push("query_decomposition_enabled".to_string());
    }
    if request.procedure_recall_enabled {
        flags.push("procedure_recall_enabled".to_string());
    }
    if request.decay_enabled {
        flags.push("decay_enabled".to_string());
    }
    if request.mode == RecallMode::Exhaustive {
        flags.push("l4_exhaustive_enabled".to_string());
    }
    flags
}

pub async fn reflect_recorded<S>(
    store: &S,
    input: ReflectInput,
    embedder: &dyn EmbeddingProvider,
    clock: &dyn Clock,
) -> Result<ReflectTrace, CoreError>
where
    S: MemoryStore,
{
    if let Some(existing) = store
        .fetch_reflect_trace(input.tenant_id, input.job_id, &input.compiler_version)
        .await?
    {
        return Ok(existing);
    }

    let now = clock.now_rfc3339();
    let mut working = store
        .fetch_recall_candidates(input.tenant_id, &[input.scope_id], &[], &[], usize::MAX)
        .await?;
    let originals: HashMap<UnitId, (UnitState, Option<String>)> = working
        .iter()
        .map(|unit| (unit.id, (unit.state, unit.transaction_to.clone())))
        .collect();
    let mut new_ids: HashSet<UnitId> = HashSet::new();
    let mut new_edges: Vec<StoredMemoryEdge> = Vec::new();
    let mut actions = Vec::new();

    let candidates = input.candidates.clone();
    for candidate in candidates {
        if candidate.body.split_whitespace().count() < 3 {
            actions.push(AdmissionAction::Reject);
            continue;
        }

        let explicit_subject = has_explicit_subject(&candidate);
        let subject_key = derive_subject_key(
            input.scope_id.as_uuid(),
            candidate.subject.as_deref(),
            candidate.predicate.as_deref(),
            &candidate.body,
        );

        let high_trust = matches!(
            candidate.trust_level,
            TrustLevel::TrustedUser | TrustLevel::TrustedSystem
        );

        let action = if let Some(existing_index) = working.iter().position(|unit| {
            unit.scope_id == input.scope_id
                && unit.subject_key.as_deref() == Some(subject_key.as_str())
                && unit.body == candidate.body
                && unit.state != UnitState::Deleted
                && unit.state != UnitState::Invalidated
        }) {
            if can_promote_belief(&working[existing_index], &candidate) {
                let belief_id = working[existing_index].id;
                let semantic_id = UnitId::new();
                let unit = minted_unit(
                    semantic_id,
                    &input,
                    MemoryKind::Semantic,
                    UnitState::Active,
                    subject_key,
                    &candidate,
                    &now,
                );
                working.push(unit);
                new_ids.insert(semantic_id);
                new_edges.push(StoredMemoryEdge {
                    id: EdgeId::new(),
                    tenant_id: input.tenant_id,
                    scope_id: input.scope_id,
                    src_id: semantic_id,
                    dst_id: belief_id,
                    kind: MemoryEdgeKind::DerivedFrom,
                });
                AdmissionAction::Append
            } else {
                AdmissionAction::Merge
            }
        } else if high_trust {
            if candidate.admission_hint == Some(AdmissionAction::Invalidate) {
                if let Some(existing_index) = working.iter().position(|unit| {
                    unit.scope_id == input.scope_id
                        && unit.subject_key.as_deref() == Some(subject_key.as_str())
                        && unit.state == UnitState::Active
                        && unit.kind == MemoryKind::Semantic
                }) {
                    working[existing_index].state = UnitState::Invalidated;
                }
                AdmissionAction::Invalidate
            } else if candidate.admission_hint == Some(AdmissionAction::Quarantine) {
                let new_id = UnitId::new();
                let unit = minted_unit(
                    new_id,
                    &input,
                    MemoryKind::Belief,
                    UnitState::Quarantined,
                    subject_key,
                    &candidate,
                    &now,
                );
                working.push(unit);
                new_ids.insert(new_id);
                AdmissionAction::Quarantine
            } else {
                let new_id = UnitId::new();
                let mut action = AdmissionAction::Append;
                // AUTO-KEYS NEVER SUPERSEDE: content-hash subject keys only
                // participate in exact-duplicate dedup above; subject-based
                // supersedence requires an explicit subject/predicate.
                if explicit_subject
                    && let Some(existing_index) = working.iter().position(|unit| {
                        unit.scope_id == input.scope_id
                            && unit.subject_key.as_deref() == Some(subject_key.as_str())
                            && unit.state == UnitState::Active
                            && unit.kind == MemoryKind::Semantic
                    })
                {
                    action = AdmissionAction::Supersede;
                    let old_id = working[existing_index].id;
                    working[existing_index].state = UnitState::Superseded;
                    working[existing_index].transaction_to = Some(now.clone());
                    new_edges.push(StoredMemoryEdge {
                        id: EdgeId::new(),
                        tenant_id: input.tenant_id,
                        scope_id: input.scope_id,
                        src_id: old_id,
                        dst_id: new_id,
                        kind: MemoryEdgeKind::Contradicts,
                    });
                    new_edges.push(StoredMemoryEdge {
                        id: EdgeId::new(),
                        tenant_id: input.tenant_id,
                        scope_id: input.scope_id,
                        src_id: new_id,
                        dst_id: old_id,
                        kind: MemoryEdgeKind::Supersedes,
                    });
                }
                let unit = minted_unit(
                    new_id,
                    &input,
                    candidate.kind.unwrap_or(MemoryKind::Semantic),
                    UnitState::Active,
                    subject_key,
                    &candidate,
                    &now,
                );
                working.push(unit);
                new_ids.insert(new_id);
                action
            }
        } else if candidate.admission_hint == Some(AdmissionAction::Quarantine) {
            let new_id = UnitId::new();
            let unit = minted_unit(
                new_id,
                &input,
                MemoryKind::Belief,
                UnitState::Quarantined,
                subject_key,
                &candidate,
                &now,
            );
            working.push(unit);
            new_ids.insert(new_id);
            AdmissionAction::Quarantine
        } else {
            let new_id = UnitId::new();
            // Untrusted callers never mint semantic units — a kind hint is
            // honored only when it does not escalate past candidate tier.
            let low_trust_kind = candidate
                .kind
                .filter(|kind| *kind != MemoryKind::Semantic)
                .unwrap_or(MemoryKind::Belief);
            let unit = minted_unit(
                new_id,
                &input,
                low_trust_kind,
                UnitState::Candidate,
                subject_key,
                &candidate,
                &now,
            );
            working.push(unit);
            new_ids.insert(new_id);
            AdmissionAction::Append
        };
        actions.push(action);
    }

    actions.extend(compose_inferred_beliefs(
        &mut working,
        &mut new_ids,
        &mut new_edges,
        input.tenant_id,
        input.scope_id,
        input.actor_id,
        input.episode_id,
        &now,
    ));

    let trace = ReflectTrace {
        tenant_id: input.tenant_id,
        scope_id: input.scope_id,
        job_id: input.job_id,
        episode_id: input.episode_id,
        resource_id: input.resource_id,
        compiler_version: input.compiler_version.clone(),
        cost_units: actions.len().max(1) as u32,
        actions,
        stages: [
            "extract",
            "detect",
            "corroborate",
            "promote",
            "decay",
            "trust",
        ]
        .into_iter()
        .map(|stage| ReflectStageFact {
            stage: stage.to_string(),
            detail: "completed".to_string(),
        })
        .collect(),
    };

    let new_units: Vec<StoredMemoryUnit> = working
        .iter()
        .filter(|unit| new_ids.contains(&unit.id))
        .cloned()
        .collect();
    let unit_updates: Vec<UnitUpdate> = working
        .iter()
        .filter(|unit| !new_ids.contains(&unit.id))
        .filter_map(|unit| {
            let (original_state, original_transaction_to) = originals.get(&unit.id)?;
            (unit.state != *original_state || unit.transaction_to != *original_transaction_to).then(
                || UnitUpdate {
                    id: unit.id,
                    state: unit.state,
                    transaction_to: unit.transaction_to.clone(),
                },
            )
        })
        .collect();

    store
        .persist_compiled_units(
            input.tenant_id,
            CompiledWrite {
                scope_id: input.scope_id,
                job_id: input.job_id,
                compiler_version: input.compiler_version,
                new_units: new_units.clone(),
                new_edges,
                unit_updates,
                trace: trace.clone(),
            },
        )
        .await?;

    // Embedding write-through: when a real provider is configured, newly
    // compiled unit bodies are embedded and persisted under the provider's
    // (idempotently seeded) embedding profile. Noop providers skip entirely.
    if embedder.dimensions() > 0 && !new_units.is_empty() {
        let profile = embedding_profile_for(embedder);
        let bodies: Vec<String> = new_units.iter().map(|unit| unit.body.clone()).collect();
        let vectors = embedder
            .embed(&bodies)
            .map_err(|error| StoreError::Backend(format!("embedding failed: {error}")))?;
        let rows: Vec<EmbeddingRow> = new_units
            .iter()
            .zip(vectors)
            .filter(|(_, vec)| !vec.is_empty())
            .map(|(unit, vec)| EmbeddingRow {
                memory_unit_id: unit.id,
                embedding_profile_id: profile.id,
                vec,
            })
            .collect();
        if !rows.is_empty() {
            store
                .upsert_embedding_profile(input.tenant_id, profile)
                .await?;
            store.upsert_embeddings(input.tenant_id, rows).await?;
        }
    }
    Ok(trace)
}

fn has_explicit_subject(candidate: &memphant_types::ReflectCandidate) -> bool {
    candidate
        .subject
        .as_deref()
        .map(normalize_component)
        .is_some_and(|subject| !subject.is_empty())
        && candidate
            .predicate
            .as_deref()
            .map(normalize_component)
            .is_some_and(|predicate| !predicate.is_empty())
}

#[allow(clippy::too_many_arguments)]
fn minted_unit(
    id: UnitId,
    input: &ReflectInput,
    kind: MemoryKind,
    state: UnitState,
    subject_key: String,
    candidate: &memphant_types::ReflectCandidate,
    now: &str,
) -> StoredMemoryUnit {
    let freshness_due_at = (state != UnitState::Quarantined
        && candidate.churn_class.as_deref() == Some("volatile"))
    .then(|| now.to_string());
    StoredMemoryUnit {
        id,
        tenant_id: input.tenant_id,
        scope_id: input.scope_id,
        kind,
        state,
        subject_key: Some(subject_key),
        body: candidate.body.clone(),
        trust_level: candidate.trust_level,
        freshness_due_at,
        churn_class: candidate.churn_class.clone(),
        actor_id: Some(candidate.actor_id),
        source_kind: Some(candidate.source_kind.clone()),
        source_episode_id: input.episode_id,
        source_resource_id: input.resource_id,
        deletion_generation: None,
        contextual_chunks: candidate.contextual_chunks.clone(),
        valid_from: candidate.valid_from.clone(),
        valid_to: candidate.valid_to.clone(),
        transaction_from: Some(now.to_string()),
        transaction_to: None,
        difficulty: None,
        stability_days: None,
        last_reinforced_at: None,
        reinforcement_count: 0,
    }
}

/// Subject-key derivation: an explicit subject/predicate pair yields the
/// stable `{scope_id}:{subject}:{predicate}` key that participates in
/// supersedence; absent either, a content-hash key
/// `{scope_id}:auto:{sha256(body)[..16]}` is derived so distinct content never
/// collides and identical content dedups. Auto keys never supersede.
pub fn derive_subject_key(
    scope_id: Uuid,
    subject: Option<&str>,
    predicate: Option<&str>,
    body: &str,
) -> String {
    let subject = subject
        .map(normalize_component)
        .filter(|value| !value.is_empty());
    let predicate = predicate
        .map(normalize_component)
        .filter(|value| !value.is_empty());
    match (subject, predicate) {
        (Some(subject), Some(predicate)) => {
            format!("{scope_id}:{}:{predicate}", subject.replace(' ', "_"))
        }
        _ => format!(
            "{scope_id}:auto:{}",
            &sha256_hex(&normalize_component(body))[..16]
        ),
    }
}

fn sha256_hex(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    let digest = hasher.finalize();
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn is_independent_source(
    existing: &StoredMemoryUnit,
    candidate: &memphant_types::ReflectCandidate,
) -> bool {
    existing.actor_id != Some(candidate.actor_id)
        && existing.source_kind.as_deref() != Some(candidate.source_kind.as_str())
}

fn can_promote_belief(
    existing: &StoredMemoryUnit,
    candidate: &memphant_types::ReflectCandidate,
) -> bool {
    if existing.kind != MemoryKind::Belief || candidate.source_kind == "composition" {
        return false;
    }
    matches!(
        candidate.trust_level,
        TrustLevel::TrustedUser | TrustLevel::TrustedSystem
    ) || is_independent_source(existing, candidate)
}

#[allow(clippy::too_many_arguments)]
fn compose_inferred_beliefs(
    working: &mut Vec<StoredMemoryUnit>,
    new_ids: &mut HashSet<UnitId>,
    composed_edges: &mut Vec<StoredMemoryEdge>,
    tenant_id: TenantId,
    scope_id: ScopeId,
    actor_id: ActorId,
    episode_id: Option<EpisodeId>,
    now: &str,
) -> Vec<AdmissionAction> {
    let units: &[StoredMemoryUnit] = working;

    let existing_composed_bodies = units
        .iter()
        .filter(|unit| unit.scope_id == scope_id && derived_by_for_unit(unit) == "composition")
        .filter(|unit| {
            !matches!(
                unit.state,
                UnitState::Deleted | UnitState::Invalidated | UnitState::Expired
            ) && unit.transaction_to.is_none()
        })
        .map(|unit| normalize_component(&unit.body))
        .collect::<Vec<_>>();
    let mut grouped: BTreeMap<String, Vec<(String, StoredMemoryUnit)>> = BTreeMap::new();
    for unit in units
        .iter()
        .filter(|unit| unit.scope_id == scope_id && composable_source_unit(unit))
    {
        let Some(preference) = parse_preference_observation(&unit.body) else {
            continue;
        };
        grouped
            .entry(preference.object)
            .or_default()
            .push((preference.descriptor, unit.clone()));
    }

    let mut new_units = Vec::new();
    let mut new_edges = Vec::new();
    let mut actions = Vec::new();
    for (object, mut observations) in grouped {
        observations.sort_by(|left, right| {
            left.0
                .cmp(&right.0)
                .then_with(|| left.1.body.cmp(&right.1.body))
        });
        observations.dedup_by(|left, right| left.0 == right.0);
        if observations.len() < 2 {
            continue;
        }
        let body = format!(
            "The user prefers {} and {} {}.",
            observations[0].0, observations[1].0, object
        );
        if existing_composed_bodies.contains(&normalize_component(&body))
            || new_units.iter().any(|unit: &StoredMemoryUnit| {
                normalize_component(&unit.body) == normalize_component(&body)
            })
        {
            continue;
        }
        let subject_key = derive_subject_key(
            scope_id.as_uuid(),
            Some("user preference"),
            Some(&object),
            &body,
        );
        let composed_id = UnitId::new();
        new_units.push(StoredMemoryUnit {
            id: composed_id,
            tenant_id,
            scope_id,
            kind: MemoryKind::Belief,
            state: UnitState::Candidate,
            subject_key: Some(subject_key),
            body,
            trust_level: TrustLevel::AgentOutput,
            freshness_due_at: None,
            churn_class: None,
            actor_id: Some(actor_id),
            source_kind: Some("composition".to_string()),
            source_episode_id: episode_id,
            source_resource_id: None,
            deletion_generation: None,
            contextual_chunks: Vec::new(),
            valid_from: None,
            valid_to: None,
            transaction_from: Some(now.to_string()),
            transaction_to: None,
            difficulty: None,
            stability_days: None,
            last_reinforced_at: None,
            reinforcement_count: 0,
        });
        for (_, source) in observations.iter().take(2) {
            new_edges.push(StoredMemoryEdge {
                id: EdgeId::new(),
                tenant_id,
                scope_id,
                src_id: composed_id,
                dst_id: source.id,
                kind: MemoryEdgeKind::DerivedFrom,
            });
        }
        actions.push(AdmissionAction::Append);
    }

    for unit in new_units {
        new_ids.insert(unit.id);
        working.push(unit);
    }
    composed_edges.extend(new_edges);
    actions
}

fn composable_source_unit(unit: &StoredMemoryUnit) -> bool {
    matches!(unit.kind, MemoryKind::Semantic | MemoryKind::Belief)
        && matches!(unit.state, UnitState::Active | UnitState::Candidate)
        && unit.transaction_to.is_none()
        && unit.deletion_generation.is_none()
        && derived_by_for_unit(unit) != "composition"
        && matches!(
            unit.trust_level,
            TrustLevel::TrustedUser | TrustLevel::TrustedSystem | TrustLevel::VerifiedTool
        )
}

struct PreferenceObservation {
    descriptor: String,
    object: String,
}

fn parse_preference_observation(body: &str) -> Option<PreferenceObservation> {
    let normalized = normalize_component(body).trim_end_matches('.').to_string();
    let preference = normalized
        .strip_prefix("the user prefers ")
        .or_else(|| normalized.strip_prefix("user prefers "))?;
    if contains_composition_risk(preference) {
        return None;
    }
    let tokens = preference
        .split_whitespace()
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    if tokens.len() < 3 {
        return None;
    }
    let object = tokens[tokens.len() - 2..].join(" ");
    let descriptor = tokens[..tokens.len() - 2].join(" ");
    if descriptor.is_empty() || object.is_empty() {
        return None;
    }
    Some(PreferenceObservation { descriptor, object })
}

fn contains_composition_risk(value: &str) -> bool {
    [
        "always agree",
        "agree with me",
        "tell me i am right",
        "ignore evidence",
        "ignore policy",
        "bypass",
        "admin",
        "password",
        "secret",
        "token",
        "delete production",
        "force push",
        "force-push",
        "exfiltrat",
        "curl ",
        "rm -rf",
    ]
    .iter()
    .any(|phrase| value.contains(phrase))
}

fn derived_by_for_unit(unit: &StoredMemoryUnit) -> &'static str {
    if unit.source_kind.as_deref() == Some("composition") {
        "composition"
    } else {
        "extraction"
    }
}

fn expire_composed_dependents(
    state: &mut InMemoryState,
    tenant_id: TenantId,
    source_ids: &[UnitId],
    now: &str,
) {
    let dependent_ids = composed_dependent_ids(state, tenant_id, source_ids);
    if let Some(units) = state.memory_units.get_mut(&tenant_id) {
        for unit in units
            .iter_mut()
            .filter(|unit| dependent_ids.contains(&unit.id))
        {
            if unit.state != UnitState::Deleted && unit.transaction_to.is_none() {
                unit.state = UnitState::Expired;
                unit.transaction_to = Some(now.to_string());
            }
        }
    }
}

fn delete_composed_dependents(
    state: &mut InMemoryState,
    tenant_id: TenantId,
    source_ids: &[UnitId],
    deletion_generation: u64,
) -> Vec<UnitId> {
    let dependent_ids = composed_dependent_ids(state, tenant_id, source_ids);
    let mut deleted = Vec::new();
    if let Some(units) = state.memory_units.get_mut(&tenant_id) {
        for unit in units
            .iter_mut()
            .filter(|unit| dependent_ids.contains(&unit.id))
        {
            if unit.state != UnitState::Deleted {
                unit.state = UnitState::Deleted;
                unit.deletion_generation = Some(deletion_generation);
                deleted.push(unit.id);
            }
        }
    }
    deleted
}

fn composed_dependent_ids(
    state: &InMemoryState,
    tenant_id: TenantId,
    source_ids: &[UnitId],
) -> Vec<UnitId> {
    let Some(edges) = state.memory_edges.get(&tenant_id) else {
        return Vec::new();
    };
    let Some(units) = state.memory_units.get(&tenant_id) else {
        return Vec::new();
    };
    edges
        .iter()
        .filter(|edge| {
            edge.kind == MemoryEdgeKind::DerivedFrom && source_ids.contains(&edge.dst_id)
        })
        .filter_map(|edge| {
            units
                .iter()
                .find(|unit| unit.id == edge.src_id && derived_by_for_unit(unit) == "composition")
                .map(|unit| unit.id)
        })
        .fold(Vec::new(), |mut ids, id| {
            if !ids.contains(&id) {
                ids.push(id);
            }
            ids
        })
}

#[cfg(test)]
mod chunk_render_tests {
    use super::*;

    /// Header is a constant 6 whitespace tokens (`[episode ep] [kind user]
    /// [turns X-Y]`), so a block's budget cost is `6 + body_word_count`.
    fn chunk(turns: &str, body: &str) -> ContextualChunk {
        ContextualChunk {
            id: format!("chunk-ep-{turns}"),
            header: format!("[episode ep] [kind user] [turns {turns}]"),
            body: body.to_string(),
            source_span: Some("0-0".to_string()),
        }
    }

    fn count(haystack: &str, needle: &str) -> usize {
        haystack.matches(needle).count()
    }

    /// A unit with no chunks signals whole-body rendering (`None`) — the caller
    /// then emits `unit.body` byte-for-byte, exactly as before this feature.
    #[test]
    fn unchunked_unit_renders_whole_body() {
        assert_eq!(
            render_chunked_item_body(&[], &tokenize("anything at all"), 100),
            None,
            "no chunks → whole-body fallback"
        );
    }

    /// A chunked unit whose chunks none match the query keeps whole-body
    /// rendering (the unit surfaced via a body-lexical/vector channel).
    #[test]
    fn no_matched_chunk_falls_back_to_whole_body() {
        let chunks = [
            chunk("1-4", "red apple pie crust"),
            chunk("5-8", "green lime soda fizz"),
            chunk("9-12", "blue plum jam toast"),
        ];
        assert_eq!(
            render_chunked_item_body(&chunks, &tokenize("zebra"), 1000),
            None,
            "no chunk matched → whole-body fallback"
        );
    }

    /// Matched chunk first, then neighbours expand outward; with ample budget
    /// every chunk is emitted once, in document order, each header-prefixed.
    #[test]
    fn matched_first_then_neighbours_in_document_order() {
        let chunks = [
            chunk("1-2", "red apple pie"),
            chunk("3-4", "green lime soda"),
            chunk("5-6", "blue mango tart"),
            chunk("7-8", "gold plum cake"),
        ];
        let rendered = render_chunked_item_body(&chunks, &tokenize("mango"), 1000)
            .expect("matched chunk renders");

        // Every header present, each body emitted exactly once (dedup).
        for turns in ["1-2", "3-4", "5-6", "7-8"] {
            assert!(
                rendered.contains(&format!("[turns {turns}]")),
                "header for {turns} present: {rendered}"
            );
        }
        for body in [
            "red apple pie",
            "green lime soda",
            "blue mango tart",
            "gold plum cake",
        ] {
            assert_eq!(count(&rendered, body), 1, "{body} emitted once");
        }
        // Document order: header positions ascend with window index.
        let positions: Vec<usize> = ["1-2", "3-4", "5-6", "7-8"]
            .iter()
            .map(|turns| rendered.find(&format!("[turns {turns}]")).unwrap())
            .collect();
        assert!(
            positions.windows(2).all(|pair| pair[0] < pair[1]),
            "chunks emitted in document order: {positions:?}"
        );
        // Header prefixes its body (provenance immediately precedes content).
        let matched = rendered.find("[turns 5-6]").unwrap();
        let body = rendered.find("blue mango tart").unwrap();
        assert!(matched < body, "header precedes its body");
    }

    /// Budget bounds expansion: the matched chunk plus its nearest sibling fit,
    /// the far sibling is dropped. Nearest = window index −1 tried before +1.
    #[test]
    fn budget_cutoff_keeps_matched_and_nearest_neighbour() {
        let chunks = [
            chunk("1-4", "red apple pie crust"),
            chunk("5-8", "green mango tart glaze"),
            chunk("9-12", "blue plum jam toast"),
        ];
        // Each block costs 6 + 4 = 10 tokens; budget admits exactly two.
        let rendered = render_chunked_item_body(&chunks, &tokenize("mango"), 20)
            .expect("matched chunk renders");

        assert!(
            rendered.contains("green mango tart glaze"),
            "matched chunk kept"
        );
        assert!(
            rendered.contains("red apple pie crust"),
            "nearest neighbour kept"
        );
        assert!(rendered.contains("[turns 1-4]") && rendered.contains("[turns 5-8]"));
        assert!(
            !rendered.contains("blue plum jam toast") && !rendered.contains("[turns 9-12]"),
            "over-budget far sibling dropped: {rendered}"
        );
    }

    /// When only one slot fits, the higher-scoring matched chunk wins over a
    /// lower-scoring matched chunk (matched-first is score-ranked, desc).
    #[test]
    fn higher_scoring_matched_chunk_wins_single_slot() {
        let chunks = [
            chunk("1-4", "green mango plum here"), // matches "mango" only
            chunk("5-8", "green mango tart here"), // matches "mango" AND "tart"
        ];
        // Each block costs 10; budget admits exactly one.
        let rendered = render_chunked_item_body(&chunks, &tokenize("mango tart"), 10)
            .expect("matched chunk renders");

        assert!(
            rendered.contains("tart"),
            "higher-scoring chunk chosen: {rendered}"
        );
        assert!(rendered.contains("[turns 5-8]"));
        assert!(
            !rendered.contains("plum") && !rendered.contains("[turns 1-4]"),
            "lower-scoring chunk dropped for the single slot: {rendered}"
        );
    }

    /// A sibling adjacent to two matched anchors is considered from both but
    /// emitted only once.
    #[test]
    fn overlapping_expansion_never_duplicates() {
        let chunks = [
            chunk("1-4", "red mango pie"),    // matched
            chunk("5-8", "green lime soda"),  // neighbour of both anchors
            chunk("9-12", "blue mango tart"), // matched
        ];
        let rendered = render_chunked_item_body(&chunks, &tokenize("mango"), 1000)
            .expect("matched chunk renders");

        assert_eq!(
            count(&rendered, "green lime soda"),
            1,
            "shared neighbour emitted once: {rendered}"
        );
        for body in ["red mango pie", "green lime soda", "blue mango tart"] {
            assert_eq!(count(&rendered, body), 1, "{body} emitted once");
        }
    }

    /// Pins the whitespace-separator invariant the budget accounting relies on:
    /// a block's declared token cost equals the rendered block string's actual
    /// whitespace-token count. A future non-whitespace header/body separator
    /// would desync `chunk_block_token_cost` from the charged text and must fail
    /// here.
    #[test]
    fn chunk_block_cost_equals_block_whitespace_tokens() {
        for c in [
            chunk("1-4", "red apple pie crust"),
            chunk("5-8", "single"),
            chunk("9-12", "many little words here now"),
        ] {
            assert_eq!(
                chunk_block_token_cost(&c),
                chunk_block(&c).split_whitespace().count(),
                "block cost == actual whitespace-token count for {}",
                c.header
            );
        }
    }
}

#[cfg(test)]
mod pack_cost_tests {
    use super::*;

    /// A chunk with the same 6-token header shape used elsewhere; a block's
    /// budget cost is `6 + body_word_count`.
    fn chunk(turns: &str, body: &str) -> ContextualChunk {
        ContextualChunk {
            id: format!("chunk-ep-{turns}"),
            header: format!("[episode ep] [kind user] [turns {turns}]"),
            body: body.to_string(),
            source_span: Some("0-0".to_string()),
        }
    }

    fn unit(id: u128, body: &str, chunks: Vec<ContextualChunk>) -> StoredMemoryUnit {
        StoredMemoryUnit {
            id: UnitId::from_u128(id),
            tenant_id: TenantId::from_u128(1),
            scope_id: ScopeId::from_u128(1),
            kind: MemoryKind::Semantic,
            state: UnitState::Active,
            subject_key: None,
            body: body.to_string(),
            trust_level: TrustLevel::TrustedUser,
            churn_class: None,
            freshness_due_at: None,
            actor_id: None,
            source_kind: None,
            source_episode_id: None,
            source_resource_id: None,
            deletion_generation: None,
            contextual_chunks: chunks,
            valid_from: None,
            valid_to: None,
            transaction_from: None,
            transaction_to: None,
            difficulty: None,
            stability_days: None,
            last_reinforced_at: None,
            reinforcement_count: 0,
        }
    }

    fn candidate(unit: StoredMemoryUnit, fused_score: f32) -> CandidateAccumulator {
        let decay = DecayScore::neutral(&unit);
        CandidateAccumulator {
            unit,
            fused_score,
            rerank_rank: None,
            rerank_score: 0.0,
            decay,
            l4_score: 0.0,
            subquery_ids: Vec::new(),
            decomposition_rank: None,
            channels: Vec::new(),
        }
    }

    /// A minimal request with abstention/rerank/decomposition OFF so the packing
    /// loop is a straight budget-gated append in candidate order — isolating the
    /// cost-charging behaviour under test.
    fn request(budget_tokens: usize) -> RecallRequest {
        RecallRequest {
            tenant_id: TenantId::from_u128(1),
            scope_id: ScopeId::from_u128(1),
            actor_id: ActorId::from_u128(1),
            allowed_scope_ids: vec![ScopeId::from_u128(1)],
            query: "quantum".to_string(),
            k: 10,
            budget_tokens,
            mode: RecallMode::Balanced,
            include_beliefs: true,
            edge_expansion_enabled: false,
            context_packing_abstention_enabled: false,
            rerank_enabled: false,
            learned_rerank_profile: None,
            query_decomposition_enabled: false,
            procedure_recall_enabled: true,
            decay_enabled: false,
            engine_version: "pack-cost-test".to_string(),
        }
    }

    /// `n` whitespace-separated filler words → an `n`-token whole body.
    fn body_of(n: usize) -> String {
        vec!["filler"; n].join(" ")
    }

    /// Budget reclaim: charging a chunk-rendered item its RENDERED token count
    /// (not the whole-body count) frees budget so a second item that whole-body
    /// charging would drop now fits. Same bodies, same budget, only chunking
    /// differs — and the packed second item is the chunk render, not the raw body.
    #[test]
    fn rendered_cost_reclaim_admits_second_item() {
        let query_tokens = tokenize("quantum");
        let budget = 31;
        // Item A: a plain 15-token item, packed first in both scenarios.
        let plain = || candidate(unit(1, &body_of(15), Vec::new()), 5.0);
        // Item B: a 40-token whole body. As chunks its matched render is two
        // 8-token blocks (6-token header + 2-token body) = 16 tokens.
        let chunks = || {
            vec![
                chunk("1-4", "quantum harmonica"), // matches the query
                chunk("5-8", "berlin note"),       // neighbour, pulled by expansion
            ]
        };

        // Whole-body charging (B has no chunks): B costs 40, does not fit → drop.
        let whole = pack_recall_context(
            vec![plain(), candidate(unit(2, &body_of(40), Vec::new()), 4.0)],
            &request(budget),
            &[],
            &query_tokens,
            Vec::new(),
            2,
        );
        assert_eq!(
            whole.items.len(),
            1,
            "whole-body charging packs only the plain item"
        );
        assert_eq!(whole.items[0].unit_id, UnitId::from_u128(1));
        assert_eq!(whole.token_estimate, 15);

        // Rendered charging (B chunked): B costs 16 → reclaimed budget admits it.
        let rendered = pack_recall_context(
            vec![plain(), candidate(unit(2, &body_of(40), chunks()), 4.0)],
            &request(budget),
            &[],
            &query_tokens,
            Vec::new(),
            2,
        );
        assert_eq!(
            rendered.items.len(),
            2,
            "rendered charging reclaims budget for the second item"
        );
        assert_eq!(
            rendered.items[1].unit_id,
            UnitId::from_u128(2),
            "the reclaimed second item is B"
        );
        assert_eq!(
            rendered.token_estimate, 31,
            "token_estimate == plain 15 + rendered 16 (actual charged costs)"
        );
        let b_body = &rendered.items[1].body;
        assert!(
            b_body.contains("[turns 1-4]") && b_body.contains("quantum harmonica"),
            "matched chunk rendered: {b_body}"
        );
        assert!(
            b_body.contains("[turns 5-8]") && b_body.contains("berlin note"),
            "neighbour chunk rendered: {b_body}"
        );
        assert!(
            !b_body.contains("filler"),
            "raw whole body not emitted: {b_body}"
        );
    }

    /// Property (review M1/M2): for a chunk-rendered item the charged cost equals
    /// the rendered text's whitespace-token count (so `token_estimate` is honest)
    /// AND never exceeds the old whole-body count (reclaim can only free budget,
    /// never overspend); an un-rendered item is charged exactly its whole-body
    /// count.
    #[test]
    fn charged_cost_matches_rendered_tokens_and_never_exceeds_whole_body() {
        let query_tokens = tokenize("quantum tart");
        // (whole-body word count, chunks) across matched / unmatched / no-chunk /
        // nothing-fits layouts.
        let cases = vec![
            (
                40,
                vec![
                    chunk("1-4", "quantum harmonica"),
                    chunk("5-8", "berlin note"),
                ],
            ),
            (
                12,
                vec![
                    chunk("1-4", "quantum tart glaze here"),
                    chunk("5-8", "plum jam toast crust"),
                ],
            ),
            // Tiny body: matched blocks each cost more than the whole-body cap,
            // so nothing fits and render falls back (charged == whole body).
            (
                6,
                vec![
                    chunk("1-2", "quantum"),
                    chunk("3-4", "berlin"),
                    chunk("5-6", "tart"),
                ],
            ),
            // No chunks → whole-body path.
            (25, vec![]),
            // Chunked but no chunk matches the query → whole-body fallback.
            (
                30,
                vec![
                    chunk("1-4", "red apple pie"),
                    chunk("5-8", "green lime soda"),
                ],
            ),
        ];
        for (body_words, chunks) in cases {
            let u = unit(1, &body_of(body_words), chunks);
            let whole_body_tokens = u.body.split_whitespace().count();
            let (rendered_body, charged) = packed_body_and_cost(&u, &query_tokens);
            match &rendered_body {
                Some(text) => {
                    assert_eq!(
                        charged,
                        text.split_whitespace().count(),
                        "charged cost == rendered token count (token_estimate honest)"
                    );
                    assert!(
                        charged <= whole_body_tokens,
                        "rendered charge {charged} <= whole-body {whole_body_tokens}"
                    );
                }
                None => assert_eq!(
                    charged, whole_body_tokens,
                    "un-rendered item charged exactly its whole-body count"
                ),
            }
        }
    }

    /// No-chunks path: every item is charged its exact whole-body count and
    /// packing decisions / token_estimate are unchanged — the default caller
    /// (chunk-write OFF) is bit-identical to before this change.
    #[test]
    fn no_chunks_path_charges_whole_body_and_is_unchanged() {
        let query_tokens = tokenize("quantum");
        let a = candidate(unit(1, &body_of(10), Vec::new()), 5.0);
        let b = candidate(unit(2, &body_of(10), Vec::new()), 4.0);
        let c = candidate(unit(3, &body_of(10), Vec::new()), 3.0);
        // Budget 25 fits two 10-token bodies (20); the third overflows.
        let packed = pack_recall_context(
            vec![a, b, c],
            &request(25),
            &[],
            &query_tokens,
            Vec::new(),
            3,
        );
        assert_eq!(
            packed.items.iter().map(|i| i.unit_id).collect::<Vec<_>>(),
            vec![UnitId::from_u128(1), UnitId::from_u128(2)],
            "first two fit in candidate order"
        );
        assert_eq!(
            packed.token_estimate, 20,
            "token_estimate == sum of whole-body counts"
        );
        assert_eq!(
            packed.items[0].body,
            body_of(10),
            "whole body emitted byte-identical"
        );
        assert_eq!(
            packed.items[1].body,
            body_of(10),
            "whole body emitted byte-identical"
        );
        assert!(
            packed
                .dropped_items
                .iter()
                .any(|d| d.unit_id == UnitId::from_u128(3) && d.reason == RecallDropReason::Budget),
            "third item dropped for budget: {:?}",
            packed.dropped_items
        );
    }
}

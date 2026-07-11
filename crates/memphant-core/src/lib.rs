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

/// Cross-encoder reranker seam (W8). A `(query, doc)`-pair scorer that reorders
/// a widened candidate pool AFTER fusion and BEFORE packing — the highest-ROI
/// retrieval layer, distinct from the retired deterministic heuristic rerank.
///
/// Contract: `rerank` returns exactly one score per input doc, IN INPUT ORDER
/// (higher = more relevant). Any other length (including the empty vec) is read
/// by the recall stage as "no-op — leave the fused order unchanged", so a real
/// backend that fails inference degrades to the pre-rerank order rather than
/// erroring the whole recall. Inference is expected to be deterministic (same
/// inputs → same scores), which the recall stage relies on for stable ordering.
pub trait CrossReranker: Send + Sync {
    /// Scores each `(query, docs[i])` pair; result `i` is the score for
    /// `docs[i]`. See the trait contract for the length/no-op rule.
    fn rerank(&self, query: &str, docs: &[&str]) -> Vec<f32>;
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

/// Default candidate-pool size for the recall vector channel — the historical
/// vector KNN fan-out.
///
/// THE POOL MAPPING (W3): the single `candidate_pool_size` construction-time
/// service knob (see [`crate::service::MemoryService::with_candidate_pool_size`])
/// sets the vector-channel KNN fetch limit DIRECTLY; this default reproduces
/// today's ranking exactly. The lexical/recent/subject families are fetched via
/// `fetch_recall_candidates` with no core-side cap (the store applies its own
/// internal per-family caps — 200 FTS / 100 recent / 200 subject — already far
/// wider than any cross-encoder rerank pool), so they are NOT the pool that
/// gates rerank; the knob widens the one historically-narrow family — vector —
/// that the W8 rerank arm needs at 64–128. Measured-tunable, not sacred.
pub const DEFAULT_CANDIDATE_POOL_SIZE: usize = VECTOR_CANDIDATE_LIMIT;

/// Recommended per-`source_episode_id` diversity cap (W4 session-quota lever).
///
/// Chosen so a default-sized pack surfaces at least four distinct episodes when
/// the candidate set can supply them: with a cap of 2, `ceil(k / 2) >= 4` for the
/// default output limits (`k = 8` service default, `k = 10` bench default). The
/// SERVICE default is OFF (`None`): the lever ships default-on only after the
/// accuracy-wave measurement campaign, so this names the value the campaign runs
/// and the recommended production setting once promoted — not an always-on knob.
pub const DEFAULT_SESSION_DIVERSITY_QUOTA: usize = 2;

/// W4 packing levers, threaded construction-time exactly like
/// `candidate_pool_size` — no `RecallRequest`/wire/OpenAPI field (item 4). BOTH
/// default OFF ([`PackLevers::default`]); with both off the packer is
/// byte-identical to today. The service builders
/// [`crate::service::MemoryService::with_sibling_gather_enabled`] and
/// [`crate::service::MemoryService::with_session_quota`] set these, and the
/// bench lane's `--sibling-gather` / `--session-quota <n>` flags thread them so
/// the measurement campaign can toggle each independently.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PackLevers {
    /// Sibling-gather post-pass: after the greedy fill, spend the pack's leftover
    /// budget expanding already chunk-rendered items with their own UNSELECTED
    /// sibling chunks (same parent episode). Never evicts a packed item, never
    /// exceeds budget, preserves document order. Off ⇒ the post-pass is skipped.
    pub sibling_gather_enabled: bool,
    /// Per-`source_episode_id` admission cap during the greedy fill. `Some(cap)`
    /// admits at most `cap` items per episode until every distinct-episode
    /// candidate has had an admission opportunity, then fills the remaining
    /// budget UNRESTRICTED from the deferred candidates (work-conserving — the
    /// quota never leaves admissible budget unused). Replacement honours the cap
    /// too. `None` disables the quota (today's unrestricted greedy fill).
    pub session_quota: Option<usize>,
}

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
    recall_with_pool(
        store,
        request,
        vector_query,
        clock,
        DEFAULT_CANDIDATE_POOL_SIZE,
        PackLevers::default(),
        false,
        None,
    )
    .await
}

/// `recall` with the candidate-pool knob exposed. `candidate_pool_size` sets the
/// vector-channel KNN fan-out — the one historically-narrow per-family fetch
/// limit (`VECTOR_CANDIDATE_LIMIT`, 32) — so the W8 cross-encoder rerank arm can
/// widen its rerank pool to 64–128 WITHOUT any wire change. The construction-time
/// [`crate::service::MemoryService`] candidate-pool option threads its value
/// here; the plain [`recall`] above delegates with [`DEFAULT_CANDIDATE_POOL_SIZE`]
/// so every existing call site keeps today's behavior. See the pool-mapping note
/// on [`DEFAULT_CANDIDATE_POOL_SIZE`].
///
/// `pack_levers` threads the W4 packing levers (sibling-gather + session
/// diversity quota) construction-time, mirroring `candidate_pool_size`; the
/// plain [`recall`] delegates with [`PackLevers::default`] (both off).
///
/// `temporal_grounding_enabled` (W5, default off via the plain [`recall`]) gates
/// query-date windowing and dated packs: on, the query's parsed date (if any)
/// becomes a soft temporal-channel window preference and packed item bodies
/// carry a `[date YYYY-MM-DD]` prefix resolved from each unit's grounded
/// `valid_from`. Off, the parsed window is `None` and every downstream path is
/// byte-identical to today.
///
/// `cross_reranker` (W8, `None` via the plain [`recall`]) is the cross-encoder
/// rerank seam: when present, AFTER fusion produces the ranked candidate list
/// and BEFORE packing, the top `candidate_pool_size` candidates are scored as
/// `(query, unit body)` pairs and reordered by cross-encoder score (ties broken
/// by prior fused rank via a stable sort). `None` leaves the fused order
/// untouched — byte-identical to today. This is independent of, and never
/// entangled with, the retired deterministic heuristic [`rerank_candidates`]
/// stage (gated by `request.rerank_enabled`, off by default).
#[allow(clippy::too_many_arguments)]
pub async fn recall_with_pool<S>(
    store: &S,
    request: RecallRequest,
    vector_query: Option<VectorQuery<'_>>,
    clock: &dyn Clock,
    candidate_pool_size: usize,
    pack_levers: PackLevers,
    temporal_grounding_enabled: bool,
    cross_reranker: Option<&dyn CrossReranker>,
) -> Result<RecallResponse, CoreError>
where
    S: MemoryStore,
{
    validate_learned_rerank_profile(request.learned_rerank_profile.as_ref())?;

    let now = clock.now_rfc3339();
    // W5: parse the query's date ONCE (clock-free). `None` whenever the flag is
    // off or the query carries no date — the whole windowing/pack path is then
    // inert. Bound to the full query; subquery passes intentionally see `None`.
    let temporal_window = temporal_grounding_enabled
        .then(|| extract_query_date(&request.query))
        .flatten();
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
                    // W3 pool knob: the vector KNN fan-out is the widen-able
                    // per-family limit (default `VECTOR_CANDIDATE_LIMIT`).
                    candidate_pool_size,
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
            temporal_window.as_ref(),
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
            let contribution = channel_weight(pass, &request.query, temporal_window.as_ref())
                / (60.0 + channel_rank as f32);
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
                    cross_rerank_rank: None,
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
                    None,
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
                        channel_weight(pass, &subquery.query, None) / (55.0 + channel_rank as f32);
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
                            cross_rerank_rank: None,
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
                    cross_rerank_rank: None,
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

    // W8 cross-encoder rerank: reorder the top `candidate_pool_size` fused
    // candidates by a real (query, body) cross-encoder before packing. A no-op
    // when no reranker is wired (the default) or the pool is empty — the fused
    // order then flows unchanged into packing.
    if let Some(reranker) = cross_reranker {
        cross_rerank_candidates(
            fused.as_mut_slice(),
            &request.query,
            reranker,
            candidate_pool_size,
        );
    }

    let iterative_scan_depth = recall_pack_scan_limit(&request, fused.len());
    let packed = pack_recall_context(
        fused,
        &request,
        &tenant_edges,
        &query_tokens,
        dropped_items,
        iterative_scan_depth,
        pack_levers,
        temporal_grounding_enabled,
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

/// W8 cross-encoder rerank stage: reorder the top `pool` fused candidates by a
/// real `(query, body)` cross-encoder, in place. Distinct from the retired
/// heuristic [`rerank_candidates`] — this reads `CandidateAccumulator::unit.body`
/// only and never touches the heuristic `rerank_score`/`rerank_rank` fields.
///
/// Determinism + ties: the top-`pool` slice is already in fused-rank order, and
/// a STABLE sort by descending cross-encoder score preserves that order for
/// equal scores — so ties break by prior rank. The tail (below `pool`) is left
/// exactly where fusion put it. A reranker that returns a score vector whose
/// length != the pool (the seam's "no-op" signal, e.g. inference failure) is
/// honored by leaving the whole order unchanged.
fn cross_rerank_candidates(
    fused: &mut [CandidateAccumulator],
    query: &str,
    reranker: &dyn CrossReranker,
    pool: usize,
) {
    let head = pool.min(fused.len());
    if head == 0 {
        return;
    }
    let docs: Vec<&str> = fused[..head]
        .iter()
        .map(|candidate| candidate.unit.body.as_str())
        .collect();
    let scores = reranker.rerank(query, &docs);
    // Contract: one score per doc, in input order. Any other length is a no-op.
    if scores.len() != head {
        return;
    }
    let mut order: Vec<usize> = (0..head).collect();
    order.sort_by(|&left, &right| {
        scores[right]
            .partial_cmp(&scores[left])
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut reordered: Vec<CandidateAccumulator> = order
        .into_iter()
        .map(|index| fused[index].clone())
        .collect();
    // Stamp the 0-based cross-encoder rank so packing honors this order first
    // (it re-sorts by `fused_score` otherwise, which would undo a bare reorder).
    // Physically reordering too keeps the head correct when the abstention
    // re-sort is disabled (packing then greedily fills in Vec order).
    for (rank, candidate) in reordered.iter_mut().enumerate() {
        candidate.cross_rerank_rank = Some(rank);
    }
    fused[..head].clone_from_slice(&reordered);
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
    /// W8 cross-encoder rank (0-based): `Some` only for candidates the
    /// cross-reranker scored (the top `candidate_pool_size` fused head). Packing
    /// honors it FIRST when any candidate carries one, so the cross-encoder
    /// ordering survives the pack re-sort. `None` for the unreranked tail and
    /// for every run without a cross-reranker (then packing is unchanged).
    cross_rerank_rank: Option<usize>,
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

/// A chunk-rendered packed item's sibling-gather state: the parent unit's full
/// chunk vector and the mask of chunks the greedy fill already selected. Only
/// stored (Some) for chunk-rendered items when the sibling-gather lever is on;
/// whole-body items and every item when the lever is off carry `None`.
struct ChunkSiblings {
    chunks: Vec<ContextualChunk>,
    selected: Vec<bool>,
}

/// Read-only per-pack context shared by every candidate admission — bundled so
/// the admission helpers stay under the argument-count limit.
struct PackCtx<'a> {
    request: &'a RecallRequest,
    tenant_edges: &'a [StoredMemoryEdge],
    query_tokens: &'a [String],
    output_limit: usize,
    sibling_gather_enabled: bool,
    /// W5: when on, each admitted item records its grounded date so the post-fill
    /// pass can prefix `[date YYYY-MM-DD]`. Off ⇒ no prefixes recorded or applied.
    temporal_grounding_enabled: bool,
}

/// Everything computed once per candidate before the admit/drop decision.
struct Admission {
    candidate: CandidateAccumulator,
    rendered_body: Option<String>,
    unit_tokens: usize,
    candidate_score: f32,
    chunk_mask: Option<Vec<bool>>,
    episode_id: Option<EpisodeId>,
}

/// The growing pack: `items` and its parallel bookkeeping vectors (token cost,
/// relevance score, source episode, sibling-gather state) plus the per-episode
/// admission counts the session-diversity quota reads.
#[derive(Default)]
struct PackAccumulator {
    items: Vec<RecallContextItem>,
    token_counts: Vec<usize>,
    relevance_scores: Vec<f32>,
    episode_ids: Vec<Option<EpisodeId>>,
    sibling_masks: Vec<Option<ChunkSiblings>>,
    /// W5: parallel to `items` — the `YYYY-MM-DD` a dated pack prefixes onto each
    /// item body, or `None` (item's unit had no grounded `valid_from`, or the
    /// temporal-grounding flag is off). Applied in one pass after the fill.
    date_prefixes: Vec<Option<String>>,
    token_estimate: usize,
    episode_counts: HashMap<EpisodeId, usize>,
}

impl PackAccumulator {
    /// Evicts the item at `index` from every parallel vector, decrements its
    /// episode's admission count (so the quota stays exact under replacement),
    /// reclaims its budget, and returns the evicted unit id for the drop record.
    fn evict(&mut self, index: usize) -> UnitId {
        let item = self.items.remove(index);
        let tokens = self.token_counts.remove(index);
        self.relevance_scores.remove(index);
        let episode = self.episode_ids.remove(index);
        self.sibling_masks.remove(index);
        self.date_prefixes.remove(index);
        if let Some(episode_id) = episode
            && let Some(count) = self.episode_counts.get_mut(&episode_id)
        {
            *count = count.saturating_sub(1);
        }
        self.token_estimate -= tokens;
        item.unit_id
    }
}

#[allow(clippy::too_many_arguments)]
fn pack_recall_context(
    mut fused: Vec<CandidateAccumulator>,
    request: &RecallRequest,
    tenant_edges: &[StoredMemoryEdge],
    query_tokens: &[String],
    mut dropped_items: Vec<RecallDroppedItem>,
    scan_limit: usize,
    pack_levers: PackLevers,
    temporal_grounding_enabled: bool,
) -> PackedRecallContext {
    let mut acc = PackAccumulator::default();
    let mut seen_subjects: HashMap<String, Vec<UnitId>> = HashMap::new();

    if request.context_packing_abstention_enabled {
        fused.sort_by(|left, right| {
            if left.cross_rerank_rank.is_some() || right.cross_rerank_rank.is_some() {
                // W8: the cross-encoder ordering governs when it ran. The scored
                // head (0-based `cross_rerank_rank`) leads in cross-encoder order;
                // the unscored tail (`None` → `usize::MAX`) follows in fusion
                // order (fused_score desc), body-tie-broken. Independent of the
                // heuristic-rerank and decomposition branches below.
                left.cross_rerank_rank
                    .unwrap_or(usize::MAX)
                    .cmp(&right.cross_rerank_rank.unwrap_or(usize::MAX))
                    .then_with(|| {
                        right
                            .fused_score
                            .partial_cmp(&left.fused_score)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .then_with(|| left.unit.body.cmp(&right.unit.body))
            } else if request.query_decomposition_enabled
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

    let ctx = PackCtx {
        request,
        tenant_edges,
        query_tokens,
        output_limit: request.k.max(1),
        sibling_gather_enabled: pack_levers.sibling_gather_enabled,
        temporal_grounding_enabled,
    };

    // Greedy fill. With the session-diversity quota on, a candidate whose episode
    // is already at its cap is DEFERRED (never dropped) so every distinct episode
    // gets an admission opportunity first; the deferred candidates then fill any
    // remaining budget UNRESTRICTED in a second pass (work-conserving). With the
    // quota off `deferred` stays empty and this is today's single-pass fill.
    let mut deferred: Vec<CandidateAccumulator> = Vec::new();
    for candidate in fused.into_iter().take(scan_limit) {
        if let Some(cap) = pack_levers.session_quota
            && let Some(episode_id) = candidate.unit.source_episode_id
            && acc.episode_counts.get(&episode_id).copied().unwrap_or(0) >= cap
        {
            deferred.push(candidate);
            continue;
        }
        admit_or_drop(
            &mut acc,
            &ctx,
            candidate,
            &mut seen_subjects,
            &mut dropped_items,
        );
    }
    for candidate in deferred {
        admit_or_drop(
            &mut acc,
            &ctx,
            candidate,
            &mut seen_subjects,
            &mut dropped_items,
        );
    }

    // Sibling-gather post-pass: expand already chunk-rendered items with their
    // own unselected siblings while budget remains (skipped when the lever off).
    if pack_levers.sibling_gather_enabled {
        sibling_gather_pass(&mut acc, request.budget_tokens);
    }

    // W5 dated packs: prefix each item body with `[date YYYY-MM-DD]` from the
    // unit's grounded `valid_from`. Applied AFTER sibling-gather (which rewrites
    // chunk-rendered bodies) so the prefix survives, and only for units that
    // carry a grounded date (never invented). Off ⇒ `date_prefixes` is all
    // `None` and this loop is a no-op, keeping the pack bytes identical.
    if temporal_grounding_enabled {
        for (item, prefix) in acc.items.iter_mut().zip(acc.date_prefixes.iter()) {
            if let Some(date) = prefix {
                item.body = format!("[date {date}]\n{}", item.body);
            }
        }
    }

    let abstention = acc.items.is_empty()
        || (request.context_packing_abstention_enabled
            && acc.items.iter().any(|item| {
                item.suppression_labels
                    .iter()
                    .any(|label| label == "unresolved_contradiction")
            }));

    PackedRecallContext {
        items: acc.items,
        dropped_items,
        token_estimate: acc.token_estimate,
        abstention,
    }
}

/// Runs today's subject-dedup + budget/replacement admission for one candidate,
/// pushing the outcome into `acc` (or a drop into `dropped_items`). Extracted
/// verbatim from the old inline loop so the greedy fill and the quota's
/// work-conserving second pass share exactly one admission path — with the
/// levers off the decisions are byte-identical to before.
fn admit_or_drop(
    acc: &mut PackAccumulator,
    ctx: &PackCtx,
    candidate: CandidateAccumulator,
    seen_subjects: &mut HashMap<String, Vec<UnitId>>,
    dropped_items: &mut Vec<RecallDroppedItem>,
) {
    let request = ctx.request;
    if request.context_packing_abstention_enabled
        && let Some(subject_key) = candidate.unit.subject_key.as_deref()
    {
        let dedup_key = normalize_component(subject_key);
        if !dedup_key.is_empty() {
            let seen_ids = seen_subjects.entry(dedup_key).or_default();
            if !seen_ids.is_empty()
                && !has_contradiction_with_any(candidate.unit.id, seen_ids, ctx.tenant_edges)
            {
                dropped_items.push(RecallDroppedItem {
                    unit_id: candidate.unit.id,
                    reason: RecallDropReason::Duplicate,
                });
                return;
            }
            seen_ids.push(candidate.unit.id);
        }
    }

    let (rendered_body, unit_tokens, chunk_mask) = packed_render(&candidate.unit, ctx.query_tokens);
    let candidate_score = packing_relevance_score(&candidate, ctx.query_tokens);
    let candidate_id = candidate.unit.id;
    let admission = Admission {
        episode_id: candidate.unit.source_episode_id,
        candidate,
        rendered_body,
        unit_tokens,
        candidate_score,
        chunk_mask,
    };

    if acc.items.len() >= ctx.output_limit {
        if let Some(replace_index) = replacement_index(
            &acc.token_counts,
            &acc.relevance_scores,
            acc.token_estimate,
            unit_tokens,
            candidate_score,
            request.budget_tokens,
        ) {
            let replaced_id = acc.evict(replace_index);
            dropped_items.push(RecallDroppedItem {
                unit_id: replaced_id,
                reason: RecallDropReason::Rerank,
            });
            admit_new(acc, ctx, admission);
        } else {
            dropped_items.push(RecallDroppedItem {
                unit_id: candidate_id,
                reason: RecallDropReason::Rerank,
            });
        }
        return;
    }
    if acc.token_estimate + unit_tokens > request.budget_tokens {
        if request.context_packing_abstention_enabled
            && let Some(replace_index) = replacement_index(
                &acc.token_counts,
                &acc.relevance_scores,
                acc.token_estimate,
                unit_tokens,
                candidate_score,
                request.budget_tokens,
            )
        {
            let replaced_id = acc.evict(replace_index);
            dropped_items.push(RecallDroppedItem {
                unit_id: replaced_id,
                reason: RecallDropReason::Budget,
            });
            admit_new(acc, ctx, admission);
            return;
        }
        dropped_items.push(RecallDroppedItem {
            unit_id: candidate_id,
            reason: RecallDropReason::Budget,
        });
        return;
    }
    admit_new(acc, ctx, admission);
}

/// Appends a newly admitted candidate to `acc`, capturing its sibling-gather
/// state (only when the lever is on and the item was chunk-rendered) before the
/// candidate is consumed into the context item.
fn admit_new(acc: &mut PackAccumulator, ctx: &PackCtx, admission: Admission) {
    let Admission {
        candidate,
        rendered_body,
        unit_tokens,
        candidate_score,
        chunk_mask,
        episode_id,
    } = admission;
    let sibling = if ctx.sibling_gather_enabled {
        chunk_mask.map(|selected| ChunkSiblings {
            chunks: candidate.unit.contextual_chunks.clone(),
            selected,
        })
    } else {
        None
    };
    // W5: capture the item's grounded date BEFORE `candidate` is consumed. `None`
    // when the flag is off or the unit has no date-leading `valid_from`.
    let date_prefix = if ctx.temporal_grounding_enabled {
        candidate
            .unit
            .valid_from
            .as_deref()
            .and_then(date_prefix_from_valid_from)
    } else {
        None
    };
    let item = context_item_for(candidate, ctx.tenant_edges, ctx.query_tokens, rendered_body);
    if let Some(episode_id) = episode_id {
        *acc.episode_counts.entry(episode_id).or_default() += 1;
    }
    acc.token_estimate += unit_tokens;
    acc.token_counts.push(unit_tokens);
    acc.relevance_scores.push(candidate_score);
    acc.episode_ids.push(episode_id);
    acc.sibling_masks.push(sibling);
    acc.date_prefixes.push(date_prefix);
    acc.items.push(item);
}

/// Sibling-gather post-pass: for each already chunk-rendered item, spend the
/// pack's leftover budget expanding it with its OWN unselected sibling chunks
/// (same parent episode), preferring document-adjacent windows around the chunks
/// the greedy fill already picked. Grows an item's selection only — never evicts
/// another item and never pushes `token_estimate` past `budget_tokens` — and
/// re-emits the body in document order via the shared chunk-block helpers.
fn sibling_gather_pass(acc: &mut PackAccumulator, budget_tokens: usize) {
    let masks = std::mem::take(&mut acc.sibling_masks);
    for (index, slot) in masks.iter().enumerate() {
        let Some(sibling) = slot else { continue };
        let remaining = budget_tokens.saturating_sub(acc.token_estimate);
        if remaining == 0 {
            continue;
        }
        let anchors: Vec<usize> = (0..sibling.chunks.len())
            .filter(|&i| sibling.selected[i])
            .collect();
        if anchors.is_empty() {
            continue;
        }
        let current_cost = acc.token_counts[index];
        let mut selected = sibling.selected.clone();
        let mut used = current_cost;
        expand_siblings(
            &sibling.chunks,
            &mut selected,
            &anchors,
            &mut used,
            current_cost + remaining,
        );
        if used > current_cost {
            acc.items[index].body = emit_selected_chunks(&sibling.chunks, &selected);
            acc.token_estimate += used - current_cost;
            acc.token_counts[index] = used;
        }
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
///
/// Now a thin `packed_render` wrapper retained for the cost-accounting tests;
/// the production path calls `packed_render` directly for the sibling mask.
#[cfg(test)]
fn packed_body_and_cost(
    unit: &StoredMemoryUnit,
    query_tokens: &[String],
) -> (Option<String>, usize) {
    let (rendered_body, charged_tokens, _mask) = packed_render(unit, query_tokens);
    (rendered_body, charged_tokens)
}

/// [`packed_body_and_cost`] plus the chunk-selection mask the sibling-gather
/// post-pass reuses. The mask is `Some` exactly when the item was chunk-rendered
/// (so its still-unselected siblings can be gathered later); `None` for the
/// whole-body path. Cost accounting is unchanged from `packed_body_and_cost`.
fn packed_render(
    unit: &StoredMemoryUnit,
    query_tokens: &[String],
) -> (Option<String>, usize, Option<Vec<bool>>) {
    let whole_body_tokens = unit.body.split_whitespace().count();
    match select_chunk_mask(&unit.contextual_chunks, query_tokens, whole_body_tokens) {
        Some(selected) => {
            let rendered = emit_selected_chunks(&unit.contextual_chunks, &selected);
            let charged_tokens = rendered.split_whitespace().count();
            (Some(rendered), charged_tokens, Some(selected))
        }
        None => (None, whole_body_tokens, None),
    }
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
///
/// Retained as a `select_chunk_mask` + `emit_selected_chunks` wrapper for the
/// chunk-render tests; production callers use those two directly (the mask feeds
/// the sibling-gather post-pass).
#[cfg(test)]
fn render_chunked_item_body(
    chunks: &[ContextualChunk],
    query_tokens: &[String],
    budget_tokens: usize,
) -> Option<String> {
    select_chunk_mask(chunks, query_tokens, budget_tokens)
        .map(|selected| emit_selected_chunks(chunks, &selected))
}

/// The chunk-selection mask behind chunk-aware rendering: `Some(selected)` marks
/// which chunks to emit (matched chunks first, then adjacent-sibling expansion,
/// all gated by `budget_tokens`); `None` signals the whole-body fallback (no
/// chunks, no chunk matched the query, or nothing fit the budget). Splitting the
/// mask out from emission lets both the admission cost path and the sibling-
/// gather post-pass share one selection algorithm.
fn select_chunk_mask(
    chunks: &[ContextualChunk],
    query_tokens: &[String],
    budget_tokens: usize,
) -> Option<Vec<bool>> {
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
    expand_siblings(
        chunks,
        &mut selected,
        &anchors,
        &mut used_tokens,
        budget_tokens,
    );

    if selected.iter().any(|&picked| picked) {
        Some(selected)
    } else {
        None
    }
}

/// Expands `selected` outward from `anchors` to adjacent-sibling chunks (±1, then
/// ±2, …), charging `chunk_block_token_cost` and stopping each step at
/// `budget_tokens`. Only ever sets more chunks to selected (a superset), so the
/// caller's already-chosen chunks are never dropped. Shared by the admission
/// render (`select_chunk_mask`) and the sibling-gather post-pass.
fn expand_siblings(
    chunks: &[ContextualChunk],
    selected: &mut [bool],
    anchors: &[usize],
    used_tokens: &mut usize,
    budget_tokens: usize,
) {
    for radius in 1..chunks.len() {
        for &anchor in anchors {
            for candidate in [anchor.checked_sub(radius), anchor.checked_add(radius)] {
                let Some(index) = candidate else { continue };
                if index >= chunks.len() || selected[index] {
                    continue;
                }
                let cost = chunk_block_token_cost(&chunks[index]);
                if *used_tokens + cost <= budget_tokens {
                    selected[index] = true;
                    *used_tokens += cost;
                }
            }
        }
    }
}

/// Emits the selected chunks in document order (chunk vector index == window
/// index), each prefixed by its provenance header, blocks joined by a blank line.
fn emit_selected_chunks(chunks: &[ContextualChunk], selected: &[bool]) -> String {
    (0..chunks.len())
        .filter(|&index| selected[index])
        .map(|index| chunk_block(&chunks[index]))
        .collect::<Vec<_>>()
        .join("\n\n")
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

#[allow(clippy::too_many_arguments)]
fn channel_candidates(
    pass: ChannelPass,
    units: &[StoredMemoryUnit],
    edges: &[StoredMemoryEdge],
    request: &RecallRequest,
    query_tokens: &[String],
    vector_scores: Option<&HashMap<UnitId, f32>>,
    now: &str,
    temporal_window: Option<&DateWindow>,
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
                ChannelPass::Temporal => temporal_score(unit, &request.query, temporal_window),
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
                .max(temporal_score(unit, &request.query, None));
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

// ============================================================================
// W5 temporal grounding: deterministic content-date parsing, query-date
// windowing, and dated packs. Every helper here is CLOCK-FREE — dates come from
// the text under scan, jiff validates/normalizes the (year, month, day) triple,
// and the system clock is never consulted. All of it is inert unless the
// `with_temporal_grounding_enabled` service flag is on (recall threads the
// parsed `Option<DateWindow>`, which stays `None` when the flag is off, keeping
// the flag-off scoring and pack bytes identical to today).
// ============================================================================

/// A half-open instant window `[start, end)` in RFC 3339 UTC, derived from a
/// parsed query date. Bounds are day-aligned midnights so a unit's grounded
/// `valid_from` (also a midnight instant) can be compared with [`cmp_rfc3339`].
#[derive(Debug, Clone, PartialEq, Eq)]
struct DateWindow {
    start: String,
    end: String,
}

/// `jiff`-validated civil date constructor: rejects impossible triples
/// (`2023-13-40`, `2023-02-30`) so a malformed date is never grounded.
fn civil_date(year: i32, month: u8, day: u8) -> Option<jiff::civil::Date> {
    let year = i16::try_from(year).ok()?;
    let month = i8::try_from(month).ok()?;
    let day = i8::try_from(day).ok()?;
    jiff::civil::Date::new(year, month, day).ok()
}

/// The canonical midnight-UTC instant for a civil date, e.g. `2023-05-30` →
/// `2023-05-30T00:00:00Z`. This is the grounded `valid_from` form and the
/// `DateWindow` bound form, both RFC 3339 so [`cmp_rfc3339`] parses them.
fn midnight_utc(date: jiff::civil::Date) -> String {
    format!("{date}T00:00:00Z")
}

/// English month name → `1..=12`, case-insensitive. Accepts full names, the
/// three-letter abbreviations, and `sept`.
fn month_from_name(word: &str) -> Option<u8> {
    let word = word.to_ascii_lowercase();
    const MONTHS: [(&str, &str, u8); 12] = [
        ("january", "jan", 1),
        ("february", "feb", 2),
        ("march", "mar", 3),
        ("april", "apr", 4),
        ("may", "may", 5),
        ("june", "jun", 6),
        ("july", "jul", 7),
        ("august", "aug", 8),
        ("september", "sep", 9),
        ("october", "oct", 10),
        ("november", "nov", 11),
        ("december", "dec", 12),
    ];
    MONTHS.iter().find_map(|(full, abbr, number)| {
        (word == *full || word == *abbr || (*number == 9 && word == "sept")).then_some(*number)
    })
}

/// Reads `min..=max` ASCII digits at `start`, returning the value and how many
/// digits were consumed. `None` when fewer than `min` digits are present.
fn ascii_digits(bytes: &[u8], start: usize, min: usize, max: usize) -> Option<(u32, usize)> {
    let mut value = 0u32;
    let mut count = 0usize;
    while count < max {
        match bytes.get(start + count) {
            Some(byte) if byte.is_ascii_digit() => {
                value = value * 10 + u32::from(byte - b'0');
                count += 1;
            }
            _ => break,
        }
    }
    (count >= min).then_some((value, count))
}

/// Skips one-or-more ASCII whitespace bytes; `None` when none were present (used
/// where at least one separator is required).
fn skip_required_spaces(bytes: &[u8], start: usize) -> Option<usize> {
    let mut pos = start;
    while bytes.get(pos).is_some_and(u8::is_ascii_whitespace) {
        pos += 1;
    }
    (pos > start).then_some(pos)
}

/// A byte position is not preceded by an ASCII digit (so a numeric run we match
/// is not the tail of a longer number).
fn no_leading_digit(bytes: &[u8], at: usize) -> bool {
    at == 0 || !bytes[at - 1].is_ascii_digit()
}

/// A byte position is not the start of another ASCII digit (so a numeric run we
/// match is not the head of a longer number).
fn no_trailing_digit(bytes: &[u8], at: usize) -> bool {
    !bytes.get(at).is_some_and(u8::is_ascii_digit)
}

/// Parses a numeric `YYYY-MM-DD` / `YYYY/MM/DD` date anchored exactly at byte
/// `i`. Returns the validated date and the byte index just past the day, or
/// `None` if the anchor is not such a date. Guards both ends against digit
/// bleed so `12023/05/30` and `2023/05/301` do not spuriously match.
fn numeric_date_at(bytes: &[u8], i: usize) -> Option<(jiff::civil::Date, usize)> {
    if !no_leading_digit(bytes, i) {
        return None;
    }
    let (year, ylen) = ascii_digits(bytes, i, 4, 4)?;
    let mut pos = i + ylen;
    let separator = *bytes.get(pos)?;
    if separator != b'-' && separator != b'/' {
        return None;
    }
    pos += 1;
    let (month, mlen) = ascii_digits(bytes, pos, 1, 2)?;
    pos += mlen;
    if *bytes.get(pos)? != separator {
        return None;
    }
    pos += 1;
    let (day, dlen) = ascii_digits(bytes, pos, 1, 2)?;
    pos += dlen;
    if !no_trailing_digit(bytes, pos) {
        return None;
    }
    let date = civil_date(year as i32, month as u8, day as u8)?;
    Some((date, pos))
}

/// Parses an English `Month D, YYYY` (comma optional) date anchored at byte `i`
/// on a word boundary. Returns the validated date and the byte index past the
/// year.
fn month_name_date_at(text: &str, i: usize) -> Option<(jiff::civil::Date, usize)> {
    let bytes = text.as_bytes();
    if i > 0 && bytes[i - 1].is_ascii_alphabetic() {
        return None;
    }
    let rest = text.get(i..)?;
    let word_len = rest
        .find(|c: char| !c.is_ascii_alphabetic())
        .unwrap_or(rest.len());
    let month = month_from_name(rest.get(..word_len)?)?;
    let mut pos = skip_required_spaces(bytes, i + word_len)?;
    let (day, dlen) = ascii_digits(bytes, pos, 1, 2)?;
    pos += dlen;
    if bytes.get(pos) == Some(&b',') {
        pos += 1;
    }
    pos = skip_required_spaces(bytes, pos)?;
    let (year, ylen) = ascii_digits(bytes, pos, 4, 4)?;
    let end = pos + ylen;
    if !no_trailing_digit(bytes, end) {
        return None;
    }
    let date = civil_date(year as i32, month, day as u8)?;
    Some((date, end))
}

/// Parses a bare `Month YYYY` (no day) anchored at byte `i` on a word boundary.
/// Returns `(year, month, end_index)`. Feeds the month-window branch of query
/// date extraction ("in May 2023").
fn month_year_at(text: &str, i: usize) -> Option<(i32, u8, usize)> {
    let bytes = text.as_bytes();
    if i > 0 && bytes[i - 1].is_ascii_alphabetic() {
        return None;
    }
    let rest = text.get(i..)?;
    let word_len = rest
        .find(|c: char| !c.is_ascii_alphabetic())
        .unwrap_or(rest.len());
    let month = month_from_name(rest.get(..word_len)?)?;
    let pos = skip_required_spaces(bytes, i + word_len)?;
    let (year, ylen) = ascii_digits(bytes, pos, 4, 4)?;
    let end = pos + ylen;
    if !no_trailing_digit(bytes, end) {
        return None;
    }
    civil_date(year as i32, month, 1)?;
    Some((year as i32, month, end))
}

/// Parses a bare four-digit calendar year (1000–2999) anchored at byte `i`. It
/// must be a standalone token — not glued to an adjacent alphanumeric on either
/// side — so `project2023` and `20230` do not read as a bare year. Feeds the
/// year-window branch of query extraction.
fn year_only_at(bytes: &[u8], i: usize) -> Option<(i32, usize)> {
    if i > 0 && bytes[i - 1].is_ascii_alphanumeric() {
        return None;
    }
    let (year, ylen) = ascii_digits(bytes, i, 4, 4)?;
    let end = i + ylen;
    if bytes.get(end).is_some_and(u8::is_ascii_alphanumeric) {
        return None;
    }
    if !(1000..=2999).contains(&year) {
        return None;
    }
    civil_date(year as i32, 1, 1)?;
    Some((year as i32, end))
}

/// Deterministic content-date extraction (§1): scans `text` left-to-right and
/// returns the FIRST full calendar date, normalized to `YYYY-MM-DD`. Recognizes
/// numeric `YYYY-MM-DD` / `YYYY/MM/DD` (the bench `[date ...]` prefix) and
/// `Month D, YYYY`. Bare month-year and bare year are NOT content dates (a
/// grounded `valid_from` names a specific day). No clock, no LLM.
pub(crate) fn parse_content_date(text: &str) -> Option<jiff::civil::Date> {
    let bytes = text.as_bytes();
    for i in 0..text.len() {
        if !text.is_char_boundary(i) {
            continue;
        }
        if let Some((date, _)) = numeric_date_at(bytes, i) {
            return Some(date);
        }
        if let Some((date, _)) = month_name_date_at(text, i) {
            return Some(date);
        }
    }
    None
}

/// Query-date extraction (§2): the FIRST full date wins as a single-day window;
/// failing that the first bare `Month YYYY` yields a whole-month window; failing
/// that the first bare year yields a whole-year window. `None` when the query
/// carries no date. Soft signal only — never a hard filter.
fn extract_query_date(query: &str) -> Option<DateWindow> {
    let bytes = query.as_bytes();
    for i in 0..query.len() {
        if !query.is_char_boundary(i) {
            continue;
        }
        let full = numeric_date_at(bytes, i)
            .map(|(date, _)| date)
            .or_else(|| month_name_date_at(query, i).map(|(date, _)| date));
        if let Some(date) = full {
            let end = date.tomorrow().ok()?;
            return Some(DateWindow {
                start: midnight_utc(date),
                end: midnight_utc(end),
            });
        }
    }
    for i in 0..query.len() {
        if !query.is_char_boundary(i) {
            continue;
        }
        if let Some((year, month, _)) = month_year_at(query, i) {
            return month_window(year, month);
        }
    }
    for i in 0..query.len() {
        if let Some((year, _)) = year_only_at(bytes, i) {
            return year_window(year);
        }
    }
    None
}

/// `[first-of-month, first-of-next-month)`.
fn month_window(year: i32, month: u8) -> Option<DateWindow> {
    let start = civil_date(year, month, 1)?;
    let (next_year, next_month) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    let end = civil_date(next_year, next_month, 1)?;
    Some(DateWindow {
        start: midnight_utc(start),
        end: midnight_utc(end),
    })
}

/// `[Jan 1, next Jan 1)`.
fn year_window(year: i32) -> Option<DateWindow> {
    let start = civil_date(year, 1, 1)?;
    let end = civil_date(year + 1, 1, 1)?;
    Some(DateWindow {
        start: midnight_utc(start),
        end: midnight_utc(end),
    })
}

/// A grounded `valid_from` instant falls inside the half-open window: parsed
/// timestamp comparison, never lexical.
fn valid_from_in_window(valid_from: &str, window: &DateWindow) -> bool {
    cmp_rfc3339(valid_from, &window.start) != std::cmp::Ordering::Less
        && cmp_rfc3339(valid_from, &window.end) == std::cmp::Ordering::Less
}

/// The `[date YYYY-MM-DD]` pack-prefix source (§3): the leading `YYYY-MM-DD` of
/// a grounded `valid_from`, or `None` when it is absent or not date-leading (so
/// a date is never invented).
fn date_prefix_from_valid_from(valid_from: &str) -> Option<String> {
    let head = valid_from.get(..10)?;
    let date: jiff::civil::Date = head.parse().ok()?;
    Some(date.to_string())
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

/// Temporal-channel score. The existing recency-INTENT signal (an explicit
/// `current`/`latest`/`now` token favouring active semantic units) is unchanged.
/// W5 adds a SOFT date-window boost: when `temporal_window` is `Some` (the query
/// carried a date AND the temporal-grounding flag is on) a recallable unit whose
/// grounded `valid_from` falls inside the queried period also scores `1.0`.
/// `temporal_window == None` (flag off, or no query date) reproduces today's
/// behaviour exactly — the boost never fires and no clock is read.
fn temporal_score(
    unit: &StoredMemoryUnit,
    query: &str,
    temporal_window: Option<&DateWindow>,
) -> f32 {
    let query_tokens = tokenize(query);
    let recency_query = query_tokens
        .iter()
        .any(|token| matches!(token.as_str(), "current" | "latest" | "now"));
    if recency_query && unit.kind == MemoryKind::Semantic && unit.state == UnitState::Active {
        return 1.0;
    }
    if let Some(window) = temporal_window
        && let Some(valid_from) = unit.valid_from.as_deref()
        && valid_from_in_window(valid_from, window)
    {
        return 1.0;
    }
    0.0
}

// Per-channel fusion weights for the weighted-RRF combiner
// (`weight / (RRF_K + rank)`). MEASURED-TUNABLE, NOT SACRED: these are the
// pre-W3 base weights, carried over verbatim so that dropping the
// query-substring hacks is the ONLY change to default ranking. Retune them from
// benchmark evidence, never from query-shape intuition.
const EXACT_CHANNEL_WEIGHT: f32 = 1.0;
const LEXICAL_CHANNEL_WEIGHT: f32 = 1.0;
const SEMANTIC_CHANNEL_WEIGHT: f32 = 2.0;
/// Baseline temporal-channel weight, and its boosted value when the query
/// carries an explicit recency token. This is a whole-token recency-INTENT
/// signal (the same one `temporal_score` keys on) — NOT a query-substring hack —
/// so it survives the W3 fusion cleanup.
const TEMPORAL_CHANNEL_WEIGHT: f32 = 0.5;
const TEMPORAL_RECENCY_CHANNEL_WEIGHT: f32 = 2.5;
const EDGE_CHANNEL_WEIGHT: f32 = 0.5;
const VECTOR_CHANNEL_WEIGHT: f32 = 2.0;

fn channel_weight(pass: ChannelPass, query: &str, temporal_window: Option<&DateWindow>) -> f32 {
    match pass {
        ChannelPass::Exact => EXACT_CHANNEL_WEIGHT,
        ChannelPass::Lexical => LEXICAL_CHANNEL_WEIGHT,
        ChannelPass::Semantic => SEMANTIC_CHANNEL_WEIGHT,
        // A dated query is explicit temporal intent, weighted like a recency
        // query. `temporal_window` is `Some` only when the flag is on AND the
        // query carried a date; when it is `None` this branch is unreachable and
        // the weight is unchanged from today.
        ChannelPass::Temporal if query_has_recency_intent(query) || temporal_window.is_some() => {
            TEMPORAL_RECENCY_CHANNEL_WEIGHT
        }
        ChannelPass::Temporal => TEMPORAL_CHANNEL_WEIGHT,
        ChannelPass::Edge => EDGE_CHANNEL_WEIGHT,
        ChannelPass::Vector => VECTOR_CHANNEL_WEIGHT,
    }
}

/// Whole-token recency intent: the (normalized, tokenized) query mentions
/// `current`, `latest`, or `now`. This mirrors the pre-W3 temporal-weight guard
/// exactly — a substring like "however" or "download" never trips it, unlike the
/// deleted `query.contains("how")` / `query.contains("error")` fusion hacks that
/// this W3 cleanup removed.
fn query_has_recency_intent(query: &str) -> bool {
    tokenize(&normalize_component(query))
        .iter()
        .any(|token| matches!(token.as_str(), "current" | "latest" | "now"))
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
mod temporal_grounding_tests {
    use super::*;

    /// §6 date-parser table: every supported content-date shape parses to the
    /// same normalized `YYYY-MM-DD`, and non-dates / impossible dates are
    /// rejected. Clock-free and deterministic.
    #[test]
    fn parse_content_date_covers_formats_and_rejects_garbage() {
        // (input, expected normalized date or None).
        let cases: &[(&str, Option<&str>)] = &[
            // The bench provenance prefix (slash-separated, zero-padded).
            (
                "[session s1] [date 2023/05/30]\nuser: hi",
                Some("2023-05-30"),
            ),
            // ISO hyphen form.
            ("logged on 2023-05-30 at noon", Some("2023-05-30")),
            // English month name, comma form and no-comma form.
            ("met on May 30, 2023 downtown", Some("2023-05-30")),
            ("due December 1 2024 sharp", Some("2024-12-01")),
            ("shipped Sept 9, 2021", Some("2021-09-09")),
            // Single-digit month/day still normalize with zero padding.
            ("2023-5-3", Some("2023-05-03")),
            // FIRST date wins when several appear.
            ("2020-01-02 then 2021-03-04", Some("2020-01-02")),
            // Garbage / impossible dates → None.
            ("no dates here at all", None),
            ("month 13: 2023-13-01", None),
            ("2023-02-30 is not real", None),
            ("version 12023/05/30 has a 5-digit lead", None),
            ("bare year 2023 alone is not a content date", None),
            ("May 2023 without a day is not a content date", None),
        ];
        for (input, expected) in cases {
            let got = parse_content_date(input).map(|date| date.to_string());
            assert_eq!(
                got.as_deref(),
                *expected,
                "parse_content_date({input:?}) mismatch"
            );
        }
    }

    /// §6 query-date table: full date → single-day window, `Month YYYY` (with or
    /// without a leading "in") → whole-month window, bare year → whole-year
    /// window, no date → `None`. Windows are half-open `[start, end)` midnights.
    #[test]
    fn extract_query_date_covers_day_month_year_and_none() {
        let day = extract_query_date("what happened on 2023-05-30 exactly").expect("day window");
        assert_eq!(day.start, "2023-05-30T00:00:00Z");
        assert_eq!(day.end, "2023-05-31T00:00:00Z");

        let month = extract_query_date("what did I do in May 2023").expect("month window");
        assert_eq!(month.start, "2023-05-01T00:00:00Z");
        assert_eq!(month.end, "2023-06-01T00:00:00Z");

        // No leading "in" still parses as a month window.
        let bare_month = extract_query_date("March 2024 status").expect("month window");
        assert_eq!(bare_month.start, "2024-03-01T00:00:00Z");
        assert_eq!(bare_month.end, "2024-04-01T00:00:00Z");

        // December rolls the month window into the next year.
        let december = extract_query_date("in December 2023").expect("month window");
        assert_eq!(december.start, "2023-12-01T00:00:00Z");
        assert_eq!(december.end, "2024-01-01T00:00:00Z");

        let year = extract_query_date("everything from 2022 please").expect("year window");
        assert_eq!(year.start, "2022-01-01T00:00:00Z");
        assert_eq!(year.end, "2023-01-01T00:00:00Z");

        // A full date takes precedence over the bare year inside it.
        let precedence = extract_query_date("around 2023-07-04 fireworks").expect("day window");
        assert_eq!(precedence.start, "2023-07-04T00:00:00Z");
        assert_eq!(precedence.end, "2023-07-05T00:00:00Z");

        assert!(extract_query_date("no temporal signal here").is_none());
        // A year glued to a token is not a bare-year signal.
        assert!(
            extract_query_date("deploy to project2023 cluster").is_none(),
            "an alphanumeric-glued year is not a bare-year window"
        );
    }

    /// The window membership check is a real half-open interval on parsed
    /// instants: `start <= valid_from < end`, never lexical.
    #[test]
    fn valid_from_window_membership_is_half_open() {
        let window = extract_query_date("in May 2023").expect("month window");
        assert!(
            valid_from_in_window("2023-05-01T00:00:00Z", &window),
            "start is inclusive"
        );
        assert!(
            valid_from_in_window("2023-05-30T00:00:00Z", &window),
            "mid-month is inside"
        );
        assert!(
            !valid_from_in_window("2023-06-01T00:00:00Z", &window),
            "end is exclusive"
        );
        assert!(
            !valid_from_in_window("2023-04-30T00:00:00Z", &window),
            "before start is outside"
        );
    }

    /// The pack-prefix source reads the leading `YYYY-MM-DD` of a grounded
    /// `valid_from`, and yields nothing for a non-date-leading string (a date is
    /// never invented).
    #[test]
    fn date_prefix_reads_valid_from_head() {
        assert_eq!(
            date_prefix_from_valid_from("2023-05-30T00:00:00Z").as_deref(),
            Some("2023-05-30")
        );
        assert_eq!(date_prefix_from_valid_from("not-a-date").as_deref(), None);
        assert_eq!(date_prefix_from_valid_from("").as_deref(), None);
    }
}

#[cfg(test)]
mod fusion_weight_tests {
    use super::*;

    /// W3: channel weights carry NO query-substring special cases. A query
    /// containing "how"/"error" (anywhere, including inside other words) yields
    /// the SAME weight as an equivalent query without them — the deleted hacks
    /// were `query.contains("how")` (Exact→2.5, Lexical→2.0) and
    /// `query.contains("error")` (Lexical→3.0).
    #[test]
    fn channel_weight_has_no_query_substring_special_cases() {
        let with_substrings = "how do I fix this error in the pipeline";
        let without = "do I fix this in the pipeline";
        for pass in [
            ChannelPass::Exact,
            ChannelPass::Lexical,
            ChannelPass::Semantic,
            ChannelPass::Edge,
            ChannelPass::Vector,
        ] {
            assert_eq!(
                channel_weight(pass, with_substrings, None),
                channel_weight(pass, without, None),
                "{pass:?} weight must not depend on 'how'/'error' substrings"
            );
        }
        // The two channels the hacks used to boost now hold their plain base
        // weight regardless of the query.
        assert_eq!(
            channel_weight(ChannelPass::Exact, with_substrings, None),
            EXACT_CHANNEL_WEIGHT
        );
        assert_eq!(
            channel_weight(ChannelPass::Lexical, with_substrings, None),
            LEXICAL_CHANNEL_WEIGHT
        );
        // Substrings buried inside other words ("however" ⊃ "how", "terror" ⊃
        // "error") never trip anything either.
        assert_eq!(
            channel_weight(ChannelPass::Lexical, "however the terror subsided", None),
            LEXICAL_CHANNEL_WEIGHT
        );
    }

    /// The whole-token temporal recency signal is NOT a substring hack and
    /// survives the cleanup: an explicit recency TOKEN still boosts the temporal
    /// channel, a non-recency query keeps the base weight, and a mere substring
    /// ("nowhere" ⊃ "now") does NOT boost.
    #[test]
    fn temporal_recency_intent_is_token_based_and_survives() {
        assert_eq!(
            channel_weight(ChannelPass::Temporal, "what is the latest status", None),
            TEMPORAL_RECENCY_CHANNEL_WEIGHT
        );
        assert_eq!(
            channel_weight(ChannelPass::Temporal, "what is the status", None),
            TEMPORAL_CHANNEL_WEIGHT
        );
        assert_eq!(
            channel_weight(ChannelPass::Temporal, "we are getting nowhere", None),
            TEMPORAL_CHANNEL_WEIGHT
        );
    }
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
            cross_rerank_rank: None,
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
            PackLevers::default(),
            false,
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
            PackLevers::default(),
            false,
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
            PackLevers::default(),
            false,
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

    /// A unit stamped with an explicit `source_episode_id` so the packing tests
    /// can exercise the per-session diversity quota and sibling-gather.
    fn unit_ep(
        id: u128,
        episode: u128,
        body: &str,
        chunks: Vec<ContextualChunk>,
    ) -> StoredMemoryUnit {
        let mut unit = unit(id, body, chunks);
        unit.source_episode_id = Some(EpisodeId::from_u128(episode));
        unit
    }

    fn distinct_episodes(packed: &PackedRecallContext) -> std::collections::HashSet<EpisodeId> {
        packed
            .items
            .iter()
            .filter_map(|item| item.citation_episode_id)
            .collect()
    }

    /// §5 sibling-gather: after the greedy fill, an already chunk-rendered item
    /// spends the pack's leftover budget on its OWN unselected sibling chunks.
    /// At admission the per-item cap (the 20-token whole-body count) admits the
    /// matched chunk plus one neighbour; the trailing sibling is left out until
    /// the post-pass pulls it in. It must never evict the co-packed plain item
    /// nor push token_estimate past the budget.
    #[test]
    fn sibling_gather_expands_item_without_eviction_or_overbudget() {
        let query_tokens = tokenize("quantum");
        // Three 8-token blocks (6-token header + 2-token body). Whole body is 20
        // tokens, so admission fits chunk[0] (matched, 8) + chunk[1] (neighbour,
        // 8) = 16; chunk[2] (24 > 20) is deferred to the sibling pass.
        let chunks = || {
            vec![
                chunk("1-2", "quantum alpha"), // matches the query
                chunk("3-4", "beta gamma"),    // neighbour pulled at admission
                chunk("5-6", "delta epsilon"), // trailing sibling, gathered later
            ]
        };
        let candidates = || {
            vec![
                candidate(unit(1, &body_of(20), chunks()), 5.0),
                candidate(unit(2, &body_of(5), Vec::new()), 4.0),
            ]
        };
        let budget = 100;

        // Off: today's behaviour — A keeps only the two admission-time chunks.
        let off = pack_recall_context(
            candidates(),
            &request(budget),
            &[],
            &query_tokens,
            Vec::new(),
            2,
            PackLevers::default(),
            false,
        );
        assert_eq!(off.items.len(), 2, "both items packed");
        assert_eq!(off.token_estimate, 16 + 5, "A charged 16, B charged 5");
        assert!(
            !off.items[0].body.contains("delta epsilon"),
            "trailing sibling absent without the lever: {}",
            off.items[0].body
        );

        // On: the post-pass pulls chunk[2] into A with leftover budget; B is
        // untouched, and token_estimate stays within budget.
        let on = pack_recall_context(
            candidates(),
            &request(budget),
            &[],
            &query_tokens,
            Vec::new(),
            2,
            PackLevers {
                sibling_gather_enabled: true,
                session_quota: None,
            },
            false,
        );
        assert_eq!(
            on.items.len(),
            2,
            "sibling-gather never evicts a packed item"
        );
        assert_eq!(
            on.items[1].unit_id,
            UnitId::from_u128(2),
            "the co-packed plain item survives"
        );
        assert_eq!(on.items[1].body, body_of(5), "plain item body unchanged");
        assert!(
            on.items[0].body.contains("[turns 5-6]") && on.items[0].body.contains("delta epsilon"),
            "the trailing sibling is gathered: {}",
            on.items[0].body
        );
        assert!(
            on.items[0].body.contains("quantum alpha") && on.items[0].body.contains("beta gamma"),
            "the admission chunks are preserved in document order: {}",
            on.items[0].body
        );
        assert_eq!(
            on.token_estimate,
            24 + 5,
            "A now charged 24 (three 8-token blocks) + B 5"
        );
        assert!(on.token_estimate <= budget, "never exceeds budget");
    }

    /// §5 quota: an unquota'd greedy fill (candidate order) lets episode 1's eight
    /// leading candidates monopolise a `k = 8` pack (one distinct episode). The
    /// quota caps admissions per episode at 2 until every session has had a
    /// look-in, so ≥4 distinct episodes surface. Bodies are query-disjoint and
    /// equally scored, so no replacement fires — the quota alone drives the change.
    #[test]
    fn session_quota_admits_distinct_episodes_over_monopoly() {
        let query_tokens = tokenize("quantum");
        let candidates = || {
            let mut v = Vec::new();
            for i in 0..8u128 {
                v.push(candidate(
                    unit_ep(100 + i, 1, "alpha beta", Vec::new()),
                    1.0,
                ));
            }
            for episode in 2..=5u128 {
                for j in 0..2u128 {
                    v.push(candidate(
                        unit_ep(episode * 10 + j, episode, "alpha beta", Vec::new()),
                        1.0,
                    ));
                }
            }
            v
        };
        let mut req = request(10_000);
        req.k = 8;

        let off = pack_recall_context(
            candidates(),
            &req,
            &[],
            &query_tokens,
            Vec::new(),
            16,
            PackLevers::default(),
            false,
        );
        assert_eq!(
            distinct_episodes(&off).len(),
            1,
            "unquota'd greedy is monopolised by episode 1"
        );

        let on = pack_recall_context(
            candidates(),
            &req,
            &[],
            &query_tokens,
            Vec::new(),
            16,
            PackLevers {
                sibling_gather_enabled: false,
                session_quota: Some(DEFAULT_SESSION_DIVERSITY_QUOTA),
            },
            false,
        );
        assert!(
            distinct_episodes(&on).len() >= 4,
            "the quota surfaces >=4 distinct episodes, got {:?}",
            distinct_episodes(&on)
        );
        assert_eq!(on.items.len(), 8, "the pack is still filled to k");
    }

    /// §5 work-conserving: the quota must never leave admissible budget unused.
    /// With one dominant session (four candidates) plus one other, the capped
    /// pass admits only two of the big session; the second, unrestricted pass
    /// then fills the rest, so the quota packs exactly the same units as the
    /// unquota'd fill.
    #[test]
    fn session_quota_is_work_conserving() {
        let query_tokens = tokenize("quantum");
        let candidates = || {
            let mut v = Vec::new();
            for i in 0..4u128 {
                v.push(candidate(
                    unit_ep(100 + i, 1, "alpha beta", Vec::new()),
                    1.0,
                ));
            }
            v.push(candidate(unit_ep(200, 2, "alpha beta", Vec::new()), 1.0));
            v
        };
        let mut req = request(10_000);
        req.k = 10;

        let off = pack_recall_context(
            candidates(),
            &req,
            &[],
            &query_tokens,
            Vec::new(),
            5,
            PackLevers::default(),
            false,
        );
        let on = pack_recall_context(
            candidates(),
            &req,
            &[],
            &query_tokens,
            Vec::new(),
            5,
            PackLevers {
                sibling_gather_enabled: false,
                session_quota: Some(2),
            },
            false,
        );
        let off_ids: std::collections::HashSet<UnitId> =
            off.items.iter().map(|item| item.unit_id).collect();
        let on_ids: std::collections::HashSet<UnitId> =
            on.items.iter().map(|item| item.unit_id).collect();
        assert_eq!(
            on_ids, off_ids,
            "the quota leaves no admissible candidate unpacked"
        );
        assert_eq!(on.items.len(), 5, "all five candidates packed");
        assert_eq!(
            on.token_estimate, off.token_estimate,
            "no budget left unused vs the unrestricted fill"
        );
    }

    /// §5 off-flags byte-identical: with both levers OFF the packer matches a
    /// reference default run bit-for-bit (composition, bodies, cost, drops) and
    /// the hand-computed golden of today's packer.
    #[test]
    fn levers_off_pack_is_byte_identical() {
        let query_tokens = tokenize("quantum");
        let scenario = || {
            vec![
                candidate(unit_ep(1, 1, &body_of(10), Vec::new()), 5.0),
                candidate(
                    unit_ep(
                        2,
                        1,
                        &body_of(40),
                        vec![
                            chunk("1-4", "quantum harmonica"),
                            chunk("5-8", "berlin note"),
                        ],
                    ),
                    4.0,
                ),
                candidate(unit_ep(3, 2, &body_of(10), Vec::new()), 3.0),
            ]
        };
        let budget = 30;
        let reference = pack_recall_context(
            scenario(),
            &request(budget),
            &[],
            &query_tokens,
            Vec::new(),
            3,
            PackLevers::default(),
            false,
        );
        let again = pack_recall_context(
            scenario(),
            &request(budget),
            &[],
            &query_tokens,
            Vec::new(),
            3,
            PackLevers::default(),
            false,
        );
        assert_eq!(
            reference
                .items
                .iter()
                .map(|item| (item.unit_id, item.body.clone()))
                .collect::<Vec<_>>(),
            again
                .items
                .iter()
                .map(|item| (item.unit_id, item.body.clone()))
                .collect::<Vec<_>>(),
            "composition + bodies identical across default runs",
        );
        assert_eq!(reference.token_estimate, again.token_estimate);
        assert_eq!(
            reference
                .dropped_items
                .iter()
                .map(|drop| (drop.unit_id, format!("{:?}", drop.reason)))
                .collect::<Vec<_>>(),
            again
                .dropped_items
                .iter()
                .map(|drop| (drop.unit_id, format!("{:?}", drop.reason)))
                .collect::<Vec<_>>(),
            "drops identical across default runs",
        );
        // Hand-computed golden of today's packer: A (whole body, 10) then B (chunk
        // render, 16) fit budget 30; C (10) overflows and drops for budget.
        assert_eq!(
            reference
                .items
                .iter()
                .map(|item| item.unit_id)
                .collect::<Vec<_>>(),
            vec![UnitId::from_u128(1), UnitId::from_u128(2)],
        );
        assert_eq!(
            reference.token_estimate, 26,
            "10 (plain) + 16 (chunk render)"
        );
        assert!(
            reference
                .dropped_items
                .iter()
                .any(|drop| drop.unit_id == UnitId::from_u128(3)
                    && drop.reason == RecallDropReason::Budget),
            "the third item drops for budget: {:?}",
            reference.dropped_items
        );
    }

    /// A unit with a grounded `valid_from`, for the dated-pack tests.
    fn unit_dated(id: u128, body: &str, valid_from: &str) -> StoredMemoryUnit {
        let mut unit = unit(id, body, Vec::new());
        unit.valid_from = Some(valid_from.to_string());
        unit
    }

    /// §6 dated packs: with the flag on, each item whose unit carries a grounded
    /// `valid_from` gets a leading `[date YYYY-MM-DD]` line resolved from it; a
    /// unit without a grounded date is left unprefixed (a date is never invented).
    #[test]
    fn temporal_grounding_prefixes_dated_items_only() {
        let query_tokens = tokenize("filler");
        let candidates = vec![
            candidate(unit_dated(1, &body_of(6), "2023-05-30T00:00:00Z"), 5.0),
            candidate(unit(2, &body_of(6), Vec::new()), 4.0),
        ];
        let packed = pack_recall_context(
            candidates,
            &request(10_000),
            &[],
            &query_tokens,
            Vec::new(),
            2,
            PackLevers::default(),
            true,
        );
        assert_eq!(packed.items.len(), 2);
        assert_eq!(
            packed.items[0].body,
            format!("[date 2023-05-30]\n{}", body_of(6)),
            "grounded item is date-prefixed"
        );
        assert_eq!(
            packed.items[1].body,
            body_of(6),
            "ungrounded item is not prefixed — a date is never invented"
        );
    }

    /// §6 flag-off byte-identity: the same pack with the flag OFF carries no
    /// prefix on any item — bit-identical to today regardless of grounded dates.
    #[test]
    fn temporal_grounding_off_is_byte_identical() {
        let query_tokens = tokenize("filler");
        let candidates = || {
            vec![
                candidate(unit_dated(1, &body_of(6), "2023-05-30T00:00:00Z"), 5.0),
                candidate(unit(2, &body_of(6), Vec::new()), 4.0),
            ]
        };
        let off = pack_recall_context(
            candidates(),
            &request(10_000),
            &[],
            &query_tokens,
            Vec::new(),
            2,
            PackLevers::default(),
            false,
        );
        assert!(
            off.items.iter().all(|item| item.body == body_of(6)),
            "flag off ⇒ no item is date-prefixed: {:?}",
            off.items.iter().map(|i| i.body.clone()).collect::<Vec<_>>()
        );
    }
}

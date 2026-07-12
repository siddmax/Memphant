//! `MemoryService`: the one application layer shared by REST, MCP, CLI and
//! the background worker. All orchestration (retain dispatch, reflect job
//! claiming/compilation, degraded read-your-own-writes recall) lives here —
//! transport handlers stay thin.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use memphant_types::{
    COMPILER_VERSION, ContextualChunk, CorrectRequest, CorrectResult, ENGINE_VERSION, EpisodeId,
    ForgetRequest, ForgetResult, MarkRequest, MarkResult, MemoryKind, RecallContextItem,
    RecallHttpRequest, RecallMode, RecallRequest, RecallResponse, ReflectCandidate, ReflectInput,
    ReflectJobKind, ReflectResult, ResourceId, ResourceKind, RetainEpisodeHttpRequest,
    RetainEpisodeHttpResponse, RetainRequest, RetainResourceRequest, RetrievalTrace, ScopeId,
    StoredEpisode, TenantId, TraceId, TrustLevel, UnitId,
};

use crate::{
    Clock, CoreError, CrossReranker, DEFAULT_CANDIDATE_POOL_SIZE, EmbeddingProvider, JobFilter,
    MemoryStore, PackLevers, ReflectJobRow, ScopePage, StoreError, VectorQuery, correct_memory,
    embedding_profile_for, forget_memory, normalize_component, parse_content_date,
    recall_with_pool, record_mark, reflect_recorded, retain_episode, retain_resource, tokenize,
};

/// Errors surfaced by the application layer. Transport layers map these onto
/// their envelope (REST status codes / MCP tool errors).
#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error(transparent)]
    Core(#[from] CoreError),
    #[error("invalid request: {0}")]
    Invalid(String),
}

impl From<StoreError> for ServiceError {
    fn from(error: StoreError) -> Self {
        Self::Core(CoreError::Store(error))
    }
}

/// Comparable trust ranking (higher = more trusted). Used to clamp
/// caller-declared trust at the API key's `max_trust` ceiling — trust is
/// provenance-derived, never forgeable.
pub fn trust_rank(level: TrustLevel) -> u8 {
    match level {
        TrustLevel::TrustedSystem => 7,
        TrustLevel::TrustedUser => 6,
        TrustLevel::VerifiedTool => 5,
        TrustLevel::UnverifiedTool => 4,
        TrustLevel::WebContent => 3,
        TrustLevel::ImportedExternal => 2,
        TrustLevel::AgentOutput => 1,
        TrustLevel::Quarantined => 0,
    }
}

/// `min(declared, ceiling)` on the trust lattice.
pub fn clamp_trust(declared: TrustLevel, ceiling: TrustLevel) -> TrustLevel {
    if trust_rank(declared) > trust_rank(ceiling) {
        ceiling
    } else {
        declared
    }
}

pub struct MemoryService<S: MemoryStore> {
    store: Arc<S>,
    clock: Arc<dyn Clock>,
    embedder: Arc<dyn EmbeddingProvider>,
    /// Rung 4 write-time toggle: when set, the reflect-stage compile mints
    /// per-episode contextual chunks (§`compile_job`). DEFAULT TRUE (promoted
    /// 2026-07-10): the paired ablation through THIS runtime path cleared —
    /// LME-S n=100 seed 20260710 session+runtime-chunks (shipped code incl.
    /// reclaim) vs session baseline: ΔQA +0.110 [+0.020, +0.190], ΔR@5 +0.117
    /// [+0.053, +0.191], ΔR@10 +0.117 [+0.053, +0.191] (all 95% CIs exclude
    /// zero; reader gpt-5.6-terra@medium, judge claude-sonnet-5, 1000-resample
    /// paired bootstrap). Proof:
    /// `docs/build-log/artifacts/real-retrieval-20260710/scaled-reader-or-session-chunkpack-rerank-off.json`
    /// and `scaled-lme-s-session-chunkpack-rerank-off.json`. The
    /// `with_contextual_chunks_write_enabled(false)` builder stays so ablations
    /// can force the chunks-off control arm.
    contextual_chunks_write_enabled: bool,
    /// R1 docs-domain twin of `contextual_chunks_write_enabled`: when set, the
    /// reflect-stage compile of a `kind=document` resource mints per-resource
    /// contextual chunks (§`compile_job` `ReflectResource` arm) via
    /// `resource_contextual_chunks` — the SAME rung-4 machinery episodes use,
    /// extended to the docs domain. DEFAULT FALSE (flag-gated until an R1-T4
    /// promotion): shipped behavior is byte-identical to today (whole-section
    /// units, no chunks). Non-document resource kinds are never chunked. Set via
    /// `with_resource_chunks_write_enabled` (the gate's `--resource-chunks` /
    /// `MEMPHANT_RESOURCE_CHUNKS` thread it through the runtime).
    resource_chunks_write_enabled: bool,
    /// W3 candidate-pool knob: the recall vector-channel KNN fan-out. DEFAULT
    /// `DEFAULT_CANDIDATE_POOL_SIZE` (32), which reproduces today's ranking
    /// exactly. Raised via `with_candidate_pool_size` so the W8 cross-encoder
    /// rerank arm can rerank a widened 64–128 vector pool — no wire change. See
    /// the pool-mapping note on `DEFAULT_CANDIDATE_POOL_SIZE`.
    candidate_pool_size: usize,
    /// W4 packing levers (sibling-gather + session-diversity quota), threaded
    /// construction-time like `candidate_pool_size`. BOTH DEFAULT OFF: they ship
    /// default-on only after the accuracy-wave measurement campaign, so the bench
    /// needs the flags. Set via `with_sibling_gather_enabled` / `with_session_quota`.
    pack_levers: PackLevers,
    /// W5 temporal-grounding toggle (DEFAULT OFF). Gates all three temporal
    /// behaviours together: reflect-stage content-date grounding of `valid_from`
    /// and dated contextual-chunk headers (`compile_job`), query-date windowing
    /// at recall, and date-prefixed packed items (recall). Off means every one of
    /// those paths is byte-identical to today. Promotion is measurement-only, so
    /// it ships off and the bench threads it via `with_temporal_grounding_enabled`.
    temporal_grounding_enabled: bool,
    /// W6 deterministic fact-extraction toggle (DEFAULT OFF). When on, the
    /// reflect-stage compile of an EPISODE mines its user turns for first-person
    /// preference/attribute statements and emits extra short, honest-subject-key
    /// ReflectCandidates alongside the raw episode unit (`compile_job`). Off means
    /// the compile is byte-identical to today (only the raw episode candidate).
    /// Independent of `temporal_grounding_enabled`: the two only interact so that
    /// a mined fact body is `[date ...]`-prefixed when BOTH are on and the body
    /// carries a parseable content date. Measurement-only promotion, so it ships
    /// off and the bench threads it via `with_fact_extraction_enabled`.
    fact_extraction_enabled: bool,
    /// W8 cross-encoder rerank seam (DEFAULT `None`). When set, recall reorders
    /// the top `candidate_pool_size` fused candidates by a real `(query, body)`
    /// cross-encoder AFTER fusion and BEFORE packing — the widened-pool rerank
    /// arm. `None` leaves recall byte-identical to today. Independent of the
    /// retired heuristic rerank stage. Set via `with_cross_reranker`; the bench
    /// lane's `--cross-rerank` threads the real fastembed reranker here.
    cross_reranker: Option<Arc<dyn CrossReranker>>,
}

impl<S: MemoryStore> Clone for MemoryService<S> {
    fn clone(&self) -> Self {
        Self {
            store: Arc::clone(&self.store),
            clock: Arc::clone(&self.clock),
            embedder: Arc::clone(&self.embedder),
            contextual_chunks_write_enabled: self.contextual_chunks_write_enabled,
            resource_chunks_write_enabled: self.resource_chunks_write_enabled,
            candidate_pool_size: self.candidate_pool_size,
            pack_levers: self.pack_levers,
            temporal_grounding_enabled: self.temporal_grounding_enabled,
            fact_extraction_enabled: self.fact_extraction_enabled,
            cross_reranker: self.cross_reranker.clone(),
        }
    }
}

impl<S: MemoryStore> MemoryService<S> {
    pub fn new(store: Arc<S>, clock: Arc<dyn Clock>, embedder: Arc<dyn EmbeddingProvider>) -> Self {
        Self {
            store,
            clock,
            embedder,
            contextual_chunks_write_enabled: true,
            resource_chunks_write_enabled: false,
            candidate_pool_size: DEFAULT_CANDIDATE_POOL_SIZE,
            pack_levers: PackLevers::default(),
            temporal_grounding_enabled: false,
            fact_extraction_enabled: false,
            cross_reranker: None,
        }
    }

    /// Overrides the rung 4 contextual-chunk write path (default on since the
    /// 2026-07-10 promotion). A builder override so ablations can force the
    /// control arm: the bench lane's `--disable runtime_chunks` passes `false`
    /// here to run the chunk-free baseline (old behavior).
    pub fn with_contextual_chunks_write_enabled(mut self, enabled: bool) -> Self {
        self.contextual_chunks_write_enabled = enabled;
        self
    }

    /// Overrides the R1 docs-domain resource-chunk write path (default OFF).
    /// When enabled, the reflect-stage compile of a `kind=document` resource
    /// mints per-resource contextual chunks (the docs twin of the episode
    /// chunk path). Construction-time only, mirroring
    /// `with_contextual_chunks_write_enabled`: no recall-request/wire change.
    /// The runtime threads `MEMPHANT_RESOURCE_CHUNKS` here so the gate's
    /// `--resource-chunks` reaches both the server and worker.
    pub fn with_resource_chunks_write_enabled(mut self, enabled: bool) -> Self {
        self.resource_chunks_write_enabled = enabled;
        self
    }

    /// Overrides the recall candidate-pool size (default
    /// `DEFAULT_CANDIDATE_POOL_SIZE`). This directly sets the vector-channel KNN
    /// fan-out for recall — the widen-able per-family limit the W8 rerank arm
    /// needs at 64–128; the bench lane's `--pool <n>` threads its value here.
    /// Construction-time only, mirroring `with_contextual_chunks_write_enabled`:
    /// no recall-request/wire field changes.
    pub fn with_candidate_pool_size(mut self, size: usize) -> Self {
        self.candidate_pool_size = size;
        self
    }

    /// Enables the W4 sibling-gather packing post-pass (default OFF). When on,
    /// after the greedy fill the packer spends leftover budget expanding already
    /// chunk-rendered items with their own unselected sibling chunks — never
    /// evicting a packed item nor exceeding budget. Construction-time only,
    /// mirroring `with_candidate_pool_size`: no recall-request/wire change. The
    /// bench lane's `--sibling-gather` threads its value here.
    pub fn with_sibling_gather_enabled(mut self, enabled: bool) -> Self {
        self.pack_levers.sibling_gather_enabled = enabled;
        self
    }

    /// Sets the W4 per-`source_episode_id` diversity quota (default OFF = `None`).
    /// `Some(cap)` caps admissions per session during the greedy fill until every
    /// distinct episode has had a look-in, then fills remaining budget
    /// unrestricted (work-conserving). `DEFAULT_SESSION_DIVERSITY_QUOTA` (2) is
    /// the recommended value. Construction-time only; the bench lane's
    /// `--session-quota <n>` threads its value here.
    pub fn with_session_quota(mut self, quota: Option<usize>) -> Self {
        self.pack_levers.session_quota = quota;
        self
    }

    /// Enables W5 temporal grounding (default OFF): reflect-stage content-date
    /// grounding of `valid_from` + dated chunk headers, query-date windowing at
    /// recall, and `[date ...]`-prefixed packed items. Construction-time only,
    /// mirroring the W3/W4 knobs; the bench lane's `--temporal-grounding` threads
    /// its value here. Off ⇒ all three paths behave exactly as today.
    pub fn with_temporal_grounding_enabled(mut self, enabled: bool) -> Self {
        self.temporal_grounding_enabled = enabled;
        self
    }

    /// Enables W6 deterministic fact extraction (default OFF): the reflect-stage
    /// episode compile mines user turns for preference/attribute statements and
    /// emits extra short, honest-subject-key ReflectCandidates. Construction-time
    /// only, mirroring the W3/W4/W5 knobs; the bench lane's `--fact-extraction`
    /// threads its value here. Off ⇒ the compile is byte-identical to today.
    pub fn with_fact_extraction_enabled(mut self, enabled: bool) -> Self {
        self.fact_extraction_enabled = enabled;
        self
    }

    /// Installs the W8 cross-encoder rerank seam (default `None`). When set,
    /// recall reorders the top `candidate_pool_size` fused candidates by this
    /// reranker's `(query, body)` scores AFTER fusion and BEFORE packing.
    /// Construction-time only, mirroring the W3/W4/W5 knobs; the bench lane's
    /// `--cross-rerank` threads the real fastembed reranker here. Unset ⇒ recall
    /// is byte-identical to today. Independent of the retired heuristic rerank.
    pub fn with_cross_reranker(mut self, reranker: Arc<dyn CrossReranker>) -> Self {
        self.cross_reranker = Some(reranker);
        self
    }

    pub fn store(&self) -> &S {
        &self.store
    }

    pub fn embedder(&self) -> &dyn EmbeddingProvider {
        self.embedder.as_ref()
    }

    /// The retain verb: payload-dispatched between `episode` (default),
    /// `resource` and direct `unit` shapes (spec 08 §209). `tenant` comes from
    /// the authenticated key, never the body; `source_trust` must already be
    /// clamped at the edge.
    pub async fn retain(
        &self,
        tenant: TenantId,
        request: RetainEpisodeHttpRequest,
    ) -> Result<RetainEpisodeHttpResponse, ServiceError> {
        let compiler_version = request
            .compiler_version
            .clone()
            .unwrap_or_else(|| COMPILER_VERSION.to_string());
        match (&request.resource, &request.unit) {
            (Some(_), Some(_)) => Err(ServiceError::Invalid(
                "retain accepts exactly one payload shape: episode body, resource, or unit"
                    .to_string(),
            )),
            (Some(resource), None) => {
                let outcome = retain_resource(
                    self.store.as_ref(),
                    RetainResourceRequest {
                        tenant_id: tenant,
                        scope_id: request.scope_id,
                        actor_id: request.actor_id,
                        uri: resource.uri.clone(),
                        kind: resource.kind,
                        content_hash: resource.content_hash.clone(),
                        mime_type: resource.mime_type.clone(),
                        revision: resource.revision.clone(),
                        body: resource.body.clone(),
                        source_trust: request.source_trust,
                        compiler_version,
                    },
                )
                .await?;
                Ok(RetainEpisodeHttpResponse {
                    episode_id: None,
                    resource_id: Some(outcome.resource_id),
                    unit_ids: Vec::new(),
                    dedup: None,
                    assigned_trust: Some(request.source_trust),
                    enqueued: vec!["reflect_resource".to_string()],
                    trace_ref: None,
                })
            }
            (None, Some(unit)) => {
                if unit.subject.trim().is_empty() || unit.predicate.trim().is_empty() {
                    return Err(ServiceError::Invalid(
                        "unit retain requires an explicit subject and predicate".to_string(),
                    ));
                }
                // A direct unit write is a synchronous reflect of one
                // caller-asserted candidate: the admission trust policy
                // applies unchanged (untrusted keys mint candidate tier).
                let job_id = memphant_types::JobId::new();
                let (trace, unit_ids) = reflect_recorded(
                    self.store.as_ref(),
                    ReflectInput {
                        tenant_id: tenant,
                        scope_id: request.scope_id,
                        actor_id: request.actor_id,
                        episode_id: None,
                        resource_id: None,
                        job_id,
                        compiler_version,
                        candidates: vec![ReflectCandidate {
                            source_kind: "direct".to_string(),
                            trust_level: request.source_trust,
                            actor_id: request.actor_id,
                            subject: Some(unit.subject.clone()),
                            predicate: Some(unit.predicate.clone()),
                            kind: Some(unit.kind),
                            body: unit.body.clone(),
                            churn_class: unit.churn_class.clone(),
                            admission_hint: None,
                            contextual_chunks: Vec::new(),
                            valid_from: None,
                            valid_to: None,
                        }],
                    },
                    self.embedder.as_ref(),
                    self.clock.as_ref(),
                )
                .await?;
                Ok(RetainEpisodeHttpResponse {
                    episode_id: None,
                    resource_id: None,
                    unit_ids,
                    dedup: None,
                    assigned_trust: Some(request.source_trust),
                    enqueued: Vec::new(),
                    trace_ref: Some(format!("memphant://trace/{}", trace.job_id.as_uuid())),
                })
            }
            (None, None) => {
                let body = request
                    .body
                    .clone()
                    .filter(|body| !body.trim().is_empty())
                    .ok_or(CoreError::EmptyBody)?;
                let outcome = retain_episode(
                    self.store.as_ref(),
                    RetainRequest {
                        tenant_id: tenant,
                        scope_id: request.scope_id,
                        actor_id: request.actor_id,
                        source_kind: request.source_kind.clone(),
                        source_trust: request.source_trust,
                        subject_hint: request.subject_hint.clone(),
                        subject: request.subject.clone(),
                        predicate: request.predicate.clone(),
                        body,
                        compiler_version,
                    },
                )
                .await?;
                Ok(RetainEpisodeHttpResponse {
                    episode_id: Some(outcome.episode_id),
                    resource_id: None,
                    unit_ids: Vec::new(),
                    dedup: Some(outcome.dedup),
                    assigned_trust: Some(request.source_trust),
                    enqueued: vec!["reflect_episode".to_string()],
                    trace_ref: None,
                })
            }
        }
    }

    /// The recall verb with the read-your-own-writes degraded fallback: when
    /// no units match AND the scope has pending reflect jobs, raw episode
    /// bodies are matched and returned with `degraded: true` (spec 08 §4).
    pub async fn recall(
        &self,
        tenant: TenantId,
        request: RecallHttpRequest,
    ) -> Result<RecallResponse, ServiceError> {
        let scope_id = request.scope_id;
        let query = request.query.clone();
        // Defensive ceilings on caller-supplied sizing, for symmetry with the
        // scope endpoint's clamp. No allocation is driven by these (output is
        // bounded by the candidate pool), so they only reject absurd values.
        const MAX_RECALL_LIMIT: usize = 1_000;
        const MAX_RECALL_BUDGET_TOKENS: usize = 1_000_000;
        let k = request.limit.unwrap_or(8).clamp(1, MAX_RECALL_LIMIT);
        // Real embedding provider → embed the query and run the vector
        // channel; the Noop provider keeps the channel honestly disabled.
        let query_vec = if self.embedder.dimensions() > 0 {
            self.embedder
                .embed_query(std::slice::from_ref(&query))
                .map_err(|error| {
                    ServiceError::Core(CoreError::Store(StoreError::Backend(format!(
                        "query embedding failed: {error}"
                    ))))
                })?
                .into_iter()
                .next()
                .filter(|vec| !vec.is_empty())
        } else {
            None
        };
        // The stored counterparts of `query_vec` live under the active
        // embedder's profile; the store filters `<=>` to that id (spec 03).
        let vector_query = query_vec.as_deref().map(|vec| VectorQuery {
            vec,
            profile_id: embedding_profile_for(self.embedder()).id,
        });
        let response = recall_with_pool(
            self.store.as_ref(),
            RecallRequest {
                tenant_id: tenant,
                scope_id,
                actor_id: request.actor_id,
                allowed_scope_ids: request
                    .allowed_scope_ids
                    .clone()
                    .unwrap_or_else(|| vec![scope_id]),
                query: query.clone(),
                k,
                budget_tokens: request
                    .budget_tokens
                    .unwrap_or(512)
                    .clamp(1, MAX_RECALL_BUDGET_TOKENS),
                mode: request.mode.unwrap_or(RecallMode::Fast),
                include_beliefs: request.include_beliefs.unwrap_or(false),
                edge_expansion_enabled: request.edge_expansion_enabled.unwrap_or(true),
                context_packing_abstention_enabled: request
                    .context_packing_abstention_enabled
                    .unwrap_or(true),
                // Real-evidence default (rung 8 disable-when, real-retrieval-20260710):
                // the deterministic reranker cost -0.143 Recall@5 on LongMemEval-S
                // (CI excludes zero), so it is opt-in until a variant earns its keep.
                rerank_enabled: request.rerank_enabled.unwrap_or(false),
                learned_rerank_profile: None,
                query_decomposition_enabled: request.query_decomposition_enabled.unwrap_or(true),
                procedure_recall_enabled: request.procedure_recall_enabled.unwrap_or(true),
                decay_enabled: request.decay_enabled.unwrap_or(true),
                engine_version: ENGINE_VERSION.to_string(),
            },
            vector_query,
            self.clock.as_ref(),
            self.candidate_pool_size,
            self.pack_levers,
            self.temporal_grounding_enabled,
            self.cross_reranker.as_deref(),
        )
        .await?;

        if !response.items.is_empty() {
            return Ok(response);
        }
        let pending = self.store.pending_job_count(tenant, scope_id).await?;
        if pending == 0 {
            return Ok(response);
        }
        let episodes = self
            .store
            .fetch_episodes_for_scope(tenant, scope_id, 256)
            .await?;
        let items = degraded_episode_items(&episodes, &query, k.max(1));
        if items.is_empty() {
            return Ok(response);
        }
        Ok(RecallResponse {
            degraded: true,
            consolidation_lag_ms: 1,
            abstention: false,
            candidate_whitelist: items.iter().map(|item| item.unit_id).collect(),
            citations: items
                .iter()
                .filter_map(|item| {
                    item.citation_episode_id
                        .map(|episode_id| memphant_types::RecallCitation {
                            unit_id: item.unit_id,
                            episode_id: Some(episode_id),
                            resource_id: None,
                        })
                })
                .collect(),
            items,
            ..response
        })
    }

    /// The reflect verb: claims THIS scope's pending jobs through the same
    /// claim/complete path the worker uses (never double-compiles) and
    /// compiles them synchronously.
    pub async fn reflect(
        &self,
        tenant: TenantId,
        scope: ScopeId,
        compiler_version: Option<String>,
    ) -> Result<ReflectResult, ServiceError> {
        let jobs = self
            .store
            .claim_reflect_jobs(
                JobFilter {
                    tenant: Some(tenant),
                    scope: Some(scope),
                },
                usize::MAX,
            )
            .await?;
        let mut consumed = 0;
        let mut created = 0;
        let mut trace_ref = None;
        for job in jobs {
            let outcome = self.compile_job(&job, compiler_version.clone()).await?;
            consumed += outcome.consumed;
            created += outcome.created;
            if outcome.consumed > 0 {
                trace_ref = Some(format!("memphant://trace/{}", job.job.id.as_uuid()));
            }
            self.store
                .complete_reflect_job(job.job.tenant_id, job.job.id)
                .await?;
        }
        Ok(ReflectResult {
            reflect_id: format!("rfl_{}", scope.as_uuid()),
            episodes_consumed: consumed,
            candidates_created: created,
            trace_ref,
        })
    }

    pub async fn correct(
        &self,
        tenant: TenantId,
        mut request: CorrectRequest,
    ) -> Result<CorrectResult, ServiceError> {
        request.tenant_id = tenant;
        Ok(correct_memory(
            self.store.as_ref(),
            request,
            self.embedder.as_ref(),
            self.clock.as_ref(),
        )
        .await?)
    }

    pub async fn forget(
        &self,
        tenant: TenantId,
        mut request: ForgetRequest,
    ) -> Result<ForgetResult, ServiceError> {
        request.tenant_id = tenant;
        Ok(forget_memory(self.store.as_ref(), request, self.clock.as_ref()).await?)
    }

    pub async fn mark(
        &self,
        tenant: TenantId,
        mut request: MarkRequest,
    ) -> Result<MarkResult, ServiceError> {
        request.tenant_id = tenant;
        Ok(record_mark(self.store.as_ref(), request).await?)
    }

    /// Tenant-bound trace fetch: a trace owned by another tenant is `None`.
    pub async fn trace(
        &self,
        tenant: TenantId,
        id: TraceId,
    ) -> Result<Option<RetrievalTrace>, ServiceError> {
        Ok(self.store.trace_by_id(tenant, id).await?)
    }

    pub async fn scope_memory_page(
        &self,
        tenant: TenantId,
        scope: ScopeId,
        cursor: Option<UnitId>,
        limit: usize,
    ) -> Result<ScopePage, ServiceError> {
        Ok(self
            .store
            .scope_memory_page(tenant, scope, cursor, limit)
            .await?)
    }

    /// One worker tick: claims up to `batch` reflect jobs (unfiltered across
    /// tenants) and compiles them. Panics are caught per job — a poisoned job
    /// stays claimed and is retried after the reclaim window until it
    /// dead-letters. Returns the number of jobs completed.
    pub async fn run_worker_tick(&self, batch: usize) -> Result<usize, ServiceError> {
        let jobs = self
            .store
            .claim_reflect_jobs(JobFilter::default(), batch)
            .await?;
        let mut completed = 0;
        for job in jobs {
            let result = CatchUnwind::new(self.compile_job(&job, None)).await;
            match result {
                Ok(Ok(_)) => {
                    self.store
                        .complete_reflect_job(job.job.tenant_id, job.job.id)
                        .await?;
                    completed += 1;
                }
                Ok(Err(error)) => {
                    eprintln!(
                        "memphant-worker: job {} failed (attempt {}): {error}",
                        job.job.id.as_uuid(),
                        job.attempts
                    );
                }
                Err(()) => {
                    eprintln!(
                        "memphant-worker: job {} panicked (attempt {})",
                        job.job.id.as_uuid(),
                        job.attempts
                    );
                }
            }
        }
        Ok(completed)
    }

    /// Compiles one claimed reflect job through `reflect_recorded` — the ONE
    /// compilation path shared by the public reflect verb and the worker.
    async fn compile_job(
        &self,
        job: &ReflectJobRow,
        compiler_override: Option<String>,
    ) -> Result<CompileOutcome, ServiceError> {
        let compiler_version =
            compiler_override.unwrap_or_else(|| job.job.compiler_version.clone());
        let (episode_id, resource_id, candidates): (_, _, Vec<ReflectCandidate>) =
            match job.job.kind {
                ReflectJobKind::ReflectEpisode => {
                    let Some(episode_id) = job.job.episode_id else {
                        return Ok(CompileOutcome::default());
                    };
                    let Some(episode) = self
                        .store
                        .fetch_episode(job.job.tenant_id, episode_id)
                        .await?
                    else {
                        // Episode gone (e.g. forgotten before compile): nothing to do.
                        return Ok(CompileOutcome::default());
                    };
                    // W5 temporal grounding: extract the episode's primary content
                    // date (deterministic, clock-free) once. Fallback order is
                    // parsed content date → episode first_observed_at → none; the
                    // store carries no first_observed_at column today, so the middle
                    // rung is NOT-YET-WIRED and we go straight to `None` when parsing
                    // fails (the bench corpus always carries the `[date ...]` prefix,
                    // so measurement is unaffected). Gated: off ⇒ no date at all.
                    let content_date = if self.temporal_grounding_enabled {
                        parse_content_date(&episode.body)
                    } else {
                        None
                    };
                    // `YYYY-MM-DD` for the chunk header slot; midnight-UTC RFC 3339
                    // for the grounded `valid_from`. Both derive from the SAME parsed
                    // date so the header and the window agree.
                    let content_date_header = content_date.map(|date| date.to_string());
                    let valid_from = content_date.map(|date| format!("{date}T00:00:00Z"));
                    // Rung 4: mint contextual chunks tied to this raw episode when
                    // the write path is enabled (default on since 2026-07-10).
                    // Every other candidate construction (resource jobs,
                    // direct-unit retains) stays chunk-free — episodes only.
                    let contextual_chunks = if self.contextual_chunks_write_enabled {
                        episode_contextual_chunks(
                            episode.id,
                            &episode.source_kind,
                            &episode.body,
                            content_date_header.as_deref(),
                        )
                    } else {
                        Vec::new()
                    };
                    // The raw episode candidate (unchanged), then — only when W6 fact
                    // extraction is on — the mined preference/attribute facts. The
                    // `[date ...]` body prefix couples to temporal grounding only:
                    // `content_date_header` is already `None` unless that flag is on.
                    let mut candidates = vec![ReflectCandidate {
                        source_kind: episode.source_kind.clone(),
                        trust_level: episode.source_trust,
                        actor_id: episode.actor_id,
                        subject: job.job.subject.clone(),
                        predicate: job.job.predicate.clone(),
                        kind: None,
                        body: episode.body.clone(),
                        churn_class: None,
                        admission_hint: None,
                        contextual_chunks,
                        valid_from,
                        valid_to: None,
                    }];
                    if self.fact_extraction_enabled {
                        candidates.extend(extract_fact_candidates(
                            &episode,
                            content_date_header.as_deref(),
                        ));
                    }
                    (Some(episode.id), None, candidates)
                }
                ReflectJobKind::ReflectResource => {
                    let Some(resource_id) = job.job.resource_id else {
                        return Ok(CompileOutcome::default());
                    };
                    let Some(resource) = self
                        .store
                        .fetch_resource(job.job.tenant_id, resource_id)
                        .await?
                    else {
                        return Ok(CompileOutcome::default());
                    };
                    let Some(body) = resource.body.clone().filter(|body| !body.trim().is_empty())
                    else {
                        // Pointer-only resource: nothing durable to compile yet.
                        return Ok(CompileOutcome::default());
                    };
                    // R1: rung-4 machinery extended to the docs domain. Mint
                    // per-resource contextual chunks for DOCUMENT resources when
                    // the (default-off) resource-chunk write path is enabled;
                    // non-document kinds and the disabled path stay chunk-free —
                    // byte-identical to today. Episodes get theirs in the
                    // ReflectEpisode arm above; this is the resource twin.
                    let contextual_chunks = if self.resource_chunks_write_enabled
                        && resource.kind == ResourceKind::Document
                    {
                        resource_contextual_chunks(resource.id, &resource.uri, &body)
                    } else {
                        Vec::new()
                    };
                    (
                        None,
                        Some(resource.id),
                        vec![ReflectCandidate {
                            source_kind: "resource".to_string(),
                            trust_level: resource.source_trust,
                            actor_id: resource.actor_id,
                            subject: None,
                            predicate: None,
                            kind: Some(MemoryKind::Resource),
                            body,
                            churn_class: None,
                            admission_hint: None,
                            contextual_chunks,
                            valid_from: None,
                            valid_to: None,
                        }],
                    )
                }
            };

        // Every candidate in a job shares the episode/resource actor; the raw
        // candidate is always first, so its actor drives the ReflectInput.
        let Some(actor_id) = candidates.first().map(|candidate| candidate.actor_id) else {
            return Ok(CompileOutcome::default());
        };

        let (trace, _) = reflect_recorded(
            self.store.as_ref(),
            ReflectInput {
                tenant_id: job.job.tenant_id,
                scope_id: job.job.scope_id,
                actor_id,
                episode_id,
                resource_id,
                job_id: job.job.id,
                compiler_version,
                candidates,
            },
            self.embedder.as_ref(),
            self.clock.as_ref(),
        )
        .await?;
        let created = trace
            .actions
            .iter()
            .filter(|action| {
                matches!(
                    action,
                    memphant_types::AdmissionAction::Append
                        | memphant_types::AdmissionAction::Supersede
                        | memphant_types::AdmissionAction::Quarantine
                        | memphant_types::AdmissionAction::Invalidate
                )
            })
            .count();
        Ok(CompileOutcome {
            consumed: 1,
            created,
        })
    }
}

#[derive(Debug, Default, Clone, Copy)]
struct CompileOutcome {
    consumed: usize,
    created: usize,
}

/// Turns (or fallback segments) per contextual-chunk window. This is the
/// turn-window granularity promoted on real evidence (LME-S n=100, 2026-07-10
/// scaled-reader campaign: ≤4-turn episodes lifted ΔR@5/ΔR@10/ΔQA with CIs
/// excluding zero). The runtime write path is the same granularity as an
/// extraction-side embodiment rather than client-side windowing.
const CONTEXTUAL_CHUNK_WINDOW: usize = 4;

/// Per-episode chunk cap — the rung 4 bloat guard (disable-when: chunk fan-out
/// hurts recall latency/cost once it stops adding coverage). W9: the window
/// (see `adaptive_chunk_window`) grows past `CONTEXTUAL_CHUNK_WINDOW` for
/// bodies long enough to otherwise mint more than this many windows, so the
/// cap bounds fan-out WITHOUT truncating the body — bodies of ≤128 segments
/// (`MAX_CONTEXTUAL_CHUNKS * CONTEXTUAL_CHUNK_WINDOW`) never trigger growth
/// and chunk exactly as before this change.
const MAX_CONTEXTUAL_CHUNKS: usize = 32;

/// W9: the window size that keeps `MAX_CONTEXTUAL_CHUNKS` windows ALWAYS
/// covering the full body, instead of the fixed `CONTEXTUAL_CHUNK_WINDOW`
/// silently truncating the tail once a body outgrows
/// `MAX_CONTEXTUAL_CHUNKS * CONTEXTUAL_CHUNK_WINDOW` segments (128 today).
/// Grows only when needed: `ceil(segment_count / MAX_CONTEXTUAL_CHUNKS)` is
/// the smallest window that fits the whole body in the cap, and taking the
/// max with `CONTEXTUAL_CHUNK_WINDOW` means short bodies are completely
/// unaffected (byte-identical to pre-W9 behavior for ≤128 segments).
fn adaptive_chunk_window(segment_count: usize) -> usize {
    CONTEXTUAL_CHUNK_WINDOW.max(segment_count.div_ceil(MAX_CONTEXTUAL_CHUNKS))
}

/// Parses a body line's `role: content` turn shape: a short leading role token,
/// then `": "`, then non-empty content. Returns `(role, content)` or `None` for
/// non-turn lines. The bench lane's per-session episodes ingest in exactly this
/// form; a bracketed provenance line like `[session s1] [date ...]` has no `": "`
/// and parses as `None`. Shared by the chunk segmenter and the W6 fact miner so
/// there is ONE turn parser, not two.
fn parse_turn(line: &str) -> Option<(&str, &str)> {
    let (role, content) = line.trim().split_once(": ")?;
    let ok = !role.is_empty()
        && role.len() <= 32
        && !content.trim().is_empty()
        && role
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, ' ' | '_' | '-'));
    ok.then_some((role, content))
}

/// A body line reads as a conversational turn when it parses as `role: content`.
fn line_is_turn(line: &str) -> bool {
    parse_turn(line).is_some()
}

/// Byte spans of the segments to window over, plus whether the body parsed as
/// turns. Turn-structured bodies window over their `role: content` lines;
/// everything else falls back to non-empty line segments.
fn segment_episode_body(body: &str) -> (Vec<(usize, usize)>, bool) {
    let mut lines: Vec<(usize, usize, bool)> = Vec::new();
    let mut offset = 0usize;
    for raw in body.split_inclusive('\n') {
        let start = offset;
        offset += raw.len();
        let content = raw.trim_end_matches(['\n', '\r']);
        if content.trim().is_empty() {
            continue;
        }
        lines.push((start, start + content.len(), line_is_turn(content)));
    }
    let turn_count = lines.iter().filter(|(_, _, is_turn)| *is_turn).count();
    // Turn-structured when turns are present and dominate (a stray `": "` in a
    // prose body never flips it).
    let turn_structured = turn_count >= 2 && turn_count * 2 >= lines.len();
    let spans = lines
        .into_iter()
        .filter(|(_, _, is_turn)| !turn_structured || *is_turn)
        .map(|(start, end, _)| (start, end))
        .collect();
    (spans, turn_structured)
}

/// Splits `body` into up to `MAX_CONTEXTUAL_CHUNKS` windows and mints one
/// `ContextualChunk` per window, each tied back to its parent episode. The
/// window is `CONTEXTUAL_CHUNK_WINDOW` turns/segments for bodies that fit
/// within the cap at that size, and grows (`adaptive_chunk_window`)
/// otherwise so the windows ALWAYS cover the full body — no silently dropped
/// tail (W9). Emits nothing when the body fits a single window (a lone chunk
/// would just duplicate the unit body) and never emits empty-body chunks —
/// the rung 4 bloat guards.
fn episode_contextual_chunks(
    episode_id: EpisodeId,
    source_kind: &str,
    body: &str,
    content_date: Option<&str>,
) -> Vec<ContextualChunk> {
    let (spans, turn_structured) = segment_episode_body(body);
    if spans.len() <= CONTEXTUAL_CHUNK_WINDOW {
        return Vec::new();
    }
    let window_size = adaptive_chunk_window(spans.len());
    let span_label = if turn_structured { "turns" } else { "segments" };
    // W5: reinstates the header date slot with the TRUE parsed content date
    // (never the compile clock). `None` ⇒ the header stays dateless, exactly as
    // before this change.
    let date_slot = content_date
        .map(|date| format!(" [date {date}]"))
        .unwrap_or_default();
    spans
        .chunks(window_size)
        // W9: `window_size` already guarantees ≤`MAX_CONTEXTUAL_CHUNKS`
        // windows for the whole body — this `take` is a defensive backstop,
        // not the active truncation it used to be.
        .take(MAX_CONTEXTUAL_CHUNKS)
        .enumerate()
        .filter_map(|(window_index, window)| {
            let start = window.first()?.0;
            let end = window.last()?.1;
            let text = body.get(start..end)?.trim();
            if text.is_empty() {
                return None;
            }
            let first = window_index * window_size + 1;
            let last = window_index * window_size + window.len();
            Some(ContextualChunk {
                id: format!("chunk-{}-{window_index}", episode_id.as_uuid()),
                header: format!(
                    "[episode {}] [kind {source_kind}]{date_slot} [{span_label} {first}-{last}]",
                    episode_id.as_uuid()
                ),
                body: text.to_string(),
                // Byte offsets (matching the `body.get(start..end)` slice
                // above) — NOT char counts, so `body[start..end]` reproduces
                // the chunk body directly even over multi-byte text.
                source_span: Some(format!("{start}-{end}")),
            })
        })
        .collect()
}

// ===========================================================================
// R1 docs-domain contextual chunks: the resource twin of the episode chunker
// above. `episode_contextual_chunks` windows a conversation over its TURNS; the
// gate ingests docs as `kind=document` resources (one markdown section each,
// 240–3200 chars, first line usually a `#`-heading, fenced code blocks common),
// so this variant windows a resource body over its PARAGRAPHS under a char
// budget. Everything that recall packing + citation touch — the `ContextualChunk`
// fields, the `chunk-{parent}-{index}` id shape, byte-offset `source_span`, and
// the "emit nothing for a single window" bloat guard — is copied verbatim from
// the episode chunker so resource and episode chunks flow through the read path
// identically. The parent whole-section unit stays stored verbatim; chunks are
// additive retrieval keys + pack content.
//
// DEVIATION FROM BRIEF (recorded in the task report): the brief asked for a
// 1-paragraph window overlap, but the PROMOTED episode twin partitions
// NON-overlapping (`spans.chunks(window_size)`). Per the mirror-the-twin rule
// (consistency with the promoted machinery wins) these windows are
// non-overlapping too: disjoint `source_span`s and the same ±1 sibling-adjacency
// semantics the read path's sibling-gather assumes.
// ===========================================================================

/// Resource-chunk char budget. Windows aim for `TARGET_MIN..=TARGET_MAX` chars of
/// markdown, may stretch to `HARD_MAX` to avoid splitting a paragraph, and never
/// split a paragraph/fenced block (an oversized single paragraph becomes its own
/// window). Char budgets (not the episode chunker's fixed turn count) because doc
/// paragraphs vary far more in length than conversational turns.
const RESOURCE_CHUNK_TARGET_MIN_CHARS: usize = 700;
const RESOURCE_CHUNK_TARGET_MAX_CHARS: usize = 1100;
const RESOURCE_CHUNK_HARD_MAX_CHARS: usize = 1600;

/// Byte spans of `body`'s paragraphs, split on blank-line boundaries but NEVER
/// inside a fenced code block (```` ``` ````/`~~~`): a blank line inside an open
/// fence stays part of the current paragraph. Mirrors `segment_episode_body`'s
/// offset bookkeeping (byte offsets via `split_inclusive('\n')`; a span's content
/// bounds exclude the trailing newline) so the downstream span math is identical.
fn segment_resource_paragraphs(body: &str) -> Vec<(usize, usize)> {
    let mut paras: Vec<(usize, usize)> = Vec::new();
    let mut current: Option<(usize, usize)> = None;
    let mut in_fence = false;
    let mut offset = 0usize;
    for raw in body.split_inclusive('\n') {
        let line_start = offset;
        offset += raw.len();
        let content = raw.trim_end_matches(['\n', '\r']);
        let is_fence = {
            let trimmed = content.trim_start();
            trimmed.starts_with("```") || trimmed.starts_with("~~~")
        };
        // A blank line OUTSIDE a fence closes the current paragraph. Inside an
        // open fence a blank line is ordinary fence content (never a boundary).
        if content.trim().is_empty() && !in_fence {
            if let Some(span) = current.take() {
                paras.push(span);
            }
            continue;
        }
        let content_end = line_start + content.len();
        match current.as_mut() {
            Some(span) => span.1 = content_end,
            None => current = Some((line_start, content_end)),
        }
        if is_fence {
            in_fence = !in_fence;
        }
    }
    if let Some(span) = current.take() {
        paras.push(span);
    }
    paras
}

/// Groups paragraph spans into non-overlapping char-budget windows (inclusive
/// `(first_para, last_para)` index pairs). Greedily grows a window until adding
/// the next paragraph would exceed `TARGET_MAX` (stretching only while the window
/// is still under `TARGET_MIN`, and never past `HARD_MAX`), then a tiny trailing
/// window is merged back into its predecessor when the merge fits `HARD_MAX`.
fn window_resource_paragraphs(body: &str, paras: &[(usize, usize)]) -> Vec<(usize, usize)> {
    let char_len = |start: usize, end: usize| {
        body.get(start..end)
            .map_or(0, |slice| slice.chars().count())
    };
    let mut windows: Vec<(usize, usize)> = Vec::new();
    let mut i = 0usize;
    while i < paras.len() {
        let start_byte = paras[i].0;
        let mut end_idx = i;
        while end_idx + 1 < paras.len() {
            let current = char_len(start_byte, paras[end_idx].1);
            if current >= RESOURCE_CHUNK_TARGET_MAX_CHARS {
                break;
            }
            let with_next = char_len(start_byte, paras[end_idx + 1].1);
            if with_next > RESOURCE_CHUNK_HARD_MAX_CHARS {
                break;
            }
            // Add the next paragraph when it keeps us within target, or when the
            // window is still below the minimum (merge-small, bounded by HARD_MAX
            // above).
            if with_next <= RESOURCE_CHUNK_TARGET_MAX_CHARS
                || current < RESOURCE_CHUNK_TARGET_MIN_CHARS
            {
                end_idx += 1;
            } else {
                break;
            }
        }
        windows.push((i, end_idx));
        i = end_idx + 1;
    }
    // Merge a tiny trailing window into its predecessor (brief: "merge tiny
    // trailing paragraphs") when the merged span stays within the hard cap.
    if windows.len() >= 2 {
        let last = windows[windows.len() - 1];
        let prev = windows[windows.len() - 2];
        let last_chars = char_len(paras[last.0].0, paras[last.1].1);
        let merged_chars = char_len(paras[prev.0].0, paras[last.1].1);
        if last_chars < RESOURCE_CHUNK_TARGET_MIN_CHARS
            && merged_chars <= RESOURCE_CHUNK_HARD_MAX_CHARS
        {
            let n = windows.len();
            windows[n - 2] = (prev.0, last.1);
            windows.pop();
        }
    }
    windows
}

/// The chunk provenance header for a document resource: the section's own first
/// markdown heading line (the gate ingests each section starting with its `###`
/// heading), falling back to the uri stem when there is no heading. The
/// docs-domain analog of the episode chunk's `[session ...]` context header — it
/// gives every retrieval-key chunk its section identity.
fn resource_chunk_header(body: &str, uri: &str) -> String {
    if let Some(heading) = body
        .lines()
        .map(str::trim)
        .find(|line| line.starts_with('#'))
    {
        return heading.to_string();
    }
    let stem = uri.rsplit(['/', '\\']).next().unwrap_or(uri);
    let stem = stem.split(['?', '#']).next().unwrap_or(stem);
    let stem = stem.rsplit_once('.').map_or(stem, |(base, _)| base);
    if stem.is_empty() {
        "resource".to_string()
    } else {
        stem.to_string()
    }
}

/// Splits a `kind=document` resource `body` into non-overlapping char-budget
/// windows and mints one `ContextualChunk` per window, each tied back to its
/// parent resource — the docs-domain twin of `episode_contextual_chunks`. Emits
/// nothing when the body fits a single window (a lone chunk would just duplicate
/// the unit body) and never emits empty-body chunks. The `take(MAX_CONTEXTUAL_
/// CHUNKS)` is a defensive backstop shared with the episode chunker: real corpus
/// sections (≤~3.2k chars) produce 1–4 windows, far under the cap.
fn resource_contextual_chunks(
    resource_id: ResourceId,
    uri: &str,
    body: &str,
) -> Vec<ContextualChunk> {
    let paras = segment_resource_paragraphs(body);
    let windows = window_resource_paragraphs(body, &paras);
    if windows.len() <= 1 {
        return Vec::new();
    }
    let header = resource_chunk_header(body, uri);
    windows
        .into_iter()
        .take(MAX_CONTEXTUAL_CHUNKS)
        .enumerate()
        .filter_map(|(window_index, (first_para, last_para))| {
            let start = paras[first_para].0;
            let end = paras[last_para].1;
            let text = body.get(start..end)?.trim();
            if text.is_empty() {
                return None;
            }
            Some(ContextualChunk {
                id: format!("chunk-{}-{window_index}", resource_id.as_uuid()),
                header: header.clone(),
                body: text.to_string(),
                // Byte offsets (matching the `body.get(start..end)` slice) — NOT
                // char counts — so `body[start..end]` reproduces the chunk body
                // verbatim, the provenance span-grading invariant (identical to
                // the episode chunk spans).
                source_span: Some(format!("{start}-{end}")),
            })
        })
        .collect()
}

// ===========================================================================
// W6 deterministic fact extraction (preference/attribute mining at reflect).
//
// v1 is a hand-rolled, clock-free, LLM-free pattern miner (canonical plan: NO
// LLM in the write path). It scans an episode's USER turns for first-person
// preference/attribute statements and emits SHORT, embeddable ReflectCandidates
// with HONEST subject keys — the lever the single-session-preference stratum
// needs, and the fix for the opaque-content-hash keys that starve supersedence.
//
// PRECISION over recall (§6): a noisy fact index poisons packs, so every rule is
// conservative — demonstrative/pronoun objects and conversational meta nouns
// ("my point is", "I like that idea") are dropped, and the ambiguous "I'm a
// <desc>" family is keyed by its own description (each a standalone fact that
// only an exact repeat or an explicit negation supersedes) rather than guessing
// a shared occupation/identity slot. The cost is missed updates on that family;
// the win is never wrongly superseding "I'm a teacher" with "I'm a vegetarian".
//
// Patterns are hand-rolled rather than regex: core carries no regex dependency
// and the v1 shapes (fixed trigger phrases + token scans) stay readable without
// one (KISS). An LLM extractor is a later experiment behind this same seam.
// ===========================================================================

/// Per-episode hard cap on extracted facts (§3 bloat guard). An episode dense
/// with first-person statements keeps only the most recent `MAX_EXTRACTED_FACTS`
/// — a noisy fact index costs more recall than it earns.
const MAX_EXTRACTED_FACTS: usize = 8;

/// Minimum word count for a mineable sentence (§3). Sub-4-word fragments
/// ("I love it", "my bad") are almost always conversational noise, not durable
/// facts; counted on the raw sentence, before any date prefix.
const MIN_FACT_SENTENCE_WORDS: usize = 4;

/// A deterministic preference/attribute fact mined from one episode body. The
/// `family`/`subject_phrase` pair becomes the honest subject key
/// (`{scope}:{family}:{subject_phrase}`, via `derive_subject_key`) so the SAME
/// subject in a later episode supersedes; `body` is the verbatim source sentence
/// (optionally `[date ...]`-prefixed).
#[derive(Debug, Clone, PartialEq, Eq)]
struct ExtractedFact {
    family: &'static str,
    subject_phrase: String,
    body: String,
}

/// Pronoun / demonstrative objects that read as conversational filler, never a
/// durable subject ("I love it", "my favorite is that", "I like these").
const PRONOUN_STOPS: &[&str] = &[
    "it",
    "that",
    "this",
    "these",
    "those",
    "them",
    "they",
    "he",
    "she",
    "him",
    "her",
    "you",
    "us",
    "me",
    "myself",
    "one",
    "ones",
    "everything",
    "anything",
    "something",
    "nothing",
    "everyone",
    "anyone",
    "someone",
    "itself",
    "here",
    "there",
    "who",
    "what",
    "which",
];

/// Meta / conversational nouns for the "my <noun> is <value>" rule — these are
/// discourse moves, not attributes ("my point is", "my question is").
const NOUN_STOPS: &[&str] = &[
    "point",
    "question",
    "guess",
    "concern",
    "answer",
    "goal",
    "issue",
    "problem",
    "idea",
    "opinion",
    "view",
    "take",
    "understanding",
    "assumption",
    "mistake",
    "bad",
    "apologies",
    "sense",
    "hope",
    "plan",
    "thought",
    "thoughts",
    "feeling",
    "feelings",
    "response",
    "reply",
    "suggestion",
    "recommendation",
    "advice",
    "impression",
    "worry",
    "fear",
];

/// Description heads that follow "I'm a/an" but signal filler, not identity
/// ("I'm a bit tired", "I'm a huge fan", "I'm a little confused").
const IDENTITY_STOPS: &[&str] = &[
    "bit", "little", "tad", "lot", "fan", "huge", "big", "couple", "few", "sort", "kind", "loyal",
];

/// Tokens skipped between the first-person "I" and a preference verb (negation
/// and hedges) so "I don't really like X" and "I like X" find the same verb.
const PREF_FILLERS: &[&str] = &[
    "do",
    "don't",
    "dont",
    "did",
    "didn't",
    "not",
    "never",
    "really",
    "no",
    "longer",
    "just",
    "also",
    "still",
    "actually",
    "totally",
    "absolutely",
    "genuinely",
    "truly",
    "simply",
    "so",
    "much",
    "always",
    "generally",
    "usually",
    "kinda",
    "sort",
    "of",
];

/// Single-word preference verbs. Polarity (love vs hate) lives in the body, not
/// the key, so a reversal supersedes/contradicts the prior fact.
const PREF_VERBS: &[&str] = &[
    "love",
    "loved",
    "like",
    "liked",
    "prefer",
    "preferred",
    "enjoy",
    "enjoyed",
    "hate",
    "hated",
    "dislike",
    "disliked",
    "adore",
    "adored",
    "fancy",
];

/// Trailing temporal/update words stripped from an object phrase so the key is
/// stable across "I like coffee", "I like coffee now", "...coffee anymore".
const OBJECT_TRAILERS: &[&str] = &[
    "anymore",
    "now",
    "today",
    "currently",
    "lately",
    "recently",
    "nowadays",
    "days",
    "these",
    "any",
    "more",
    "longer",
    "too",
    "though",
    "either",
    "here",
];

/// Clause-boundary words: an object/desc/noun phrase ends before the first of
/// these so "I love hiking but hate crowds" keys on "hiking", not the whole tail.
const CLAUSE_STOPS: &[&str] = &[
    "and", "but", "because", "so", "although", "though", "however", "yet", "while", "whereas",
];

/// Strips leading/trailing punctuation from a token, keeping inner apostrophes
/// (so "don't" / "can't" survive intact for verb matching).
fn strip_punct(token: &str) -> &str {
    token.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '\'')
}

/// Splits a turn's content into sentences on terminal `.`/`!`/`?`. A deterministic
/// extension of the line/turn splitting — NOT a second word tokenizer. All three
/// delimiters are single-byte ASCII, so the byte slices land on char boundaries
/// even over multi-byte content.
fn split_sentences(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0usize;
    for (i, byte) in text.bytes().enumerate() {
        if matches!(byte, b'.' | b'!' | b'?') {
            let sentence = text[start..i].trim();
            if !sentence.is_empty() {
                out.push(sentence);
            }
            start = i + 1;
        }
    }
    let tail = text[start..].trim();
    if !tail.is_empty() {
        out.push(tail);
    }
    out
}

/// Trims a token slice at the first clause boundary, returning the leading
/// tokens (may be empty).
fn clause_trim<'a>(tokens: &[&'a str]) -> Vec<&'a str> {
    let mut out = Vec::new();
    for token in tokens {
        let word = strip_punct(token);
        if CLAUSE_STOPS.contains(&word) {
            break;
        }
        out.push(*token);
        // A trailing comma/semicolon ends the clause after including this word.
        if token.ends_with([',', ';', ':']) {
            break;
        }
    }
    out
}

/// Cleans a preference object phrase into a stable normalized key half, or
/// `None` when it is empty or a bare pronoun/demonstrative (§6).
fn clean_object(tokens: &[&str]) -> Option<String> {
    let mut words: Vec<&str> = clause_trim(tokens)
        .iter()
        .map(|token| strip_punct(token))
        .filter(|word| !word.is_empty())
        .collect();
    // Drop a single leading article so "I love the ocean" / "I hate the ocean"
    // share `preference:ocean`.
    if matches!(words.first().copied(), Some("a" | "an" | "the")) {
        words.remove(0);
    }
    // Drop trailing temporal/update words.
    while matches!(words.last().copied(), Some(word) if OBJECT_TRAILERS.contains(&word)) {
        words.pop();
    }
    let first = *words.first()?;
    if PRONOUN_STOPS.contains(&first) {
        return None;
    }
    let phrase = normalize_component(&words.join(" "));
    (!phrase.is_empty()).then_some(phrase)
}

/// Normalizes a noun/desc phrase (subject-side), rejecting empties and bare
/// pronouns; unlike `clean_object` it keeps leading articles out by caller
/// choice and never strips trailers (subjects are not temporal).
fn clean_subject_phrase(tokens: &[&str]) -> Option<String> {
    let words: Vec<&str> = clause_trim(tokens)
        .iter()
        .map(|token| strip_punct(token))
        .filter(|word| !word.is_empty())
        .collect();
    let first = *words.first()?;
    if PRONOUN_STOPS.contains(&first) {
        return None;
    }
    let phrase = normalize_component(&words.join(" "));
    (!phrase.is_empty()).then_some(phrase)
}

/// Matches ONE fact in a single sentence, first rule wins (superlative before
/// the generic "my <noun> is" so "favorite" lands in the preference namespace).
/// `None` when the sentence is too short or matches no v1 pattern.
fn match_fact(sentence: &str) -> Option<ExtractedFact> {
    if sentence.split_whitespace().count() < MIN_FACT_SENTENCE_WORDS {
        return None;
    }
    let lower = sentence.to_ascii_lowercase();
    let tokens: Vec<&str> = lower.split_whitespace().collect();
    let (family, subject_phrase) = match_superlative(&tokens)
        .or_else(|| match_my_new_noun(&tokens))
        .or_else(|| match_my_noun_is(&tokens))
        .or_else(|| match_identity(&tokens))
        .or_else(|| match_preference_verb(&tokens))?;
    Some(ExtractedFact {
        family,
        subject_phrase,
        body: sentence.to_string(),
    })
}

/// "my [all-time] favorite <noun> is <value>" → `preference:favorite <noun>`
/// (the value is deliberately OUT of the key so a later value supersedes).
fn match_superlative(tokens: &[&str]) -> Option<(&'static str, String)> {
    let fav = tokens
        .iter()
        .position(|token| strip_punct(token) == "favorite")?;
    // The word(s) before "favorite" must root it in "my [all-time] favorite".
    let before =
        |offset: usize| -> Option<&str> { fav.checked_sub(offset).map(|i| strip_punct(tokens[i])) };
    let rooted = before(1) == Some("my")
        || (before(1) == Some("all-time") && before(2) == Some("my"))
        || (before(1) == Some("time") && before(2) == Some("all") && before(3) == Some("my"));
    if !rooted {
        return None;
    }
    let is_at = tokens
        .iter()
        .enumerate()
        .skip(fav + 1)
        .find(|(_, token)| strip_punct(token) == "is")
        .map(|(index, _)| index)?;
    let noun = clean_subject_phrase(&tokens[fav + 1..is_at])?;
    // A value must follow, and it must not be a bare pronoun.
    let _value = clean_subject_phrase(&tokens[is_at + 1..])?;
    Some(("preference", format!("favorite {noun}")))
}

/// "my new <noun> is <value>" → `attribute:<noun>` (shares the key of the plain
/// "my <noun> is <value>" so an explicit update supersedes).
fn match_my_new_noun(tokens: &[&str]) -> Option<(&'static str, String)> {
    let my = tokens.iter().position(|token| strip_punct(token) == "my")?;
    if strip_punct(tokens.get(my + 1)?) != "new" {
        return None;
    }
    let is_at = tokens
        .iter()
        .enumerate()
        .skip(my + 2)
        .find(|(_, token)| strip_punct(token) == "is")
        .map(|(index, _)| index)?;
    let noun = clean_subject_phrase(&tokens[my + 2..is_at])?;
    let _value = clean_subject_phrase(&tokens[is_at + 1..])?;
    (!NOUN_STOPS.contains(&noun.as_str())).then_some(("attribute", noun))
}

/// "my <noun> is <value>" → `attribute:<noun>`, dropping meta/discourse nouns.
fn match_my_noun_is(tokens: &[&str]) -> Option<(&'static str, String)> {
    let my = tokens.iter().position(|token| strip_punct(token) == "my")?;
    let is_at = tokens
        .iter()
        .enumerate()
        .skip(my + 1)
        .find(|(_, token)| strip_punct(token) == "is")
        .map(|(index, _)| index)?;
    let noun_tokens = &tokens[my + 1..is_at];
    // Keep the noun tight (1..=3 words) so "my <clause> is" prose doesn't key.
    if noun_tokens.is_empty() || noun_tokens.len() > 3 {
        return None;
    }
    let noun = clean_subject_phrase(noun_tokens)?;
    let _value = clean_subject_phrase(&tokens[is_at + 1..])?;
    // Reject any meta noun (checked per word so "only point" is caught too).
    if noun.split(' ').any(|word| NOUN_STOPS.contains(&word)) {
        return None;
    }
    Some(("attribute", noun))
}

/// "I am a/an <desc>" / "I'm a/an <desc>" (incl. negated "not a/an") →
/// `attribute:<desc>`. Keyed by the description itself: two different identities
/// coexist, and only an exact repeat or an explicit negation supersedes.
fn match_identity(tokens: &[&str]) -> Option<(&'static str, String)> {
    // Locate the "I'm" / "I am" anchor and the token index just after it.
    let mut after_pronoun = None;
    for (i, token) in tokens.iter().enumerate() {
        match strip_punct(token) {
            "i'm" => {
                after_pronoun = Some(i + 1);
                break;
            }
            "i" if strip_punct(tokens.get(i + 1).copied().unwrap_or("")) == "am" => {
                after_pronoun = Some(i + 2);
                break;
            }
            _ => {}
        }
    }
    let mut idx = after_pronoun?;
    // Skip a negation/hedge ("not", "no longer", "really").
    while matches!(
        strip_punct(tokens.get(idx).copied().unwrap_or("")),
        "not" | "no" | "longer" | "really" | "actually" | "also" | "still"
    ) {
        idx += 1;
    }
    // Require an article: "a"/"an" — this filters bare "I am happy" adjectives.
    if !matches!(
        strip_punct(tokens.get(idx).copied().unwrap_or("")),
        "a" | "an"
    ) {
        return None;
    }
    let desc_tokens = &tokens[idx + 1..];
    if desc_tokens.is_empty() {
        return None;
    }
    let desc = clean_subject_phrase(desc_tokens)?;
    let head = desc.split(' ').next().unwrap_or("");
    if IDENTITY_STOPS.contains(&head) {
        return None;
    }
    // Keep the description tight (1..=4 words).
    if desc.split(' ').count() > 4 {
        return None;
    }
    Some(("attribute", desc))
}

/// Preference verbs (incl. multi-word "can't stand", "switched to") → the
/// `preference:<object>` key, with negation/hedges skipped between "I" and the
/// verb so update phrasings share the positive assertion's key.
fn match_preference_verb(tokens: &[&str]) -> Option<(&'static str, String)> {
    for (i, token) in tokens.iter().enumerate() {
        if strip_punct(token) != "i" {
            continue;
        }
        let mut j = i + 1;
        while j < tokens.len() && PREF_FILLERS.contains(&strip_punct(tokens[j])) {
            j += 1;
        }
        if j >= tokens.len() {
            continue;
        }
        let verb = strip_punct(tokens[j]);
        let next = tokens.get(j + 1).map(|token| strip_punct(token));
        // Multi-word verbs ("can't stand", "switched to") consume two tokens.
        let verb_end = if (matches!(verb, "can't" | "cannot" | "can") && next == Some("stand"))
            || (verb == "switched" && next == Some("to"))
        {
            j + 2
        } else if PREF_VERBS.contains(&verb) {
            j + 1
        } else {
            // This "I" did not front a preference verb (e.g. "When I travel I
            // love X"); try the next "I" rather than giving up on the sentence.
            continue;
        };
        // A verb was found: its object decides the fact. A bad (pronoun) object
        // rejects the sentence outright (precision) — we do not hunt further.
        return clean_object(&tokens[verb_end..]).map(|object| ("preference", object));
    }
    None
}

/// Mines an episode body for W6 facts: scans USER turns (and non-role prose)
/// sentence-by-sentence, dedups by subject keeping the LAST occurrence (later
/// turns win), caps at `MAX_EXTRACTED_FACTS` keeping the most recent, and bakes
/// the `[date ...]` prefix into each body when `content_date` is supplied.
fn extract_facts(body: &str, content_date: Option<&str>) -> Vec<ExtractedFact> {
    let mut found: Vec<ExtractedFact> = Vec::new();
    for line in body.split_inclusive('\n') {
        let content = line.trim();
        if content.is_empty() {
            continue;
        }
        // §3: only user turns are mined. A role-prefixed line with any role
        // other than "user" (assistant/system/tool) is skipped; a non-role prose
        // line is treated as user-authored text.
        let text = match parse_turn(content) {
            Some((role, turn)) => {
                if normalize_component(role) != "user" {
                    continue;
                }
                turn
            }
            None => content,
        };
        for sentence in split_sentences(text) {
            if let Some(fact) = match_fact(sentence) {
                found.push(fact);
            }
        }
    }

    // Within-episode dedup by subject, keeping the LAST occurrence and its
    // position (later turns win), then cap to the most recent.
    let mut deduped: Vec<ExtractedFact> = Vec::new();
    for fact in found {
        if let Some(pos) = deduped.iter().position(|kept| {
            kept.family == fact.family && kept.subject_phrase == fact.subject_phrase
        }) {
            deduped.remove(pos);
        }
        deduped.push(fact);
    }
    if deduped.len() > MAX_EXTRACTED_FACTS {
        deduped.drain(0..deduped.len() - MAX_EXTRACTED_FACTS);
    }

    if let Some(date) = content_date {
        for fact in &mut deduped {
            fact.body = format!("[date {date}]\n{}", fact.body);
        }
    }
    deduped
}

/// Turns mined facts into extra ReflectCandidates for one episode: honest
/// `subject`/`predicate` (→ the `{scope}:{family}:{phrase}` key), the parent
/// episode's trust/actor so citations and admission are unchanged, NO contextual
/// chunks (§3), and NO `valid_from` (the date is baked into the body prefix
/// instead, so recall's dated-pack pass never double-prefixes).
fn extract_fact_candidates(
    episode: &StoredEpisode,
    content_date: Option<&str>,
) -> Vec<ReflectCandidate> {
    extract_facts(&episode.body, content_date)
        .into_iter()
        .map(|fact| ReflectCandidate {
            source_kind: episode.source_kind.clone(),
            trust_level: episode.source_trust,
            actor_id: episode.actor_id,
            subject: Some(fact.family.to_string()),
            predicate: Some(fact.subject_phrase),
            kind: None,
            body: fact.body,
            churn_class: None,
            admission_hint: None,
            contextual_chunks: Vec::new(),
            valid_from: None,
            valid_to: None,
        })
        .collect()
}

/// Degraded read-your-own-writes items: raw episode bodies lexically matched
/// against the query, cited back to their episode.
fn degraded_episode_items(
    episodes: &[StoredEpisode],
    query: &str,
    k: usize,
) -> Vec<RecallContextItem> {
    let query_tokens = tokenize(query);
    let mut scored: Vec<(&StoredEpisode, f32)> = episodes
        .iter()
        .filter_map(|episode| {
            let score = crate::token_set_overlap_text_score(&episode.body, &query_tokens);
            (score > 0.0).then_some((episode, score))
        })
        .collect();
    scored.sort_by(|left, right| {
        right
            .1
            .partial_cmp(&left.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.0.body.cmp(&right.0.body))
            // Total-order tie-break: `dedup_key` is unique per (tenant, scope),
            // so same-body/same-score episodes still cite deterministically.
            .then_with(|| left.0.dedup_key.cmp(&right.0.dedup_key))
    });
    scored
        .into_iter()
        .take(k)
        .map(|(episode, _)| RecallContextItem {
            unit_id: unit_id_for_episode(episode.id),
            body: episode.body.clone(),
            kind: MemoryKind::Episodic,
            derived_by: "raw_episode".to_string(),
            inclusion_reason: "degraded_read_your_own_writes".to_string(),
            citation_episode_id: Some(episode.id),
            citation_resource_id: None,
            suppression_labels: Vec::new(),
        })
        .collect()
}

/// Deterministic synthetic unit id for a degraded raw-episode item (there is
/// no compiled unit yet; the id mirrors the episode identity).
fn unit_id_for_episode(episode_id: EpisodeId) -> UnitId {
    UnitId::from_u128(episode_id.as_uuid().as_u128())
}

/// A minimal `catch_unwind` future adapter (std-only; core has no futures
/// dependency). Job compilation panics must not take down the worker loop.
struct CatchUnwind<F> {
    inner: F,
}

impl<F> CatchUnwind<F> {
    fn new(inner: F) -> Self {
        Self { inner }
    }
}

impl<F: Future> Future for CatchUnwind<F> {
    type Output = Result<F::Output, ()>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // SAFETY: structural pin projection to the only field; `inner` is
        // never moved out of the pinned wrapper.
        let inner = unsafe { self.map_unchecked_mut(|this| &mut this.inner) };
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| inner.poll(cx))) {
            Ok(Poll::Ready(output)) => Poll::Ready(Ok(output)),
            Ok(Poll::Pending) => Poll::Pending,
            Err(_) => Poll::Ready(Err(())),
        }
    }
}

#[cfg(test)]
mod chunk_tests {
    use super::*;

    /// Builds a turn-structured body of `turn_count` `user: ...` turns, for
    /// the W9 window-growth tests below (a leading provenance line, exactly
    /// like every other body in this module, so it exercises the same
    /// turn-structured segmentation path).
    fn turn_body(turn_count: usize) -> String {
        let mut body = String::from("[session s1] [date 2023/05/30]\n");
        for turn in 0..turn_count {
            body.push_str(&format!("user: turn number {turn} here.\n"));
        }
        body
    }

    /// Extracts the `a-b` range out of a chunk header's trailing
    /// `[label a-b]` slot (`label` is `"turns"` or `"segments"`) — no regex
    /// dependency, matching the hand-rolled-parsing rationale used elsewhere
    /// in this module.
    fn parse_span_range(header: &str, label: &str) -> (usize, usize) {
        let marker = format!("[{label} ");
        let start = header.rfind(&marker).expect("span slot present") + marker.len();
        let rest = &header[start..];
        let end = rest.find(']').expect("span slot closes");
        let (first, last) = rest[..end].split_once('-').expect("span is a-b");
        (
            first.parse().expect("first is numeric"),
            last.parse().expect("last is numeric"),
        )
    }

    /// `role: content` bodies window over turns with a `[turns a-b]` header
    /// and exact byte-offset spans over the parent body.
    #[test]
    fn turn_structured_body_windows_over_turns() {
        let episode_id = EpisodeId::new();
        let body = "[session s1] [date 2023/05/30]\n\
user: a b c.\n\
assistant: d e f.\n\
user: g h i.\n\
assistant: j k l.\n\
user: m n o.\n";
        let chunks = episode_contextual_chunks(episode_id, "user", body, None);
        assert_eq!(chunks.len(), 2, "five turns / window 4 → two windows");

        let uuid = episode_id.as_uuid();
        assert_eq!(
            chunks[0].header,
            format!("[episode {uuid}] [kind user] [turns 1-4]")
        );
        assert_eq!(
            chunks[1].header,
            format!("[episode {uuid}] [kind user] [turns 5-5]")
        );

        // Spans are byte offsets of the window within the body; the body
        // slice at that span equals the chunk body (ASCII → byte == char).
        let first_start = body.find("user: a b c.").unwrap();
        let fourth = "assistant: j k l.";
        let first_end = body.find(fourth).unwrap() + fourth.len();
        assert_eq!(
            chunks[0].source_span,
            Some(format!("{first_start}-{first_end}"))
        );
        assert_eq!(chunks[0].body, &body[first_start..first_end]);
        assert!(chunks[0].body.starts_with("user: a b c."));
        assert!(chunks[0].body.ends_with("assistant: j k l."));
    }

    /// W5 §1: a parsed content date is stamped into the chunk header `[date ...]`
    /// slot (between kind and the span label), using the TRUE date, never the
    /// clock. `None` (the default and the flag-off path) keeps the header
    /// dateless — every existing header assertion above exercises that case.
    #[test]
    fn dated_header_stamps_content_date_between_kind_and_span() {
        let episode_id = EpisodeId::new();
        let body = "[session s1] [date 2023/05/30]\n\
user: a b c.\n\
assistant: d e f.\n\
user: g h i.\n\
assistant: j k l.\n\
user: m n o.\n";
        let uuid = episode_id.as_uuid();
        let chunks = episode_contextual_chunks(episode_id, "user", body, Some("2023-05-30"));
        assert_eq!(chunks.len(), 2);
        assert_eq!(
            chunks[0].header,
            format!("[episode {uuid}] [kind user] [date 2023-05-30] [turns 1-4]")
        );
        assert_eq!(
            chunks[1].header,
            format!("[episode {uuid}] [kind user] [date 2023-05-30] [turns 5-5]")
        );
    }

    /// Non-turn prose falls back to line segments with a `[segments a-b]` label.
    #[test]
    fn non_turn_body_falls_back_to_line_segments() {
        let episode_id = EpisodeId::new();
        let body = "Line one about apples.\n\
Line two about oranges.\n\
Line three about pears.\n\
Line four about grapes.\n\
Line five about kiwis.\n";
        let chunks = episode_contextual_chunks(episode_id, "doc", body, None);
        assert_eq!(chunks.len(), 2, "five lines / window 4 → two windows");
        assert!(
            chunks[0].header.contains("[segments 1-4]"),
            "fallback labels windows as segments: {}",
            chunks[0].header
        );
        assert!(chunks[1].header.contains("[segments 5-5]"));
        assert!(chunks[0].body.starts_with("Line one about apples."));
    }

    /// A body that fits a single window would only duplicate the unit body:
    /// emit nothing (bloat guard).
    #[test]
    fn single_window_body_emits_no_chunks() {
        let episode_id = EpisodeId::new();
        let four_turns = "[session s1] [date 2023/05/30]\n\
user: a b c.\n\
assistant: d e f.\n\
user: g h i.\n\
assistant: j k l.\n";
        assert!(episode_contextual_chunks(episode_id, "user", four_turns, None).is_empty());
        // A lone prose line is also a single window.
        assert!(episode_contextual_chunks(episode_id, "doc", "one solitary line", None).is_empty());
        // And an empty body yields nothing.
        assert!(episode_contextual_chunks(episode_id, "doc", "", None).is_empty());
    }

    /// Never mint more than `MAX_CONTEXTUAL_CHUNKS` per episode. W9: this cap
    /// now holds by construction (the window grows to fit), so the same body
    /// that used to have its tail silently dropped once the fixed 4-turn
    /// window hit the 32-chunk cap is now fully covered too.
    #[test]
    fn per_episode_chunk_cap_is_enforced() {
        let episode_id = EpisodeId::new();
        let turns = (MAX_CONTEXTUAL_CHUNKS + 2) * CONTEXTUAL_CHUNK_WINDOW;
        let body = turn_body(turns);
        let chunks = episode_contextual_chunks(episode_id, "user", &body, None);
        assert!(
            chunks.len() <= MAX_CONTEXTUAL_CHUNKS,
            "cap holds: {} chunks for {turns} turns",
            chunks.len()
        );
        assert!(chunks.iter().all(|chunk| !chunk.body.trim().is_empty()));

        let (_, last) = parse_span_range(&chunks.last().unwrap().header, "turns");
        assert_eq!(
            last, turns,
            "last chunk must reach the final turn, not truncate the tail"
        );
    }

    /// W9 property: for any body length, either it fits a single window (no
    /// chunks minted — the pre-existing bloat guard, unaffected by W9) or the
    /// windows are contiguous, start at turn 1, and the last window's `last`
    /// equals the turn count exactly — the whole body is covered, remainder
    /// included, no matter how far past the cap-at-fixed-window threshold
    /// (128 segments) the body runs.
    #[test]
    fn full_body_coverage_property_for_growing_bodies() {
        for &turn_count in &[1usize, 129, 500, 10_000] {
            let episode_id = EpisodeId::new();
            let body = turn_body(turn_count);
            let chunks = episode_contextual_chunks(episode_id, "user", &body, None);

            if turn_count <= CONTEXTUAL_CHUNK_WINDOW {
                assert!(
                    chunks.is_empty(),
                    "n={turn_count}: single-window body mints no chunks"
                );
                continue;
            }

            assert!(
                !chunks.is_empty(),
                "n={turn_count}: multi-window body must mint chunks"
            );
            assert!(
                chunks.len() <= MAX_CONTEXTUAL_CHUNKS,
                "n={turn_count}: cap must hold, got {} chunks",
                chunks.len()
            );

            let mut expected_start = 1usize;
            for (idx, chunk) in chunks.iter().enumerate() {
                let (first, last) = parse_span_range(&chunk.header, "turns");
                assert_eq!(
                    first, expected_start,
                    "n={turn_count}: window {idx} must start where the previous one left off"
                );
                assert!(
                    last >= first,
                    "n={turn_count}: window {idx} range must be non-empty"
                );
                expected_start = last + 1;
            }

            let (_, last) = parse_span_range(&chunks.last().unwrap().header, "turns");
            assert_eq!(
                last, turn_count,
                "n={turn_count}: last window must reach the final turn — no dropped tail"
            );
        }
    }

    /// §2: bodies of ≤128 segments (`MAX_CONTEXTUAL_CHUNKS * CONTEXTUAL_CHUNK_WINDOW`)
    /// must be byte-identical to pre-W9 behavior — the window stays fixed at
    /// `CONTEXTUAL_CHUNK_WINDOW` and every window is exactly that size except
    /// a possible final remainder.
    #[test]
    fn bodies_at_or_under_128_segments_use_fixed_window() {
        for &turn_count in &[5usize, 32, 100, 128] {
            let episode_id = EpisodeId::new();
            let body = turn_body(turn_count);
            let chunks = episode_contextual_chunks(episode_id, "user", &body, None);

            let expected_windows = turn_count.div_ceil(CONTEXTUAL_CHUNK_WINDOW);
            assert_eq!(
                chunks.len(),
                expected_windows,
                "n={turn_count}: window must stay fixed at {CONTEXTUAL_CHUNK_WINDOW} up to 128 segments"
            );
            for (idx, chunk) in chunks.iter().enumerate() {
                let (first, last) = parse_span_range(&chunk.header, "turns");
                let expected_first = idx * CONTEXTUAL_CHUNK_WINDOW + 1;
                let expected_last = (expected_first + CONTEXTUAL_CHUNK_WINDOW - 1).min(turn_count);
                assert_eq!(first, expected_first, "n={turn_count}: window {idx} start");
                assert_eq!(last, expected_last, "n={turn_count}: window {idx} end");
            }
        }
    }

    /// §1: header ranges stay truthful under window growth — a small,
    /// fully-worked example (rather than the property sweep above) so a
    /// failure here points straight at the arithmetic.
    #[test]
    fn header_ranges_truthful_under_growth() {
        let episode_id = EpisodeId::new();
        let turn_count = 129;
        let body = turn_body(turn_count);
        let chunks = episode_contextual_chunks(episode_id, "user", &body, None);

        // window_size = max(4, ceil(129 / 32)) = 5; ceil(129 / 5) = 26 windows.
        assert_eq!(chunks.len(), 26, "129 turns / grown window 5 → 26 windows");
        assert!(chunks.len() <= MAX_CONTEXTUAL_CHUNKS);

        let uuid = episode_id.as_uuid();
        assert_eq!(
            chunks[0].header,
            format!("[episode {uuid}] [kind user] [turns 1-5]")
        );
        assert_eq!(
            chunks[1].header,
            format!("[episode {uuid}] [kind user] [turns 6-10]")
        );
        // Last window carries the 4-turn remainder (129 == 25 * 5 + 4)
        // instead of being dropped.
        assert_eq!(
            chunks[25].header,
            format!("[episode {uuid}] [kind user] [turns 126-129]")
        );
    }

    /// Ids are deterministic in episode id + window index across calls.
    #[test]
    fn chunk_ids_are_deterministic() {
        let episode_id = EpisodeId::new();
        let body = "[session s1] [date 2023/05/30]\n\
user: a b c.\n\
assistant: d e f.\n\
user: g h i.\n\
assistant: j k l.\n\
user: m n o.\n";
        let uuid = episode_id.as_uuid();
        let first = episode_contextual_chunks(episode_id, "user", body, None);
        let second = episode_contextual_chunks(episode_id, "user", body, None);
        let ids: Vec<_> = first.iter().map(|chunk| chunk.id.clone()).collect();
        assert_eq!(
            ids,
            vec![format!("chunk-{uuid}-0"), format!("chunk-{uuid}-1")]
        );
        assert_eq!(
            ids,
            second
                .iter()
                .map(|chunk| chunk.id.clone())
                .collect::<Vec<_>>()
        );
    }

    /// Non-ASCII bodies expose the byte-vs-char bug directly: the reported
    /// `source_span` must be byte offsets so slicing the original body at
    /// that span reproduces the chunk body exactly, even when multi-byte
    /// characters precede the window.
    #[test]
    fn source_span_is_byte_offsets_for_multibyte_body() {
        let episode_id = EpisodeId::new();
        // Turns 1-4 are packed with multi-byte characters (é, ö, 世界, 🎉),
        // so byte offsets and char offsets diverge well before the second
        // window (turn 5) starts; five turns / window 4 → two chunks.
        let body = "user: héllo wörld.\n\
assistant: 世界 reply here.\n\
user: third turn 🎉 emoji.\n\
assistant: fourth turn plain.\n\
user: fifth turn plain.\n";
        let chunks = episode_contextual_chunks(episode_id, "conversation", body, None);
        assert_eq!(chunks.len(), 2, "five turns / window 4 → two windows");

        let chunk = &chunks[1];
        let span = chunk.source_span.as_deref().expect("span present");
        let (start_str, end_str) = span.split_once('-').expect("span is start-end");
        let start: usize = start_str.parse().expect("start is a byte offset");
        let end: usize = end_str.parse().expect("end is a byte offset");

        // Byte offsets differ from char offsets here because turns 1-4 (which
        // precede this window) contain multi-byte characters — this would
        // mis-slice (or, at a non-boundary byte, panic) if the span were
        // still reported in chars.
        assert_ne!(
            start,
            body[..start].chars().count(),
            "test body must actually contain multi-byte offsets before the span"
        );
        assert_eq!(
            &body[start..end],
            chunk.body,
            "slicing the episode body at the reported byte span reproduces the chunk body exactly"
        );
    }
}

#[cfg(test)]
mod resource_chunk_tests {
    use super::*;

    const RESOURCE_ID: u128 = 0x0000_0000_0000_4d3d_0000_0000_0000_0007;

    /// A single-line paragraph of at least `chars` ASCII characters, no trailing
    /// whitespace, tagged so windows/chunks are identifiable.
    fn para(tag: &str, chars: usize) -> String {
        let mut body = format!("{tag}:");
        while body.chars().count() < chars {
            body.push_str(" lorem ipsum dolor sit amet consectetur");
        }
        body
    }

    /// Parses a chunk's `start-end` byte span.
    fn span_of(chunk: &ContextualChunk) -> (usize, usize) {
        let raw = chunk.source_span.as_deref().expect("chunk carries a span");
        let (start, end) = raw.split_once('-').expect("span is start-end");
        (
            start.parse().expect("start is a byte offset"),
            end.parse().expect("end is a byte offset"),
        )
    }

    #[test]
    fn blank_lines_split_paragraphs_outside_fences() {
        let body = "# Title\n\nFirst paragraph here.\n\nSecond paragraph here.\n";
        let paras = segment_resource_paragraphs(body);
        assert_eq!(paras.len(), 3, "heading + two paragraphs: {paras:?}");
        assert_eq!(&body[paras[0].0..paras[0].1], "# Title");
        assert_eq!(&body[paras[1].0..paras[1].1], "First paragraph here.");
        assert_eq!(&body[paras[2].0..paras[2].1], "Second paragraph here.");
    }

    #[test]
    fn fenced_block_is_never_split_on_internal_blank_lines() {
        let body = "# Heading\n\n\
Some intro prose before the code block goes here.\n\n\
```rust\n\
fn foo() {\n\
\n\
    let x = 1;\n\
\n\
    x + 1\n\
}\n\
```\n\n\
Trailing prose after the fence.\n";
        let paras = segment_resource_paragraphs(body);
        // heading, intro, WHOLE fence (one paragraph despite two internal blank
        // lines), trailing prose = 4 paragraphs.
        assert_eq!(paras.len(), 4, "fence stays a single paragraph: {paras:?}");
        let fence = &body[paras[2].0..paras[2].1];
        assert!(
            fence.starts_with("```rust"),
            "fence span starts at the opening fence: {fence:?}"
        );
        assert!(
            fence.contains("let x = 1;") && fence.contains("x + 1"),
            "fence interior (across its blank lines) is intact: {fence:?}"
        );
        assert!(
            fence.ends_with("```"),
            "fence span includes the closing fence: {fence:?}"
        );
    }

    #[test]
    fn windows_are_a_gapless_non_overlapping_partition_within_hard_cap() {
        let body = [
            para("a", 500),
            para("b", 500),
            para("c", 500),
            para("d", 500),
        ]
        .join("\n\n");
        let paras = segment_resource_paragraphs(&body);
        assert_eq!(paras.len(), 4);
        let windows = window_resource_paragraphs(&body, &paras);
        assert!(
            windows.len() >= 2,
            "a ~2k-char body yields multiple windows: {windows:?}"
        );
        // Non-overlapping + gapless: window[k+1] starts exactly one paragraph
        // past window[k]'s inclusive end (mirrors the episode chunker's
        // `spans.chunks(window_size)` partition — the mirror-the-twin deviation
        // from the brief's requested overlap).
        for pair in windows.windows(2) {
            assert_eq!(
                pair[1].0,
                pair[0].1 + 1,
                "windows partition paragraphs without overlap or gaps"
            );
        }
        assert_eq!(windows.first().unwrap().0, 0, "coverage starts at para 0");
        assert_eq!(
            windows.last().unwrap().1,
            paras.len() - 1,
            "coverage reaches the last paragraph (no dropped tail)"
        );
        for &(first, last) in &windows {
            let chars = body[paras[first].0..paras[last].1].chars().count();
            assert!(
                chars <= RESOURCE_CHUNK_HARD_MAX_CHARS,
                "each window stays within the hard cap: {chars} chars"
            );
        }
    }

    #[test]
    fn tiny_trailing_paragraph_merges_into_predecessor() {
        // Two ~1000-char paragraphs (each fills its own window) then a tiny tail.
        // Greedy leaves the tail as a lone sub-min window; the post-pass merges it
        // into its predecessor (the merged span stays within HARD_MAX).
        let body = [
            para("a", 1000),
            para("b", 1000),
            "tiny final note.".to_string(),
        ]
        .join("\n\n");
        let paras = segment_resource_paragraphs(&body);
        assert_eq!(paras.len(), 3);
        let windows = window_resource_paragraphs(&body, &paras);
        assert_eq!(
            windows,
            vec![(0, 0), (1, 2)],
            "the tiny trailing paragraph is folded into its predecessor window"
        );
    }

    #[test]
    fn source_span_reproduces_chunk_body_verbatim_over_multibyte_text() {
        // Leading multi-byte content so later windows sit at byte offsets that
        // differ from char offsets — proving the span is BYTE-based.
        let body = [
            para("café•α", 500),
            para("β• second", 500),
            para("γ•δ third", 500),
            para("δ• fourth", 500),
        ]
        .join("\n\n");
        let chunks =
            resource_contextual_chunks(ResourceId::from_u128(RESOURCE_ID), "doc.md", &body);
        assert!(chunks.len() >= 2, "multi-window body: {}", chunks.len());
        for chunk in &chunks {
            let (start, end) = span_of(chunk);
            assert_eq!(
                &body[start..end],
                chunk.body,
                "slicing the resource body at the reported byte span reproduces the chunk body"
            );
            assert!(
                body.contains(chunk.body.as_str()),
                "chunk body is a verbatim substring of the parent resource body"
            );
        }
        let last = chunks.last().unwrap();
        let (start, _) = span_of(last);
        assert_ne!(
            start,
            body[..start].chars().count(),
            "a later window's byte offset must diverge from its char offset (multi-byte proof)"
        );
    }

    #[test]
    fn single_window_or_short_body_emits_no_chunks() {
        let id = ResourceId::from_u128(RESOURCE_ID);
        // One short section = a single window = no chunks (the whole-section unit
        // is the memory; a lone chunk would just duplicate it).
        assert!(
            resource_contextual_chunks(id, "doc.md", "# Only\n\nOne short paragraph.").is_empty()
        );
        // Empty / whitespace-only body yields nothing.
        assert!(resource_contextual_chunks(id, "doc.md", "").is_empty());
        assert!(resource_contextual_chunks(id, "doc.md", "\n\n   \n").is_empty());
    }

    #[test]
    fn chunks_are_deterministic() {
        let id = ResourceId::from_u128(RESOURCE_ID);
        let body = [
            para("a", 500),
            para("b", 500),
            para("c", 500),
            para("d", 500),
        ]
        .join("\n\n");
        let first = resource_contextual_chunks(id, "doc.md", &body);
        let second = resource_contextual_chunks(id, "doc.md", &body);
        assert_eq!(first, second, "same input yields identical chunks");
        assert!(first.len() >= 2);
    }

    #[test]
    fn header_is_first_heading_then_uri_stem() {
        // First markdown heading wins, verbatim (the gate ingests sections
        // starting with their `###` heading).
        assert_eq!(
            resource_chunk_header("### Config Reference\n\nBody.", "x/config.md"),
            "### Config Reference"
        );
        // No heading → uri stem (path tail without extension/query/fragment).
        assert_eq!(
            resource_chunk_header(
                "Just prose, no heading.",
                "https://d.io/guides/setup.md?v=2"
            ),
            "setup"
        );
        assert_eq!(resource_chunk_header("prose", ""), "resource");
    }

    #[test]
    fn chunk_id_and_header_link_to_parent_resource() {
        let id = ResourceId::from_u128(RESOURCE_ID);
        let body = format!(
            "### Deploy Guide\n\n{}\n\n{}\n\n{}\n\n{}",
            para("a", 500),
            para("b", 500),
            para("c", 500),
            para("d", 500)
        );
        let chunks = resource_contextual_chunks(id, "deploy.md", &body);
        assert!(chunks.len() >= 2);
        let uuid = id.as_uuid();
        for (index, chunk) in chunks.iter().enumerate() {
            assert_eq!(
                chunk.id,
                format!("chunk-{uuid}-{index}"),
                "chunk id derives from the parent resource + window index"
            );
            assert_eq!(
                chunk.header, "### Deploy Guide",
                "every chunk carries the section heading as its context header"
            );
            assert!(!chunk.body.trim().is_empty(), "no empty-body chunks");
        }
    }
}

#[cfg(test)]
mod fact_tests {
    use super::*;

    /// Reduce a matched fact to `(family, subject_phrase)` for table assertions;
    /// `None` when the sentence is a near-miss the extractor rejects.
    fn matched(sentence: &str) -> Option<(&'static str, String)> {
        match_fact(sentence).map(|fact| (fact.family, fact.subject_phrase))
    }

    /// §5 pattern table: deterministic hits across all four v1 families, plus the
    /// §6 near-miss rejections (conversational false positives). Precision matters
    /// more than recall — a demonstrative/pronoun object or a meta noun is dropped.
    #[test]
    fn pattern_table_hits_and_near_miss_rejections() {
        // Hits: (sentence, family, subject_phrase).
        let hits: &[(&str, &str, &str)] = &[
            // superlative → preference, keyed on the SUBJECT ("favorite tea"),
            // never the value, so a later value supersedes.
            ("My favorite tea is chamomile", "preference", "favorite tea"),
            (
                "My all-time favorite band is Queen",
                "preference",
                "favorite band",
            ),
            // preference verbs → preference, keyed on the object.
            (
                "I really love hiking outdoors",
                "preference",
                "hiking outdoors",
            ),
            ("I switched to oat milk lately", "preference", "oat milk"),
            // explicit update: negation + trailing "anymore" normalize to the
            // same object key as the positive assertion (→ supersede).
            ("I don't like coffee anymore", "preference", "coffee"),
            (
                "I can't stand loud crowded bars",
                "preference",
                "loud crowded bars",
            ),
            // a non-verb-fronting leading "I" does not blind the scan to the
            // verb-fronting "I" later in the sentence.
            (
                "When I travel I love quiet trails",
                "preference",
                "quiet trails",
            ),
            // identity / attribute.
            ("My name is Sidney Carter", "attribute", "name"),
            ("My birthday is in early May", "attribute", "birthday"),
            ("I am a software engineer", "attribute", "software engineer"),
            // "my new <noun> is" shares the attribute:<noun> key (→ supersede).
            ("My new phone is a pixel", "attribute", "phone"),
        ];
        for (sentence, family, phrase) in hits {
            assert_eq!(
                matched(sentence),
                Some((*family, phrase.to_string())),
                "hit: {sentence:?}"
            );
        }

        // Near-miss rejections → None.
        let rejects: &[&str] = &[
            "I like that idea",                // demonstrative object
            "I love it",                       // pronoun object (and < 4 words)
            "My point is that we ship",        // meta noun
            "My question is whether it works", // meta noun
            "I'm a big fan of yours",          // filler identity ("big fan")
            "The weather is nice today",       // no first-person marker
            "She loves the ocean",             // not first person
            "I think we should go soon",       // no preference verb
        ];
        for sentence in rejects {
            assert_eq!(matched(sentence), None, "reject: {sentence:?}");
        }
    }

    /// §3 word-count guard: sentences under `MIN_FACT_SENTENCE_WORDS` are never
    /// mined even when they match a pattern.
    #[test]
    fn short_sentences_are_skipped() {
        assert_eq!(matched("I love it"), None);
        assert_eq!(
            matched("My car is red"),
            Some(("attribute", "car".to_string()))
        );
    }

    /// §3 assistant-turn exclusion at the pure level: a first-person statement in
    /// an assistant turn is never mined; user turns and non-role prose are.
    #[test]
    fn extract_skips_assistant_turns() {
        let body = "[session s1]\n\
assistant: I love that plan and my favorite bit is the finish.\n\
user: My favorite fruit is a ripe mango.\n";
        let facts = extract_facts(body, None);
        assert_eq!(facts.len(), 1, "only the user turn is mined: {facts:?}");
        assert_eq!(facts[0].family, "preference");
        assert_eq!(facts[0].subject_phrase, "favorite fruit");
        assert_eq!(facts[0].body, "My favorite fruit is a ripe mango");
    }

    /// §3 within-episode dedup keeps the LAST occurrence of a subject (later
    /// turns win) and the §2 date prefix is baked into the body only when a date
    /// is supplied.
    #[test]
    fn dedup_keeps_last_and_dates_prefix_when_supplied() {
        let body = "user: My favorite tea is plain green tea.\n\
user: My favorite tea is smoky oolong tea.\n";
        let facts = extract_facts(body, Some("2023-05-30"));
        assert_eq!(facts.len(), 1, "deduped to one favorite-tea fact");
        assert!(
            facts[0].body.contains("oolong") && !facts[0].body.contains("green"),
            "later assertion wins: {}",
            facts[0].body
        );
        assert!(
            facts[0].body.starts_with("[date 2023-05-30]"),
            "date prefix baked in: {}",
            facts[0].body
        );
    }

    /// §3 cap: never more than `MAX_EXTRACTED_FACTS`, keeping the most recent.
    #[test]
    fn cap_holds_and_keeps_most_recent() {
        let mut body = String::new();
        for i in 0..(MAX_EXTRACTED_FACTS + 4) {
            body.push_str(&format!("user: I really love hobby number {i} here.\n"));
        }
        let facts = extract_facts(&body, None);
        assert_eq!(facts.len(), MAX_EXTRACTED_FACTS, "cap holds");
        assert!(
            facts
                .last()
                .unwrap()
                .body
                .contains(&format!("number {} here", MAX_EXTRACTED_FACTS + 3)),
            "the most recent facts are kept: {:?}",
            facts.last().unwrap().body
        );
    }
}

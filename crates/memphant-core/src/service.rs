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
    ReflectJobKind, ReflectResult, ResourceId, RetainEpisodeHttpRequest, RetainEpisodeHttpResponse,
    RetainRequest, RetainResourceRequest, RetrievalTrace, ScopeId, StoredEpisode, TenantId,
    TraceId, TrustLevel, UnitId,
};

use crate::{
    Clock, CoreError, DEFAULT_CANDIDATE_POOL_SIZE, EmbeddingProvider, JobFilter, MemoryStore,
    PackLevers, ReflectJobRow, ScopePage, StoreError, VectorQuery, correct_memory,
    embedding_profile_for, forget_memory, recall_with_pool, record_mark, reflect_recorded,
    retain_episode, retain_resource, tokenize,
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
}

impl<S: MemoryStore> Clone for MemoryService<S> {
    fn clone(&self) -> Self {
        Self {
            store: Arc::clone(&self.store),
            clock: Arc::clone(&self.clock),
            embedder: Arc::clone(&self.embedder),
            contextual_chunks_write_enabled: self.contextual_chunks_write_enabled,
            candidate_pool_size: self.candidate_pool_size,
            pack_levers: self.pack_levers,
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
            candidate_pool_size: DEFAULT_CANDIDATE_POOL_SIZE,
            pack_levers: PackLevers::default(),
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
                let trace = reflect_recorded(
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
                let unit_ids = self
                    .store
                    .scope_memory_page(tenant, request.scope_id, None, usize::MAX)
                    .await
                    .map(|page| {
                        page.items
                            .iter()
                            .filter(|stored| stored.source_kind.as_deref() == Some("direct"))
                            .filter(|stored| stored.body == unit.body)
                            .map(|stored| stored.id)
                            .collect()
                    })
                    .unwrap_or_default();
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
        let k = request.limit.unwrap_or(8);
        // Real embedding provider → embed the query and run the vector
        // channel; the Noop provider keeps the channel honestly disabled.
        let query_vec = if self.embedder.dimensions() > 0 {
            self.embedder
                .embed(std::slice::from_ref(&query))
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
                budget_tokens: request.budget_tokens.unwrap_or(512),
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
            self.store.complete_reflect_job(job.job.id).await?;
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
        Ok(correct_memory(self.store.as_ref(), request, self.clock.as_ref()).await?)
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
                    self.store.complete_reflect_job(job.job.id).await?;
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
        let (episode_id, resource_id, candidate) = match job.job.kind {
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
                // Rung 4: mint contextual chunks tied to this raw episode when
                // the write path is enabled (default on since 2026-07-10).
                // Every other candidate construction (resource jobs,
                // direct-unit retains) stays chunk-free — episodes only.
                let contextual_chunks = if self.contextual_chunks_write_enabled {
                    episode_contextual_chunks(episode.id, &episode.source_kind, &episode.body)
                } else {
                    Vec::new()
                };
                (
                    Some(episode.id),
                    None,
                    ReflectCandidate {
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
                        valid_from: None,
                        valid_to: None,
                    },
                )
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
                (
                    None,
                    Some(resource.id),
                    ReflectCandidate {
                        source_kind: "resource".to_string(),
                        trust_level: resource.source_trust,
                        actor_id: resource.actor_id,
                        subject: None,
                        predicate: None,
                        kind: Some(MemoryKind::Resource),
                        body,
                        churn_class: None,
                        admission_hint: None,
                        contextual_chunks: Vec::new(),
                        valid_from: None,
                        valid_to: None,
                    },
                )
            }
        };

        let trace = reflect_recorded(
            self.store.as_ref(),
            ReflectInput {
                tenant_id: job.job.tenant_id,
                scope_id: job.job.scope_id,
                actor_id: candidate.actor_id,
                episode_id,
                resource_id,
                job_id: job.job.id,
                compiler_version,
                candidates: vec![candidate],
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
/// hurts recall latency/cost once it stops adding coverage). An episode long
/// enough to mint more windows keeps only its first `MAX_CONTEXTUAL_CHUNKS`
/// (covering up to `MAX_CONTEXTUAL_CHUNKS * CONTEXTUAL_CHUNK_WINDOW` turns).
const MAX_CONTEXTUAL_CHUNKS: usize = 32;

/// A body line reads as a conversational turn when it has the `role: content`
/// shape: a short leading role token, then `": "`, then non-empty content. The
/// bench lane's per-session episodes ingest in exactly this form; a bracketed
/// provenance line like `[session s1] [date ...]` has no `": "` and is skipped.
fn line_is_turn(line: &str) -> bool {
    let Some((role, content)) = line.trim().split_once(": ") else {
        return false;
    };
    !role.is_empty()
        && role.len() <= 32
        && !content.trim().is_empty()
        && role
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, ' ' | '_' | '-'))
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

/// Splits `body` into windows of up to `CONTEXTUAL_CHUNK_WINDOW` turns/segments
/// and mints one `ContextualChunk` per window, each tied back to its parent
/// episode. Emits nothing when the body fits a single window (a lone chunk
/// would just duplicate the unit body) and never emits empty-body chunks — the
/// rung 4 bloat guards.
fn episode_contextual_chunks(
    episode_id: EpisodeId,
    source_kind: &str,
    body: &str,
) -> Vec<ContextualChunk> {
    let (spans, turn_structured) = segment_episode_body(body);
    if spans.len() <= CONTEXTUAL_CHUNK_WINDOW {
        return Vec::new();
    }
    let span_label = if turn_structured { "turns" } else { "segments" };
    spans
        .chunks(CONTEXTUAL_CHUNK_WINDOW)
        .take(MAX_CONTEXTUAL_CHUNKS)
        .enumerate()
        .filter_map(|(window_index, window)| {
            let start = window.first()?.0;
            let end = window.last()?.1;
            let text = body.get(start..end)?.trim();
            if text.is_empty() {
                return None;
            }
            let first = window_index * CONTEXTUAL_CHUNK_WINDOW + 1;
            let last = window_index * CONTEXTUAL_CHUNK_WINDOW + window.len();
            Some(ContextualChunk {
                id: format!("chunk-{}-{window_index}", episode_id.as_uuid()),
                header: format!(
                    "[episode {}] [kind {source_kind}] [{span_label} {first}-{last}]",
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

// Suppress unused warnings for the resource id import used only in signatures.
#[allow(dead_code)]
fn _resource_id_type_anchor(id: ResourceId) -> ResourceId {
    id
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
        let chunks = episode_contextual_chunks(episode_id, "user", body);
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

    /// Non-turn prose falls back to line segments with a `[segments a-b]` label.
    #[test]
    fn non_turn_body_falls_back_to_line_segments() {
        let episode_id = EpisodeId::new();
        let body = "Line one about apples.\n\
Line two about oranges.\n\
Line three about pears.\n\
Line four about grapes.\n\
Line five about kiwis.\n";
        let chunks = episode_contextual_chunks(episode_id, "doc", body);
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
        assert!(episode_contextual_chunks(episode_id, "user", four_turns).is_empty());
        // A lone prose line is also a single window.
        assert!(episode_contextual_chunks(episode_id, "doc", "one solitary line").is_empty());
        // And an empty body yields nothing.
        assert!(episode_contextual_chunks(episode_id, "doc", "").is_empty());
    }

    /// Never mint more than `MAX_CONTEXTUAL_CHUNKS` per episode.
    #[test]
    fn per_episode_chunk_cap_is_enforced() {
        let episode_id = EpisodeId::new();
        let turns = (MAX_CONTEXTUAL_CHUNKS + 2) * CONTEXTUAL_CHUNK_WINDOW;
        let mut body = String::from("[session s1] [date 2023/05/30]\n");
        for turn in 0..turns {
            body.push_str(&format!("user: turn number {turn} here.\n"));
        }
        let chunks = episode_contextual_chunks(episode_id, "user", &body);
        assert_eq!(chunks.len(), MAX_CONTEXTUAL_CHUNKS, "cap holds");
        assert!(chunks.iter().all(|chunk| !chunk.body.trim().is_empty()));
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
        let first = episode_contextual_chunks(episode_id, "user", body);
        let second = episode_contextual_chunks(episode_id, "user", body);
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
        let chunks = episode_contextual_chunks(episode_id, "conversation", body);
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

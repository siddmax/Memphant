//! `MemoryService`: the one application layer shared by REST, MCP, CLI and
//! the background worker. All orchestration (retain dispatch, reflect job
//! claiming/compilation, degraded read-your-own-writes recall) lives here —
//! transport handlers stay thin.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use memphant_types::{
    COMPILER_VERSION, CorrectRequest, CorrectResult, ENGINE_VERSION, EpisodeId, ForgetRequest,
    ForgetResult, MarkRequest, MarkResult, MemoryKind, RecallContextItem, RecallHttpRequest,
    RecallMode, RecallRequest, RecallResponse, ReflectCandidate, ReflectInput, ReflectJobKind,
    ReflectResult, ResourceId, RetainEpisodeHttpRequest, RetainEpisodeHttpResponse, RetainRequest,
    RetainResourceRequest, RetrievalTrace, ScopeId, StoredEpisode, TenantId, TraceId, TrustLevel,
    UnitId,
};

use crate::{
    Clock, CoreError, EmbeddingProvider, JobFilter, MemoryStore, ReflectJobRow, ScopePage,
    StoreError, correct_memory, forget_memory, recall, record_mark, reflect_recorded,
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
}

impl<S: MemoryStore> Clone for MemoryService<S> {
    fn clone(&self) -> Self {
        Self {
            store: Arc::clone(&self.store),
            clock: Arc::clone(&self.clock),
            embedder: Arc::clone(&self.embedder),
        }
    }
}

impl<S: MemoryStore> MemoryService<S> {
    pub fn new(store: Arc<S>, clock: Arc<dyn Clock>, embedder: Arc<dyn EmbeddingProvider>) -> Self {
        Self {
            store,
            clock,
            embedder,
        }
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
        let response = recall(
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
                rerank_enabled: request.rerank_enabled.unwrap_or(true),
                learned_rerank_profile: None,
                query_decomposition_enabled: request.query_decomposition_enabled.unwrap_or(true),
                procedure_recall_enabled: request.procedure_recall_enabled.unwrap_or(true),
                decay_enabled: request.decay_enabled.unwrap_or(true),
                engine_version: ENGINE_VERSION.to_string(),
            },
            self.clock.as_ref(),
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
                        contextual_chunks: Vec::new(),
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

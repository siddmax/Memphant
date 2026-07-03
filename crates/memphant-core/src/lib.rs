use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use fsrs::{FSRS, FSRS6_DEFAULT_DECAY, MemoryState, current_retrievability};
use memphant_types::{
    AdmissionAction, CorrectRequest, CorrectResult, DedupOutcome, EdgeId, EpisodeId, ForgetRequest,
    ForgetResult, JobId, LearnedRerankProfile, MarkOutcome, MarkRequest, MarkResult,
    MemoryEdgeKind, MemoryKind, NewEpisode, NewMemoryEdge, NewMemoryUnit, ProcedureTraceFact,
    QueuedReflectJob, RecallCandidateTrace, RecallChannel, RecallCitation, RecallContextItem,
    RecallDropReason, RecallDroppedItem, RecallMode, RecallPolicyFilter, RecallRequest,
    RecallResponse, ReflectInput, ReflectJob, ReflectJobKind, ReflectStageFact, ReflectTrace,
    RetainInput, RetainOutcome, RetainRequest, RetainResourceOutcome, RetainResourceRequest,
    RetainResult, RetrievalTrace, ReviewEvent, ScopeId, StoredEpisode, StoredMemoryEdge,
    StoredMemoryUnit, StoredResource, TenantId, TraceId, TrustLevel, UnitId, UnitState,
};
use memphant_types::{NewResource, ResourceExtractorState, ResourceId};

const CURRENT_VALIDITY_CUTOFF: &str = "2026-07-03T00:00:00Z";
const DECAY_MODEL_ID: &str = "fixed-prior-dsr-v1";
const L4_SANDBOX_ID: &str = "deterministic-local-l4-v1";
const DEFAULT_STABILITY_DAYS: f32 = 7.0;
const DEFAULT_DIFFICULTY: f32 = 5.0;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("transaction already committed")]
    TransactionAlreadyCommitted,
    #[error("store mutex poisoned")]
    Poisoned,
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

#[async_trait]
pub trait MemoryStore: Send + Sync {
    type Txn: Send;

    async fn begin(&self) -> Self::Txn;
    async fn commit(&self, tx: Self::Txn) -> Result<(), StoreError>;
    async fn stage_episode(
        &self,
        tx: &mut Self::Txn,
        episode: NewEpisode,
    ) -> Result<RetainOutcome, StoreError>;
    async fn stage_memory_unit(
        &self,
        tx: &mut Self::Txn,
        unit: NewMemoryUnit,
    ) -> Result<UnitId, StoreError>;
    async fn stage_resource(
        &self,
        tx: &mut Self::Txn,
        resource: NewResource,
    ) -> Result<ResourceId, StoreError>;
    async fn stage_memory_edge(
        &self,
        tx: &mut Self::Txn,
        edge: NewMemoryEdge,
    ) -> Result<EdgeId, StoreError>;
    async fn enqueue_reflect(
        &self,
        tx: &mut Self::Txn,
        job: ReflectJob,
    ) -> Result<JobId, StoreError>;
}

#[derive(Clone, Default)]
pub struct InMemoryStore {
    inner: Arc<Mutex<InMemoryState>>,
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
    deletion_generation: u64,
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
            .filter(|unit| unit.freshness_due && unit.state == UnitState::Active)
            .collect()
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

    pub fn trace_by_id(&self, trace_id: TraceId) -> Option<RetrievalTrace> {
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

#[async_trait]
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
            freshness_due: unit.freshness_due,
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
            content_hash: resource.content_hash,
            mime_type: resource.mime_type,
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
        });
        Ok(id)
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
                content_hash: request.content_hash,
                mime_type: request.mime_type,
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
            },
        )
        .await?;
    store.commit(tx).await?;
    Ok(RetainResourceOutcome { resource_id })
}

pub async fn correct_memory(
    store: &InMemoryStore,
    request: CorrectRequest,
) -> Result<CorrectResult, CoreError> {
    if request.correction.value.trim().is_empty() {
        return Err(CoreError::Invalid(
            "correction value cannot be empty".to_string(),
        ));
    }

    let mut state = store.inner.lock().map_err(|_| StoreError::Poisoned)?;
    let units = state
        .memory_units
        .get_mut(&request.tenant_id)
        .ok_or_else(|| CoreError::NotFound("memory_unit".to_string()))?;
    let old_index = units
        .iter()
        .position(|unit| {
            unit.id == request.selector.memory_unit_id
                && unit.scope_id == request.scope_id
                && unit.state != UnitState::Deleted
        })
        .ok_or_else(|| CoreError::NotFound("memory_unit".to_string()))?;
    let mut replacement = units[old_index].clone();
    let old_id = replacement.id;
    let new_id = UnitId::new();
    let is_retroactive =
        request.correction.valid_from.is_some() || request.correction.valid_to.is_some();

    units[old_index].state = UnitState::Superseded;
    units[old_index].transaction_to = Some(CURRENT_VALIDITY_CUTOFF.to_string());
    replacement.id = new_id;
    replacement.body = request.correction.value;
    replacement.state = UnitState::Active;
    replacement.actor_id = Some(request.actor_id);
    replacement.deletion_generation = None;
    replacement.valid_from = request.correction.valid_from;
    replacement.valid_to = request.correction.valid_to;
    replacement.transaction_from = Some(CURRENT_VALIDITY_CUTOFF.to_string());
    replacement.transaction_to = None;
    units.push(replacement);

    state
        .memory_edges
        .entry(request.tenant_id)
        .or_default()
        .push(StoredMemoryEdge {
            id: EdgeId::new(),
            tenant_id: request.tenant_id,
            scope_id: request.scope_id,
            src_id: new_id,
            dst_id: old_id,
            kind: MemoryEdgeKind::Supersedes,
        });

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

pub async fn forget_memory(
    store: &InMemoryStore,
    request: ForgetRequest,
) -> Result<ForgetResult, CoreError> {
    if request.selector.memory_unit_id.is_none() && request.selector.scope_id.is_none() {
        return Err(CoreError::Invalid(
            "forget selector must include memory_unit_id or scope_id".to_string(),
        ));
    }

    let mut state = store.inner.lock().map_err(|_| StoreError::Poisoned)?;
    state.deletion_generation = state.deletion_generation.saturating_add(1);
    let deletion_generation = state.deletion_generation;
    let Some(units) = state.memory_units.get_mut(&request.tenant_id) else {
        return Ok(ForgetResult {
            deletion_generation,
            policy: "hard_delete".to_string(),
            invalidated_units: Vec::new(),
            verification: "no_recall_path_returns_forgotten".to_string(),
            trace_ref: None,
        });
    };

    let selector_scope = request.selector.scope_id.unwrap_or(request.scope_id);
    let invalidated_units = units
        .iter_mut()
        .filter(|unit| {
            if unit.tenant_id != request.tenant_id {
                return false;
            }
            if let Some(unit_id) = request.selector.memory_unit_id {
                unit.id == unit_id && unit.scope_id == selector_scope
            } else {
                unit.scope_id == selector_scope
            }
        })
        .map(|unit| {
            unit.state = UnitState::Deleted;
            unit.deletion_generation = Some(deletion_generation);
            unit.id
        })
        .collect();

    Ok(ForgetResult {
        deletion_generation,
        policy: "hard_delete".to_string(),
        invalidated_units,
        verification: "no_recall_path_returns_forgotten".to_string(),
        trace_ref: None,
    })
}

pub async fn record_mark(
    store: &InMemoryStore,
    request: MarkRequest,
) -> Result<MarkResult, CoreError> {
    if request.caller_id.trim().is_empty() {
        return Err(CoreError::Invalid("caller_id cannot be empty".to_string()));
    }

    let mut state = store.inner.lock().map_err(|_| StoreError::Poisoned)?;
    let events = state.review_events.entry(request.tenant_id).or_default();
    if !events
        .iter()
        .any(|event| event.trace_id == request.trace_id && event.caller_id == request.caller_id)
    {
        events.push(ReviewEvent {
            tenant_id: request.tenant_id,
            trace_id: request.trace_id,
            caller_id: request.caller_id,
            used_ids: request.used_ids,
            outcome: request.outcome,
        });
    }

    Ok(MarkResult {
        accepted: true,
        trace_id: request.trace_id,
    })
}

pub async fn recall(
    store: &InMemoryStore,
    request: RecallRequest,
) -> Result<RecallResponse, CoreError> {
    validate_learned_rerank_profile(request.learned_rerank_profile.as_ref())?;

    let mut state = store.inner.lock().map_err(|_| StoreError::Poisoned)?;
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
        state
            .retrieval_traces
            .entry(request.tenant_id)
            .or_default()
            .push(trace);
        return Err(CoreError::PolicyDenied("scope".to_string()));
    }

    let all_units = state
        .memory_units
        .values()
        .map(Vec::len)
        .sum::<usize>()
        .max(1);
    let tenant_units = state
        .memory_units
        .get(&request.tenant_id)
        .cloned()
        .unwrap_or_default();
    let tenant_edges = state
        .memory_edges
        .get(&request.tenant_id)
        .cloned()
        .unwrap_or_default();
    let tenant_episodes = state
        .episodes
        .get(&request.tenant_id)
        .cloned()
        .unwrap_or_default();
    let tenant_review_events = state
        .review_events
        .get(&request.tenant_id)
        .cloned()
        .unwrap_or_default();
    let dropped_items = trace_filter_drops(&tenant_units, &request);
    let scope_units = tenant_units
        .iter()
        .filter(|unit| request.allowed_scope_ids.contains(&unit.scope_id))
        .count();
    let filter_selectivity = Some(scope_units as f32 / all_units as f32);
    let query_tokens = tokenize(&request.query);
    let decomposition = decompose_query(&request);
    let mut candidates_by_unit: HashMap<UnitId, CandidateAccumulator> = HashMap::new();
    let mut candidate_traces = Vec::new();

    let channels = [
        RecallChannel::Exact,
        RecallChannel::Lexical,
        RecallChannel::Vector,
        RecallChannel::Temporal,
        RecallChannel::Edge,
    ];
    for channel in channels
        .into_iter()
        .filter(|channel| request.edge_expansion_enabled || *channel != RecallChannel::Edge)
    {
        let mut ranked = channel_candidates(
            channel,
            &tenant_units,
            &tenant_edges,
            &request,
            &query_tokens,
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
            let contribution =
                channel_weight(channel, &request.query) / (60.0 + channel_rank as f32);
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
            for channel in channels
                .into_iter()
                .filter(|channel| request.edge_expansion_enabled || *channel != RecallChannel::Edge)
            {
                let mut ranked = channel_candidates(
                    channel,
                    &tenant_units,
                    &tenant_edges,
                    &request,
                    &subquery_tokens,
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
                        channel_weight(channel, &subquery.query) / (55.0 + channel_rank as f32);
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
        let mut ranked =
            l4_exhaustive_candidates(&tenant_units, &tenant_episodes, &request, &query_tokens);
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
    let feature_flags = recall_feature_flags(&request);
    let trace = RetrievalTrace {
        id: trace_id,
        tenant_id: request.tenant_id,
        scope_id: request.scope_id,
        actor_id: request.actor_id,
        query_hash: hash_query(&request.query),
        engine_version: request.engine_version.clone(),
        feature_flags,
        channel_runs: recall_stage_facts(),
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
    state
        .retrieval_traces
        .entry(request.tenant_id)
        .or_default()
        .push(trace);

    Ok(RecallResponse {
        trace_id,
        items,
        candidate_whitelist,
        citations,
        abstention,
        degraded: false,
        suppression_labels,
    })
}

fn trace_filter_drops(
    units: &[StoredMemoryUnit],
    request: &RecallRequest,
) -> Vec<RecallDroppedItem> {
    units
        .iter()
        .filter_map(|unit| {
            let reason = if !request.allowed_scope_ids.contains(&unit.scope_id) {
                Some(RecallDropReason::Scope)
            } else if unit.deletion_generation.is_some() {
                Some(RecallDropReason::Deleted)
            } else if unit.transaction_to.is_some() || !valid_for_query(unit, &request.query) {
                Some(RecallDropReason::Stale)
            } else if let Some(reason) = procedure_drop_reason(unit, request) {
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
    let vector = vector_score(&candidate.unit, query_tokens);
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

        let unit_tokens = candidate.unit.body.split_whitespace().count();
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
                items.push(context_item_for(candidate, tenant_edges, query_tokens));
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
                items.push(context_item_for(candidate, tenant_edges, query_tokens));
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
        items.push(context_item_for(candidate, tenant_edges, query_tokens));
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
        + vector_score(&candidate.unit, query_tokens)
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

fn context_item_for(
    candidate: CandidateAccumulator,
    tenant_edges: &[StoredMemoryEdge],
    query_tokens: &[String],
) -> RecallContextItem {
    let suppression_labels = suppression_labels_for(&candidate.unit, tenant_edges);
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
        body: candidate.unit.body,
        kind: candidate.unit.kind,
        inclusion_reason: inclusion_reason.to_string(),
        citation_episode_id: candidate.unit.source_episode_id,
        citation_resource_id: candidate.unit.source_resource_id,
        suppression_labels,
    }
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
    format!(
        "{}:{}:{}:{}",
        scope_id,
        normalize_component(source_kind),
        subject,
        normalize_component(body)
    )
}

fn normalize_component(value: &str) -> String {
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

fn tokenize(value: &str) -> Vec<String> {
    normalize_component(value)
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn channel_candidates(
    channel: RecallChannel,
    units: &[StoredMemoryUnit],
    edges: &[StoredMemoryEdge],
    request: &RecallRequest,
    query_tokens: &[String],
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
            )
        })
        .filter_map(|unit| {
            let score = match channel {
                RecallChannel::Exact => exact_score(unit, query_tokens),
                RecallChannel::Lexical => lexical_score(unit, query_tokens),
                RecallChannel::Vector => vector_score(unit, query_tokens),
                RecallChannel::Temporal => temporal_score(unit, &request.query),
                RecallChannel::Edge => edge_score(
                    unit,
                    units,
                    edges,
                    query_tokens,
                    request.procedure_recall_enabled,
                ),
                RecallChannel::Exhaustive => 0.0,
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
            )
        })
        .filter_map(|unit| {
            let episode = unit
                .source_episode_id
                .and_then(|episode_id| episodes.iter().find(|episode| episode.id == episode_id))?;
            let raw_score = vector_text_score(&episode.body, query_tokens);
            let direct_score = exact_score(unit, query_tokens)
                .max(lexical_score(unit, query_tokens))
                .max(vector_score(unit, query_tokens))
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
                recallable(candidate, true, procedure_recall_enabled, "")
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
) -> bool {
    if unit.deletion_generation.is_some() {
        return false;
    }
    if unit.transaction_to.is_some() || !valid_for_query(unit, query) {
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

fn valid_for_query(unit: &StoredMemoryUnit, query: &str) -> bool {
    if unit.kind != MemoryKind::Semantic || is_historical_query(query) {
        return true;
    }
    if is_current_query(query) {
        return unit
            .valid_to
            .as_deref()
            .is_none_or(|valid_to| valid_to > CURRENT_VALIDITY_CUTOFF);
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

fn vector_score(unit: &StoredMemoryUnit, query_tokens: &[String]) -> f32 {
    vector_text_score(&unit.body, query_tokens).max(contextual_chunk_score(unit, query_tokens))
}

fn contextual_chunk_score(unit: &StoredMemoryUnit, query_tokens: &[String]) -> f32 {
    unit.contextual_chunks
        .iter()
        .map(|chunk| vector_text_score(&format!("{} {}", chunk.header, chunk.body), query_tokens))
        .fold(0.0, f32::max)
}

fn vector_text_score(text: &str, query_tokens: &[String]) -> f32 {
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

fn channel_weight(channel: RecallChannel, query: &str) -> f32 {
    let query = normalize_component(query);
    let query_tokens = tokenize(&query);
    match channel {
        RecallChannel::Exact if query.contains("how") => 2.5,
        RecallChannel::Exact => 1.0,
        RecallChannel::Lexical if query.contains("error") => 3.0,
        RecallChannel::Lexical if query.contains("how") => 2.0,
        RecallChannel::Lexical => 1.0,
        RecallChannel::Vector => 2.0,
        RecallChannel::Temporal
            if query_tokens
                .iter()
                .any(|token| matches!(token.as_str(), "current" | "latest" | "now")) =>
        {
            2.5
        }
        RecallChannel::Temporal => 0.5,
        RecallChannel::Edge => 0.5,
        RecallChannel::Exhaustive => 4.0,
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

fn recall_stage_facts() -> Vec<ReflectStageFact> {
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
        detail: "completed".to_string(),
    })
    .collect()
}

fn recall_feature_flags(request: &RecallRequest) -> Vec<String> {
    let mut flags = vec![
        "entity_exact_enabled".to_string(),
        "fts_enabled".to_string(),
        "vector_enabled".to_string(),
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

pub async fn reflect_recorded(
    store: &InMemoryStore,
    input: ReflectInput,
) -> Result<ReflectTrace, CoreError> {
    let mut state = store.inner.lock().map_err(|_| StoreError::Poisoned)?;
    if let Some(existing) = state
        .reflect_traces
        .get(&input.tenant_id)
        .and_then(|traces| {
            traces.iter().find(|trace| {
                trace.job_id == input.job_id && trace.compiler_version == input.compiler_version
            })
        })
    {
        return Ok(existing.clone());
    }

    let mut actions = Vec::new();

    for candidate in input.candidates {
        let Some(subject_key) =
            derive_subject_key(candidate.subject.as_deref(), candidate.predicate.as_deref())
        else {
            actions.push(AdmissionAction::Reject);
            continue;
        };

        if candidate.body.split_whitespace().count() < 3 {
            actions.push(AdmissionAction::Reject);
            continue;
        }

        let high_trust = matches!(
            candidate.trust_level,
            TrustLevel::TrustedUser | TrustLevel::TrustedSystem
        );
        let mut edges = Vec::new();
        let action = {
            let units = state.memory_units.entry(input.tenant_id).or_default();
            if let Some(existing_index) = units.iter().position(|unit| {
                unit.scope_id == input.scope_id
                    && unit.subject_key.as_deref() == Some(subject_key.as_str())
                    && unit.body == candidate.body
                    && unit.state != UnitState::Deleted
                    && unit.state != UnitState::Invalidated
            }) {
                if !high_trust
                    && units[existing_index].kind == MemoryKind::Belief
                    && is_independent_source(&units[existing_index], &candidate)
                {
                    let belief_id = units[existing_index].id;
                    let semantic_id = UnitId::new();
                    units.push(StoredMemoryUnit {
                        id: semantic_id,
                        tenant_id: input.tenant_id,
                        scope_id: input.scope_id,
                        kind: MemoryKind::Semantic,
                        state: UnitState::Active,
                        subject_key: Some(subject_key),
                        body: candidate.body,
                        trust_level: candidate.trust_level,
                        freshness_due: candidate.churn_class.as_deref() == Some("volatile"),
                        churn_class: candidate.churn_class,
                        actor_id: Some(candidate.actor_id),
                        source_kind: Some(candidate.source_kind),
                        source_episode_id: Some(input.episode_id),
                        source_resource_id: None,
                        deletion_generation: None,
                        contextual_chunks: candidate.contextual_chunks,
                        valid_from: candidate.valid_from,
                        valid_to: candidate.valid_to,
                        transaction_from: Some(CURRENT_VALIDITY_CUTOFF.to_string()),
                        transaction_to: None,
                        difficulty: None,
                        stability_days: None,
                        last_reinforced_at: None,
                        reinforcement_count: 0,
                    });
                    edges.push(StoredMemoryEdge {
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
                let new_id = UnitId::new();
                let mut action = AdmissionAction::Append;
                if candidate.admission_hint == Some(AdmissionAction::Invalidate) {
                    if let Some(existing_index) = units.iter().position(|unit| {
                        unit.scope_id == input.scope_id
                            && unit.subject_key.as_deref() == Some(subject_key.as_str())
                            && unit.state == UnitState::Active
                            && unit.kind == MemoryKind::Semantic
                    }) {
                        units[existing_index].state = UnitState::Invalidated;
                    }
                    AdmissionAction::Invalidate
                } else if candidate.admission_hint == Some(AdmissionAction::Quarantine) {
                    units.push(StoredMemoryUnit {
                        id: new_id,
                        tenant_id: input.tenant_id,
                        scope_id: input.scope_id,
                        kind: MemoryKind::Belief,
                        state: UnitState::Quarantined,
                        subject_key: Some(subject_key),
                        body: candidate.body,
                        trust_level: candidate.trust_level,
                        freshness_due: false,
                        churn_class: candidate.churn_class,
                        actor_id: Some(candidate.actor_id),
                        source_kind: Some(candidate.source_kind),
                        source_episode_id: Some(input.episode_id),
                        source_resource_id: None,
                        deletion_generation: None,
                        contextual_chunks: candidate.contextual_chunks,
                        valid_from: candidate.valid_from,
                        valid_to: candidate.valid_to,
                        transaction_from: Some(CURRENT_VALIDITY_CUTOFF.to_string()),
                        transaction_to: None,
                        difficulty: None,
                        stability_days: None,
                        last_reinforced_at: None,
                        reinforcement_count: 0,
                    });
                    AdmissionAction::Quarantine
                } else {
                    if let Some(existing_index) = units.iter().position(|unit| {
                        unit.scope_id == input.scope_id
                            && unit.subject_key.as_deref() == Some(subject_key.as_str())
                            && unit.state == UnitState::Active
                            && unit.kind == MemoryKind::Semantic
                    }) {
                        action = AdmissionAction::Supersede;
                        let old_id = units[existing_index].id;
                        units[existing_index].state = UnitState::Superseded;
                        units[existing_index].transaction_to =
                            Some(CURRENT_VALIDITY_CUTOFF.to_string());
                        edges.push(StoredMemoryEdge {
                            id: EdgeId::new(),
                            tenant_id: input.tenant_id,
                            scope_id: input.scope_id,
                            src_id: old_id,
                            dst_id: new_id,
                            kind: MemoryEdgeKind::Contradicts,
                        });
                        edges.push(StoredMemoryEdge {
                            id: EdgeId::new(),
                            tenant_id: input.tenant_id,
                            scope_id: input.scope_id,
                            src_id: new_id,
                            dst_id: old_id,
                            kind: MemoryEdgeKind::Supersedes,
                        });
                    }
                    units.push(StoredMemoryUnit {
                        id: new_id,
                        tenant_id: input.tenant_id,
                        scope_id: input.scope_id,
                        kind: MemoryKind::Semantic,
                        state: UnitState::Active,
                        subject_key: Some(subject_key),
                        body: candidate.body,
                        trust_level: candidate.trust_level,
                        freshness_due: candidate.churn_class.as_deref() == Some("volatile"),
                        churn_class: candidate.churn_class,
                        actor_id: Some(candidate.actor_id),
                        source_kind: Some(candidate.source_kind),
                        source_episode_id: Some(input.episode_id),
                        source_resource_id: None,
                        deletion_generation: None,
                        contextual_chunks: candidate.contextual_chunks,
                        valid_from: candidate.valid_from,
                        valid_to: candidate.valid_to,
                        transaction_from: Some(CURRENT_VALIDITY_CUTOFF.to_string()),
                        transaction_to: None,
                        difficulty: None,
                        stability_days: None,
                        last_reinforced_at: None,
                        reinforcement_count: 0,
                    });
                    action
                }
            } else {
                if candidate.admission_hint == Some(AdmissionAction::Quarantine) {
                    units.push(StoredMemoryUnit {
                        id: UnitId::new(),
                        tenant_id: input.tenant_id,
                        scope_id: input.scope_id,
                        kind: MemoryKind::Belief,
                        state: UnitState::Quarantined,
                        subject_key: Some(subject_key),
                        body: candidate.body,
                        trust_level: candidate.trust_level,
                        freshness_due: false,
                        churn_class: candidate.churn_class,
                        actor_id: Some(candidate.actor_id),
                        source_kind: Some(candidate.source_kind),
                        source_episode_id: Some(input.episode_id),
                        source_resource_id: None,
                        deletion_generation: None,
                        contextual_chunks: candidate.contextual_chunks,
                        valid_from: candidate.valid_from,
                        valid_to: candidate.valid_to,
                        transaction_from: Some(CURRENT_VALIDITY_CUTOFF.to_string()),
                        transaction_to: None,
                        difficulty: None,
                        stability_days: None,
                        last_reinforced_at: None,
                        reinforcement_count: 0,
                    });
                    AdmissionAction::Quarantine
                } else {
                    units.push(StoredMemoryUnit {
                        id: UnitId::new(),
                        tenant_id: input.tenant_id,
                        scope_id: input.scope_id,
                        kind: MemoryKind::Belief,
                        state: UnitState::Candidate,
                        subject_key: Some(subject_key),
                        body: candidate.body,
                        trust_level: candidate.trust_level,
                        freshness_due: candidate.churn_class.as_deref() == Some("volatile"),
                        churn_class: candidate.churn_class,
                        actor_id: Some(candidate.actor_id),
                        source_kind: Some(candidate.source_kind),
                        source_episode_id: Some(input.episode_id),
                        source_resource_id: None,
                        deletion_generation: None,
                        contextual_chunks: candidate.contextual_chunks,
                        valid_from: candidate.valid_from,
                        valid_to: candidate.valid_to,
                        transaction_from: Some(CURRENT_VALIDITY_CUTOFF.to_string()),
                        transaction_to: None,
                        difficulty: None,
                        stability_days: None,
                        last_reinforced_at: None,
                        reinforcement_count: 0,
                    });
                    AdmissionAction::Append
                }
            }
        };
        state
            .memory_edges
            .entry(input.tenant_id)
            .or_default()
            .extend(edges);
        actions.push(action);
    }

    let trace = ReflectTrace {
        tenant_id: input.tenant_id,
        scope_id: input.scope_id,
        job_id: input.job_id,
        episode_id: input.episode_id,
        compiler_version: input.compiler_version,
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
    state
        .reflect_traces
        .entry(input.tenant_id)
        .or_default()
        .push(trace.clone());
    Ok(trace)
}

fn derive_subject_key(subject: Option<&str>, predicate: Option<&str>) -> Option<String> {
    let subject = subject.map(normalize_component)?;
    let predicate = predicate.map(normalize_component)?;
    if subject.is_empty() || predicate.is_empty() {
        return None;
    }
    Some(format!("{}:{}", subject.replace(' ', "_"), predicate))
}

fn is_independent_source(
    existing: &StoredMemoryUnit,
    candidate: &memphant_types::ReflectCandidate,
) -> bool {
    existing.actor_id != Some(candidate.actor_id)
        && existing.source_kind.as_deref() != Some(candidate.source_kind.as_str())
}

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use memphant_types::{
    AdmissionAction, CorrectRequest, CorrectResult, DedupOutcome, EdgeId, EpisodeId, ForgetRequest,
    ForgetResult, JobId, MarkRequest, MarkResult, MemoryEdgeKind, MemoryKind, NewEpisode,
    NewMemoryEdge, NewMemoryUnit, QueuedReflectJob, RecallCandidateTrace, RecallChannel,
    RecallCitation, RecallContextItem, RecallDropReason, RecallDroppedItem, RecallPolicyFilter,
    RecallRequest, RecallResponse, ReflectInput, ReflectJob, ReflectJobKind, ReflectStageFact,
    ReflectTrace, RetainInput, RetainOutcome, RetainRequest, RetainResourceOutcome,
    RetainResourceRequest, RetainResult, RetrievalTrace, ReviewEvent, ScopeId, StoredEpisode,
    StoredMemoryEdge, StoredMemoryUnit, StoredResource, TenantId, TraceId, TrustLevel, UnitId,
    UnitState,
};
use memphant_types::{NewResource, ResourceExtractorState, ResourceId};

const CURRENT_VALIDITY_CUTOFF: &str = "2026-07-03T00:00:00Z";

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
    state
        .review_events
        .entry(request.tenant_id)
        .or_default()
        .push(ReviewEvent {
            tenant_id: request.tenant_id,
            trace_id: request.trace_id,
            caller_id: request.caller_id,
            used_ids: request.used_ids,
            outcome: request.outcome,
        });

    Ok(MarkResult {
        accepted: true,
        trace_id: request.trace_id,
    })
}

pub async fn recall(
    store: &InMemoryStore,
    request: RecallRequest,
) -> Result<RecallResponse, CoreError> {
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
            engine_version: request.engine_version,
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
            abstention_signal: true,
            latency_ms: 0,
            token_estimate: 0,
            cost_micros: 0,
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
    let mut dropped_items = trace_filter_drops(&tenant_units, &request);
    let scope_units = tenant_units
        .iter()
        .filter(|unit| request.allowed_scope_ids.contains(&unit.scope_id))
        .count();
    let filter_selectivity = Some(scope_units as f32 / all_units as f32);
    let query_tokens = tokenize(&request.query);
    let mut candidates_by_unit: HashMap<UnitId, CandidateAccumulator> = HashMap::new();
    let mut candidate_traces = Vec::new();

    for channel in [
        RecallChannel::Exact,
        RecallChannel::Lexical,
        RecallChannel::Vector,
        RecallChannel::Temporal,
        RecallChannel::Edge,
    ] {
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
                    channels: vec![(channel, channel_rank, score)],
                });
            candidate_traces.push(RecallCandidateTrace {
                unit_id: unit.id,
                channel,
                channel_rank,
                channel_score: score,
                fused_rank: None,
                fused_score: None,
                trust_level: unit.trust_level,
                state: unit.state,
                discard_reason: None,
            });
        }
    }

    let mut fused: Vec<_> = candidates_by_unit.into_values().collect();
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

    let mut token_estimate = 0;
    let mut items = Vec::new();
    for candidate in fused.into_iter().take(request.k) {
        let unit_tokens = candidate.unit.body.split_whitespace().count();
        if token_estimate + unit_tokens > request.budget_tokens {
            dropped_items.push(RecallDroppedItem {
                unit_id: candidate.unit.id,
                reason: RecallDropReason::Budget,
            });
            continue;
        }
        token_estimate += unit_tokens;
        let suppression_labels = suppression_labels_for(&candidate.unit, &tenant_edges);
        let matched_contextual_chunk = contextual_chunk_score(&candidate.unit, &query_tokens) > 0.0;
        items.push(RecallContextItem {
            unit_id: candidate.unit.id,
            body: candidate.unit.body,
            kind: candidate.unit.kind,
            inclusion_reason: if matched_contextual_chunk {
                "contextual_chunk".to_string()
            } else {
                "fused_top_k".to_string()
            },
            citation_episode_id: candidate.unit.source_episode_id,
            citation_resource_id: candidate.unit.source_resource_id,
            suppression_labels,
        });
    }

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
    let trace_id = TraceId::new();
    let abstention = items.is_empty();
    let trace = RetrievalTrace {
        id: trace_id,
        tenant_id: request.tenant_id,
        scope_id: request.scope_id,
        actor_id: request.actor_id,
        query_hash: hash_query(&request.query),
        engine_version: request.engine_version,
        feature_flags: vec![
            "entity_exact_enabled".to_string(),
            "fts_enabled".to_string(),
            "vector_enabled".to_string(),
            "temporal_enabled".to_string(),
            "contextual_chunks_enabled".to_string(),
            "context_packing_abstention_enabled".to_string(),
        ],
        channel_runs: recall_stage_facts(),
        candidates: candidate_traces,
        policy_filters: Vec::new(),
        context_items: items.clone(),
        dropped_items,
        citations: citations.clone(),
        filter_selectivity,
        iterative_scan_depth: Some(1),
        consolidation_lag_ms: 0,
        weight_vector_id: "default".to_string(),
        mode_requested: request.mode,
        mode_executed: request.mode,
        escalation_reason: "none".to_string(),
        abstention_signal: abstention,
        latency_ms: 0,
        token_estimate,
        cost_micros: 0,
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

#[derive(Clone)]
struct CandidateAccumulator {
    unit: StoredMemoryUnit,
    fused_score: f32,
    channels: Vec<(RecallChannel, usize, f32)>,
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
        .filter(|unit| recallable(unit, request.include_beliefs, &request.query))
        .filter_map(|unit| {
            let score = match channel {
                RecallChannel::Exact => exact_score(unit, query_tokens),
                RecallChannel::Lexical => lexical_score(unit, query_tokens),
                RecallChannel::Vector => vector_score(unit, query_tokens),
                RecallChannel::Temporal => temporal_score(unit, &request.query),
                RecallChannel::Edge => edge_score(unit, units, edges, query_tokens),
            };
            (score > 0.0).then(|| (unit.clone(), score))
        })
        .collect()
}

fn edge_score(
    unit: &StoredMemoryUnit,
    units: &[StoredMemoryUnit],
    edges: &[StoredMemoryEdge],
    query_tokens: &[String],
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
                recallable(candidate, true, "")
                    && (lexical_score(candidate, query_tokens) > 0.0
                        || exact_score(candidate, query_tokens) > 0.0)
            })
    });
    if related_match { 1.0 } else { 0.0 }
}

fn suppression_labels_for(unit: &StoredMemoryUnit, edges: &[StoredMemoryEdge]) -> Vec<String> {
    if edges.iter().any(|edge| {
        edge.kind == MemoryEdgeKind::Contradicts
            && (edge.src_id == unit.id || edge.dst_id == unit.id)
    }) {
        vec!["unresolved_contradiction".to_string()]
    } else {
        Vec::new()
    }
}

fn recallable(unit: &StoredMemoryUnit, include_beliefs: bool, query: &str) -> bool {
    if unit.deletion_generation.is_some() {
        return false;
    }
    if unit.transaction_to.is_some() || !valid_for_query(unit, query) {
        return false;
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
        .filter(|token| query_tokens.contains(token))
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
        .filter(|token| query_tokens.contains(token))
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
        .filter(|token| query_tokens.contains(token))
        .collect::<std::collections::HashSet<_>>()
        .len();
    if union == 0 {
        0.0
    } else {
        intersection as f32 / union as f32
    }
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
    }
}

fn recall_stage_facts() -> Vec<ReflectStageFact> {
    [
        "stage0_policy",
        "exact",
        "lexical",
        "vector",
        "temporal",
        "edge",
        "fusion",
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

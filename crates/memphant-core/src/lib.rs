use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use memphant_types::{
    AdmissionAction, DedupOutcome, EdgeId, EpisodeId, JobId, MemoryEdgeKind, MemoryKind,
    NewEpisode, NewMemoryUnit, QueuedReflectJob, ReflectInput, ReflectJob, ReflectJobKind,
    ReflectStageFact, ReflectTrace, RetainInput, RetainOutcome, RetainRequest, RetainResult,
    StoredEpisode, StoredMemoryEdge, StoredMemoryUnit, TenantId, TrustLevel, UnitId, UnitState,
};

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
    memory_units: HashMap<TenantId, Vec<StoredMemoryUnit>>,
    memory_edges: HashMap<TenantId, Vec<StoredMemoryEdge>>,
    reflect_jobs: HashMap<TenantId, Vec<QueuedReflectJob>>,
    reflect_traces: HashMap<TenantId, Vec<ReflectTrace>>,
}

#[derive(Default)]
pub struct InMemoryTxn {
    episodes: Vec<StoredEpisode>,
    episode_observation_updates: Vec<(TenantId, EpisodeId)>,
    memory_units: Vec<StoredMemoryUnit>,
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
            .filter(|unit| unit.kind == MemoryKind::Belief)
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
        for unit in tx.memory_units {
            state
                .memory_units
                .entry(unit.tenant_id)
                .or_default()
                .push(unit);
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
            churn_class: None,
            freshness_due: false,
            actor_id: None,
            source_kind: None,
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
                episode_id: outcome.episode_id,
                kind: ReflectJobKind::ReflectEpisode,
                compiler_version: request.compiler_version,
            },
        )
        .await?;
    store.commit(tx).await?;
    Ok(outcome)
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
                if let Some(existing_index) = units.iter().position(|unit| {
                    unit.scope_id == input.scope_id
                        && unit.subject_key.as_deref() == Some(subject_key.as_str())
                        && unit.state == UnitState::Active
                        && unit.kind == MemoryKind::Semantic
                }) {
                    action = AdmissionAction::Supersede;
                    let old_id = units[existing_index].id;
                    units[existing_index].state = UnitState::Superseded;
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
                });
                action
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
                });
                AdmissionAction::Append
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

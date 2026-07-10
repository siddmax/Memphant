//! Runtime wiring shared by the server, worker, MCP and CLI binaries:
//! `AnyStore` (env-selected store backend behind the non-dyn-safe AFIT
//! `MemoryStore` trait), `MemoryService` construction, and the embedding
//! provider seam (Noop today; fastembed behind a feature later).

use std::sync::Arc;

use memphant_core::service::MemoryService;
use memphant_core::{
    ApiKeyRow, CompiledWrite, CorrectOutcome, CorrectionWrite, EmbeddingProvider, EmbeddingRow,
    ForgetOutcome, ForgetWrite, InMemoryStore, InMemoryTxn, JobFilter, MemoryStore, NoopEmbedding,
    ReflectJobRow, ReviewEventRow, ScopePage, StoreError, SystemClock,
};
use memphant_store_postgres::{PgStore, PgTxn};
use memphant_types::{
    EpisodeId, JobId, MemoryKind, NewEpisode, NewMemoryEdge, NewMemoryUnit, NewResource,
    ReflectJob, ReflectTrace, ResourceId, RetainOutcome, RetrievalTrace, ScopeId, StoredEpisode,
    StoredMemoryEdge, StoredMemoryUnit, StoredResource, TenantId, TraceId, UnitId,
};

pub use memphant_store_postgres::PgStore as Postgres;

/// The env-selected store: `MemoryStore` is AFIT (not object-safe), so the
/// binaries dispatch statically through this enum.
#[derive(Clone)]
pub enum AnyStore {
    Mem(InMemoryStore),
    Pg(PgStore),
}

pub enum AnyTxn {
    Mem(InMemoryTxn),
    Pg(PgTxn),
}

impl AnyStore {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Mem(_) => "memory",
            Self::Pg(_) => "postgres",
        }
    }

    pub fn as_pg(&self) -> Option<&PgStore> {
        match self {
            Self::Pg(store) => Some(store),
            Self::Mem(_) => None,
        }
    }
}

/// Builds the store from `DATABASE_URL`: present → `PgStore::connect` + ping
/// (fails loudly), absent → EPHEMERAL in-memory store with a loud warning.
pub async fn build_store() -> Result<AnyStore, StoreError> {
    match std::env::var("DATABASE_URL") {
        Ok(url) if !url.trim().is_empty() => {
            let store = PgStore::connect(&url).await?;
            Ok(AnyStore::Pg(store))
        }
        _ => {
            eprintln!("memphant: EPHEMERAL in-memory store — set DATABASE_URL for durability");
            Ok(AnyStore::Mem(InMemoryStore::default()))
        }
    }
}

/// The embedding provider seam. Noop today (vector channel traced as
/// disabled); a fastembed-backed provider lands behind a feature flag.
pub fn build_embedder() -> Arc<dyn EmbeddingProvider> {
    Arc::new(NoopEmbedding)
}

/// Standard `MemoryService` wiring: injected system clock + embedder seam.
pub fn build_service(store: AnyStore) -> MemoryService<AnyStore> {
    MemoryService::new(Arc::new(store), Arc::new(SystemClock), build_embedder())
}

fn txn_mismatch<T>() -> Result<T, StoreError> {
    Err(StoreError::Backend(
        "transaction/store backend mismatch".to_string(),
    ))
}

macro_rules! delegate {
    ($self:ident, $store:ident => $body:expr) => {
        match $self {
            AnyStore::Mem($store) => $body,
            AnyStore::Pg($store) => $body,
        }
    };
}

impl MemoryStore for AnyStore {
    type Txn = AnyTxn;

    async fn begin(&self) -> Self::Txn {
        match self {
            Self::Mem(store) => AnyTxn::Mem(store.begin().await),
            Self::Pg(store) => AnyTxn::Pg(store.begin().await),
        }
    }

    async fn commit(&self, tx: Self::Txn) -> Result<(), StoreError> {
        match (self, tx) {
            (Self::Mem(store), AnyTxn::Mem(tx)) => store.commit(tx).await,
            (Self::Pg(store), AnyTxn::Pg(tx)) => store.commit(tx).await,
            _ => txn_mismatch(),
        }
    }

    async fn stage_episode(
        &self,
        tx: &mut Self::Txn,
        episode: NewEpisode,
    ) -> Result<RetainOutcome, StoreError> {
        match (self, tx) {
            (Self::Mem(store), AnyTxn::Mem(tx)) => store.stage_episode(tx, episode).await,
            (Self::Pg(store), AnyTxn::Pg(tx)) => store.stage_episode(tx, episode).await,
            _ => txn_mismatch(),
        }
    }

    async fn stage_memory_unit(
        &self,
        tx: &mut Self::Txn,
        unit: NewMemoryUnit,
    ) -> Result<UnitId, StoreError> {
        match (self, tx) {
            (Self::Mem(store), AnyTxn::Mem(tx)) => store.stage_memory_unit(tx, unit).await,
            (Self::Pg(store), AnyTxn::Pg(tx)) => store.stage_memory_unit(tx, unit).await,
            _ => txn_mismatch(),
        }
    }

    async fn stage_resource(
        &self,
        tx: &mut Self::Txn,
        resource: NewResource,
    ) -> Result<ResourceId, StoreError> {
        match (self, tx) {
            (Self::Mem(store), AnyTxn::Mem(tx)) => store.stage_resource(tx, resource).await,
            (Self::Pg(store), AnyTxn::Pg(tx)) => store.stage_resource(tx, resource).await,
            _ => txn_mismatch(),
        }
    }

    async fn stage_memory_edge(
        &self,
        tx: &mut Self::Txn,
        edge: NewMemoryEdge,
    ) -> Result<memphant_types::EdgeId, StoreError> {
        match (self, tx) {
            (Self::Mem(store), AnyTxn::Mem(tx)) => store.stage_memory_edge(tx, edge).await,
            (Self::Pg(store), AnyTxn::Pg(tx)) => store.stage_memory_edge(tx, edge).await,
            _ => txn_mismatch(),
        }
    }

    async fn enqueue_reflect(
        &self,
        tx: &mut Self::Txn,
        job: ReflectJob,
    ) -> Result<JobId, StoreError> {
        match (self, tx) {
            (Self::Mem(store), AnyTxn::Mem(tx)) => store.enqueue_reflect(tx, job).await,
            (Self::Pg(store), AnyTxn::Pg(tx)) => store.enqueue_reflect(tx, job).await,
            _ => txn_mismatch(),
        }
    }

    async fn fetch_recall_candidates(
        &self,
        tenant: TenantId,
        scopes: &[ScopeId],
        kinds: &[MemoryKind],
        query_terms: &[String],
        query_vec: Option<&[f32]>,
        limit: usize,
    ) -> Result<Vec<StoredMemoryUnit>, StoreError> {
        delegate!(self, store => store
            .fetch_recall_candidates(tenant, scopes, kinds, query_terms, query_vec, limit)
            .await)
    }

    async fn fetch_units_by_ids(
        &self,
        tenant: TenantId,
        ids: &[UnitId],
    ) -> Result<Vec<StoredMemoryUnit>, StoreError> {
        delegate!(self, store => store.fetch_units_by_ids(tenant, ids).await)
    }

    async fn fetch_edges(
        &self,
        tenant: TenantId,
        unit_ids: &[UnitId],
    ) -> Result<Vec<StoredMemoryEdge>, StoreError> {
        delegate!(self, store => store.fetch_edges(tenant, unit_ids).await)
    }

    async fn fetch_review_events(
        &self,
        tenant: TenantId,
        unit_ids: &[UnitId],
    ) -> Result<Vec<ReviewEventRow>, StoreError> {
        delegate!(self, store => store.fetch_review_events(tenant, unit_ids).await)
    }

    async fn fetch_episodes_for_scope(
        &self,
        tenant: TenantId,
        scope: ScopeId,
        limit: usize,
    ) -> Result<Vec<StoredEpisode>, StoreError> {
        delegate!(self, store => store.fetch_episodes_for_scope(tenant, scope, limit).await)
    }

    async fn pending_job_count(
        &self,
        tenant: TenantId,
        scope: ScopeId,
    ) -> Result<usize, StoreError> {
        delegate!(self, store => store.pending_job_count(tenant, scope).await)
    }

    async fn fetch_episode(
        &self,
        tenant: TenantId,
        id: EpisodeId,
    ) -> Result<Option<StoredEpisode>, StoreError> {
        delegate!(self, store => store.fetch_episode(tenant, id).await)
    }

    async fn fetch_resource(
        &self,
        tenant: TenantId,
        id: ResourceId,
    ) -> Result<Option<StoredResource>, StoreError> {
        delegate!(self, store => store.fetch_resource(tenant, id).await)
    }

    async fn apply_correction(
        &self,
        tenant: TenantId,
        correction: CorrectionWrite,
    ) -> Result<CorrectOutcome, StoreError> {
        delegate!(self, store => store.apply_correction(tenant, correction).await)
    }

    async fn apply_forget(
        &self,
        tenant: TenantId,
        forget: ForgetWrite,
    ) -> Result<ForgetOutcome, StoreError> {
        delegate!(self, store => store.apply_forget(tenant, forget).await)
    }

    async fn record_review_events(
        &self,
        tenant: TenantId,
        events: Vec<ReviewEventRow>,
    ) -> Result<(), StoreError> {
        delegate!(self, store => store.record_review_events(tenant, events).await)
    }

    async fn store_trace(&self, tenant: TenantId, trace: RetrievalTrace) -> Result<(), StoreError> {
        delegate!(self, store => store.store_trace(tenant, trace).await)
    }

    async fn trace_by_id(
        &self,
        tenant: TenantId,
        id: TraceId,
    ) -> Result<Option<RetrievalTrace>, StoreError> {
        delegate!(self, store => store.trace_by_id(tenant, id).await)
    }

    async fn scope_memory_page(
        &self,
        tenant: TenantId,
        scope: ScopeId,
        cursor: Option<UnitId>,
        limit: usize,
    ) -> Result<ScopePage, StoreError> {
        delegate!(self, store => store.scope_memory_page(tenant, scope, cursor, limit).await)
    }

    async fn claim_reflect_jobs(
        &self,
        filter: JobFilter,
        limit: usize,
    ) -> Result<Vec<ReflectJobRow>, StoreError> {
        delegate!(self, store => store.claim_reflect_jobs(filter, limit).await)
    }

    async fn complete_reflect_job(&self, id: JobId) -> Result<(), StoreError> {
        delegate!(self, store => store.complete_reflect_job(id).await)
    }

    async fn persist_compiled_units(
        &self,
        tenant: TenantId,
        write: CompiledWrite,
    ) -> Result<(), StoreError> {
        delegate!(self, store => store.persist_compiled_units(tenant, write).await)
    }

    async fn fetch_reflect_trace(
        &self,
        tenant: TenantId,
        job_id: JobId,
        compiler_version: &str,
    ) -> Result<Option<ReflectTrace>, StoreError> {
        delegate!(self, store => store.fetch_reflect_trace(tenant, job_id, compiler_version).await)
    }

    async fn upsert_embeddings(
        &self,
        tenant: TenantId,
        rows: Vec<EmbeddingRow>,
    ) -> Result<(), StoreError> {
        delegate!(self, store => store.upsert_embeddings(tenant, rows).await)
    }

    async fn lookup_api_key(&self, key_hash: &str) -> Result<Option<ApiKeyRow>, StoreError> {
        delegate!(self, store => store.lookup_api_key(key_hash).await)
    }

    async fn ping(&self) -> Result<(), StoreError> {
        delegate!(self, store => store.ping().await)
    }

    async fn dead_letter_count(&self) -> Result<u64, StoreError> {
        delegate!(self, store => store.dead_letter_count().await)
    }
}

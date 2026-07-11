//! Runtime wiring shared by the server, worker, MCP and CLI binaries:
//! `AnyStore` (env-selected store backend behind the non-dyn-safe AFIT
//! `MemoryStore` trait), `MemoryService` construction, and the embedding
//! provider seam. Binaries built with the `fastembed` feature (the shipped
//! server/worker default) embed with local bge-small-en-v1.5 unless
//! `MEMPHANT_EMBEDDINGS=off`; feature-less binaries fall back to Noop.

use std::sync::Arc;

use memphant_core::service::MemoryService;
use memphant_core::{
    ApiKeyRow, CompiledWrite, CorrectOutcome, CorrectionWrite, EmbeddingProfileRow,
    EmbeddingProvider, EmbeddingRow, ForgetOutcome, ForgetWrite, InMemoryStore, InMemoryTxn,
    JobFilter, MemoryStore, NoopEmbedding, ReflectJobRow, ReviewEventRow, ScopePage, StoreError,
    SystemClock,
};
use memphant_store_postgres::{PgStore, PgTxn};
use memphant_types::{
    EpisodeId, JobId, MemoryKind, NewEpisode, NewMemoryEdge, NewMemoryUnit, NewResource,
    ReflectJob, ReflectTrace, ResourceId, RetainOutcome, RetrievalTrace, ScopeId, StoredEpisode,
    StoredMemoryEdge, StoredMemoryUnit, StoredResource, TenantId, TraceId, UnitId,
};
use uuid::Uuid;

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

#[cfg(feature = "fastembed")]
pub mod embeddings;

/// Which embedding provider a `MEMPHANT_EMBEDDINGS` setting selects, resolved
/// independently of the `fastembed` cargo feature so it is unit-testable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EmbedderChoice {
    /// Explicit opt-out (`off`/`noop`): the vector channel is honestly disabled.
    Noop,
    /// Default (unset/empty): fastembed when the feature is compiled in, else a
    /// graceful Noop fallback (e.g. a binary built without the feature).
    DefaultFastembed,
    /// Explicit `fastembed`: a build without the feature is a loud error.
    ForcedFastembed,
}

/// Maps a `MEMPHANT_EMBEDDINGS` value onto the provider choice. Pure: the
/// feature gating happens at construction in `build_embedder`.
fn embedder_choice(setting: Option<&str>) -> EmbedderChoice {
    match setting {
        Some("off") | Some("noop") => EmbedderChoice::Noop,
        Some("fastembed") => EmbedderChoice::ForcedFastembed,
        None | Some("") => EmbedderChoice::DefaultFastembed,
        Some(other) => panic!("unknown MEMPHANT_EMBEDDINGS provider: {other}"),
    }
}

/// The embedding provider seam, selected by `MEMPHANT_EMBEDDINGS`:
/// - unset/empty (DEFAULT) → local bge-small-en-v1.5 when built with the
///   `fastembed` feature (the shipped server/worker default); a binary built
///   without the feature falls back to `NoopEmbedding` with a loud warning.
/// - `off`/`noop` → `NoopEmbedding` (tests/CI; vector channel traced disabled)
/// - `fastembed` → force bge-small-en-v1.5; a build lacking the feature is a
///   loud panic rather than a silent degrade.
pub fn build_embedder() -> Arc<dyn EmbeddingProvider> {
    match embedder_choice(std::env::var("MEMPHANT_EMBEDDINGS").ok().as_deref()) {
        EmbedderChoice::Noop => Arc::new(NoopEmbedding),
        EmbedderChoice::DefaultFastembed => fastembed_or(|| {
            eprintln!(
                "memphant: fastembed feature not compiled in — vector channel DISABLED \
                 (build with --features fastembed, or set MEMPHANT_EMBEDDINGS=off to silence)"
            );
            Arc::new(NoopEmbedding)
        }),
        EmbedderChoice::ForcedFastembed => fastembed_or(|| {
            panic!(
                "MEMPHANT_EMBEDDINGS=fastembed requires a binary built with --features fastembed"
            )
        }),
    }
}

/// Constructs the fastembed provider when the feature is present; otherwise
/// runs `fallback` (a graceful Noop for the default, a panic for the forced
/// path). The `fallback` closure is unused in the fastembed build.
#[cfg(feature = "fastembed")]
fn fastembed_or(
    _fallback: impl FnOnce() -> Arc<dyn EmbeddingProvider>,
) -> Arc<dyn EmbeddingProvider> {
    Arc::new(
        embeddings::FastEmbedProvider::new()
            .expect("fastembed model initialization failed (bge-small-en-v1.5)"),
    )
}

#[cfg(not(feature = "fastembed"))]
fn fastembed_or(
    fallback: impl FnOnce() -> Arc<dyn EmbeddingProvider>,
) -> Arc<dyn EmbeddingProvider> {
    fallback()
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
        limit: usize,
    ) -> Result<Vec<StoredMemoryUnit>, StoreError> {
        delegate!(self, store => store
            .fetch_recall_candidates(tenant, scopes, kinds, query_terms, limit)
            .await)
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
        delegate!(self, store => store
            .fetch_vector_candidates(tenant, scopes, kinds, query_vec, profile_id, limit)
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

    async fn upsert_embedding_profile(
        &self,
        tenant: TenantId,
        profile: EmbeddingProfileRow,
    ) -> Result<(), StoreError> {
        delegate!(self, store => store.upsert_embedding_profile(tenant, profile).await)
    }

    async fn fetch_embeddings(
        &self,
        tenant: TenantId,
        unit_ids: &[UnitId],
    ) -> Result<Vec<EmbeddingRow>, StoreError> {
        delegate!(self, store => store.fetch_embeddings(tenant, unit_ids).await)
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

#[cfg(test)]
mod tests {
    use super::{EmbedderChoice, embedder_choice};

    #[test]
    fn env_default_selects_fastembed() {
        // The shipped server/worker embed by default: unset/empty → fastembed.
        assert_eq!(embedder_choice(None), EmbedderChoice::DefaultFastembed);
        assert_eq!(embedder_choice(Some("")), EmbedderChoice::DefaultFastembed);
    }

    #[test]
    fn env_opt_out_selects_noop() {
        // `off` (and the legacy `noop` alias) disable the vector channel for
        // tests/CI without a model load.
        assert_eq!(embedder_choice(Some("off")), EmbedderChoice::Noop);
        assert_eq!(embedder_choice(Some("noop")), EmbedderChoice::Noop);
    }

    #[test]
    fn env_explicit_fastembed_is_forced() {
        // An explicit request must fail loudly if the feature is absent, so it
        // resolves to the forced variant rather than the graceful default.
        assert_eq!(
            embedder_choice(Some("fastembed")),
            EmbedderChoice::ForcedFastembed
        );
    }

    #[test]
    #[should_panic(expected = "unknown MEMPHANT_EMBEDDINGS provider")]
    fn env_unknown_provider_panics() {
        let _ = embedder_choice(Some("word2vec"));
    }
}

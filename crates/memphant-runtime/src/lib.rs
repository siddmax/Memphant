//! Runtime wiring shared by the server, worker, MCP and CLI binaries:
//! `AnyStore` (env-selected store backend behind the non-dyn-safe AFIT
//! `MemoryStore` trait), `MemoryService` construction, and the embedding
//! provider seam. Binaries built with the `fastembed` feature (the shipped
//! server/worker default) embed with local bge-small-en-v1.5 unless
//! `MEMPHANT_EMBEDDINGS=off`; feature-less binaries fall back to Noop.

use std::sync::Arc;

use memphant_core::service::MemoryService;
use memphant_core::{
    ApiKeyRow, CompiledWrite, CorrectOutcome, CorrectionWrite, EmbedError, EmbeddingProfileRow,
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

pub mod api_embeddings;

/// Single source of truth mapping an embedder selector id to a provider, shared
/// by the runtime `MEMPHANT_EMBEDDINGS` env var (via [`build_embedder`]) AND
/// the eval bench `--embed-model` flag — so the docs-gate harness (T3) can swap
/// any arm purely by that id, no rebuild. Local (fastembed/qwen3) arms are
/// cargo-feature gated and yield a build-instruction error when absent; the six
/// API arms are always compiled and yield a missing-key error when their env
/// var is unset. Construction is cheap for the API arms (reads the key, builds a
/// pooled agent — no network round-trip).
///
/// Accepted ids:
/// - `off` | `noop` → [`NoopEmbedding`] (vector channel honestly disabled)
/// - `fastembed` → the legacy default local arm (bge-small-en-v1.5)
/// - `small` | `base` | `modernbert` | `gemma` → the T1 fastembed arms
/// - `qwen3` → the T1b Qwen3-Embedding-0.6B arm
/// - `voyage-4` | `voyage-4-lite` | `voyage-4-large` | `voyage-code-3`
///   | `voyage-context-4` | `gemini-embedding-001`
///   | `openai-text-embedding-3-small` → the T2 API arms
pub fn embedder_from_id(id: &str) -> Result<Arc<dyn EmbeddingProvider>, String> {
    use api_embeddings::{
        GeminiEmbedding, OpenAiEmbedding, VoyageContextualizedEmbedding, VoyageEmbedding,
        VoyageModel,
    };
    match id {
        "off" | "noop" => Ok(Arc::new(NoopEmbedding)),
        "fastembed" | "small" | "base" | "modernbert" | "gemma" => fastembed_arm(id),
        "qwen3" => qwen3_arm(),
        "voyage-4" => api(VoyageEmbedding::new(VoyageModel::Voyage4)),
        "voyage-4-lite" => api(VoyageEmbedding::new(VoyageModel::Voyage4Lite)),
        "voyage-4-large" => api(VoyageEmbedding::new(VoyageModel::Voyage4Large)),
        "voyage-code-3" => api(VoyageEmbedding::new(VoyageModel::VoyageCode3)),
        "voyage-context-4" => api(VoyageContextualizedEmbedding::new()),
        "gemini-embedding-001" => api(GeminiEmbedding::new()),
        "openai-text-embedding-3-small" => api(OpenAiEmbedding::new()),
        other => Err(format!(
            "unknown embedder id: {other} (accepted: off, noop, fastembed, small, base, \
             modernbert, gemma, qwen3, voyage-4, voyage-4-lite, voyage-4-large, voyage-code-3, \
             voyage-context-4, gemini-embedding-001, openai-text-embedding-3-small)"
        )),
    }
}

/// Wraps an API provider construction (`Result<P, EmbedError>`) into the shared
/// `Result<Arc<dyn EmbeddingProvider>, String>` grammar return.
fn api<P>(provider: Result<P, EmbedError>) -> Result<Arc<dyn EmbeddingProvider>, String>
where
    P: EmbeddingProvider + 'static,
{
    provider
        .map(|provider| Arc::new(provider) as Arc<dyn EmbeddingProvider>)
        .map_err(|error| error.to_string())
}

/// The fastembed local arms (`fastembed`/`small`/`base`/`modernbert`/`gemma`),
/// when the feature is compiled in.
#[cfg(feature = "fastembed")]
fn fastembed_arm(id: &str) -> Result<Arc<dyn EmbeddingProvider>, String> {
    let model = match id {
        "fastembed" | "small" => embeddings::FastEmbedModel::BgeSmallEnV15,
        "base" => embeddings::FastEmbedModel::BgeBaseEnV15,
        "modernbert" => embeddings::FastEmbedModel::ModernBertEmbedLarge,
        "gemma" => embeddings::FastEmbedModel::EmbeddingGemma300M,
        other => unreachable!("fastembed_arm dispatched a non-fastembed id: {other}"),
    };
    embeddings::FastEmbedProvider::with_model(model)
        .map(|provider| Arc::new(provider) as Arc<dyn EmbeddingProvider>)
        .map_err(|error| format!("fastembed initialization failed ({id}): {error}"))
}

#[cfg(not(feature = "fastembed"))]
fn fastembed_arm(id: &str) -> Result<Arc<dyn EmbeddingProvider>, String> {
    Err(format!(
        "embedder '{id}' requires a binary built with --features fastembed"
    ))
}

/// The T1b Qwen3-Embedding-0.6B arm, when the `qwen3` feature is compiled in.
#[cfg(feature = "qwen3")]
fn qwen3_arm() -> Result<Arc<dyn EmbeddingProvider>, String> {
    embeddings::Qwen3Provider::new()
        .map(|provider| Arc::new(provider) as Arc<dyn EmbeddingProvider>)
        .map_err(|error| format!("qwen3 initialization failed: {error}"))
}

#[cfg(not(feature = "qwen3"))]
fn qwen3_arm() -> Result<Arc<dyn EmbeddingProvider>, String> {
    Err(
        "embedder 'qwen3' requires a binary built with --features qwen3 \
         (Qwen3-Embedding-0.6B via fastembed's candle backend)"
            .to_string(),
    )
}

/// The embedding provider seam, selected by `MEMPHANT_EMBEDDINGS`:
/// - unset/empty (DEFAULT) → local bge-small-en-v1.5 when built with the
///   `fastembed` feature (the shipped server/worker default); a binary built
///   without the feature falls back to `NoopEmbedding` with a loud warning.
/// - any explicit id → routed through [`embedder_from_id`]; a construction
///   failure (feature not compiled, API key unset, unknown id) is a loud panic
///   carrying the grammar's own message. This preserves the documented legacy
///   semantics: `off`/`noop` → Noop, `fastembed` → panic-if-feature-missing,
///   unknown value → panic listing the accepted values.
pub fn build_embedder() -> Arc<dyn EmbeddingProvider> {
    match std::env::var("MEMPHANT_EMBEDDINGS").ok().as_deref() {
        None | Some("") => default_embedder(),
        Some(id) => {
            embedder_from_id(id).unwrap_or_else(|error| panic!("MEMPHANT_EMBEDDINGS={id}: {error}"))
        }
    }
}

/// The DEFAULT (unset) path: local bge-small when the fastembed feature is
/// present, else a graceful `NoopEmbedding` with a loud warning (NOT a panic).
fn default_embedder() -> Arc<dyn EmbeddingProvider> {
    fastembed_or(|| {
        eprintln!(
            "memphant: fastembed feature not compiled in — vector channel DISABLED \
             (build with --features fastembed, or set MEMPHANT_EMBEDDINGS=off to silence)"
        );
        Arc::new(NoopEmbedding)
    })
}

/// Constructs the default fastembed provider (bge-small) when the feature is
/// present; otherwise runs `fallback`. The `fallback` closure is unused in the
/// fastembed build.
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

    async fn begin(&self) -> Result<Self::Txn, StoreError> {
        Ok(match self {
            Self::Mem(store) => AnyTxn::Mem(store.begin().await?),
            Self::Pg(store) => AnyTxn::Pg(store.begin().await?),
        })
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

    async fn fetch_scope_open_units(
        &self,
        tenant: TenantId,
        scope: ScopeId,
    ) -> Result<Vec<StoredMemoryUnit>, StoreError> {
        delegate!(self, store => store.fetch_scope_open_units(tenant, scope).await)
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

    async fn complete_reflect_job(&self, tenant: TenantId, id: JobId) -> Result<(), StoreError> {
        delegate!(self, store => store.complete_reflect_job(tenant, id).await)
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
    use super::embedder_from_id;
    use memphant_core::{EmbedError, EmbeddingProvider, embedding_profile_for};

    #[test]
    fn off_and_noop_construct_the_disabled_noop_provider() {
        // `off` (and the legacy `noop` alias) disable the vector channel for
        // tests/CI without a model load — dims 0 traces the channel disabled.
        for id in ["off", "noop"] {
            let provider = embedder_from_id(id).expect("noop construction");
            assert_eq!(provider.dimensions(), 0, "{id} must be the disabled Noop");
        }
    }

    #[test]
    fn grammar_recognizes_the_network_free_ids() {
        // Recognition = maps to a real branch, never the unknown-id error. Only
        // ids whose construction is network-free are exercised here: `off`/`noop`
        // (Noop) and the seven API arms (which only read a key + build a pooled
        // agent — no round-trip). The local fastembed/qwen3 arms are DELIBERATELY
        // excluded: constructing them downloads model weights, so their
        // recognition is asserted in `local_arm_ids_recognized_without_the_feature`
        // under a feature-off build instead.
        const NETWORK_FREE_IDS: [&str; 9] = [
            "off",
            "noop",
            "voyage-4",
            "voyage-4-lite",
            "voyage-4-large",
            "voyage-code-3",
            "voyage-context-4",
            "gemini-embedding-001",
            "openai-text-embedding-3-small",
        ];
        for id in NETWORK_FREE_IDS {
            if let Err(error) = embedder_from_id(id) {
                // API arms with an unset key error for a RECOGNIZED reason.
                assert!(
                    !error.contains("unknown embedder id"),
                    "id {id} must be recognized by the grammar: {error}"
                );
            }
        }
    }

    /// Without the fastembed feature the local arms construct nothing — they
    /// return a cheap build-instruction error — so recognition is provable here
    /// with zero model downloads. Cfg'd out under `--all-features` (where the
    /// feature is on and constructing them WOULD download weights); the arms are
    /// still structurally explicit match arms in [`embedder_from_id`].
    /// `Arc<dyn EmbeddingProvider>` isn't `Debug`, so `expect_err` (which needs
    /// `T: Debug` to format the Ok case) can't be used — match instead.
    fn expect_grammar_err(id: &str) -> String {
        match embedder_from_id(id) {
            Err(error) => error,
            Ok(_) => panic!("expected an error for id {id}"),
        }
    }

    #[cfg(not(feature = "fastembed"))]
    #[test]
    fn local_arm_ids_recognized_without_the_feature() {
        for id in ["fastembed", "small", "base", "modernbert", "gemma", "qwen3"] {
            let error = expect_grammar_err(id);
            assert!(
                !error.contains("unknown embedder id"),
                "id {id} must be recognized by the grammar: {error}"
            );
            assert!(
                error.contains("--features"),
                "recognized-but-uncompiled arm must name the missing feature: {error}"
            );
        }
    }

    #[test]
    fn unknown_id_error_lists_the_accepted_values() {
        let error = expect_grammar_err("word2vec");
        assert!(error.contains("unknown embedder id"), "{error}");
        // A representative from each family must appear in the accepted list.
        for expected in [
            "off",
            "fastembed",
            "qwen3",
            "voyage-context-4",
            "gemini-embedding-001",
            "openai-text-embedding-3-small",
        ] {
            assert!(
                error.contains(expected),
                "accepted list must name {expected}: {error}"
            );
        }
    }

    /// A pure identity stub reporting only `id()`+`dimensions()`, so
    /// `embedding_profile_for` can be exercised without constructing a real
    /// (feature- or key-gated) provider.
    struct IdDims(&'static str, usize);
    impl EmbeddingProvider for IdDims {
        fn embed(&self, _texts: &[String]) -> Result<Vec<Vec<f32>>, EmbedError> {
            Ok(Vec::new())
        }
        fn dimensions(&self) -> usize {
            self.1
        }
        fn id(&self) -> &str {
            self.0
        }
    }

    #[test]
    fn every_arm_derives_a_distinct_embedding_profile() {
        // The whole "coexist cleanly" claim, extended over the seven T2 API arms:
        // every arm keys a different profile id (hash of id+dims), so their
        // stored vectors never mix under `<=>` — even where dims coincide
        // (voyage arms + modernbert + qwen3 all 1024), the id disambiguates.
        use crate::api_embeddings::{
            GEMINI_DIMS, GEMINI_ID, OPENAI_DIMS, OPENAI_ID, VOYAGE_CONTEXT_ID, VOYAGE_DIMS,
            VoyageModel,
        };
        let arms = [
            // Seven API arms (id + live-pinned dims).
            IdDims(VoyageModel::Voyage4.id(), VOYAGE_DIMS),
            IdDims(VoyageModel::Voyage4Lite.id(), VOYAGE_DIMS),
            IdDims(VoyageModel::Voyage4Large.id(), VOYAGE_DIMS),
            IdDims(VoyageModel::VoyageCode3.id(), VOYAGE_DIMS),
            IdDims(VOYAGE_CONTEXT_ID, VOYAGE_DIMS),
            IdDims(GEMINI_ID, GEMINI_DIMS),
            IdDims(OPENAI_ID, OPENAI_DIMS),
            // Local arm identities (stable ids from T1/T1b), to prove the API
            // arms never collide with the fastembed/qwen3 arms or Noop.
            IdDims("fastembed:bge-small-en-v1.5", 384),
            IdDims("fastembed:bge-base-en-v1.5", 768),
            IdDims("fastembed:modernbert-embed-large", 1024),
            IdDims("fastembed:embeddinggemma-300m", 768),
            IdDims("fastembed:qwen3-embedding-0.6b", 1024),
            IdDims("noop", 0),
        ];
        let profiles: Vec<_> = arms
            .iter()
            .map(|arm| embedding_profile_for(arm as &dyn EmbeddingProvider))
            .collect();
        for (left_index, left) in profiles.iter().enumerate() {
            for (right_index, right) in profiles.iter().enumerate() {
                if left_index != right_index {
                    assert_ne!(
                        left.id, right.id,
                        "arms {} and {} must derive distinct profiles",
                        arms[left_index].0, arms[right_index].0
                    );
                }
            }
        }
    }
}

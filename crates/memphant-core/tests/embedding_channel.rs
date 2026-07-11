//! Embedding seam contract (Task 6): with a real (stub) provider, compiled
//! units get persisted embeddings and recall runs a genuinely scored `vector`
//! channel; with the Noop provider the channel stays honestly disabled.

use std::sync::Arc;

use memphant_core::service::MemoryService;
use memphant_core::{
    EmbeddingProvider, FixedClock, InMemoryStore, MemoryStore, NoopEmbedding, StubEmbedding,
    VECTOR_CANDIDATE_LIMIT, cosine_similarity, embedding_profile_for,
};
use memphant_types::{
    ActorId, RecallChannel, RecallHttpRequest, RetainEpisodeHttpRequest, ScopeId, TenantId,
    TrustLevel,
};

const CLOCK: FixedClock = FixedClock("2026-07-09T00:00:00Z");

fn stub_service(store: InMemoryStore) -> MemoryService<InMemoryStore> {
    MemoryService::new(
        Arc::new(store),
        Arc::new(CLOCK),
        Arc::new(StubEmbedding::default()),
    )
}

fn noop_service(store: InMemoryStore) -> MemoryService<InMemoryStore> {
    MemoryService::new(Arc::new(store), Arc::new(CLOCK), Arc::new(NoopEmbedding))
}

fn retain_request(
    tenant_id: TenantId,
    scope_id: ScopeId,
    actor_id: ActorId,
    body: &str,
) -> RetainEpisodeHttpRequest {
    RetainEpisodeHttpRequest {
        tenant_id,
        scope_id,
        actor_id,
        source_kind: "user".to_string(),
        source_trust: TrustLevel::TrustedUser,
        subject_hint: None,
        subject: None,
        predicate: None,
        body: Some(body.to_string()),
        resource: None,
        unit: None,
        compiler_version: None,
    }
}

fn recall_request(
    tenant_id: TenantId,
    scope_id: ScopeId,
    actor_id: ActorId,
    query: &str,
) -> RecallHttpRequest {
    RecallHttpRequest {
        tenant_id,
        scope_id,
        actor_id,
        allowed_scope_ids: None,
        query: query.to_string(),
        limit: None,
        budget_tokens: None,
        mode: None,
        include_beliefs: None,
        edge_expansion_enabled: None,
        context_packing_abstention_enabled: None,
        rerank_enabled: None,
        query_decomposition_enabled: None,
        procedure_recall_enabled: None,
        decay_enabled: None,
        include_trace: None,
    }
}

#[tokio::test]
async fn compile_persists_embeddings_under_seeded_profile() {
    let store = InMemoryStore::default();
    let service = stub_service(store.clone());
    let tenant = TenantId::new();
    let scope = ScopeId::new();
    let actor = ActorId::new();

    service
        .retain(
            tenant,
            retain_request(tenant, scope, actor, "Release region is Taipei."),
        )
        .await
        .expect("retain");
    service.reflect(tenant, scope, None).await.expect("reflect");

    let page = store
        .scope_memory_page(tenant, scope, None, 100)
        .await
        .expect("page");
    assert!(!page.items.is_empty(), "reflect compiled at least one unit");
    let unit_ids: Vec<_> = page.items.iter().map(|unit| unit.id).collect();
    let rows = store
        .fetch_embeddings(tenant, &unit_ids)
        .await
        .expect("embeddings");
    assert!(
        !rows.is_empty(),
        "compiled units have persisted embeddings when a real provider is configured"
    );
    let stub = StubEmbedding::default();
    let profile = embedding_profile_for(&stub);
    assert!(
        rows.iter().all(|row| {
            row.embedding_profile_id == profile.id && row.vec.len() == stub.dimensions()
        }),
        "embeddings are keyed to the provider's deterministic profile"
    );
    // The stored vector matches a fresh embedding of the same body (cosine 1).
    let expected = stub
        .embed(&[page.items[0].body.clone()])
        .expect("stub embed")
        .remove(0);
    let stored = rows
        .iter()
        .find(|row| row.memory_unit_id == page.items[0].id)
        .expect("row for first unit");
    assert!(cosine_similarity(&expected, &stored.vec) > 0.999);
}

#[tokio::test]
async fn vector_channel_scores_candidates_with_real_provider() {
    let store = InMemoryStore::default();
    let service = stub_service(store.clone());
    let tenant = TenantId::new();
    let scope = ScopeId::new();
    let actor = ActorId::new();

    service
        .retain(
            tenant,
            retain_request(tenant, scope, actor, "Release region is Taipei."),
        )
        .await
        .expect("retain");
    service.reflect(tenant, scope, None).await.expect("reflect");

    let response = service
        .recall(
            tenant,
            recall_request(tenant, scope, actor, "Release region is Taipei."),
        )
        .await
        .expect("recall");
    assert!(!response.items.is_empty(), "recall returns the unit");

    let trace = service
        .trace(tenant, response.trace_id)
        .await
        .expect("trace fetch")
        .expect("trace stored");
    let vector_candidates: Vec<_> = trace
        .candidates
        .iter()
        .filter(|candidate| candidate.channel == RecallChannel::Vector)
        .collect();
    assert!(
        !vector_candidates.is_empty(),
        "the vector channel produced scored candidates"
    );
    assert!(
        vector_candidates
            .iter()
            .all(|candidate| candidate.channel_score > 0.0),
        "vector candidates carry real cosine scores"
    );
    let vector_stage = trace
        .channel_runs
        .iter()
        .find(|stage| stage.stage == "vector")
        .expect("vector stage traced");
    assert_eq!(vector_stage.detail, "completed");
    assert!(
        trace.feature_flags.contains(&"vector_enabled".to_string()),
        "feature flags report the vector channel as enabled"
    );
}

#[tokio::test]
async fn in_memory_vector_candidates_return_cosine_distance() {
    // The extended store contract: `fetch_vector_candidates` returns
    // (unit, cosine DISTANCE) under the active profile — the in-memory analogue
    // of pgvector `<=>`, NOT a raw-vector fetch the caller must re-cosine.
    let store = InMemoryStore::default();
    let service = stub_service(store.clone());
    let tenant = TenantId::new();
    let scope = ScopeId::new();
    let actor = ActorId::new();

    service
        .retain(
            tenant,
            retain_request(tenant, scope, actor, "Release region is Taipei."),
        )
        .await
        .expect("retain");
    service.reflect(tenant, scope, None).await.expect("reflect");

    let stub = StubEmbedding::default();
    let profile = embedding_profile_for(&stub);
    let query_vec = stub
        .embed(&["Release region Osaka.".to_string()])
        .expect("embed query")
        .remove(0);

    let pairs = store
        .fetch_vector_candidates(
            tenant,
            &[scope],
            &[],
            &query_vec,
            profile.id,
            VECTOR_CANDIDATE_LIMIT,
        )
        .await
        .expect("vector candidates");
    assert_eq!(pairs.len(), 1, "the single embedded unit is returned once");
    let (unit, distance) = &pairs[0];

    let rows = store
        .fetch_embeddings(tenant, &[unit.id])
        .await
        .expect("embeddings");
    let stored = rows
        .iter()
        .find(|row| row.embedding_profile_id == profile.id)
        .expect("row under the active profile");
    let expected_distance = 1.0 - cosine_similarity(&query_vec, &stored.vec);
    assert!(
        (distance - expected_distance).abs() < 1e-6,
        "store returns cosine distance (1 - similarity); got {distance}, expected {expected_distance}"
    );
    // A vector NEARER the query has a SMALLER distance — direction sanity.
    assert!(*distance >= 0.0 && *distance <= 2.0);
}

#[tokio::test]
async fn vector_channel_score_is_one_minus_store_distance() {
    // The wiring contract: the recall vector channel's traced score is exactly
    // `1 - distance` where `distance` is what the store returned — not an
    // app-side recompute from raw vectors.
    let store = InMemoryStore::default();
    let service = stub_service(store.clone());
    let tenant = TenantId::new();
    let scope = ScopeId::new();
    let actor = ActorId::new();
    let query = "Release region Osaka.";

    service
        .retain(
            tenant,
            retain_request(tenant, scope, actor, "Release region is Taipei."),
        )
        .await
        .expect("retain");
    service.reflect(tenant, scope, None).await.expect("reflect");

    let stub = StubEmbedding::default();
    let profile = embedding_profile_for(&stub);
    let query_vec = stub
        .embed(&[query.to_string()])
        .expect("embed query")
        .remove(0);
    let pairs = store
        .fetch_vector_candidates(
            tenant,
            &[scope],
            &[],
            &query_vec,
            profile.id,
            VECTOR_CANDIDATE_LIMIT,
        )
        .await
        .expect("vector candidates");
    let unit_id = pairs[0].0.id;
    let distance = pairs[0].1;

    let response = service
        .recall(tenant, recall_request(tenant, scope, actor, query))
        .await
        .expect("recall");
    let trace = service
        .trace(tenant, response.trace_id)
        .await
        .expect("trace fetch")
        .expect("trace stored");
    let vector_candidate = trace
        .candidates
        .iter()
        .find(|candidate| {
            candidate.channel == RecallChannel::Vector && candidate.unit_id == unit_id
        })
        .expect("vector candidate for the embedded unit");
    assert!(
        (vector_candidate.channel_score - (1.0 - distance)).abs() < 1e-6,
        "vector channel score {} must equal 1 - store distance {}",
        vector_candidate.channel_score,
        1.0 - distance
    );
}

#[tokio::test]
async fn noop_provider_keeps_vector_channel_disabled() {
    let store = InMemoryStore::default();
    let service = noop_service(store.clone());
    let tenant = TenantId::new();
    let scope = ScopeId::new();
    let actor = ActorId::new();

    service
        .retain(
            tenant,
            retain_request(tenant, scope, actor, "Release region is Taipei."),
        )
        .await
        .expect("retain");
    service.reflect(tenant, scope, None).await.expect("reflect");

    let page = store
        .scope_memory_page(tenant, scope, None, 100)
        .await
        .expect("page");
    let unit_ids: Vec<_> = page.items.iter().map(|unit| unit.id).collect();
    let rows = store
        .fetch_embeddings(tenant, &unit_ids)
        .await
        .expect("embeddings");
    assert!(rows.is_empty(), "Noop provider persists no embeddings");

    let response = service
        .recall(
            tenant,
            recall_request(tenant, scope, actor, "Release region is Taipei."),
        )
        .await
        .expect("recall");
    let trace = service
        .trace(tenant, response.trace_id)
        .await
        .expect("trace fetch")
        .expect("trace stored");
    assert!(
        trace
            .candidates
            .iter()
            .all(|candidate| candidate.channel != RecallChannel::Vector),
        "no fake vector candidates without a real provider"
    );
    let vector_stage = trace
        .channel_runs
        .iter()
        .find(|stage| stage.stage == "vector")
        .expect("vector stage traced");
    assert_eq!(vector_stage.detail, "disabled");
    assert!(trace.feature_flags.contains(&"vector_disabled".to_string()));
}

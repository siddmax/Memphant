//! W3 candidate-pool knob: the construction-time `with_candidate_pool_size`
//! service option widens the recall vector-channel KNN fan-out. A larger pool
//! admits a vector candidate that the smaller (default) pool truncated away —
//! the widened pool the W8 cross-encoder rerank arm reranks over.

use std::sync::Arc;

use memphant_core::service::MemoryService;
use memphant_core::{
    EmbeddingProvider, EmbeddingRow, FixedClock, InMemoryStore, MemoryStore, StubEmbedding,
    embedding_profile_for,
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

/// A larger candidate pool admits a vector candidate the default-sized pool
/// truncated away. Construction is fully deterministic: the units' embeddings
/// are overwritten so the three "near" units sit at cosine distance 0 (they are
/// the query vector itself) and the one "far" unit sits at distance 1 (an
/// orthogonal one-hot). The query is token-disjoint from every body, so the far
/// unit is reachable ONLY via the vector channel — with `pool = near_count` it
/// is truncated out; with `pool = near_count + 1` it is admitted.
#[tokio::test]
async fn larger_pool_admits_vector_candidate_default_pool_missed() {
    let store = InMemoryStore::default();
    let ingest = stub_service(store.clone());
    let tenant = TenantId::new();
    let scope = ScopeId::new();
    let actor = ActorId::new();

    // Distinct single-sentence bodies; only the "dolphin" one is the far target.
    // None share a token with the recall query below.
    let bodies = [
        "Aardvarks assemble ceramic widgets nightly.",
        "Bison brew umbrella tonics daily.",
        "Cranes carry lantern parcels weekly.",
        "Dolphins deliver marble crates monthly.",
    ];
    for body in bodies {
        ingest
            .retain(tenant, retain_request(tenant, scope, actor, body))
            .await
            .expect("retain");
    }
    ingest.reflect(tenant, scope, None).await.expect("reflect");

    let page = store
        .scope_memory_page(tenant, scope, None, 100)
        .await
        .expect("page");
    assert!(page.items.len() >= 4, "each episode compiled a unit");

    // The query vector (token-disjoint from every body), plus a partly-off-axis
    // vector for the far unit.
    let stub = StubEmbedding::default();
    let profile = embedding_profile_for(&stub);
    let query = "zulu xylophone";
    let query_vec = stub
        .embed(&[query.to_string()])
        .expect("embed query")
        .remove(0);
    let off_axis = query_vec
        .iter()
        .position(|value| *value == 0.0)
        .expect("query vector has a zero component to lean the far unit off-axis");
    // Far unit at cosine distance 0.5 — score `1 - 0.5 = 0.5 > 0`, so it clears
    // the vector channel's `score > 0` filter, yet strictly farther than the
    // near units at distance 0. `query_vec + sqrt(3)*e_j` (with query_vec ⟂ e_j)
    // has cosine `1 / sqrt(1 + 3) = 0.5`.
    let mut far_vec = query_vec.clone();
    far_vec[off_axis] += 3.0_f32.sqrt();

    // Overwrite each unit's embedding: near units == query vector (distance 0),
    // the far unit at distance 0.5 (farther, but still a positive score).
    let mut near_ids = Vec::new();
    let mut far_ids = Vec::new();
    let mut rows = Vec::new();
    for unit in &page.items {
        let is_far = unit.body.to_lowercase().contains("dolphin");
        let vec = if is_far {
            far_vec.clone()
        } else {
            query_vec.clone()
        };
        rows.push(EmbeddingRow {
            memory_unit_id: unit.id,
            embedding_profile_id: profile.id,
            vec,
        });
        if is_far {
            far_ids.push(unit.id);
        } else {
            near_ids.push(unit.id);
        }
    }
    store
        .upsert_embeddings(tenant, rows)
        .await
        .expect("overwrite embeddings");
    assert!(!near_ids.is_empty(), "at least one near unit");
    assert_eq!(far_ids.len(), 1, "exactly one far (dolphin) unit");
    let far_id = far_ids[0];
    let near_pool = near_ids.len();

    // Small pool == near_count: the far unit is truncated out of the vector
    // fetch entirely.
    let small = stub_service(store.clone()).with_candidate_pool_size(near_pool);
    let small_response = small
        .recall(tenant, recall_request(tenant, scope, actor, query))
        .await
        .expect("recall (small pool)");
    let small_trace = small
        .trace(tenant, small_response.trace_id)
        .await
        .expect("trace fetch")
        .expect("trace stored");
    let far_in_small = small_trace
        .candidates
        .iter()
        .any(|candidate| candidate.channel == RecallChannel::Vector && candidate.unit_id == far_id);
    assert!(
        !far_in_small,
        "default-sized pool ({near_pool}) truncates the far unit out of the vector channel"
    );
    let near_in_small = small_trace
        .candidates
        .iter()
        .filter(|candidate| candidate.channel == RecallChannel::Vector)
        .count();
    assert_eq!(
        near_in_small, near_pool,
        "the small pool still surfaces exactly the near units via the vector channel"
    );

    // Large pool == near_count + 1: the far unit is now admitted.
    let large = stub_service(store.clone()).with_candidate_pool_size(near_pool + 1);
    let large_response = large
        .recall(tenant, recall_request(tenant, scope, actor, query))
        .await
        .expect("recall (large pool)");
    let large_trace = large
        .trace(tenant, large_response.trace_id)
        .await
        .expect("trace fetch")
        .expect("trace stored");
    let far_in_large = large_trace
        .candidates
        .iter()
        .any(|candidate| candidate.channel == RecallChannel::Vector && candidate.unit_id == far_id);
    assert!(
        far_in_large,
        "the widened pool ({}) admits the far unit as a vector candidate",
        near_pool + 1
    );
}

//! R1.5-T0 recall-pool-depth knob: the construction-time `with_recall_pool_depth`
//! service option widens the recall vector-channel KNN fan-out (among other
//! internal limits). A larger pool admits a vector candidate that the smaller
//! pool truncated away — the widened pool the W8 cross-encoder rerank arm
//! reranks over.

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
    context: &memphant_types::ResolvedMemoryContext,
    body: &str,
) -> RetainEpisodeHttpRequest {
    RetainEpisodeHttpRequest {
        subject_id: context.data_subject_id,
        scope_id: context.scope_id,
        actor_id: context.actor_id,
        agent_node_id: context.agent_node_id,
        subject_generation: context.subject_generation,
        source_ref: "test:fixture".to_string(),
        observed_at: "2026-07-09T00:00:00Z".to_string(),
        payload: memphant_types::RetainPayload::Episode(memphant_types::RetainEpisodePayload {
            source_kind: "user".to_string(),
            body: body.to_string(),
        }),
    }
}

fn recall_request(
    tenant_id: TenantId,
    scope_id: ScopeId,
    actor_id: ActorId,
    query: &str,
) -> RecallHttpRequest {
    RecallHttpRequest {
        subject_id: memphant_types::SubjectId::from_u128(tenant_id.as_uuid().as_u128()),
        scope_id,
        agent_node_id: memphant_types::AgentNodeId::from_u128(scope_id.as_uuid().as_u128()),
        subject_generation: 0,
        actor_id,
        query: query.to_string(),
        limit: None,
        budget_tokens: None,
        mode: None,
        include_beliefs: None,
        transaction_as_of: None,
        valid_at: None,
        aggregation_window: None,
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
    store.seed_context_binding(&memphant_store_testkit::resolved_context(
        tenant, scope, actor,
    ));

    // Distinct single-sentence bodies; only the "dolphin" one is the far target.
    // None share a token with the recall query below.
    let bodies = [
        "Aardvarks assemble ceramic widgets nightly.",
        "Bison brew umbrella tonics daily.",
        "Cranes carry lantern parcels weekly.",
        "Dolphins deliver marble crates monthly.",
    ];
    for (retain_index, body) in bodies.into_iter().enumerate() {
        ingest
            .retain(
                &memphant_store_testkit::resolved_context(tenant, scope, actor),
                &format!("test:{retain_index}"),
                TrustLevel::TrustedUser,
                retain_request(
                    &memphant_store_testkit::resolved_context(tenant, scope, actor),
                    body,
                ),
            )
            .await
            .expect("retain");
    }
    while ingest.run_worker_tick(usize::MAX).await.expect("reflect") > 0 {}

    let context = memphant_store_testkit::resolved_context(tenant, scope, actor);
    let page = store
        .scope_memory_page(&context, None, 100)
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
        .upsert_embeddings(&context, rows)
        .await
        .expect("overwrite embeddings");
    assert!(!near_ids.is_empty(), "at least one near unit");
    assert_eq!(far_ids.len(), 1, "exactly one far (dolphin) unit");
    let far_id = far_ids[0];
    let near_pool = near_ids.len();

    // Small pool == near_count: the far unit is truncated out of the vector
    // fetch entirely.
    let small = stub_service(store.clone()).with_recall_pool_depth(near_pool);
    let small_response = small
        .recall(
            memphant_store_testkit::resolved_context(tenant, scope, actor),
            recall_request(tenant, scope, actor, query),
        )
        .await
        .expect("recall (small pool)");
    let small_trace = small
        .trace(
            &memphant_store_testkit::resolved_context(tenant, scope, actor),
            small_response.trace_id,
        )
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
    let large = stub_service(store.clone()).with_recall_pool_depth(near_pool + 1);
    let large_response = large
        .recall(
            memphant_store_testkit::resolved_context(tenant, scope, actor),
            recall_request(tenant, scope, actor, query),
        )
        .await
        .expect("recall (large pool)");
    let large_trace = large
        .trace(
            &memphant_store_testkit::resolved_context(tenant, scope, actor),
            large_response.trace_id,
        )
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

/// The deterministic reranker's vector feature must be the real cosine score
/// carried by the vector recall channel. A vector-only target starts behind ten
/// lexical distractors in fused order, but its perfect cosine match must move
/// it into the reranked top ten and therefore into the returned pack.
#[tokio::test]
async fn deterministic_rerank_admits_strong_vector_only_candidate() {
    let store = InMemoryStore::default();
    let ingest = stub_service(store.clone());
    let tenant = TenantId::new();
    let scope = ScopeId::new();
    let actor = ActorId::new();
    store.seed_context_binding(&memphant_store_testkit::resolved_context(
        tenant, scope, actor,
    ));
    let query = "amber";
    let bodies = [
        "Amber aardvark carries ceramic parcels.",
        "Amber bison carries ceramic parcels.",
        "Amber crane carries ceramic parcels.",
        "Amber dingo carries ceramic parcels.",
        "Amber egret carries ceramic parcels.",
        "Amber ferret carries ceramic parcels.",
        "Amber gecko carries ceramic parcels.",
        "Amber heron carries ceramic parcels.",
        "Amber ibis carries ceramic parcels.",
        "Amber jackal carries ceramic parcels.",
        "Dolphins deliver marble crates monthly.",
    ];
    for (retain_index, body) in bodies.into_iter().enumerate() {
        ingest
            .retain(
                &memphant_store_testkit::resolved_context(tenant, scope, actor),
                &format!("test:{retain_index}"),
                TrustLevel::TrustedUser,
                retain_request(
                    &memphant_store_testkit::resolved_context(tenant, scope, actor),
                    body,
                ),
            )
            .await
            .expect("retain");
    }
    while ingest.run_worker_tick(usize::MAX).await.expect("reflect") > 0 {}

    let context = memphant_store_testkit::resolved_context(tenant, scope, actor);
    let page = store
        .scope_memory_page(&context, None, 100)
        .await
        .expect("page");
    assert_eq!(page.items.len(), bodies.len(), "one unit per episode");
    let target_id = page
        .items
        .iter()
        .find(|unit| unit.body.contains("Dolphins"))
        .expect("vector-only target")
        .id;

    let stub = StubEmbedding::default();
    let profile = embedding_profile_for(&stub);
    let query_vec = stub
        .embed_query(&[query.to_string()])
        .expect("embed query")
        .remove(0);
    let off_axis = query_vec
        .iter()
        .position(|value| *value == 0.0)
        .expect("sparse query vector has an orthogonal component");
    let mut orthogonal = vec![0.0; query_vec.len()];
    orthogonal[off_axis] = 1.0;
    store
        .upsert_embeddings(
            &context,
            page.items
                .iter()
                .map(|unit| EmbeddingRow {
                    memory_unit_id: unit.id,
                    embedding_profile_id: profile.id,
                    vec: if unit.id == target_id {
                        query_vec.clone()
                    } else {
                        orthogonal.clone()
                    },
                })
                .collect(),
        )
        .await
        .expect("overwrite embeddings");

    let service = stub_service(store.clone()).with_recall_pool_depth(bodies.len());
    // The deterministic reranker under test is opt-in (`MemoryService::recall`
    // hard-codes `rerank_enabled: false` as the real-evidence default), so this
    // controlled evaluation enters through `recall_internal` with the reranker on.
    let response = service
        .recall_internal(memphant_types::RecallRequest {
            context: memphant_store_testkit::resolved_context(tenant, scope, actor),
            query: query.to_string(),
            k: 10,
            budget_tokens: 8192,
            mode: memphant_types::RecallMode::Fast,
            include_beliefs: false,
            edge_expansion_enabled: false,
            context_packing_abstention_enabled: true,
            rerank_enabled: true,
            learned_rerank_profile: None,
            query_decomposition_enabled: true,
            procedure_recall_enabled: true,
            decay_enabled: true,
            engine_version: "candidate-pool-rerank-test".to_string(),
            transaction_as_of: None,
            valid_at: None,
            aggregation_window: None,
        })
        .await
        .expect("recall");
    let trace = service
        .trace(
            &memphant_store_testkit::resolved_context(tenant, scope, actor),
            response.trace_id,
        )
        .await
        .expect("trace fetch")
        .expect("trace stored");
    let target = trace
        .candidates
        .iter()
        .find(|candidate| {
            candidate.unit_id == target_id && candidate.channel == RecallChannel::Vector
        })
        .expect("target reached the vector channel");

    assert_eq!(
        target.channel_score, 1.0,
        "target is a perfect vector match"
    );
    assert!(
        trace
            .candidates
            .iter()
            .all(|candidate| candidate.unit_id != target_id
                || candidate.channel != RecallChannel::Lexical),
        "target has zero lexical overlap"
    );
    assert!(
        target.rerank_rank.is_some_and(|rank| rank <= 10),
        "strong vector target rerank rank was {:?}",
        target.rerank_rank
    );
    assert!(
        response.candidate_whitelist.contains(&target_id),
        "reranked vector target must displace a lexical distractor"
    );
}

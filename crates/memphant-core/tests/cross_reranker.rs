//! W8 cross-encoder rerank seam: `with_cross_reranker` reorders the top
//! `recall_pool_depth` fused candidates by `(query, body)` scores AFTER fusion
//! and BEFORE packing. Stub rerankers prove the three contract properties:
//! reordering, prior-rank tie stability, and inert-when-absent/declined.

use std::sync::Arc;

use memphant_core::service::MemoryService;
use memphant_core::{
    CrossRerankCandidateSelection, CrossReranker, CrossRerankerConfig, EmbedError,
    EmbeddingProvider, FixedClock, InMemoryStore, StubEmbedding,
};
use memphant_types::{
    ActorId, CrossRerankFailure, RecallHttpRequest, RetainEpisodeHttpRequest, ScopeId, TenantId,
    TrustLevel,
};

const CLOCK: FixedClock = FixedClock("2026-07-09T00:00:00Z");

/// Boosts docs containing `needle` (score 1.0) above the rest (0.0). One score
/// per doc, in input order — the seam contract.
struct BoostReranker {
    needle: String,
}
impl CrossReranker for BoostReranker {
    fn config(&self) -> CrossRerankerConfig {
        test_config(64)
    }

    fn rerank(&self, _query: &str, docs: &[&str]) -> Result<Vec<f32>, String> {
        Ok(docs
            .iter()
            .map(|doc| if doc.contains(&self.needle) { 1.0 } else { 0.0 })
            .collect())
    }
}

/// Scores every doc identically: a stable sort must leave the order untouched.
struct ConstantReranker;
impl CrossReranker for ConstantReranker {
    fn config(&self) -> CrossRerankerConfig {
        test_config(64)
    }

    fn rerank(&self, _query: &str, docs: &[&str]) -> Result<Vec<f32>, String> {
        Ok(vec![0.5; docs.len()])
    }
}

/// Declines by returning a wrong-length (empty) vector — the seam's documented
/// no-op signal; the fused order must survive unchanged.
struct DecliningReranker;
impl CrossReranker for DecliningReranker {
    fn config(&self) -> CrossRerankerConfig {
        test_config(64)
    }

    fn rerank(&self, _query: &str, _docs: &[&str]) -> Result<Vec<f32>, String> {
        Ok(Vec::new())
    }
}

/// Sleeps a fixed, small-but-measurable amount before scoring — so the
/// R1.5-T1 `cross_rerank_ms` trace field is provably nonzero without relying
/// on a real model's (variable, possibly sub-millisecond-rounding) timing.
struct SlowReranker;
impl CrossReranker for SlowReranker {
    fn config(&self) -> CrossRerankerConfig {
        test_config(64)
    }

    fn rerank(&self, _query: &str, docs: &[&str]) -> Result<Vec<f32>, String> {
        std::thread::sleep(std::time::Duration::from_millis(5));
        Ok(vec![0.5; docs.len()])
    }
}

struct ReversePrefixReranker;
impl CrossReranker for ReversePrefixReranker {
    fn config(&self) -> CrossRerankerConfig {
        test_config(2)
    }

    fn rerank(&self, _query: &str, docs: &[&str]) -> Result<Vec<f32>, String> {
        assert_eq!(docs.len(), 2, "core applies the static candidate limit");
        Ok(vec![0.0, 1.0])
    }
}

struct ErrorReranker;
impl CrossReranker for ErrorReranker {
    fn config(&self) -> CrossRerankerConfig {
        test_config(2)
    }

    fn rerank(&self, _query: &str, _docs: &[&str]) -> Result<Vec<f32>, String> {
        Err("backend unavailable".to_string())
    }
}

/// Makes one otherwise lexical-free document the nearest vector neighbor while
/// every `widget` distractor shares a different vector. This lets the service
/// test prove candidate admission, not merely score ordering.
struct QuotaTestEmbedding;
impl EmbeddingProvider for QuotaTestEmbedding {
    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbedError> {
        Ok(texts
            .iter()
            .map(|text| {
                if text.contains("semantic candidate") || text == "widget lookup" {
                    vec![1.0, 0.0]
                } else {
                    vec![0.0, 1.0]
                }
            })
            .collect())
    }

    fn dimensions(&self) -> usize {
        2
    }

    fn id(&self) -> &str {
        "quota-test"
    }
}

fn test_config(candidate_limit: usize) -> CrossRerankerConfig {
    CrossRerankerConfig {
        provider: "test".to_string(),
        model: "deterministic".to_string(),
        candidate_limit,
        max_length: 512,
        batch_size: None,
    }
}

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
        limit: Some(10),
        budget_tokens: Some(8192),
        mode: None,
        include_beliefs: None,
        // Keep the retired heuristic rerank and decomposition OFF so the test
        // isolates the cross-encoder seam from those (independent) stages.
        transaction_as_of: None,
        valid_at: None,
        aggregation_window: None,
    }
}

/// Five units share the query token `widget` (all retrieved via lexical) and
/// each carries a unique marker, so a reranker can target exactly one.
const BODIES: [&str; 5] = [
    "The widget alpha rotates smoothly every morning.",
    "The widget bravo hums a quiet tune at noon.",
    "The widget charlie glows amber under load.",
    "The widget delta clicks twice before resetting.",
    "The widget echo pulses when the queue drains.",
];

async fn seed(store: &InMemoryStore) -> (TenantId, ScopeId, ActorId) {
    let ingest = stub_service(store.clone());
    let tenant = TenantId::new();
    let scope = ScopeId::new();
    let actor = ActorId::new();
    store.seed_context_binding(&memphant_store_testkit::resolved_context(
        tenant, scope, actor,
    ));
    for (retain_index, body) in BODIES.into_iter().enumerate() {
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
    (tenant, scope, actor)
}

#[tokio::test]
async fn vector_lexical_balance_preserves_a_vector_only_candidate_for_reranking() {
    let store = InMemoryStore::default();
    let ingest = MemoryService::new(
        Arc::new(store.clone()),
        Arc::new(CLOCK),
        Arc::new(QuotaTestEmbedding),
    );
    let tenant = TenantId::new();
    let scope = ScopeId::new();
    let actor = ActorId::new();
    store.seed_context_binding(&memphant_store_testkit::resolved_context(
        tenant, scope, actor,
    ));
    for index in 0..70 {
        let body = format!("widget distractor number {index} with repeated widget terms");
        ingest
            .retain(
                &memphant_store_testkit::resolved_context(tenant, scope, actor),
                &format!("distractor:{index}"),
                TrustLevel::TrustedUser,
                retain_request(
                    &memphant_store_testkit::resolved_context(tenant, scope, actor),
                    &body,
                ),
            )
            .await
            .expect("retain distractor");
    }
    for index in 0..32 {
        let body = format!("semantic candidate {index} contains hidden material");
        ingest
            .retain(
                &memphant_store_testkit::resolved_context(tenant, scope, actor),
                &format!("semantic:{index}"),
                TrustLevel::TrustedUser,
                retain_request(
                    &memphant_store_testkit::resolved_context(tenant, scope, actor),
                    &body,
                ),
            )
            .await
            .expect("retain semantic candidate");
    }
    let target = "semantic candidate 9 contains hidden material";
    while ingest.run_worker_tick(usize::MAX).await.expect("reflect") > 0 {}

    let baseline = MemoryService::new(
        Arc::new(store.clone()),
        Arc::new(CLOCK),
        Arc::new(QuotaTestEmbedding),
    )
    .with_cross_reranker(Arc::new(BoostReranker {
        needle: target.to_string(),
    }))
    .recall(
        memphant_store_testkit::resolved_context(tenant, scope, actor),
        recall_request(tenant, scope, actor, "widget lookup"),
    )
    .await
    .expect("baseline recall");
    assert!(
        baseline.items.iter().all(|item| item.body != target),
        "the fused-head reranker never receives the vector-only target"
    );

    let service = MemoryService::new(
        Arc::new(store),
        Arc::new(CLOCK),
        Arc::new(QuotaTestEmbedding),
    )
    .with_cross_rerank_candidate_selection(CrossRerankCandidateSelection::VectorLexicalBalanced)
    .with_cross_reranker(Arc::new(BoostReranker {
        needle: target.to_string(),
    }));
    let bodies = service
        .recall(
            memphant_store_testkit::resolved_context(tenant, scope, actor),
            recall_request(tenant, scope, actor, "widget lookup"),
        )
        .await
        .expect("recall")
        .items
        .into_iter()
        .map(|item| item.body)
        .collect::<Vec<_>>();

    assert_eq!(bodies.first().map(String::as_str), Some(target));
}

async fn recalled_bodies(
    service: &MemoryService<InMemoryStore>,
    tenant: TenantId,
    scope: ScopeId,
    actor: ActorId,
) -> Vec<String> {
    service
        .recall(
            memphant_store_testkit::resolved_context(tenant, scope, actor),
            recall_request(tenant, scope, actor, "widget"),
        )
        .await
        .expect("recall")
        .items
        .into_iter()
        .map(|item| item.body)
        .collect()
}

/// The cross-encoder reorders: boosting the candidate that fusion ranked LAST
/// lifts it to rank 1, and it was not there without the reranker.
#[tokio::test]
async fn cross_reranker_reorders_candidates() {
    let store = InMemoryStore::default();
    let (tenant, scope, actor) = seed(&store).await;

    let baseline = recalled_bodies(&stub_service(store.clone()), tenant, scope, actor).await;
    assert!(baseline.len() >= 2, "fusion returns several candidates");
    let last = baseline.last().expect("non-empty").clone();
    assert_ne!(baseline[0], last, "the target starts BELOW rank 1");

    // The unique marker word of the fusion-last body (e.g. "echo").
    let needle = last
        .split_whitespace()
        .nth(2)
        .expect("marker token")
        .to_string();

    let reranked = recalled_bodies(
        &stub_service(store.clone()).with_cross_reranker(Arc::new(BoostReranker { needle })),
        tenant,
        scope,
        actor,
    )
    .await;
    assert_eq!(
        reranked[0], last,
        "the boosted candidate is lifted to rank 1 by the cross-encoder"
    );
}

/// No reranker, a declining reranker (wrong-length no-op), and a constant-score
/// reranker (equal scores, stable sort) all produce byte-identical output — the
/// seam is inert unless the cross-encoder expresses a strict preference, and
/// ties fall back to prior fused rank.
#[tokio::test]
async fn absent_or_neutral_reranker_is_byte_identical() {
    let store = InMemoryStore::default();
    let (tenant, scope, actor) = seed(&store).await;

    let baseline = recalled_bodies(&stub_service(store.clone()), tenant, scope, actor).await;

    let declined = recalled_bodies(
        &stub_service(store.clone()).with_cross_reranker(Arc::new(DecliningReranker)),
        tenant,
        scope,
        actor,
    )
    .await;
    assert_eq!(
        declined, baseline,
        "a declining (wrong-length) reranker leaves the fused order unchanged"
    );

    let constant = recalled_bodies(
        &stub_service(store.clone()).with_cross_reranker(Arc::new(ConstantReranker)),
        tenant,
        scope,
        actor,
    )
    .await;
    assert_eq!(
        constant, baseline,
        "equal cross-encoder scores break ties by prior fused rank (stable), \
         so the order is identical to no rerank"
    );
}

/// Determinism: the same reranker over the same corpus yields byte-identical
/// order across repeated recalls.
#[tokio::test]
async fn cross_rerank_is_deterministic() {
    let store = InMemoryStore::default();
    let (tenant, scope, actor) = seed(&store).await;
    let service = stub_service(store.clone()).with_cross_reranker(Arc::new(BoostReranker {
        needle: "charlie".to_string(),
    }));
    let first = recalled_bodies(&service, tenant, scope, actor).await;
    let second = recalled_bodies(&service, tenant, scope, actor).await;
    assert_eq!(first, second, "cross-rerank is order-deterministic");
    assert!(
        first[0].contains("charlie"),
        "the boosted candidate leads: {first:?}"
    );
}

/// R1.5-T1: `RetrievalTrace::cross_rerank_ms` is `0` (and `feature_flags`
/// carries no `cross_rerank_enabled` entry) when no reranker is installed —
/// the flag-off byte-identity case — and is a real, nonzero measurement (with
/// the flag present) when one runs.
#[tokio::test]
async fn cross_rerank_ms_is_recorded_on_the_trace_only_when_a_reranker_runs() {
    let store = InMemoryStore::default();
    let (tenant, scope, actor) = seed(&store).await;

    let absent_service = stub_service(store.clone());
    let absent = absent_service
        .recall(
            memphant_store_testkit::resolved_context(tenant, scope, actor),
            recall_request(tenant, scope, actor, "widget"),
        )
        .await
        .expect("recall");
    let absent_trace = absent_service
        .store()
        .trace_by_id_any_tenant(absent.trace_id)
        .expect("trace exists");
    assert_eq!(
        absent_trace.cross_rerank_ms, 0,
        "no reranker installed ⇒ cross_rerank_ms stays the legitimate 0 (not run)"
    );
    assert!(
        !absent_trace
            .feature_flags
            .iter()
            .any(|flag| flag == "cross_rerank_enabled"),
        "no reranker installed ⇒ no cross_rerank_enabled feature flag: {:?}",
        absent_trace.feature_flags
    );

    let slow_service = stub_service(store.clone()).with_cross_reranker(Arc::new(SlowReranker));
    let present = slow_service
        .recall(
            memphant_store_testkit::resolved_context(tenant, scope, actor),
            recall_request(tenant, scope, actor, "widget"),
        )
        .await
        .expect("recall");
    let present_trace = slow_service
        .store()
        .trace_by_id_any_tenant(present.trace_id)
        .expect("trace exists");
    assert!(
        present_trace.cross_rerank_ms >= 5,
        "the reranker's 5ms sleep must be reflected in the trace: {}",
        present_trace.cross_rerank_ms
    );
    assert!(
        present_trace
            .feature_flags
            .iter()
            .any(|flag| flag == "cross_rerank_enabled"),
        "a reranker installed ⇒ cross_rerank_enabled feature flag present: {:?}",
        present_trace.feature_flags
    );
}

#[tokio::test]
async fn candidate_limit_reorders_only_the_scored_prefix_and_traces_exact_inputs() {
    let store = InMemoryStore::default();
    let (tenant, scope, actor) = seed(&store).await;
    let baseline = recalled_bodies(&stub_service(store.clone()), tenant, scope, actor).await;
    let service = stub_service(store.clone()).with_cross_reranker(Arc::new(ReversePrefixReranker));

    let response = service
        .recall(
            memphant_store_testkit::resolved_context(tenant, scope, actor),
            recall_request(tenant, scope, actor, "widget"),
        )
        .await
        .expect("recall");
    let bodies: Vec<_> = response
        .items
        .iter()
        .map(|item| item.body.clone())
        .collect();
    assert_eq!(bodies[0], baseline[1]);
    assert_eq!(bodies[1], baseline[0]);
    assert_eq!(
        &bodies[2..],
        &baseline[2..],
        "unscored fused tail is stable"
    );

    let trace = service
        .store()
        .trace_by_id_any_tenant(response.trace_id)
        .expect("trace exists");
    let facts = trace.cross_rerank.expect("rerank facts");
    assert_eq!(facts.provider, "test");
    assert_eq!(facts.model, "deterministic");
    assert_eq!(facts.candidate_limit, 2);
    assert_eq!(facts.candidate_count, 2);
    assert_eq!(facts.max_length, 512);
    assert_eq!(facts.batch_size, None);
    let mut lengths = baseline[..2]
        .iter()
        .map(|body| body.chars().count())
        .collect::<Vec<_>>();
    lengths.sort_unstable();
    assert_eq!(facts.input_chars_p50, lengths[0]);
    assert_eq!(facts.input_chars_p95, lengths[1]);
    assert_eq!(facts.input_chars_max, lengths[1]);
    assert_eq!(facts.failure, CrossRerankFailure::None);
}

#[tokio::test]
async fn reranker_error_fails_open_and_records_the_failure() {
    let store = InMemoryStore::default();
    let (tenant, scope, actor) = seed(&store).await;
    let baseline = recalled_bodies(&stub_service(store.clone()), tenant, scope, actor).await;
    let service = stub_service(store.clone()).with_cross_reranker(Arc::new(ErrorReranker));

    let response = service
        .recall(
            memphant_store_testkit::resolved_context(tenant, scope, actor),
            recall_request(tenant, scope, actor, "widget"),
        )
        .await
        .expect("recall fails open");
    let bodies: Vec<_> = response
        .items
        .iter()
        .map(|item| item.body.clone())
        .collect();
    assert_eq!(
        bodies, baseline,
        "rerank failure preserves fused recall order"
    );
    let trace = service
        .store()
        .trace_by_id_any_tenant(response.trace_id)
        .expect("trace exists");
    assert_eq!(
        trace.cross_rerank.expect("rerank facts").failure,
        CrossRerankFailure::Error
    );
}

#[tokio::test]
async fn empty_reranker_output_fails_open_and_records_empty() {
    let store = InMemoryStore::default();
    let (tenant, scope, actor) = seed(&store).await;
    let service = stub_service(store).with_cross_reranker(Arc::new(DecliningReranker));
    let response = service
        .recall(
            memphant_store_testkit::resolved_context(tenant, scope, actor),
            recall_request(tenant, scope, actor, "widget"),
        )
        .await
        .expect("recall fails open");
    let trace = service
        .store()
        .trace_by_id_any_tenant(response.trace_id)
        .expect("trace exists");
    assert_eq!(
        trace.cross_rerank.expect("rerank facts").failure,
        CrossRerankFailure::Empty
    );
}

//! W8 cross-encoder rerank seam: `with_cross_reranker` reorders the top
//! `recall_pool_depth` fused candidates by `(query, body)` scores AFTER fusion
//! and BEFORE packing. Stub rerankers prove the three contract properties:
//! reordering, prior-rank tie stability, and inert-when-absent/declined.

use std::sync::Arc;

use memphant_core::service::MemoryService;
use memphant_core::{CrossReranker, FixedClock, InMemoryStore, StubEmbedding};
use memphant_types::{
    ActorId, RecallHttpRequest, RetainEpisodeHttpRequest, ScopeId, TenantId, TrustLevel,
};

const CLOCK: FixedClock = FixedClock("2026-07-09T00:00:00Z");

/// Boosts docs containing `needle` (score 1.0) above the rest (0.0). One score
/// per doc, in input order — the seam contract.
struct BoostReranker {
    needle: String,
}
impl CrossReranker for BoostReranker {
    fn rerank(&self, _query: &str, docs: &[&str]) -> Vec<f32> {
        docs.iter()
            .map(|doc| if doc.contains(&self.needle) { 1.0 } else { 0.0 })
            .collect()
    }
}

/// Scores every doc identically: a stable sort must leave the order untouched.
struct ConstantReranker;
impl CrossReranker for ConstantReranker {
    fn rerank(&self, _query: &str, docs: &[&str]) -> Vec<f32> {
        vec![0.5; docs.len()]
    }
}

/// Declines by returning a wrong-length (empty) vector — the seam's documented
/// no-op signal; the fused order must survive unchanged.
struct DecliningReranker;
impl CrossReranker for DecliningReranker {
    fn rerank(&self, _query: &str, _docs: &[&str]) -> Vec<f32> {
        Vec::new()
    }
}

/// Sleeps a fixed, small-but-measurable amount before scoring — so the
/// R1.5-T1 `cross_rerank_ms` trace field is provably nonzero without relying
/// on a real model's (variable, possibly sub-millisecond-rounding) timing.
struct SlowReranker;
impl CrossReranker for SlowReranker {
    fn rerank(&self, _query: &str, docs: &[&str]) -> Vec<f32> {
        std::thread::sleep(std::time::Duration::from_millis(5));
        vec![0.5; docs.len()]
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
        limit: Some(10),
        budget_tokens: Some(8192),
        mode: None,
        include_beliefs: None,
        edge_expansion_enabled: None,
        context_packing_abstention_enabled: None,
        // Keep the retired heuristic rerank and decomposition OFF so the test
        // isolates the cross-encoder seam from those (independent) stages.
        rerank_enabled: Some(false),
        query_decomposition_enabled: Some(false),
        procedure_recall_enabled: None,
        decay_enabled: None,
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
    for body in BODIES {
        ingest
            .retain(tenant, retain_request(tenant, scope, actor, body))
            .await
            .expect("retain");
    }
    ingest.reflect(tenant, scope, None).await.expect("reflect");
    (tenant, scope, actor)
}

async fn recalled_bodies(
    service: &MemoryService<InMemoryStore>,
    tenant: TenantId,
    scope: ScopeId,
    actor: ActorId,
) -> Vec<String> {
    service
        .recall(tenant, recall_request(tenant, scope, actor, "widget"))
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
        .recall(tenant, recall_request(tenant, scope, actor, "widget"))
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
        .recall(tenant, recall_request(tenant, scope, actor, "widget"))
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

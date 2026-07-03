use std::collections::HashMap;

use memphant_core::{CoreError, InMemoryStore, MemoryStore, recall};
use memphant_types::{
    ActorId, ContextualChunk, MemoryEdgeKind, MemoryKind, NewEpisode, NewMemoryEdge, NewMemoryUnit,
    RecallChannel, RecallDropReason, RecallMode, RecallRequest, ScopeId, TenantId, TrustLevel,
    UnitId, UnitState,
};
use serde::Deserialize;

fn tenant(value: u128) -> TenantId {
    TenantId::from_u128(value)
}

fn scope(value: u128) -> ScopeId {
    ScopeId::from_u128(value)
}

fn actor(value: u128) -> ActorId {
    ActorId::from_u128(value)
}

#[tokio::test]
async fn recall_writes_trace_for_scope_denial() {
    let store = InMemoryStore::default();
    let tenant_id = tenant(70_000);
    let allowed_scope = scope(70_001);
    let denied_scope = scope(70_002);
    let actor_id = actor(70_003);

    let error = recall(
        &store,
        RecallRequest {
            tenant_id,
            scope_id: denied_scope,
            actor_id,
            allowed_scope_ids: vec![allowed_scope],
            query: "Which callback version is current?".to_string(),
            k: 3,
            budget_tokens: 80,
            mode: RecallMode::Fast,
            include_beliefs: false,
            engine_version: "engine-wsc-test".to_string(),
        },
    )
    .await
    .expect_err("denied recall returns a policy error");

    assert!(matches!(error, CoreError::PolicyDenied(_)));
    let traces = store.retrieval_traces(tenant_id);
    assert_eq!(traces.len(), 1);
    assert_eq!(traces[0].scope_id, denied_scope);
    assert_eq!(traces[0].policy_filters[0].reason, RecallDropReason::Scope);
    assert!(traces[0].context_items.is_empty());
    assert!(traces[0].abstention_signal);
}

#[tokio::test]
async fn contextual_chunk_recall_finds_source_unit_and_traces_flag() {
    let store = InMemoryStore::default();
    let tenant_id = tenant(72_000);
    let scope_id = scope(72_001);
    let actor_id = actor(72_002);

    let mut tx = store.begin().await;
    let episode = store
        .stage_episode(
            &mut tx,
            NewEpisode {
                tenant_id,
                scope_id,
                actor_id,
                source_kind: "system".to_string(),
                source_trust: TrustLevel::TrustedSystem,
                dedup_key: "chunked-runbook".to_string(),
                body: "The deployment runbook mentions an emergency breaker named albatross."
                    .to_string(),
            },
        )
        .await
        .expect("episode seeded");
    let unit_id = store
        .stage_memory_unit(
            &mut tx,
            NewMemoryUnit {
                tenant_id,
                scope_id,
                kind: MemoryKind::Semantic,
                state: UnitState::Active,
                subject_key: Some("deployment runbook".to_string()),
                body: "Runbook contains a gated switch.".to_string(),
                trust_level: TrustLevel::TrustedSystem,
                churn_class: None,
                freshness_due: false,
                actor_id: Some(actor_id),
                source_kind: Some("system".to_string()),
                source_episode_id: Some(episode.episode_id),
                source_resource_id: None,
                deletion_generation: None,
                contextual_chunks: vec![ContextualChunk {
                    id: "chunk-albatross-breaker".to_string(),
                    header: "Deployment runbook / emergency breaker".to_string(),
                    body: "The emergency breaker codeword is albatross.".to_string(),
                    source_span: Some("episode:0-74".to_string()),
                }],
            },
        )
        .await
        .expect("unit seeded");
    store.commit(tx).await.expect("seed committed");

    let response = recall(
        &store,
        RecallRequest {
            tenant_id,
            scope_id,
            actor_id,
            allowed_scope_ids: vec![scope_id],
            query: "What is the albatross codeword?".to_string(),
            k: 1,
            budget_tokens: 80,
            mode: RecallMode::Fast,
            include_beliefs: false,
            engine_version: "engine-ws4-test".to_string(),
        },
    )
    .await
    .expect("recall succeeds");

    assert_eq!(response.items.len(), 1);
    assert_eq!(response.items[0].unit_id, unit_id);
    assert_eq!(response.items[0].inclusion_reason, "contextual_chunk");

    let traces = store.retrieval_traces(tenant_id);
    assert_eq!(traces.len(), 1);
    assert!(
        traces[0]
            .feature_flags
            .iter()
            .any(|flag| flag == "contextual_chunks_enabled")
    );
    assert!(traces[0].candidates.iter().any(|candidate| {
        candidate.unit_id == unit_id
            && candidate.channel == RecallChannel::Vector
            && candidate.channel_score > 0.0
    }));
}

#[derive(Debug, Deserialize)]
struct GoldenCase {
    id: String,
    query: String,
    #[serde(default)]
    budget_tokens: Option<usize>,
    expected_weight_vector_id: String,
    expected_stage_names: Vec<String>,
    #[serde(default)]
    expect_filter_selectivity_lt_one: bool,
    seed: GoldenSeed,
    expect: GoldenExpect,
}

#[derive(Debug, Deserialize)]
struct GoldenSeed {
    units: Vec<GoldenUnit>,
    #[serde(default)]
    edges: Vec<GoldenEdge>,
}

#[derive(Debug, Deserialize)]
struct GoldenUnit {
    name: String,
    #[serde(default = "primary_tenant")]
    tenant: String,
    #[serde(default = "primary_scope")]
    scope: String,
    episode_body: String,
    kind: MemoryKind,
    state: UnitState,
    subject_key: Option<String>,
    body: String,
    trust_level: TrustLevel,
    #[serde(default)]
    deletion_generation: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct GoldenExpect {
    answer_bearing_units: Vec<String>,
    #[serde(default)]
    forbidden_units: Vec<String>,
    #[serde(default)]
    dropped: Vec<GoldenDropped>,
    #[serde(default)]
    suppression_labels: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct GoldenDropped {
    unit: String,
    reason: RecallDropReason,
}

#[derive(Debug, Deserialize)]
struct GoldenEdge {
    src: String,
    dst: String,
    kind: MemoryEdgeKind,
}

fn primary_tenant() -> String {
    "primary".to_string()
}

fn primary_scope() -> String {
    "primary".to_string()
}

#[tokio::test]
async fn recall_golden_fixtures_pass() {
    let cases: Vec<GoldenCase> = serde_json::from_str(include_str!(
        "../../../examples/evals/wsc-recall-goldens.json"
    ))
    .expect("fixtures parse");

    for case in cases {
        let store = InMemoryStore::default();
        let tenant_id = tenant(71_000);
        let scope_id = scope(71_001);
        let denied_scope_id = scope(71_003);
        let other_tenant_id = tenant(71_004);
        let actor_id = actor(71_002);
        let mut named_units: HashMap<String, UnitId> = HashMap::new();

        let mut tx = store.begin().await;
        for unit in &case.seed.units {
            let unit_tenant_id = if unit.tenant == "other" {
                other_tenant_id
            } else {
                tenant_id
            };
            let unit_scope_id = if unit.scope == "denied" {
                denied_scope_id
            } else {
                scope_id
            };
            let episode = store
                .stage_episode(
                    &mut tx,
                    NewEpisode {
                        tenant_id: unit_tenant_id,
                        scope_id: unit_scope_id,
                        actor_id,
                        source_kind: "system".to_string(),
                        source_trust: unit.trust_level,
                        dedup_key: format!("{}:{}", case.id, unit.name),
                        body: unit.episode_body.clone(),
                    },
                )
                .await
                .unwrap_or_else(|error| panic!("{} episode seed failed: {error}", case.id));
            let unit_id = store
                .stage_memory_unit(
                    &mut tx,
                    NewMemoryUnit {
                        tenant_id: unit_tenant_id,
                        scope_id: unit_scope_id,
                        kind: unit.kind,
                        state: unit.state,
                        subject_key: unit.subject_key.clone(),
                        body: unit.body.clone(),
                        trust_level: unit.trust_level,
                        churn_class: None,
                        freshness_due: false,
                        actor_id: Some(actor_id),
                        source_kind: Some("system".to_string()),
                        source_episode_id: Some(episode.episode_id),
                        source_resource_id: None,
                        deletion_generation: unit.deletion_generation,
                        contextual_chunks: Vec::new(),
                    },
                )
                .await
                .unwrap_or_else(|error| panic!("{} unit seed failed: {error}", case.id));
            named_units.insert(unit.name.clone(), unit_id);
        }
        for edge in &case.seed.edges {
            store
                .stage_memory_edge(
                    &mut tx,
                    NewMemoryEdge {
                        tenant_id,
                        scope_id,
                        src_id: *named_units
                            .get(&edge.src)
                            .unwrap_or_else(|| panic!("{} missing edge src {}", case.id, edge.src)),
                        dst_id: *named_units
                            .get(&edge.dst)
                            .unwrap_or_else(|| panic!("{} missing edge dst {}", case.id, edge.dst)),
                        kind: edge.kind,
                    },
                )
                .await
                .unwrap_or_else(|error| panic!("{} edge seed failed: {error}", case.id));
        }
        store
            .commit(tx)
            .await
            .unwrap_or_else(|error| panic!("{} seed commit failed: {error}", case.id));

        let response = recall(
            &store,
            RecallRequest {
                tenant_id,
                scope_id,
                actor_id,
                allowed_scope_ids: vec![scope_id],
                query: case.query.clone(),
                k: 3,
                budget_tokens: case.budget_tokens.unwrap_or(80),
                mode: RecallMode::Fast,
                include_beliefs: false,
                engine_version: "engine-wsc-test".to_string(),
            },
        )
        .await
        .unwrap_or_else(|error| panic!("{} recall failed: {error}", case.id));

        for expected_name in &case.expect.answer_bearing_units {
            let expected_id = named_units
                .get(expected_name)
                .unwrap_or_else(|| panic!("{} missing unit {expected_name}", case.id));
            assert!(
                response.candidate_whitelist.contains(expected_id),
                "{} whitelist contains {}",
                case.id,
                expected_name
            );
        }
        for forbidden_name in &case.expect.forbidden_units {
            let forbidden_id = named_units
                .get(forbidden_name)
                .unwrap_or_else(|| panic!("{} missing unit {forbidden_name}", case.id));
            assert!(
                !response.candidate_whitelist.contains(forbidden_id),
                "{} whitelist excludes {}",
                case.id,
                forbidden_name
            );
        }
        assert!(
            response
                .citations
                .iter()
                .all(|citation| response.candidate_whitelist.contains(&citation.unit_id)),
            "{} citations stay inside candidate whitelist",
            case.id
        );
        assert!(
            response
                .citations
                .iter()
                .any(|citation| citation.episode_id.is_some()),
            "{} emits source episode citation",
            case.id
        );

        let traces = store.retrieval_traces(tenant_id);
        let trace = traces
            .last()
            .unwrap_or_else(|| panic!("{} missing retrieval trace", case.id));
        let stage_names: Vec<_> = trace
            .channel_runs
            .iter()
            .map(|stage| stage.stage.clone())
            .collect();
        assert_eq!(stage_names, case.expected_stage_names, "{}", case.id);
        assert_eq!(
            trace.weight_vector_id, case.expected_weight_vector_id,
            "{}",
            case.id
        );
        if case.expect_filter_selectivity_lt_one {
            assert!(
                trace.filter_selectivity.unwrap_or(1.0) < 1.0,
                "{} filter_selectivity shows shared-corpus filtering",
                case.id
            );
        }
        for expected_drop in &case.expect.dropped {
            let dropped_id = named_units.get(&expected_drop.unit).unwrap_or_else(|| {
                panic!("{} missing dropped unit {}", case.id, expected_drop.unit)
            });
            assert!(
                trace
                    .dropped_items
                    .iter()
                    .any(|item| item.unit_id == *dropped_id && item.reason == expected_drop.reason),
                "{} trace drops {} as {:?}",
                case.id,
                expected_drop.unit,
                expected_drop.reason
            );
        }
        for expected_label in &case.expect.suppression_labels {
            assert!(
                response
                    .suppression_labels
                    .iter()
                    .any(|label| label == expected_label),
                "{} response includes suppression label {}",
                case.id,
                expected_label
            );
        }
    }
}

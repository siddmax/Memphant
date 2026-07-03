use memphant_core::{InMemoryStore, reflect_recorded, retain_episode};
use memphant_types::{
    ActorId, AdmissionAction, MemoryEdgeKind, ReflectCandidate, ReflectInput, RetainRequest,
    ScopeId, TenantId, TrustLevel,
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

#[derive(Debug, Deserialize)]
struct GoldenCase {
    id: String,
    episodes: Vec<GoldenEpisode>,
    expected_actions: Vec<AdmissionAction>,
    expected_unit_count: usize,
    #[serde(default)]
    expected_semantic_bodies: Vec<String>,
    #[serde(default)]
    expected_belief_bodies: Vec<String>,
    #[serde(default)]
    expected_quarantined_bodies: Vec<String>,
    #[serde(default)]
    expected_freshness_due_bodies: Vec<String>,
    #[serde(default)]
    expected_edge_kinds: Vec<MemoryEdgeKind>,
}

#[derive(Debug, Deserialize)]
struct GoldenEpisode {
    source_kind: String,
    trust_level: TrustLevel,
    actor: u128,
    subject: Option<String>,
    predicate: Option<String>,
    body: String,
    churn_class: Option<String>,
    admission_hint: Option<AdmissionAction>,
}

#[tokio::test]
async fn write_compiler_golden_fixtures_pass() {
    let cases: Vec<GoldenCase> = serde_json::from_str(include_str!(
        "../../../examples/evals/wsb-write-goldens.json"
    ))
    .expect("fixtures parse");

    for case in cases {
        let store = InMemoryStore::default();
        let tenant_id = tenant(10_000);
        let scope_id = scope(20_000);
        let mut observed_actions = Vec::new();

        for episode in &case.episodes {
            let retained = retain_episode(
                &store,
                RetainRequest {
                    tenant_id,
                    scope_id,
                    actor_id: actor(episode.actor),
                    source_kind: episode.source_kind.clone(),
                    source_trust: episode.trust_level,
                    subject_hint: episode.subject.clone(),
                    body: episode.body.clone(),
                    compiler_version: "compiler-wsb-golden".to_string(),
                },
            )
            .await
            .unwrap_or_else(|error| panic!("{} retain failed: {error}", case.id));
            let job = store
                .reflect_jobs(tenant_id)
                .last()
                .cloned()
                .unwrap_or_else(|| panic!("{} missing reflect job", case.id));

            let trace = reflect_recorded(
                &store,
                ReflectInput {
                    tenant_id,
                    scope_id,
                    actor_id: actor(episode.actor),
                    episode_id: retained.episode_id,
                    job_id: job.id,
                    compiler_version: "compiler-wsb-golden".to_string(),
                    candidates: vec![ReflectCandidate {
                        source_kind: episode.source_kind.clone(),
                        trust_level: episode.trust_level,
                        actor_id: actor(episode.actor),
                        subject: episode.subject.clone(),
                        predicate: episode.predicate.clone(),
                        body: episode.body.clone(),
                        churn_class: episode.churn_class.clone(),
                        admission_hint: episode.admission_hint,
                    }],
                },
            )
            .await
            .unwrap_or_else(|error| panic!("{} reflect failed: {error}", case.id));

            assert_eq!(
                trace.stage_names(),
                [
                    "extract",
                    "detect",
                    "corroborate",
                    "promote",
                    "decay",
                    "trust"
                ],
                "{} trace stage contract",
                case.id
            );
            assert!(trace.cost_units > 0, "{} trace cost is recorded", case.id);
            observed_actions.extend(trace.actions);
        }

        assert_eq!(observed_actions, case.expected_actions, "{}", case.id);
        assert_eq!(
            store.memory_units(tenant_id).len(),
            case.expected_unit_count,
            "{} unit count",
            case.id
        );
        let semantic_bodies: Vec<_> = store
            .active_semantic_units(tenant_id)
            .into_iter()
            .map(|unit| unit.body)
            .collect();
        let belief_bodies: Vec<_> = store
            .belief_units(tenant_id)
            .into_iter()
            .map(|unit| unit.body)
            .collect();
        let edge_kinds: Vec<_> = store
            .memory_edges(tenant_id)
            .into_iter()
            .map(|edge| edge.kind)
            .collect();
        let quarantined_bodies: Vec<_> = store
            .quarantined_units(tenant_id)
            .into_iter()
            .map(|unit| unit.body)
            .collect();
        let freshness_due_bodies: Vec<_> = store
            .freshness_due_units(tenant_id)
            .into_iter()
            .map(|unit| unit.body)
            .collect();

        assert_eq!(
            semantic_bodies, case.expected_semantic_bodies,
            "{}",
            case.id
        );
        assert_eq!(belief_bodies, case.expected_belief_bodies, "{}", case.id);
        assert_eq!(
            quarantined_bodies, case.expected_quarantined_bodies,
            "{}",
            case.id
        );
        assert_eq!(
            freshness_due_bodies, case.expected_freshness_due_bodies,
            "{}",
            case.id
        );
        assert_eq!(edge_kinds, case.expected_edge_kinds, "{}", case.id);
        if case.id == "stale_fact_handling" {
            let active = store.active_semantic_units(tenant_id);
            assert_eq!(active[0].churn_class.as_deref(), Some("volatile"));
            assert!(active[0].freshness_due);
        }
    }
}

#[tokio::test]
async fn reflect_recorded_is_idempotent_for_duplicate_job_delivery() {
    let store = InMemoryStore::default();
    let tenant_id = tenant(30_000);
    let scope_id = scope(40_000);
    let actor_id = actor(50_000);
    let retained = retain_episode(
        &store,
        RetainRequest {
            tenant_id,
            scope_id,
            actor_id,
            source_kind: "user".to_string(),
            source_trust: TrustLevel::TrustedUser,
            subject_hint: Some("deployment region".to_string()),
            body: "Deployment region is Taipei.".to_string(),
            compiler_version: "compiler-wsb-golden".to_string(),
        },
    )
    .await
    .expect("retain succeeds");
    let job = store.reflect_jobs(tenant_id)[0].clone();
    let input = ReflectInput {
        tenant_id,
        scope_id,
        actor_id,
        episode_id: retained.episode_id,
        job_id: job.id,
        compiler_version: "compiler-wsb-golden".to_string(),
        candidates: vec![ReflectCandidate {
            source_kind: "user".to_string(),
            trust_level: TrustLevel::TrustedUser,
            actor_id,
            subject: Some("deployment region".to_string()),
            predicate: Some("value".to_string()),
            body: "Deployment region is Taipei.".to_string(),
            churn_class: None,
            admission_hint: None,
        }],
    };

    let first = reflect_recorded(&store, input.clone())
        .await
        .expect("first reflect succeeds");
    let second = reflect_recorded(&store, input)
        .await
        .expect("redelivery reflect succeeds");

    assert_eq!(first, second);
    assert_eq!(store.memory_units(tenant_id).len(), 1);
    assert_eq!(store.reflect_traces(tenant_id).len(), 1);
}

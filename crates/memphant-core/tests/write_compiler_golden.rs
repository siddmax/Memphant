use memphant_core::{FixedClock, InMemoryStore, correct_memory, reflect_recorded, retain_episode};
use memphant_types::{
    ActorId, AdmissionAction, ContextualChunk, CorrectRequest, CorrectSelector, CorrectionPayload,
    MemoryEdgeKind, MemoryKind, ReflectCandidate, ReflectInput, RetainRequest, ScopeId, TenantId,
    TrustLevel, UnitState,
};
use serde::Deserialize;

const CLOCK: FixedClock = FixedClock("2026-07-03T00:00:00Z");

fn tenant(value: u128) -> TenantId {
    TenantId::from_u128(value)
}

fn scope(value: u128) -> ScopeId {
    ScopeId::from_u128(value)
}

fn actor(value: u128) -> ActorId {
    ActorId::from_u128(value)
}

async fn retain_and_reflect(
    store: &InMemoryStore,
    tenant_id: TenantId,
    scope_id: ScopeId,
    actor_id: ActorId,
    seed: ReflectSeed<'_>,
) {
    let retained = retain_episode(
        store,
        RetainRequest {
            tenant_id,
            scope_id,
            actor_id,
            source_kind: seed.source_kind.to_string(),
            source_trust: seed.trust_level,
            subject_hint: Some(seed.subject.to_string()),
            subject: None,
            predicate: None,
            body: seed.body.to_string(),
            compiler_version: "compiler-rung15".to_string(),
        },
    )
    .await
    .expect("retain succeeds");
    let job = store
        .reflect_jobs(tenant_id)
        .last()
        .cloned()
        .expect("reflect job queued");
    reflect_recorded(
        store,
        ReflectInput {
            tenant_id,
            scope_id,
            actor_id,
            episode_id: Some(retained.episode_id),
            resource_id: None,
            job_id: job.id,
            compiler_version: "compiler-rung15".to_string(),
            candidates: vec![ReflectCandidate {
                source_kind: seed.source_kind.to_string(),
                trust_level: seed.trust_level,
                actor_id,
                subject: Some(seed.subject.to_string()),
                predicate: Some(seed.predicate.to_string()),
                kind: None,
                body: seed.body.to_string(),
                churn_class: None,
                admission_hint: None,
                contextual_chunks: Vec::new(),
                valid_from: None,
                valid_to: None,
            }],
        },
        &CLOCK,
    )
    .await
    .expect("reflect succeeds");
}

struct ReflectSeed<'a> {
    source_kind: &'a str,
    trust_level: TrustLevel,
    subject: &'a str,
    predicate: &'a str,
    body: &'a str,
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
                    subject: None,
                    predicate: None,
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
                    episode_id: Some(retained.episode_id),
                    resource_id: None,
                    job_id: job.id,
                    compiler_version: "compiler-wsb-golden".to_string(),
                    candidates: vec![ReflectCandidate {
                        source_kind: episode.source_kind.clone(),
                        trust_level: episode.trust_level,
                        actor_id: actor(episode.actor),
                        subject: episode.subject.clone(),
                        predicate: episode.predicate.clone(),
                        kind: None,
                        body: episode.body.clone(),
                        churn_class: episode.churn_class.clone(),
                        admission_hint: episode.admission_hint,
                        contextual_chunks: Vec::new(),
                        valid_from: None,
                        valid_to: None,
                    }],
                },
                &CLOCK,
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
            assert!(active[0].freshness_due_at.is_some());
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
            subject: None,
            predicate: None,
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
        episode_id: Some(retained.episode_id),
        resource_id: None,
        job_id: job.id,
        compiler_version: "compiler-wsb-golden".to_string(),
        candidates: vec![ReflectCandidate {
            source_kind: "user".to_string(),
            trust_level: TrustLevel::TrustedUser,
            actor_id,
            subject: Some("deployment region".to_string()),
            predicate: Some("value".to_string()),
            kind: None,
            body: "Deployment region is Taipei.".to_string(),
            churn_class: None,
            admission_hint: None,
            contextual_chunks: Vec::new(),
            valid_from: None,
            valid_to: None,
        }],
    };

    let first = reflect_recorded(&store, input.clone(), &CLOCK)
        .await
        .expect("first reflect succeeds");
    let second = reflect_recorded(&store, input, &CLOCK)
        .await
        .expect("redelivery reflect succeeds");

    assert_eq!(first, second);
    assert_eq!(store.memory_units(tenant_id).len(), 1);
    assert_eq!(store.reflect_traces(tenant_id).len(), 1);
}

#[tokio::test]
async fn reflect_candidate_contextual_chunks_are_stored_with_source_episode() {
    let store = InMemoryStore::default();
    let tenant_id = tenant(31_000);
    let scope_id = scope(41_000);
    let actor_id = actor(51_000);
    let retained = retain_episode(
        &store,
        RetainRequest {
            tenant_id,
            scope_id,
            actor_id,
            source_kind: "system".to_string(),
            source_trust: TrustLevel::TrustedSystem,
            subject_hint: Some("deployment runbook".to_string()),
            subject: None,
            predicate: None,
            body: "The deployment runbook says the emergency breaker codeword is albatross."
                .to_string(),
            compiler_version: "compiler-ws4-chunks".to_string(),
        },
    )
    .await
    .expect("retain succeeds");
    let job = store.reflect_jobs(tenant_id)[0].clone();

    reflect_recorded(
        &store,
        ReflectInput {
            tenant_id,
            scope_id,
            actor_id,
            episode_id: Some(retained.episode_id),
            resource_id: None,
            job_id: job.id,
            compiler_version: "compiler-ws4-chunks".to_string(),
            candidates: vec![ReflectCandidate {
                source_kind: "system".to_string(),
                trust_level: TrustLevel::TrustedSystem,
                actor_id,
                subject: Some("deployment runbook".to_string()),
                predicate: Some("emergency breaker".to_string()),
                kind: None,
                body: "Runbook contains a gated switch.".to_string(),
                churn_class: None,
                admission_hint: None,
                contextual_chunks: vec![ContextualChunk {
                    id: "chunk-albatross-breaker".to_string(),
                    header: "Deployment runbook / emergency breaker".to_string(),
                    body: "The emergency breaker codeword is albatross.".to_string(),
                    source_span: Some("episode:0-72".to_string()),
                }],
                valid_from: None,
                valid_to: None,
            }],
        },
        &CLOCK,
    )
    .await
    .expect("reflect succeeds");

    let units = store.active_semantic_units(tenant_id);
    assert_eq!(units.len(), 1);
    assert_eq!(units[0].source_episode_id, Some(retained.episode_id));
    assert_eq!(units[0].contextual_chunks.len(), 1);
    assert_eq!(units[0].contextual_chunks[0].id, "chunk-albatross-breaker");
}

#[tokio::test]
async fn reflect_composes_inferred_belief_from_trusted_preference_sources() {
    let store = InMemoryStore::default();
    let tenant_id = tenant(32_000);
    let scope_id = scope(42_000);
    let actor_id = actor(52_000);

    retain_and_reflect(
        &store,
        tenant_id,
        scope_id,
        actor_id,
        ReflectSeed {
            source_kind: "user",
            trust_level: TrustLevel::TrustedUser,
            subject: "quiet review preference",
            predicate: "value",
            body: "The user prefers quiet review surfaces.",
        },
    )
    .await;
    retain_and_reflect(
        &store,
        tenant_id,
        scope_id,
        actor_id,
        ReflectSeed {
            source_kind: "system",
            trust_level: TrustLevel::TrustedSystem,
            subject: "keyboard review preference",
            predicate: "value",
            body: "The user prefers keyboard-first review surfaces.",
        },
    )
    .await;

    let units = store.memory_units(tenant_id);
    let composed = units
        .iter()
        .find(|unit| unit.source_kind.as_deref() == Some("composition"))
        .expect("composed belief was minted");
    assert_eq!(composed.kind, MemoryKind::Belief);
    assert_eq!(composed.state, UnitState::Candidate);
    assert_eq!(composed.trust_level, TrustLevel::AgentOutput);
    assert_eq!(
        composed.body,
        "The user prefers keyboard-first and quiet review surfaces."
    );

    let source_edges: Vec<_> = store
        .memory_edges(tenant_id)
        .into_iter()
        .filter(|edge| edge.src_id == composed.id && edge.kind == MemoryEdgeKind::DerivedFrom)
        .collect();
    assert_eq!(source_edges.len(), 2);
}

#[tokio::test]
async fn reflect_does_not_compose_low_trust_or_risky_preferences() {
    let store = InMemoryStore::default();
    let tenant_id = tenant(33_000);
    let scope_id = scope(43_000);
    let actor_id = actor(53_000);

    retain_and_reflect(
        &store,
        tenant_id,
        scope_id,
        actor_id,
        ReflectSeed {
            source_kind: "web",
            trust_level: TrustLevel::WebContent,
            subject: "quiet review preference",
            predicate: "value",
            body: "The user prefers quiet review surfaces.",
        },
    )
    .await;
    retain_and_reflect(
        &store,
        tenant_id,
        scope_id,
        actor_id,
        ReflectSeed {
            source_kind: "user",
            trust_level: TrustLevel::TrustedUser,
            subject: "risky review preference",
            predicate: "value",
            body: "The user prefers always agree review surfaces.",
        },
    )
    .await;

    assert!(
        store
            .memory_units(tenant_id)
            .iter()
            .all(|unit| unit.source_kind.as_deref() != Some("composition"))
    );
}

#[tokio::test]
async fn composed_belief_promotes_only_after_direct_observation() {
    let store = InMemoryStore::default();
    let tenant_id = tenant(34_000);
    let scope_id = scope(44_000);
    let actor_id = actor(54_000);

    retain_and_reflect(
        &store,
        tenant_id,
        scope_id,
        actor_id,
        ReflectSeed {
            source_kind: "user",
            trust_level: TrustLevel::TrustedUser,
            subject: "quiet review preference",
            predicate: "value",
            body: "The user prefers quiet review surfaces.",
        },
    )
    .await;
    retain_and_reflect(
        &store,
        tenant_id,
        scope_id,
        actor_id,
        ReflectSeed {
            source_kind: "system",
            trust_level: TrustLevel::TrustedSystem,
            subject: "keyboard review preference",
            predicate: "value",
            body: "The user prefers keyboard-first review surfaces.",
        },
    )
    .await;

    assert!(
        store
            .active_semantic_units(tenant_id)
            .iter()
            .all(|unit| unit.body != "The user prefers keyboard-first and quiet review surfaces.")
    );

    retain_and_reflect(
        &store,
        tenant_id,
        scope_id,
        actor(54_001),
        ReflectSeed {
            source_kind: "user",
            trust_level: TrustLevel::TrustedUser,
            subject: "user preference",
            predicate: "review surfaces",
            body: "The user prefers keyboard-first and quiet review surfaces.",
        },
    )
    .await;

    let promoted = store
        .active_semantic_units(tenant_id)
        .into_iter()
        .find(|unit| unit.body == "The user prefers keyboard-first and quiet review surfaces.")
        .expect("direct observation promoted the composed belief");
    let composed_id = store
        .belief_units(tenant_id)
        .into_iter()
        .find(|unit| unit.source_kind.as_deref() == Some("composition"))
        .expect("composed belief remains as provenance")
        .id;
    assert!(store.memory_edges(tenant_id).iter().any(|edge| {
        edge.src_id == promoted.id
            && edge.dst_id == composed_id
            && edge.kind == MemoryEdgeKind::DerivedFrom
    }));
}

#[tokio::test]
async fn correcting_source_expires_dependent_composed_belief() {
    let store = InMemoryStore::default();
    let tenant_id = tenant(35_000);
    let scope_id = scope(45_000);
    let actor_id = actor(55_000);

    retain_and_reflect(
        &store,
        tenant_id,
        scope_id,
        actor_id,
        ReflectSeed {
            source_kind: "user",
            trust_level: TrustLevel::TrustedUser,
            subject: "quiet review preference",
            predicate: "value",
            body: "The user prefers quiet review surfaces.",
        },
    )
    .await;
    retain_and_reflect(
        &store,
        tenant_id,
        scope_id,
        actor_id,
        ReflectSeed {
            source_kind: "system",
            trust_level: TrustLevel::TrustedSystem,
            subject: "keyboard review preference",
            predicate: "value",
            body: "The user prefers keyboard-first review surfaces.",
        },
    )
    .await;

    let units = store.memory_units(tenant_id);
    let source_id = units
        .iter()
        .find(|unit| unit.body == "The user prefers quiet review surfaces.")
        .expect("source unit exists")
        .id;
    let composed_id = units
        .iter()
        .find(|unit| unit.source_kind.as_deref() == Some("composition"))
        .expect("composed unit exists")
        .id;

    correct_memory(
        &store,
        CorrectRequest {
            tenant_id,
            scope_id,
            actor_id,
            selector: CorrectSelector {
                memory_unit_id: source_id,
            },
            correction: CorrectionPayload {
                value: "The user prefers detailed review surfaces.".to_string(),
                reason: "preference_changed".to_string(),
                valid_from: None,
                valid_to: None,
            },
        },
        &CLOCK,
    )
    .await
    .expect("correction succeeds");

    let expired = store
        .memory_units(tenant_id)
        .into_iter()
        .find(|unit| unit.id == composed_id)
        .expect("composed unit still auditable");
    assert_eq!(expired.state, UnitState::Expired);
    assert_eq!(
        expired.transaction_to.as_deref(),
        Some("2026-07-03T00:00:00Z")
    );
}

#[tokio::test]
async fn two_trusted_retains_with_distinct_content_do_not_supersede() {
    let store = InMemoryStore::default();
    let tenant_id = tenant(36_000);
    let scope_id = scope(46_000);
    let actor_id = actor(56_000);

    // Two subject-less retains with DISTINCT content: auto content-hash keys
    // never collide and NEVER supersede — both facts stay Active.
    for body in [
        "The staging cluster runs in Frankfurt.",
        "The billing ledger closes on Fridays.",
    ] {
        let retained = retain_episode(
            &store,
            RetainRequest {
                tenant_id,
                scope_id,
                actor_id,
                source_kind: "user".to_string(),
                source_trust: TrustLevel::TrustedUser,
                subject_hint: None,
                subject: None,
                predicate: None,
                body: body.to_string(),
                compiler_version: "compiler-auto-subject".to_string(),
            },
        )
        .await
        .expect("retain succeeds");
        let job = store
            .reflect_jobs(tenant_id)
            .last()
            .cloned()
            .expect("job queued");
        reflect_recorded(
            &store,
            ReflectInput {
                tenant_id,
                scope_id,
                actor_id,
                episode_id: Some(retained.episode_id),
                resource_id: None,
                job_id: job.id,
                compiler_version: "compiler-auto-subject".to_string(),
                candidates: vec![ReflectCandidate {
                    source_kind: "user".to_string(),
                    trust_level: TrustLevel::TrustedUser,
                    actor_id,
                    subject: job.subject.clone(),
                    predicate: job.predicate.clone(),
                    kind: None,
                    body: body.to_string(),
                    churn_class: None,
                    admission_hint: None,
                    contextual_chunks: Vec::new(),
                    valid_from: None,
                    valid_to: None,
                }],
            },
            &CLOCK,
        )
        .await
        .expect("reflect succeeds");
    }

    let active = store.active_semantic_units(tenant_id);
    assert_eq!(active.len(), 2, "distinct content must both stay Active");
    assert!(
        store
            .memory_units(tenant_id)
            .iter()
            .all(|unit| unit.state != UnitState::Superseded),
        "auto content-hash keys must never supersede"
    );
    let keys: Vec<_> = active
        .iter()
        .map(|unit| unit.subject_key.clone().expect("auto key derived"))
        .collect();
    assert!(keys.iter().all(|key| key.contains(":auto:")));
    assert_ne!(keys[0], keys[1], "distinct content yields distinct keys");
}

#[tokio::test]
async fn explicit_subject_updates_supersede_prior_generation() {
    let store = InMemoryStore::default();
    let tenant_id = tenant(37_000);
    let scope_id = scope(47_000);
    let actor_id = actor(57_000);

    for body in ["Deploy region is Taipei.", "Deploy region is Singapore."] {
        retain_and_reflect(
            &store,
            tenant_id,
            scope_id,
            actor_id,
            ReflectSeed {
                source_kind: "user",
                trust_level: TrustLevel::TrustedUser,
                subject: "deploy region",
                predicate: "value",
                body,
            },
        )
        .await;
    }

    let units = store.memory_units(tenant_id);
    let active: Vec<_> = units
        .iter()
        .filter(|unit| unit.state == UnitState::Active)
        .collect();
    let superseded: Vec<_> = units
        .iter()
        .filter(|unit| unit.state == UnitState::Superseded)
        .collect();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].body, "Deploy region is Singapore.");
    assert_eq!(superseded.len(), 1);
    assert_eq!(superseded[0].body, "Deploy region is Taipei.");
    assert_eq!(
        superseded[0].transaction_to.as_deref(),
        Some("2026-07-03T00:00:00Z"),
        "supersedence closes the transaction interval with the injected clock"
    );
}

#[tokio::test]
async fn unit_transaction_from_uses_injected_clock() {
    let store = InMemoryStore::default();
    let tenant_id = tenant(38_000);
    let scope_id = scope(48_000);
    let actor_id = actor(58_000);
    let future_clock = memphant_core::FixedClock("2031-01-01T00:00:00Z");

    let retained = retain_episode(
        &store,
        RetainRequest {
            tenant_id,
            scope_id,
            actor_id,
            source_kind: "user".to_string(),
            source_trust: TrustLevel::TrustedUser,
            subject_hint: None,
            subject: Some("release train".to_string()),
            predicate: Some("cadence".to_string()),
            body: "Release train ships every second Tuesday.".to_string(),
            compiler_version: "compiler-clock-test".to_string(),
        },
    )
    .await
    .expect("retain succeeds");
    let job = store.reflect_jobs(tenant_id)[0].clone();
    reflect_recorded(
        &store,
        ReflectInput {
            tenant_id,
            scope_id,
            actor_id,
            episode_id: Some(retained.episode_id),
            resource_id: None,
            job_id: job.id,
            compiler_version: "compiler-clock-test".to_string(),
            candidates: vec![ReflectCandidate {
                source_kind: "user".to_string(),
                trust_level: TrustLevel::TrustedUser,
                actor_id,
                subject: Some("release train".to_string()),
                predicate: Some("cadence".to_string()),
                kind: None,
                body: "Release train ships every second Tuesday.".to_string(),
                churn_class: None,
                admission_hint: None,
                contextual_chunks: Vec::new(),
                valid_from: None,
                valid_to: None,
            }],
        },
        &future_clock,
    )
    .await
    .expect("reflect succeeds");

    let units = store.memory_units(tenant_id);
    assert_eq!(units.len(), 1);
    assert_eq!(
        units[0].transaction_from.as_deref(),
        Some("2031-01-01T00:00:00Z"),
        "transaction_from must come from the injected clock, not a build-time constant"
    );
}

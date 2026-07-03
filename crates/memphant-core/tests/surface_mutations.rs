use memphant_core::{
    InMemoryStore, MemoryStore, correct_memory, forget_memory, recall, record_mark,
};
use memphant_types::{
    ActorId, CorrectRequest, CorrectSelector, CorrectionPayload, ForgetRequest, ForgetSelector,
    MarkOutcome, MarkRequest, MemoryKind, NewEpisode, NewMemoryUnit, RecallMode, RecallRequest,
    ScopeId, TenantId, TraceId, TrustLevel, UnitState,
};

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
async fn correct_supersedes_old_generation_and_recall_returns_new_value() {
    let store = InMemoryStore::default();
    let tenant_id = tenant(80_000);
    let scope_id = scope(80_001);
    let actor_id = actor(80_002);
    let old_id = seed_active_unit(
        &store,
        tenant_id,
        scope_id,
        actor_id,
        "callback_token:value",
        "Callback token is v1.",
    )
    .await;

    let corrected = correct_memory(
        &store,
        CorrectRequest {
            tenant_id,
            scope_id,
            actor_id,
            selector: CorrectSelector {
                memory_unit_id: old_id,
            },
            correction: CorrectionPayload {
                value: "Callback token is v2.".to_string(),
                reason: "stale_fact".to_string(),
                valid_from: None,
                valid_to: None,
            },
        },
    )
    .await
    .expect("correction succeeds");

    assert_eq!(corrected.superseded, vec![old_id]);
    assert_eq!(corrected.created.len(), 1);
    let units = store.memory_units(tenant_id);
    assert_eq!(
        units.iter().find(|unit| unit.id == old_id).unwrap().state,
        UnitState::Superseded
    );
    assert_eq!(
        units
            .iter()
            .find(|unit| unit.id == corrected.created[0])
            .unwrap()
            .body,
        "Callback token is v2."
    );

    let recalled = recall(
        &store,
        RecallRequest {
            tenant_id,
            scope_id,
            actor_id,
            allowed_scope_ids: vec![scope_id],
            query: "Which callback token is current?".to_string(),
            k: 3,
            budget_tokens: 80,
            mode: RecallMode::Fast,
            include_beliefs: false,
            engine_version: "engine-wsd-test".to_string(),
        },
    )
    .await
    .expect("recall succeeds");

    assert_eq!(recalled.items.len(), 1);
    assert_eq!(recalled.items[0].body, "Callback token is v2.");
}

#[tokio::test]
async fn forget_marks_memory_deleted_and_recall_hides_it() {
    let store = InMemoryStore::default();
    let tenant_id = tenant(81_000);
    let scope_id = scope(81_001);
    let actor_id = actor(81_002);
    let unit_id = seed_active_unit(
        &store,
        tenant_id,
        scope_id,
        actor_id,
        "refund_window:value",
        "Refund window is 30 days.",
    )
    .await;

    let forgotten = forget_memory(
        &store,
        ForgetRequest {
            tenant_id,
            scope_id,
            actor_id,
            selector: ForgetSelector {
                memory_unit_id: Some(unit_id),
                scope_id: None,
            },
            reason: "user_request".to_string(),
        },
    )
    .await
    .expect("forget succeeds");

    assert_eq!(forgotten.invalidated_units, vec![unit_id]);
    assert!(forgotten.deletion_generation > 0);
    let unit = store
        .memory_units(tenant_id)
        .into_iter()
        .find(|unit| unit.id == unit_id)
        .expect("unit remains as tombstone");
    assert_eq!(unit.state, UnitState::Deleted);
    assert_eq!(
        unit.deletion_generation,
        Some(forgotten.deletion_generation)
    );

    let recalled = recall(
        &store,
        RecallRequest {
            tenant_id,
            scope_id,
            actor_id,
            allowed_scope_ids: vec![scope_id],
            query: "What is the refund window?".to_string(),
            k: 3,
            budget_tokens: 80,
            mode: RecallMode::Fast,
            include_beliefs: false,
            engine_version: "engine-wsd-test".to_string(),
        },
    )
    .await
    .expect("recall succeeds");

    assert!(recalled.items.is_empty());
}

#[tokio::test]
async fn mark_records_outcome_feedback_for_trace() {
    let store = InMemoryStore::default();
    let tenant_id = tenant(82_000);
    let scope_id = scope(82_001);
    let actor_id = actor(82_002);
    let unit_id = seed_active_unit(
        &store,
        tenant_id,
        scope_id,
        actor_id,
        "deploy_region:value",
        "Deploy region is Taipei.",
    )
    .await;
    let trace_id = TraceId::new();

    let marked = record_mark(
        &store,
        MarkRequest {
            tenant_id,
            trace_id,
            caller_id: "surface-contract-test".to_string(),
            used_ids: vec![unit_id],
            outcome: MarkOutcome::Success,
        },
    )
    .await
    .expect("mark succeeds");

    assert!(marked.accepted);
    let events = store.review_events(tenant_id);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].trace_id, trace_id);
    assert_eq!(events[0].used_ids, vec![unit_id]);
    assert_eq!(events[0].outcome, MarkOutcome::Success);
}

async fn seed_active_unit(
    store: &InMemoryStore,
    tenant_id: TenantId,
    scope_id: ScopeId,
    actor_id: ActorId,
    subject_key: &str,
    body: &str,
) -> memphant_types::UnitId {
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
                dedup_key: format!("{subject_key}:{body}"),
                body: body.to_string(),
            },
        )
        .await
        .expect("episode seed");
    let unit_id = store
        .stage_memory_unit(
            &mut tx,
            NewMemoryUnit {
                tenant_id,
                scope_id,
                kind: MemoryKind::Semantic,
                state: UnitState::Active,
                subject_key: Some(subject_key.to_string()),
                body: body.to_string(),
                trust_level: TrustLevel::TrustedSystem,
                churn_class: None,
                freshness_due: false,
                actor_id: Some(actor_id),
                source_kind: Some("system".to_string()),
                source_episode_id: Some(episode.episode_id),
                source_resource_id: None,
                deletion_generation: None,
                contextual_chunks: Vec::new(),
            },
        )
        .await
        .expect("unit seed");
    store.commit(tx).await.expect("seed commit");
    unit_id
}

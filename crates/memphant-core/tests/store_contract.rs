use memphant_core::{InMemoryStore, MemoryStore, retain_episode, retain_resource};
use memphant_types::{
    ActorId, MemoryKind, NewEpisode, NewMemoryUnit, ResourceExtractorState, RetainRequest,
    RetainResourceRequest, ScopeId, TenantId, TrustLevel, UnitState,
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
async fn retain_pipeline_stores_episode_and_reflect_job_atomically() {
    let store = InMemoryStore::default();
    let tenant_id = tenant(900);
    let scope_id = scope(901);
    let actor_id = actor(902);

    let result = retain_episode(
        &store,
        RetainRequest {
            tenant_id,
            scope_id,
            actor_id,
            source_kind: "user".to_string(),
            source_trust: TrustLevel::TrustedUser,
            subject_hint: Some("deploy channel".to_string()),
            subject: None,
            predicate: None,
            body: "Remember the deploy channel is #launch.".to_string(),
            compiler_version: "compiler-wsb-test".to_string(),
        },
    )
    .await
    .expect("retain succeeds");

    let episodes = store.episodes(tenant_id);
    let jobs = store.reflect_jobs(tenant_id);

    assert_eq!(episodes.len(), 1);
    assert_eq!(jobs.len(), 1);
    assert_eq!(episodes[0].id, result.episode_id);
    assert_eq!(jobs[0].episode_id, Some(result.episode_id));
    assert_eq!(jobs[0].compiler_version, "compiler-wsb-test");
}

#[tokio::test]
async fn retain_pipeline_collapses_duplicate_episode_by_dedup_key() {
    let store = InMemoryStore::default();
    let tenant_id = tenant(910);
    let scope_id = scope(911);
    let actor_id = actor(912);
    let request = RetainRequest {
        tenant_id,
        scope_id,
        actor_id,
        source_kind: "tool".to_string(),
        source_trust: TrustLevel::VerifiedTool,
        subject_hint: Some("node version".to_string()),
        subject: None,
        predicate: None,
        body: "Staging pins Node 24.15.0.".to_string(),
        compiler_version: "compiler-wsb-test".to_string(),
    };

    let first = retain_episode(&store, request.clone())
        .await
        .expect("first retain succeeds");
    let second = retain_episode(&store, request)
        .await
        .expect("second retain succeeds");

    let episodes = store.episodes(tenant_id);
    let jobs = store.reflect_jobs(tenant_id);

    assert_eq!(episodes.len(), 1);
    assert_eq!(episodes[0].id, first.episode_id);
    assert_eq!(episodes[0].observation_count, 2);
    assert!(!first.dedup.matched);
    assert!(second.dedup.matched);
    assert_eq!(second.episode_id, first.episode_id);
    assert_eq!(second.dedup.observation_count, 2);
    assert_eq!(jobs.len(), 2);
    assert!(
        jobs.iter()
            .all(|job| job.episode_id == Some(first.episode_id))
    );
}

#[tokio::test]
async fn retain_resource_stores_pointer_before_extraction_and_enqueues_reflect() {
    let store = InMemoryStore::default();
    let tenant_id = tenant(920);
    let scope_id = scope(921);
    let actor_id = actor(922);

    let retained = retain_resource(
        &store,
        RetainResourceRequest {
            tenant_id,
            scope_id,
            actor_id,
            uri: "https://example.test/runbooks/deploy.md".to_string(),
            kind: None,
            content_hash: "sha256:deploy-runbook".to_string(),
            mime_type: "text/markdown".to_string(),
            revision: None,
            body: Some("Deploy runbook body: roll forward, never force-push.".to_string()),
            source_trust: TrustLevel::WebContent,
            compiler_version: "compiler-wsb-test".to_string(),
        },
    )
    .await
    .expect("resource retain succeeds");

    let resources = store.resources(tenant_id);
    let jobs = store.reflect_jobs(tenant_id);

    assert_eq!(resources.len(), 1);
    assert_eq!(resources[0].id, retained.resource_id);
    assert_eq!(
        resources[0].extractor_state,
        ResourceExtractorState::Registered
    );
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].resource_id, Some(retained.resource_id));
}

#[tokio::test]
async fn committed_transaction_publishes_staged_episode_and_unit() {
    let store = InMemoryStore::default();
    let tenant_id = tenant(1);
    let scope_id = scope(2);

    let mut tx = store.begin().await;
    let episode = store
        .stage_episode(
            &mut tx,
            NewEpisode {
                tenant_id,
                scope_id,
                actor_id: actor(3),
                source_kind: "user".to_string(),
                source_trust: TrustLevel::TrustedUser,
                dedup_key: "scope:user:hello".to_string(),
                body: "Remember the deploy channel is #launch.".to_string(),
            },
        )
        .await
        .expect("episode stages");
    let unit = store
        .stage_memory_unit(
            &mut tx,
            NewMemoryUnit {
                tenant_id,
                scope_id,
                kind: MemoryKind::Semantic,
                state: UnitState::Active,
                subject_key: Some("deploy_channel:value".to_string()),
                body: "Deploy channel is #launch.".to_string(),
                trust_level: TrustLevel::TrustedUser,
                churn_class: None,
                freshness_due_at: None,
                actor_id: None,
                source_kind: None,
                source_episode_id: Some(episode.episode_id),
                source_resource_id: None,
                deletion_generation: None,
                contextual_chunks: Vec::new(),
                valid_from: None,
                valid_to: None,
                transaction_from: None,
                transaction_to: None,
            },
        )
        .await
        .expect("unit stages");

    assert!(store.episodes(tenant_id).is_empty());
    assert!(store.memory_units(tenant_id).is_empty());

    store.commit(tx).await.expect("commit succeeds");

    assert_eq!(store.episodes(tenant_id)[0].id, episode.episode_id);
    assert_eq!(store.memory_units(tenant_id)[0].id, unit);
}

#[tokio::test]
async fn dropped_transaction_rolls_back_staged_rows() {
    let store = InMemoryStore::default();
    let tenant_id = tenant(10);

    {
        let mut tx = store.begin().await;
        store
            .stage_episode(
                &mut tx,
                NewEpisode {
                    tenant_id,
                    scope_id: scope(20),
                    actor_id: actor(30),
                    source_kind: "agent".to_string(),
                    source_trust: TrustLevel::AgentOutput,
                    dedup_key: "agent:discarded".to_string(),
                    body: "This row is staged only.".to_string(),
                },
            )
            .await
            .expect("episode stages");
    }

    assert!(store.episodes(tenant_id).is_empty());
}

#[test]
fn new_episode_and_unit_shapes_require_tenant_and_scope_ids() {
    let episode = NewEpisode {
        tenant_id: tenant(100),
        scope_id: scope(200),
        actor_id: actor(300),
        source_kind: "tool".to_string(),
        source_trust: TrustLevel::VerifiedTool,
        dedup_key: "tool:result".to_string(),
        body: "Tool result stored as raw episode.".to_string(),
    };
    let unit = NewMemoryUnit {
        tenant_id: episode.tenant_id,
        scope_id: episode.scope_id,
        kind: MemoryKind::Episodic,
        state: UnitState::Captured,
        subject_key: None,
        body: episode.body.clone(),
        trust_level: episode.source_trust,
        churn_class: None,
        freshness_due_at: None,
        actor_id: Some(episode.actor_id),
        source_kind: Some(episode.source_kind.clone()),
        source_episode_id: None,
        source_resource_id: None,
        deletion_generation: None,
        contextual_chunks: Vec::new(),
        valid_from: None,
        valid_to: None,
        transaction_from: None,
        transaction_to: None,
    };

    assert_eq!(episode.tenant_id, unit.tenant_id);
    assert_eq!(episode.scope_id, unit.scope_id);
}

#[tokio::test]
async fn candidates_fetch_respects_tenant_and_scope() {
    let store = InMemoryStore::default();
    let tenant_a = tenant(70_000);
    let tenant_b = tenant(70_001);
    let scope_a = scope(70_002);
    let scope_b = scope(70_003);

    for (tenant_id, scope_id, body) in [
        (tenant_a, scope_a, "Tenant A scope A fact."),
        (tenant_a, scope_b, "Tenant A scope B fact."),
        (tenant_b, scope_a, "Tenant B scope A fact."),
    ] {
        let mut tx = store.begin().await;
        store
            .stage_memory_unit(
                &mut tx,
                NewMemoryUnit {
                    tenant_id,
                    scope_id,
                    kind: MemoryKind::Semantic,
                    state: UnitState::Active,
                    subject_key: None,
                    body: body.to_string(),
                    trust_level: TrustLevel::TrustedSystem,
                    churn_class: None,
                    freshness_due_at: None,
                    actor_id: None,
                    source_kind: None,
                    source_episode_id: None,
                    source_resource_id: None,
                    deletion_generation: None,
                    contextual_chunks: Vec::new(),
                    valid_from: None,
                    valid_to: None,
                    transaction_from: None,
                    transaction_to: None,
                },
            )
            .await
            .expect("unit stages");
        store.commit(tx).await.expect("commit");
    }

    let candidates = store
        .fetch_recall_candidates(tenant_a, &[scope_a], &[], &[], None, usize::MAX)
        .await
        .expect("fetch succeeds");

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].body, "Tenant A scope A fact.");
}

#[tokio::test]
async fn trace_by_id_with_wrong_tenant_returns_none() {
    let store = InMemoryStore::default();
    let tenant_a = tenant(71_000);
    let tenant_b = tenant(71_001);
    let scope_id = scope(71_002);
    let actor_id = actor(71_003);

    let response = memphant_core::recall(
        &store,
        memphant_types::RecallRequest {
            tenant_id: tenant_a,
            scope_id,
            actor_id,
            allowed_scope_ids: vec![scope_id],
            query: "anything".to_string(),
            k: 4,
            budget_tokens: 128,
            mode: memphant_types::RecallMode::Fast,
            include_beliefs: false,
            edge_expansion_enabled: true,
            context_packing_abstention_enabled: true,
            rerank_enabled: true,
            learned_rerank_profile: None,
            query_decomposition_enabled: true,
            procedure_recall_enabled: true,
            decay_enabled: true,
            engine_version: "store-contract-test".to_string(),
        },
        None,
        &memphant_core::FixedClock("2026-07-03T00:00:00Z"),
    )
    .await
    .expect("recall succeeds");

    let own = store
        .trace_by_id(tenant_a, response.trace_id)
        .await
        .expect("lookup succeeds");
    assert!(own.is_some(), "owner tenant sees its trace");

    let cross = store
        .trace_by_id(tenant_b, response.trace_id)
        .await
        .expect("lookup succeeds");
    assert!(cross.is_none(), "wrong tenant must get None, never a trace");
}

#[tokio::test]
async fn forget_by_episode_hides_units_and_tombstone_blocks_recompilation() {
    use memphant_core::{FixedClock, NoopEmbedding, forget_memory, recall, reflect_recorded};
    use memphant_types::{
        ForgetRequest, ForgetSelector, RecallMode, RecallRequest, ReflectCandidate, ReflectInput,
    };

    const CLOCK: FixedClock = FixedClock("2026-07-03T00:00:00Z");
    let store = InMemoryStore::default();
    let tenant_id = tenant(72_000);
    let scope_id = scope(72_001);
    let actor_id = actor(72_002);

    let retained = retain_episode(
        &store,
        RetainRequest {
            tenant_id,
            scope_id,
            actor_id,
            source_kind: "user".to_string(),
            source_trust: TrustLevel::TrustedUser,
            subject_hint: None,
            subject: Some("payment processor".to_string()),
            predicate: Some("value".to_string()),
            body: "Payment processor is AcmePay.".to_string(),
            compiler_version: "compiler-forget-test".to_string(),
        },
    )
    .await
    .expect("retain succeeds");
    let job = store.reflect_jobs(tenant_id)[0].clone();
    let reflect_input = |compiler_version: &str| ReflectInput {
        tenant_id,
        scope_id,
        actor_id,
        episode_id: Some(retained.episode_id),
        resource_id: None,
        job_id: job.id,
        compiler_version: compiler_version.to_string(),
        candidates: vec![ReflectCandidate {
            source_kind: "user".to_string(),
            trust_level: TrustLevel::TrustedUser,
            actor_id,
            subject: Some("payment processor".to_string()),
            predicate: Some("value".to_string()),
            kind: None,
            body: "Payment processor is AcmePay.".to_string(),
            churn_class: None,
            admission_hint: None,
            contextual_chunks: Vec::new(),
            valid_from: None,
            valid_to: None,
        }],
    };
    reflect_recorded(
        &store,
        reflect_input("compiler-forget-test"),
        &NoopEmbedding,
        &CLOCK,
    )
    .await
    .expect("reflect succeeds");
    assert_eq!(store.active_semantic_units(tenant_id).len(), 1);

    let forgotten = forget_memory(
        &store,
        ForgetRequest {
            tenant_id,
            scope_id,
            actor_id,
            selector: ForgetSelector {
                memory_unit_id: None,
                episode_id: Some(retained.episode_id),
                resource_id: None,
                scope_id,
            },
            reason: "user_request".to_string(),
        },
        &CLOCK,
    )
    .await
    .expect("forget succeeds");
    assert_eq!(forgotten.invalidated_units.len(), 1);
    assert_eq!(
        forgotten.verification, "post_forget_recall_probe_hits=0",
        "forget verification is a real recall probe, not a hardcoded string"
    );
    assert!(store.active_semantic_units(tenant_id).is_empty());

    // A second reflect with a BUMPED compiler version must NOT resurrect the
    // forgotten fact: the forgotten-source tombstone blocks re-derivation.
    reflect_recorded(
        &store,
        reflect_input("compiler-forget-test-v2"),
        &NoopEmbedding,
        &CLOCK,
    )
    .await
    .expect("recompilation runs");
    assert!(
        store.active_semantic_units(tenant_id).is_empty(),
        "tombstoned episode must not re-derive units"
    );

    let recalled = recall(
        &store,
        RecallRequest {
            tenant_id,
            scope_id,
            actor_id,
            allowed_scope_ids: vec![scope_id],
            query: "Which payment processor do we use?".to_string(),
            k: 4,
            budget_tokens: 128,
            mode: RecallMode::Fast,
            include_beliefs: true,
            edge_expansion_enabled: true,
            context_packing_abstention_enabled: true,
            rerank_enabled: true,
            learned_rerank_profile: None,
            query_decomposition_enabled: true,
            procedure_recall_enabled: true,
            decay_enabled: true,
            engine_version: "store-contract-test".to_string(),
        },
        None,
        &CLOCK,
    )
    .await
    .expect("recall succeeds");
    assert!(recalled.items.is_empty(), "forgotten memory must stay gone");
}

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
            content_hash: "sha256:deploy-runbook".to_string(),
            mime_type: "text/markdown".to_string(),
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
                freshness_due: false,
                actor_id: None,
                source_kind: None,
                source_episode_id: Some(episode.episode_id),
                source_resource_id: None,
                deletion_generation: None,
                contextual_chunks: Vec::new(),
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
        freshness_due: false,
        actor_id: Some(episode.actor_id),
        source_kind: Some(episode.source_kind.clone()),
        source_episode_id: None,
        source_resource_id: None,
        deletion_generation: None,
        contextual_chunks: Vec::new(),
    };

    assert_eq!(episode.tenant_id, unit.tenant_id);
    assert_eq!(episode.scope_id, unit.scope_id);
}

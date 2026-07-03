use memphant_core::{InMemoryStore, MemoryStore};
use memphant_types::{
    ActorId, MemoryKind, NewEpisode, NewMemoryUnit, ScopeId, TenantId, TrustLevel, UnitState,
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
    };

    assert_eq!(episode.tenant_id, unit.tenant_id);
    assert_eq!(episode.scope_id, unit.scope_id);
}

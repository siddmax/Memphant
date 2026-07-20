use std::sync::Arc;

use memphant_core::{
    FixedClock, InMemoryStore, MemoryStore, MutationClaim, MutationClaimOutcome,
    MutationLedgerStore, MutationResponse, MutationVerb, StoreError,
};
use memphant_types::{
    ContextBindingAgentRef, ContextBindingEntityRef, ContextBindingRequest, ContextBindingScopeRef,
    NewEpisode, ResolvedMemoryContext, TenantId, TrustLevel,
};

async fn context(store: &InMemoryStore, tenant: TenantId, name: &str) -> ResolvedMemoryContext {
    let binding = store
        .resolve_context_binding(
            tenant,
            name.to_string(),
            ContextBindingRequest {
                subject: ContextBindingEntityRef {
                    external_ref: format!("subject:{name}"),
                    kind: "user".to_string(),
                },
                actor: ContextBindingEntityRef {
                    external_ref: format!("actor:{name}"),
                    kind: "user".to_string(),
                },
                scope: ContextBindingScopeRef {
                    external_ref: format!("scope:{name}"),
                    kind: "memory".to_string(),
                    parent_external_ref: None,
                },
                agent_node: ContextBindingAgentRef {
                    external_ref: format!("agent:{name}"),
                    parent_external_ref: None,
                },
                access_policies: Vec::new(),
            },
        )
        .await
        .unwrap();
    store
        .resolve_memory_context(
            tenant,
            binding.subject_id,
            binding.actor_id,
            binding.scope_id,
            binding.agent_node_id,
        )
        .await
        .unwrap()
}

fn claim(context: &ResolvedMemoryContext, hash: u8) -> MutationClaim {
    MutationClaim::new(context, MutationVerb::Retain, "retain-1", [hash; 32]).unwrap()
}

fn episode(context: &ResolvedMemoryContext, key: &str) -> NewEpisode {
    NewEpisode {
        tenant_id: context.tenant_id,
        data_subject_id: context.data_subject_id,
        scope_id: context.scope_id,
        actor_id: context.actor_id,
        agent_node_id: context.agent_node_id,
        subject_generation: context.subject_generation,
        source_kind: "user".to_string(),
        source_ref: "test:fixture".to_string(),
        observed_at: "2026-07-09T00:00:00Z".to_string(),
        source_trust: TrustLevel::TrustedUser,
        dedup_key: key.to_string(),
        body: "atomic mutation body".to_string(),
    }
}

fn response(body: &str) -> MutationResponse {
    MutationResponse::success(201, body.as_bytes().to_vec()).unwrap()
}

async fn run_mutation(
    store: Arc<InMemoryStore>,
    context: ResolvedMemoryContext,
    label: &'static str,
) -> MutationResponse {
    let mut tx = store.begin_at(&context, &FixedClock("2026-07-15T00:00:00Z"));
    match store
        .stage_mutation_claim(&mut tx, claim(&context, 1))
        .await
        .unwrap()
    {
        MutationClaimOutcome::Execute => {
            store
                .stage_episode(&mut tx, episode(&context, label))
                .await
                .unwrap();
            let response = response(label);
            store
                .stage_mutation_response(&mut tx, response.clone())
                .await
                .unwrap();
            store.commit(tx).await.unwrap();
            response
        }
        MutationClaimOutcome::Replay(response) => {
            store.commit(tx).await.unwrap();
            response
        }
    }
}

#[tokio::test]
async fn replay_is_exact_and_conflicting_hash_or_context_is_rejected() {
    let store = InMemoryStore::default();
    let tenant = TenantId::new();
    let first = context(&store, tenant, "first").await;
    let second = context(&store, tenant, "second").await;
    let clock = FixedClock("2026-07-15T00:00:00Z");

    let mut tx = store.begin_at(&first, &clock);
    assert_eq!(
        store
            .stage_mutation_claim(&mut tx, claim(&first, 1))
            .await
            .unwrap(),
        MutationClaimOutcome::Execute
    );
    store
        .stage_episode(&mut tx, episode(&first, "once"))
        .await
        .unwrap();
    store
        .stage_mutation_response(&mut tx, response("canonical"))
        .await
        .unwrap();
    store.commit(tx).await.unwrap();

    let mut replay = store.begin_at(&first, &clock);
    assert_eq!(
        store
            .stage_mutation_claim(&mut replay, claim(&first, 1))
            .await
            .unwrap(),
        MutationClaimOutcome::Replay(response("canonical"))
    );
    store.commit(replay).await.unwrap();
    assert_eq!(store.episodes(tenant).len(), 1);

    let mut changed_hash = store.begin_at(&first, &clock);
    assert!(matches!(
        store
            .stage_mutation_claim(&mut changed_hash, claim(&first, 2))
            .await,
        Err(StoreError::IdempotencyConflict)
    ));
    let mut changed_context = store.begin_at(&second, &clock);
    assert!(matches!(
        store
            .stage_mutation_claim(&mut changed_context, claim(&second, 1))
            .await,
        Err(StoreError::IdempotencyConflict)
    ));
}

#[tokio::test]
async fn rollback_and_expiry_leave_no_stale_ledger_or_writes() {
    let store = InMemoryStore::default();
    let tenant = TenantId::new();
    let context = context(&store, tenant, "expiry").await;
    let start = FixedClock("2026-07-15T00:00:00Z");

    let mut abandoned = store.begin_at(&context, &start);
    assert_eq!(
        store
            .stage_mutation_claim(&mut abandoned, claim(&context, 1))
            .await
            .unwrap(),
        MutationClaimOutcome::Execute
    );
    store
        .stage_episode(&mut abandoned, episode(&context, "abandoned"))
        .await
        .unwrap();
    store
        .stage_mutation_response(&mut abandoned, response("abandoned"))
        .await
        .unwrap();
    drop(abandoned);

    let mut committed = store.begin_at(&context, &start);
    assert_eq!(
        store
            .stage_mutation_claim(&mut committed, claim(&context, 1))
            .await
            .unwrap(),
        MutationClaimOutcome::Execute
    );
    store
        .stage_episode(&mut committed, episode(&context, "first"))
        .await
        .unwrap();
    store
        .stage_mutation_response(&mut committed, response("first"))
        .await
        .unwrap();
    store.commit(committed).await.unwrap();
    assert_eq!(store.episodes(tenant).len(), 1);

    let expired = FixedClock("2026-07-16T00:00:00Z");
    let mut replacement = store.begin_at(&context, &expired);
    assert_eq!(
        store
            .stage_mutation_claim(&mut replacement, claim(&context, 2))
            .await
            .unwrap(),
        MutationClaimOutcome::Execute
    );
    store
        .stage_episode(&mut replacement, episode(&context, "replacement"))
        .await
        .unwrap();
    store
        .stage_mutation_response(&mut replacement, response("replacement"))
        .await
        .unwrap();
    store.commit(replacement).await.unwrap();
    assert_eq!(store.episodes(tenant).len(), 2);
}

#[tokio::test]
async fn concurrent_commits_apply_the_mutation_once() {
    let store = Arc::new(InMemoryStore::default());
    let tenant = TenantId::new();
    let context = context(&store, tenant, "concurrent").await;
    let (left, right) = tokio::join!(
        run_mutation(store.clone(), context.clone(), "left"),
        run_mutation(store.clone(), context.clone(), "right")
    );
    assert_eq!(
        left, right,
        "the concurrent caller must receive exact replay"
    );
    assert_eq!(store.episodes(tenant).len(), 1);

    let mut replay = store.begin_at(&context, &FixedClock("2026-07-15T00:00:00Z"));
    assert!(matches!(
        store
            .stage_mutation_claim(&mut replay, claim(&context, 1))
            .await
            .unwrap(),
        MutationClaimOutcome::Replay(_)
    ));
}

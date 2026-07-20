use memphant_core::{
    ClaimMutationOutcome, JobFilter, MemoryStore, MutationClaim, MutationClaimOutcome,
    MutationLedgerStore, MutationResponse, MutationVerb, StoreError,
};
use memphant_store_postgres::PgStore;
use memphant_types::{
    ContextBindingAgentRef, ContextBindingEntityRef, ContextBindingRequest, ContextBindingScopeRef,
    NewEpisode, ReflectJob, ReflectJobKind, ResolvedMemoryContext, TenantId, TrustLevel,
};
use uuid::Uuid;

async fn store() -> PgStore {
    let url = std::env::var("MEMPHANT_TEST_DATABASE_URL")
        .expect("MEMPHANT_TEST_DATABASE_URL must point at a migrated Postgres");
    PgStore::connect(&url).await.expect("connect PgStore")
}

async fn tenant(store: &PgStore) -> TenantId {
    TenantId::from_u128(
        store
            .create_tenant(&format!("erasure-{}", Uuid::now_v7()))
            .await
            .unwrap()
            .as_u128(),
    )
}

fn binding_request(name: &str) -> ContextBindingRequest {
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
    }
}

async fn bind(
    store: &PgStore,
    tenant: TenantId,
    client_ref: &str,
    name: &str,
) -> ResolvedMemoryContext {
    let binding = store
        .resolve_context_binding(tenant, client_ref.to_string(), binding_request(name))
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

fn claim(
    context: &ResolvedMemoryContext,
    verb: MutationVerb,
    key: &str,
    hash: u8,
) -> MutationClaim {
    MutationClaim::new(context, verb, key, [hash; 32]).unwrap()
}

fn episode(context: &ResolvedMemoryContext, label: &str) -> NewEpisode {
    NewEpisode {
        tenant_id: context.tenant_id,
        data_subject_id: context.data_subject_id,
        scope_id: context.scope_id,
        actor_id: context.actor_id,
        agent_node_id: context.agent_node_id,
        subject_generation: context.subject_generation,
        source_kind: "user".to_string(),
        source_ref: "test:subject-erasure".to_string(),
        observed_at: "2026-07-15T00:00:00Z".to_string(),
        source_trust: TrustLevel::TrustedUser,
        dedup_key: label.to_string(),
        body: label.to_string(),
    }
}

async fn seed_subject(store: &PgStore, context: &ResolvedMemoryContext, label: &str) {
    let mut tx = store.begin(context).await.unwrap();
    let retained = store
        .stage_episode(&mut tx, episode(context, label))
        .await
        .unwrap();
    store
        .enqueue_reflect(
            &mut tx,
            ReflectJob {
                tenant_id: context.tenant_id,
                data_subject_id: context.data_subject_id,
                scope_id: context.scope_id,
                actor_id: context.actor_id,
                agent_node_id: context.agent_node_id,
                subject_generation: context.subject_generation,
                episode_id: Some(retained.episode_id),
                resource_id: None,
                kind: ReflectJobKind::ReflectEpisode,
                compiler_version: "erasure-test".to_string(),
                subject: None,
                predicate: None,
            },
        )
        .await
        .unwrap();
    store.commit(tx).await.unwrap();
}

async fn owned_count(store: &PgStore, context: &ResolvedMemoryContext, table: &str) -> i64 {
    let mut tx = store.pool().begin().await.unwrap();
    sqlx::query("select memphant.bind_tenant($1)")
        .bind(context.tenant_id.as_uuid())
        .execute(&mut *tx)
        .await
        .unwrap();
    let sql = match table {
        "episode" => {
            "select count(*) from memphant.episode
             where tenant_id = $1 and data_subject_id = $2"
        }
        "job_state" => {
            "select count(*) from memphant.job_state
             where tenant_id = $1 and data_subject_id = $2"
        }
        "mutation_ledger" => {
            "select count(*) from memphant.mutation_ledger
             where tenant_id = $1 and data_subject_id = $2"
        }
        _ => panic!("unsupported subject-owned table"),
    };
    let count = sqlx::query_scalar(sql)
        .bind(context.tenant_id.as_uuid())
        .bind(context.data_subject_id.as_uuid())
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    tx.rollback().await.unwrap();
    count
}

#[tokio::test]
#[ignore = "requires MEMPHANT_TEST_DATABASE_URL"]
async fn erasure_is_atomic_isolated_replayable_and_rebinds_empty() {
    let store = store().await;
    let tenant_id = tenant(&store).await;
    let erased = bind(&store, tenant_id, "erased-client", "shared").await;
    let survivor = bind(&store, tenant_id, "survivor-client", "survivor").await;
    let other_tenant = tenant(&store).await;
    let other = bind(&store, other_tenant, "erased-client", "shared").await;
    seed_subject(&store, &erased, "erased").await;
    seed_subject(&store, &survivor, "survivor").await;
    seed_subject(&store, &other, "other").await;

    let old_claim = claim(&erased, MutationVerb::Retain, "old-receipt", 1);
    let mut old_tx = store.begin(&erased).await.unwrap();
    assert_eq!(
        store
            .stage_mutation_claim(&mut old_tx, old_claim.clone())
            .await
            .unwrap(),
        MutationClaimOutcome::Execute
    );
    store
        .stage_mutation_response(
            &mut old_tx,
            MutationResponse::success(201, b"old".to_vec()).unwrap(),
        )
        .await
        .unwrap();
    store.commit(old_tx).await.unwrap();

    let stale_job = store
        .claim_reflect_jobs(
            JobFilter {
                tenant: Some(tenant_id),
                scope: Some(erased.scope_id),
            },
            1,
        )
        .await
        .unwrap()
        .pop()
        .unwrap();
    let erase_claim = claim(&erased, MutationVerb::EraseSubject, "erase-1", 9);
    let mut tx = store.begin(&erased).await.unwrap();
    assert_eq!(
        store
            .stage_mutation_claim(&mut tx, erase_claim.clone())
            .await
            .unwrap(),
        MutationClaimOutcome::Execute
    );
    let receipt = store.stage_subject_erasure(&mut tx).await.unwrap();
    assert_eq!(receipt.generation, erased.subject_generation + 1);
    assert_eq!(
        serde_json::to_value(&receipt)
            .unwrap()
            .as_object()
            .unwrap()
            .len(),
        2
    );
    store.commit(tx).await.unwrap();

    assert_eq!(owned_count(&store, &erased, "episode").await, 0);
    assert_eq!(owned_count(&store, &erased, "job_state").await, 0);
    assert_eq!(owned_count(&store, &erased, "mutation_ledger").await, 1);
    assert_eq!(owned_count(&store, &survivor, "episode").await, 1);
    assert_eq!(owned_count(&store, &other, "episode").await, 1);
    assert_eq!(
        store.complete_reflect_job(&stale_job).await.unwrap(),
        ClaimMutationOutcome::Stale
    );
    assert_eq!(owned_count(&store, &erased, "job_state").await, 0);
    assert!(matches!(
        store
            .resolve_memory_context(
                tenant_id,
                erased.data_subject_id,
                erased.actor_id,
                erased.scope_id,
                erased.agent_node_id,
            )
            .await,
        Err(StoreError::SubjectErased)
    ));

    let mut replay = store.begin(&erased).await.unwrap();
    assert_eq!(
        store
            .stage_mutation_claim(&mut replay, erase_claim.clone())
            .await
            .unwrap(),
        MutationClaimOutcome::Replay(
            MutationResponse::success(200, serde_json::to_vec(&receipt).unwrap()).unwrap()
        )
    );
    store.commit(replay).await.unwrap();
    let mut receipt_conflict = store.begin(&erased).await.unwrap();
    assert!(matches!(
        store
            .stage_mutation_claim(
                &mut receipt_conflict,
                claim(&erased, MutationVerb::EraseSubject, "erase-1", 8),
            )
            .await,
        Err(StoreError::IdempotencyConflict)
    ));
    let mut other_erasure = store.begin(&erased).await.unwrap();
    assert!(matches!(
        store
            .stage_mutation_claim(
                &mut other_erasure,
                claim(&erased, MutationVerb::EraseSubject, "erase-2", 9),
            )
            .await,
        Err(StoreError::SubjectErased)
    ));
    let mut stale_write = store.begin(&erased).await.unwrap();
    assert!(matches!(
        store
            .stage_mutation_claim(&mut stale_write, old_claim)
            .await,
        Err(StoreError::SubjectErased)
    ));

    let rebound = bind(&store, tenant_id, "erased-client", "shared").await;
    assert_ne!(rebound.data_subject_id, erased.data_subject_id);
    assert_eq!(rebound.subject_generation, 0);
    assert_eq!(owned_count(&store, &rebound, "episode").await, 0);
    assert_eq!(owned_count(&store, &rebound, "job_state").await, 0);
}

#[tokio::test]
#[ignore = "requires MEMPHANT_TEST_DATABASE_URL"]
async fn dropped_erasure_transaction_rolls_back_every_effect() {
    let store = store().await;
    let tenant = tenant(&store).await;
    let context = bind(&store, tenant, "client", "subject").await;
    seed_subject(&store, &context, "keep-me").await;
    let erase_claim = claim(&context, MutationVerb::EraseSubject, "erase-drop", 7);
    {
        let mut tx = store.begin(&context).await.unwrap();
        store
            .stage_mutation_claim(&mut tx, erase_claim.clone())
            .await
            .unwrap();
        store.stage_subject_erasure(&mut tx).await.unwrap();
    }

    store
        .resolve_memory_context(
            tenant,
            context.data_subject_id,
            context.actor_id,
            context.scope_id,
            context.agent_node_id,
        )
        .await
        .unwrap();
    assert_eq!(owned_count(&store, &context, "episode").await, 1);
    assert_eq!(owned_count(&store, &context, "mutation_ledger").await, 0);
    let mut retry = store.begin(&context).await.unwrap();
    assert_eq!(
        store
            .stage_mutation_claim(&mut retry, erase_claim)
            .await
            .unwrap(),
        MutationClaimOutcome::Execute
    );
}

#[tokio::test]
#[ignore = "requires MEMPHANT_TEST_DATABASE_URL"]
async fn erasure_rejects_transactions_that_already_staged_subject_writes() {
    let store = store().await;
    let tenant = tenant(&store).await;
    let context = bind(&store, tenant, "client", "subject").await;
    let mut tx = store.begin(&context).await.unwrap();
    store
        .stage_mutation_claim(
            &mut tx,
            claim(&context, MutationVerb::EraseSubject, "erase-write", 6),
        )
        .await
        .unwrap();
    store
        .stage_episode(&mut tx, episode(&context, "must-not-disappear"))
        .await
        .unwrap();
    assert!(matches!(
        store.stage_subject_erasure(&mut tx).await,
        Err(StoreError::Conflict(message)) if message.contains("empty transaction")
    ));
    drop(tx);

    let mut review_tx = store.begin(&context).await.unwrap();
    store
        .stage_mutation_claim(
            &mut review_tx,
            claim(&context, MutationVerb::EraseSubject, "erase-review", 7),
        )
        .await
        .unwrap();
    store
        .stage_review_events(&mut review_tx, Vec::new())
        .await
        .unwrap();
    assert!(matches!(
        store.stage_subject_erasure(&mut review_tx).await,
        Err(StoreError::Conflict(message)) if message.contains("empty transaction")
    ));
}

#[tokio::test]
#[ignore = "requires MEMPHANT_TEST_DATABASE_URL"]
async fn unknown_subject_is_not_misreported_as_erased() {
    let store = store().await;
    let context = bind(&store, tenant(&store).await, "client", "subject").await;
    let unknown = ResolvedMemoryContext {
        data_subject_id: memphant_types::SubjectId::new(),
        ..context
    };
    assert!(matches!(
        store
            .resolve_memory_context(
                unknown.tenant_id,
                unknown.data_subject_id,
                unknown.actor_id,
                unknown.scope_id,
                unknown.agent_node_id,
            )
            .await,
        Err(StoreError::NotFound("memory context"))
    ));
    let mut tx = store.begin(&unknown).await.unwrap();
    assert!(matches!(
        store
            .stage_mutation_claim(&mut tx, claim(&unknown, MutationVerb::Retain, "unknown", 1),)
            .await,
        Err(StoreError::NotFound("memory context"))
    ));
}

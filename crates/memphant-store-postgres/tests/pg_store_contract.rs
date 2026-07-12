//! Live-Postgres contract tests for `PgStore`, mirroring the in-memory
//! `store_contract.rs` scenarios plus durability/cross-tenant/queue checks.
//!
//! Gated: every test is `#[ignore]` and reads `MEMPHANT_TEST_DATABASE_URL`.
//! Run with:
//!   MEMPHANT_TEST_DATABASE_URL=postgres://memphant:memphant@localhost:5432/memphant \
//!     cargo test -p memphant-store-postgres -- --ignored --test-threads=1

use std::sync::Arc;

use memphant_core::service::MemoryService;
use memphant_core::{
    FixedClock, JobFilter, MemoryStore, NoopEmbedding, forget_memory, recall, reflect_recorded,
    retain_episode, retain_resource,
};
use memphant_store_postgres::PgStore;
use memphant_types::{
    ActorId, ForgetRequest, ForgetSelector, JobId, MemoryKind, RecallHttpRequest, RecallMode,
    RecallRequest, ReflectCandidate, ReflectInput, RetainEpisodeHttpRequest, RetainRequest,
    RetainResourceRequest, RetainUnitPayload, ScopeId, TenantId, TrustLevel,
};
use uuid::Uuid;

const CLOCK: FixedClock = FixedClock("2026-07-09T00:00:00Z");

fn db_url() -> String {
    std::env::var("MEMPHANT_TEST_DATABASE_URL")
        .expect("MEMPHANT_TEST_DATABASE_URL must point at a migrated Postgres")
}

async fn connect() -> PgStore {
    PgStore::connect(&db_url()).await.expect("connect PgStore")
}

async fn fresh_tenant(store: &PgStore) -> TenantId {
    let id = store
        .create_tenant(&format!("pg-contract-{}", Uuid::now_v7()))
        .await
        .expect("create tenant");
    TenantId::from_u128(id.as_u128())
}

fn service(store: PgStore) -> MemoryService<PgStore> {
    MemoryService::new(Arc::new(store), Arc::new(CLOCK), Arc::new(NoopEmbedding))
}

fn retain_request(
    tenant_id: TenantId,
    scope_id: ScopeId,
    actor_id: ActorId,
    body: &str,
    subject: Option<&str>,
) -> RetainRequest {
    RetainRequest {
        tenant_id,
        scope_id,
        actor_id,
        source_kind: "user".to_string(),
        source_trust: TrustLevel::TrustedUser,
        subject_hint: subject.map(str::to_string),
        subject: subject.map(str::to_string),
        predicate: subject.map(|_| "value".to_string()),
        body: body.to_string(),
        compiler_version: "compiler-pg-contract".to_string(),
    }
}

fn recall_request(
    tenant_id: TenantId,
    scope_id: ScopeId,
    actor_id: ActorId,
    query: &str,
) -> RecallRequest {
    RecallRequest {
        tenant_id,
        scope_id,
        actor_id,
        allowed_scope_ids: vec![scope_id],
        query: query.to_string(),
        k: 4,
        budget_tokens: 256,
        mode: RecallMode::Fast,
        include_beliefs: true,
        edge_expansion_enabled: true,
        context_packing_abstention_enabled: true,
        rerank_enabled: true,
        learned_rerank_profile: None,
        query_decomposition_enabled: true,
        procedure_recall_enabled: true,
        decay_enabled: true,
        engine_version: "pg-contract-test".to_string(),
    }
}

#[tokio::test]
#[ignore = "requires MEMPHANT_TEST_DATABASE_URL"]
async fn retain_stores_episode_and_reflect_job_and_dedups() {
    let store = connect().await;
    let tenant = fresh_tenant(&store).await;
    let scope = ScopeId::new();
    let actor = ActorId::new();

    let request = retain_request(tenant, scope, actor, "Staging pins Node 24.15.0.", None);
    let first = retain_episode(&store, request.clone())
        .await
        .expect("retain");
    let second = retain_episode(&store, request).await.expect("retain again");

    assert!(!first.dedup.matched);
    assert!(second.dedup.matched);
    assert_eq!(second.episode_id, first.episode_id);
    assert_eq!(second.dedup.observation_count, 2);

    let episodes = store
        .fetch_episodes_for_scope(tenant, scope, 10)
        .await
        .expect("episodes");
    assert_eq!(episodes.len(), 1);
    assert_eq!(episodes[0].observation_count, 2);
    assert!(store.pending_job_count(tenant, scope).await.expect("count") >= 1);
}

#[tokio::test]
#[ignore = "requires MEMPHANT_TEST_DATABASE_URL"]
async fn durability_write_with_pool_a_read_with_fresh_pool_b() {
    let store_a = connect().await;
    let tenant = fresh_tenant(&store_a).await;
    let scope = ScopeId::new();
    let actor = ActorId::new();

    let svc_a = service(store_a);
    retain_episode(
        svc_a.store(),
        retain_request(
            tenant,
            scope,
            actor,
            "Durable release region is Taipei.",
            Some("release region"),
        ),
    )
    .await
    .expect("retain");
    svc_a.reflect(tenant, scope, None).await.expect("reflect");

    // A COMPLETELY fresh pool must see the compiled unit.
    let store_b = connect().await;
    let recalled = recall(
        &store_b,
        recall_request(tenant, scope, actor, "Where is the durable release region?"),
        None,
        &CLOCK,
    )
    .await
    .expect("recall via fresh pool");
    assert_eq!(recalled.items[0].body, "Durable release region is Taipei.");

    // The recall's trace is durable and tenant-bound through yet another pool.
    let store_c = connect().await;
    let trace = store_c
        .trace_by_id(tenant, recalled.trace_id)
        .await
        .expect("trace lookup");
    assert!(trace.is_some(), "trace persists across pools");
}

#[tokio::test]
#[ignore = "requires MEMPHANT_TEST_DATABASE_URL"]
async fn cross_tenant_candidates_and_traces_are_isolated() {
    let store = connect().await;
    let tenant_a = fresh_tenant(&store).await;
    let tenant_b = fresh_tenant(&store).await;
    let scope = ScopeId::new();
    let actor = ActorId::new();

    let svc = service(store.clone());
    retain_episode(
        svc.store(),
        retain_request(tenant_a, scope, actor, "Tenant A secret deploy fact.", None),
    )
    .await
    .expect("retain");
    svc.reflect(tenant_a, scope, None).await.expect("reflect");

    let own = store
        .fetch_recall_candidates(tenant_a, &[scope], &[], &["deploy".to_string()], 100)
        .await
        .expect("candidates");
    assert!(!own.is_empty());

    let cross = store
        .fetch_recall_candidates(tenant_b, &[scope], &[], &["deploy".to_string()], 100)
        .await
        .expect("candidates");
    assert!(cross.is_empty(), "tenant B must never see tenant A units");

    let recalled = recall(
        &store,
        recall_request(tenant_a, scope, actor, "secret deploy fact"),
        None,
        &CLOCK,
    )
    .await
    .expect("recall");
    let own_trace = store
        .trace_by_id(tenant_a, recalled.trace_id)
        .await
        .expect("lookup");
    assert!(own_trace.is_some());
    let cross_trace = store
        .trace_by_id(tenant_b, recalled.trace_id)
        .await
        .expect("lookup");
    assert!(
        cross_trace.is_none(),
        "wrong tenant gets None, never a trace"
    );
}

#[tokio::test]
#[ignore = "requires MEMPHANT_TEST_DATABASE_URL"]
async fn claim_reflect_jobs_is_disjoint_and_does_not_reclaim_fresh_claims() {
    let store = connect().await;
    let tenant = fresh_tenant(&store).await;
    let scope = ScopeId::new();
    let actor = ActorId::new();

    retain_episode(
        &store,
        retain_request(tenant, scope, actor, "Claim scenario fact one.", None),
    )
    .await
    .expect("retain one");
    retain_episode(
        &store,
        retain_request(tenant, scope, actor, "Claim scenario fact two.", None),
    )
    .await
    .expect("retain two");

    let filter = JobFilter {
        tenant: Some(tenant),
        scope: Some(scope),
    };
    let first = store.claim_reflect_jobs(filter, 1).await.expect("claim 1");
    let second = store.claim_reflect_jobs(filter, 1).await.expect("claim 2");
    assert_eq!(first.len(), 1);
    assert_eq!(second.len(), 1);
    assert_ne!(
        first[0].job.id.as_uuid(),
        second[0].job.id.as_uuid(),
        "a freshly claimed job must not be handed out twice"
    );

    // Both jobs are claimed; nothing is left to claim inside the window.
    let third = store.claim_reflect_jobs(filter, 10).await.expect("claim 3");
    assert!(third.is_empty());
}

#[tokio::test]
#[ignore = "requires MEMPHANT_TEST_DATABASE_URL"]
async fn exhausted_jobs_dead_letter_and_surface_in_count() {
    let store = connect().await;
    let tenant = fresh_tenant(&store).await;
    let scope = ScopeId::new();
    let actor = ActorId::new();

    retain_episode(
        &store,
        retain_request(tenant, scope, actor, "Dead letter scenario fact.", None),
    )
    .await
    .expect("retain");

    sqlx::query("update memphant.job_state set attempts = 5 where tenant_id = $1")
        .bind(tenant.as_uuid())
        .execute(store.pool())
        .await
        .expect("exhaust attempts");

    let claimed = store
        .claim_reflect_jobs(
            JobFilter {
                tenant: Some(tenant),
                scope: Some(scope),
            },
            10,
        )
        .await
        .expect("claim");
    assert!(claimed.is_empty(), "exhausted jobs are never re-claimed");

    let dead: i64 = sqlx::query_scalar(
        "select count(*) from memphant.job_state where tenant_id = $1 and state = 'dead'",
    )
    .bind(tenant.as_uuid())
    .fetch_one(store.pool())
    .await
    .expect("dead count");
    assert!(dead >= 1);
    assert!(store.dead_letter_count().await.expect("dead letters") >= 1);
}

/// A tenant-scoped claim must dead-letter only its own exhausted jobs, never
/// another tenant's. Regression guard for the sweep that used to run globally
/// on every claim — it crossed the tenant boundary (tenant A's foreground
/// reflect wrote tenant B's rows) and seq-scanned job_state on the hot path.
#[tokio::test]
#[ignore = "requires MEMPHANT_TEST_DATABASE_URL"]
async fn dead_letter_sweep_stays_within_the_claim_filter() {
    let store = connect().await;
    let tenant_a = fresh_tenant(&store).await;
    let tenant_b = fresh_tenant(&store).await;
    let scope_a = ScopeId::new();
    let scope_b = ScopeId::new();
    let actor = ActorId::new();

    retain_episode(
        &store,
        retain_request(tenant_a, scope_a, actor, "A fact.", None),
    )
    .await
    .expect("retain A");
    retain_episode(
        &store,
        retain_request(tenant_b, scope_b, actor, "B fact.", None),
    )
    .await
    .expect("retain B");

    // Exhaust BOTH tenants' jobs, then run a claim scoped to A only.
    sqlx::query("update memphant.job_state set attempts = 5 where tenant_id = any($1)")
        .bind(vec![tenant_a.as_uuid(), tenant_b.as_uuid()])
        .execute(store.pool())
        .await
        .expect("exhaust attempts");

    store
        .claim_reflect_jobs(
            JobFilter {
                tenant: Some(tenant_a),
                scope: Some(scope_a),
            },
            10,
        )
        .await
        .expect("claim A");

    let a_dead: i64 = sqlx::query_scalar(
        "select count(*) from memphant.job_state where tenant_id = $1 and state = 'dead'",
    )
    .bind(tenant_a.as_uuid())
    .fetch_one(store.pool())
    .await
    .expect("A dead count");
    let b_dead: i64 = sqlx::query_scalar(
        "select count(*) from memphant.job_state where tenant_id = $1 and state = 'dead'",
    )
    .bind(tenant_b.as_uuid())
    .fetch_one(store.pool())
    .await
    .expect("B dead count");

    assert!(
        a_dead >= 1,
        "A's exhausted job dead-letters on A's own claim"
    );
    assert_eq!(
        b_dead, 0,
        "B's job must NOT dead-letter from A's tenant-scoped claim"
    );
}

#[tokio::test]
#[ignore = "requires MEMPHANT_TEST_DATABASE_URL"]
async fn forget_by_episode_tombstone_blocks_recompilation_durably() {
    let store = connect().await;
    let tenant = fresh_tenant(&store).await;
    let scope = ScopeId::new();
    let actor = ActorId::new();
    let svc = service(store.clone());

    let retained = retain_episode(
        svc.store(),
        retain_request(
            tenant,
            scope,
            actor,
            "Payment processor is AcmePay.",
            Some("payment processor"),
        ),
    )
    .await
    .expect("retain");
    svc.reflect(tenant, scope, None).await.expect("reflect");

    let recalled = recall(
        &store,
        recall_request(tenant, scope, actor, "Which payment processor do we use?"),
        None,
        &CLOCK,
    )
    .await
    .expect("recall");
    assert_eq!(recalled.items[0].body, "Payment processor is AcmePay.");

    let forgotten = forget_memory(
        &store,
        ForgetRequest {
            tenant_id: tenant,
            scope_id: scope,
            actor_id: actor,
            selector: ForgetSelector {
                memory_unit_id: None,
                episode_id: Some(retained.episode_id),
                resource_id: None,
                scope_id: scope,
            },
            reason: "user_request".to_string(),
        },
        &CLOCK,
    )
    .await
    .expect("forget");
    assert_eq!(forgotten.invalidated_units.len(), 1);
    assert_eq!(forgotten.verification, "post_forget_recall_probe_hits=0");

    // Re-enqueue + recompile with a bumped compiler: the durable tombstone
    // must refuse re-derivation from the forgotten episode.
    retain_episode(
        &store,
        RetainRequest {
            compiler_version: "compiler-pg-contract-v2".to_string(),
            ..retain_request(
                tenant,
                scope,
                actor,
                "Payment processor is AcmePay.",
                Some("payment processor"),
            )
        },
    )
    .await
    .expect("re-enqueue");
    svc.reflect(tenant, scope, None).await.expect("recompile");

    let recalled_again = recall(
        &store,
        recall_request(tenant, scope, actor, "Which payment processor do we use?"),
        None,
        &CLOCK,
    )
    .await
    .expect("recall after recompile");
    assert!(
        recalled_again.items.is_empty(),
        "tombstoned episode must not re-derive units"
    );
}

#[tokio::test]
#[ignore = "requires MEMPHANT_TEST_DATABASE_URL"]
async fn resource_retain_reflect_recall_round_trips_via_service() {
    let store = connect().await;
    let tenant = fresh_tenant(&store).await;
    let scope = ScopeId::new();
    let actor = ActorId::new();
    let svc = service(store.clone());

    let retained = retain_resource(
        svc.store(),
        RetainResourceRequest {
            tenant_id: tenant,
            scope_id: scope,
            actor_id: actor,
            uri: "https://example.test/runbooks/deploy.md".to_string(),
            kind: Some(memphant_types::ResourceKind::Document),
            content_hash: "sha256:deploy-runbook".to_string(),
            mime_type: "text/markdown".to_string(),
            revision: Some("rev-42".to_string()),
            body: Some("Deploy runbook: canary first, then roll forward regions.".to_string()),
            source_trust: TrustLevel::TrustedUser,
            compiler_version: "compiler-pg-contract".to_string(),
        },
    )
    .await
    .expect("retain resource");
    svc.reflect(tenant, scope, None).await.expect("reflect");

    let recalled = recall(
        &store,
        recall_request(
            tenant,
            scope,
            actor,
            "How does the deploy runbook roll forward?",
        ),
        None,
        &CLOCK,
    )
    .await
    .expect("recall");
    let item = recalled
        .items
        .iter()
        .find(|item| item.kind == MemoryKind::Resource)
        .expect("resource-derived item");
    assert_eq!(item.citation_resource_id, Some(retained.resource_id));
}

#[tokio::test]
#[ignore = "requires MEMPHANT_TEST_DATABASE_URL"]
async fn scope_memory_page_cursors_without_overlap() {
    let store = connect().await;
    let tenant = fresh_tenant(&store).await;
    let scope = ScopeId::new();
    let actor = ActorId::new();
    let svc = service(store.clone());

    for index in 0..5 {
        retain_episode(
            svc.store(),
            retain_request(
                tenant,
                scope,
                actor,
                &format!("Paginated durable fact number {index}."),
                Some(&format!("paginated fact {index}")),
            ),
        )
        .await
        .expect("retain");
    }
    svc.reflect(tenant, scope, None).await.expect("reflect");

    let page_one = store
        .scope_memory_page(tenant, scope, None, 3)
        .await
        .expect("page one");
    assert_eq!(page_one.items.len(), 3);
    assert!(page_one.has_more);
    let cursor = page_one.next_cursor.expect("cursor");

    let page_two = store
        .scope_memory_page(tenant, scope, Some(cursor), 3)
        .await
        .expect("page two");
    assert!(!page_two.items.is_empty());
    assert!(!page_two.has_more);

    let ids_one: std::collections::HashSet<_> = page_one
        .items
        .iter()
        .map(|unit| unit.id.as_uuid())
        .collect();
    let ids_two: std::collections::HashSet<_> = page_two
        .items
        .iter()
        .map(|unit| unit.id.as_uuid())
        .collect();
    assert!(ids_one.is_disjoint(&ids_two));
    assert_eq!(ids_one.len() + ids_two.len(), 5);
}

#[tokio::test]
#[ignore = "requires MEMPHANT_TEST_DATABASE_URL"]
async fn api_key_lookup_and_revocation_round_trip() {
    let store = connect().await;
    let tenant = fresh_tenant(&store).await;
    let key_hash = format!("hash-{}", Uuid::now_v7());

    let key_id = store
        .create_api_key(
            tenant.as_uuid(),
            &key_hash,
            "contract",
            TrustLevel::TrustedUser,
        )
        .await
        .expect("create key");

    let row = store
        .lookup_api_key(&key_hash)
        .await
        .expect("lookup")
        .expect("key exists");
    assert_eq!(row.tenant_id, tenant);
    assert_eq!(row.max_trust, TrustLevel::TrustedUser);
    assert!(!row.revoked);

    assert!(store.revoke_api_key(key_id).await.expect("revoke"));
    let row = store
        .lookup_api_key(&key_hash)
        .await
        .expect("lookup")
        .expect("key still resolvable");
    assert!(row.revoked, "revoked keys resolve as revoked, never valid");
    assert!(
        !store.revoke_api_key(key_id).await.expect("second revoke"),
        "double revoke is a no-op"
    );
}

#[tokio::test]
#[ignore = "requires MEMPHANT_TEST_DATABASE_URL"]
async fn degraded_read_your_own_writes_serves_unreflected_episodes() {
    let store = connect().await;
    let tenant = fresh_tenant(&store).await;
    let scope = ScopeId::new();
    let actor = ActorId::new();
    let svc = service(store.clone());

    retain_episode(
        svc.store(),
        retain_request(
            tenant,
            scope,
            actor,
            "Fallback rollout window is Thursday night.",
            None,
        ),
    )
    .await
    .expect("retain");

    // No reflect: the service-level recall must fall back to raw episodes.
    let response = svc
        .recall(
            tenant,
            RecallHttpRequest {
                tenant_id: tenant,
                scope_id: scope,
                actor_id: actor,
                allowed_scope_ids: Some(vec![scope]),
                query: "When is the fallback rollout window?".to_string(),
                limit: Some(4),
                budget_tokens: Some(256),
                mode: None,
                include_beliefs: None,
                edge_expansion_enabled: None,
                context_packing_abstention_enabled: None,
                rerank_enabled: None,
                query_decomposition_enabled: None,
                procedure_recall_enabled: None,
                decay_enabled: None,
            },
        )
        .await
        .expect("service recall");
    assert!(response.degraded);
    assert_eq!(
        response.items[0].body,
        "Fallback rollout window is Thursday night."
    );
}

/// Six turns behind a `[session]` provenance line: turn windows of 4 yield
/// two chunks (turns 1-4, 5-6). Mirrors
/// `memphant-core/tests/contextual_chunk_write.rs::EPISODE_BODY`.
const CHUNK_EPISODE_BODY: &str = "[session s1] [date 2023/05/30]\n\
user: I moved to Berlin in March.\n\
assistant: Got it, you moved to Berlin in March.\n\
user: My favorite tea is oolong.\n\
assistant: Noted, oolong tea it is.\n\
user: I drive a blue Tesla.\n\
assistant: A blue Tesla, understood.\n";

#[tokio::test]
#[ignore = "requires MEMPHANT_TEST_DATABASE_URL"]
async fn contextual_chunks_round_trip_through_postgres_via_fresh_pool() {
    let store_a = connect().await;
    let tenant = fresh_tenant(&store_a).await;
    let scope = ScopeId::new();
    let actor = ActorId::new();

    // Default construction mints contextual chunks (promoted to default-on
    // 2026-07-10) — the product path, same as `service()` elsewhere in this
    // file.
    let svc_a = service(store_a);
    let retained = retain_episode(
        svc_a.store(),
        retain_request(tenant, scope, actor, CHUNK_EPISODE_BODY, None),
    )
    .await
    .expect("retain");
    svc_a.reflect(tenant, scope, None).await.expect("reflect");

    // A COMPLETELY fresh pool must see the compiled unit's chunks: this is
    // the payload jsonb round trip through `memphant.memory_unit`, never
    // exercised by an automated test before this one (rung 4 was "by
    // construction" only, per InMemoryStore assertions).
    let store_b = connect().await;
    let page = store_b
        .scope_memory_page(tenant, scope, None, 100)
        .await
        .expect("page via fresh pool");
    let unit = page
        .items
        .iter()
        .find(|unit| unit.source_episode_id == Some(retained.episode_id))
        .expect("episode-derived unit");

    assert_eq!(
        unit.contextual_chunks.len(),
        2,
        "six turns / window 4 yields two chunks, surviving the payload jsonb round trip"
    );
    let episode_uuid = retained.episode_id.as_uuid();
    for chunk in &unit.contextual_chunks {
        assert!(
            chunk.id.starts_with(&format!("chunk-{episode_uuid}-")),
            "chunk id derives from parent episode: {}",
            chunk.id
        );
        assert!(
            chunk.header.contains(&format!("[episode {episode_uuid}]")),
            "header carries parent episode provenance: {}",
            chunk.header
        );
        assert!(!chunk.body.trim().is_empty(), "no empty-body chunks");
        assert!(
            chunk
                .source_span
                .as_deref()
                .is_some_and(|span| span.contains('-')),
            "chunk carries a source span"
        );
    }
    assert!(
        unit.contextual_chunks[0].header.contains("[turns 1-4]"),
        "first window covers turns 1-4: {}",
        unit.contextual_chunks[0].header
    );
    assert!(
        unit.contextual_chunks[1].header.contains("[turns 5-6]"),
        "second window covers turns 5-6: {}",
        unit.contextual_chunks[1].header
    );
    assert_ne!(
        unit.contextual_chunks[0].id, unit.contextual_chunks[1].id,
        "window ids are distinct"
    );
}

/// Multi-tenant job-claim fairness (plan addendum W1-b): `claim_reflect_jobs`
/// claims per-tenant round-robin — each tenant's eligible jobs are ranked by
/// age (`row_number() over (partition by tenant_id order by created_at)`) and
/// the batch draws every tenant's oldest before any tenant's second
/// (`crates/memphant-store-postgres/src/store.rs::claim_reflect_jobs`). A tenant
/// with a large backlog can therefore no longer starve a low-volume tenant's
/// single, more urgent job — the exact gap that produced the orphaned-backlog
/// e2e failure named in the research audit (scratchpad/research/tests-audit.md,
/// gap 3).
///
/// This is the GREEN assertion that flipped the former `_red_baseline`: tenant A
/// floods a 50-job backlog created first, tenant B queues one job strictly
/// after, and a single worker batch of 16 MUST include tenant B's job (it lands
/// at round-robin position 2, right behind tenant A's oldest).
#[tokio::test]
#[ignore = "requires MEMPHANT_TEST_DATABASE_URL"]
async fn multi_tenant_claim_batch_does_not_starve_the_low_volume_tenant() {
    let store = connect().await;
    let tenant_a = fresh_tenant(&store).await;
    let tenant_b = fresh_tenant(&store).await;
    let scope_a = ScopeId::new();
    let scope_b = ScopeId::new();
    let actor_a = ActorId::new();
    let actor_b = ActorId::new();

    // Tenant A floods the global queue with a 50-job backlog...
    for index in 0..50 {
        retain_episode(
            &store,
            retain_request(
                tenant_a,
                scope_a,
                actor_a,
                &format!("Tenant A backlog fact number {index}."),
                None,
            ),
        )
        .await
        .expect("retain tenant A backlog item");
    }

    // ...then tenant B queues a single job strictly AFTER tenant A's backlog.
    retain_episode(
        &store,
        retain_request(
            tenant_b,
            scope_b,
            actor_b,
            "Tenant B single urgent fact.",
            None,
        ),
    )
    .await
    .expect("retain tenant B job");

    // Same batch size the real memphant-worker binary claims per tick
    // (crates/memphant-worker/src/main.rs::BATCH).
    const WORKER_BATCH: usize = 16;
    let claimed = store
        .claim_reflect_jobs(JobFilter::default(), WORKER_BATCH)
        .await
        .expect("claim");

    assert_eq!(claimed.len(), WORKER_BATCH, "a full batch is claimed");
    assert!(
        claimed.iter().any(|row| row.job.tenant_id == tenant_b),
        "tenant B's single job must be claimed in the first batch despite tenant \
         A's 50-job head-start backlog — per-tenant fair claiming interleaves by \
         age rank, so B lands right behind A's oldest job"
    );
}

#[tokio::test]
#[ignore = "requires MEMPHANT_TEST_DATABASE_URL"]
async fn stub_embeddings_persist_and_power_the_vector_channel() {
    use memphant_core::StubEmbedding;

    let store = connect().await;
    let tenant = fresh_tenant(&store).await;
    let scope = ScopeId::new();
    let actor = ActorId::new();
    let svc = MemoryService::new(
        Arc::new(store.clone()),
        Arc::new(CLOCK),
        Arc::new(StubEmbedding::default()),
    );

    retain_episode(
        svc.store(),
        retain_request(tenant, scope, actor, "Release region is Taipei.", None),
    )
    .await
    .expect("retain");
    svc.reflect(tenant, scope, None).await.expect("reflect");

    let page = store
        .scope_memory_page(tenant, scope, None, 100)
        .await
        .expect("page");
    assert!(!page.items.is_empty());
    let unit_ids: Vec<_> = page.items.iter().map(|unit| unit.id).collect();
    let rows = store
        .fetch_embeddings(tenant, &unit_ids)
        .await
        .expect("fetch embeddings");
    assert!(
        !rows.is_empty(),
        "compiled unit embeddings are durably persisted in Postgres"
    );
    assert!(rows.iter().all(|row| row.vec.len() == 32));

    let response = svc
        .recall(
            tenant,
            RecallHttpRequest {
                tenant_id: tenant,
                scope_id: scope,
                actor_id: actor,
                allowed_scope_ids: Some(vec![scope]),
                query: "Release region is Taipei.".to_string(),
                limit: Some(4),
                budget_tokens: Some(256),
                mode: None,
                include_beliefs: None,
                edge_expansion_enabled: None,
                context_packing_abstention_enabled: None,
                rerank_enabled: None,
                query_decomposition_enabled: None,
                procedure_recall_enabled: None,
                decay_enabled: None,
            },
        )
        .await
        .expect("recall");
    assert!(!response.items.is_empty());
    let trace = svc
        .trace(tenant, response.trace_id)
        .await
        .expect("trace fetch")
        .expect("trace stored");
    assert!(
        trace.candidates.iter().any(|candidate| candidate.channel
            == memphant_types::RecallChannel::Vector
            && candidate.channel_score > 0.0),
        "pgvector-backed vector channel produced scored candidates"
    );
}

/// The spec-mandated `embedding_profile_id` predicate (spec 03): the vector
/// query MUST filter by the active profile. This is not cosmetic — a unit that
/// also carries an embedding under a DIFFERENT-dimension profile would make the
/// `<=>` join compare mismatched dimensions and raise a pgvector error if the
/// predicate were dropped. The query succeeding (and returning the unit via its
/// active-profile row) is the regression guard.
#[tokio::test]
#[ignore = "requires MEMPHANT_TEST_DATABASE_URL"]
async fn vector_candidates_filter_by_embedding_profile() {
    use memphant_core::{
        EmbeddingProfileRow, EmbeddingProvider, EmbeddingRow, StubEmbedding,
        VECTOR_CANDIDATE_LIMIT, embedding_profile_for,
    };

    let store = connect().await;
    let tenant = fresh_tenant(&store).await;
    let scope = ScopeId::new();
    let actor = ActorId::new();
    let svc = MemoryService::new(
        Arc::new(store.clone()),
        Arc::new(CLOCK),
        Arc::new(StubEmbedding::default()),
    );

    retain_episode(
        svc.store(),
        retain_request(tenant, scope, actor, "Release region is Taipei.", None),
    )
    .await
    .expect("retain");
    svc.reflect(tenant, scope, None).await.expect("reflect");

    let page = store
        .scope_memory_page(tenant, scope, None, 100)
        .await
        .expect("page");
    assert!(!page.items.is_empty());
    let unit = page.items[0].id;

    let stub = StubEmbedding::default();
    let active_profile = embedding_profile_for(&stub); // 32-dim

    // A SECOND profile with a DIFFERENT dimension, plus an embedding for the
    // SAME unit under it. Without the `embedding_profile_id = $pid` predicate,
    // the vector join would also select this 4-dim row and pgvector would raise
    // a dimension-mismatch error against the 32-dim query vector.
    let other_profile = EmbeddingProfileRow {
        id: Uuid::now_v7(),
        provider: "stub-alt".to_string(),
        model: "stub-alt".to_string(),
        dimensions: 4,
        distance: "cosine".to_string(),
        version: "1".to_string(),
        index_strategy: "exact".to_string(),
    };
    store
        .upsert_embedding_profile(tenant, other_profile.clone())
        .await
        .expect("seed cross profile");
    store
        .upsert_embeddings(
            tenant,
            vec![EmbeddingRow {
                memory_unit_id: unit,
                embedding_profile_id: other_profile.id,
                vec: vec![0.1, 0.2, 0.3, 0.4],
            }],
        )
        .await
        .expect("insert cross-profile embedding");

    let query_vec = stub
        .embed(&["Release region Taipei.".to_string()])
        .expect("embed query")
        .remove(0);
    assert_eq!(query_vec.len(), 32);

    // WITH the predicate: only the 32-dim active-profile row is compared, so the
    // query succeeds and returns the unit. WITHOUT it, this call errors.
    let pairs = store
        .fetch_vector_candidates(
            tenant,
            &[scope],
            &[],
            &query_vec,
            active_profile.id,
            VECTOR_CANDIDATE_LIMIT,
        )
        .await
        .expect("vector query must filter cross-profile rows, not error on them");
    assert!(
        pairs.iter().any(|(candidate, _)| candidate.id == unit),
        "unit is returned via its active-profile (32-dim) embedding"
    );

    // Querying under the OTHER profile with a matching 4-dim vector reaches the
    // same unit via THAT profile's row — proving selection is by profile id.
    let other_pairs = store
        .fetch_vector_candidates(
            tenant,
            &[scope],
            &[],
            &[0.1_f32, 0.2, 0.3, 0.4],
            other_profile.id,
            VECTOR_CANDIDATE_LIMIT,
        )
        .await
        .expect("other-profile query");
    assert!(
        other_pairs
            .iter()
            .any(|(candidate, _)| candidate.id == unit),
        "unit is reachable under the cross profile via its 4-dim embedding"
    );
}

/// R0 determinism guard: re-ingesting the same corpus into a fresh tenant mints
/// new UUIDs, so any recall ordering point that breaks ties by insertion/heap
/// order reshuffles run-to-run (the measured ±1-question docs-gate variance).
/// The corpus is three token-PERMUTATIONS of the same words: distinct bodies
/// (distinct dedup keys — `normalize_component` preserves word order) but an
/// identical `StubEmbedding` vector, so they TIE exactly on vector distance.
/// Ingested in NON-body order, a tie-cut that fell back to insertion/uuid order
/// would diverge from the content order. The `<=>, unit.body` tie-break instead
/// pins the vector candidates to body order, and the whole recall-candidate
/// ordering is then identical across two independently-UUID'd tenants.
#[tokio::test]
#[ignore = "requires MEMPHANT_TEST_DATABASE_URL"]
async fn recall_ordering_is_content_stable_across_reingest() {
    use memphant_core::{
        EmbeddingProvider, StubEmbedding, VECTOR_CANDIDATE_LIMIT, embedding_profile_for,
    };

    // Inserted top-to-bottom; sorts bottom-to-top by body — insertion order is
    // the reverse of the content order the tie-break must produce.
    const CORPUS: [&str; 3] = [
        "region alpha bravo",
        "bravo region alpha",
        "alpha bravo region",
    ];
    let mut sorted_bodies: Vec<String> = CORPUS.iter().map(|s| s.to_string()).collect();
    sorted_bodies.sort();

    let store = connect().await;
    let stub = StubEmbedding::default();
    let profile = embedding_profile_for(&stub);
    let query_vec = stub
        .embed(&["region".to_string()])
        .expect("embed query")
        .remove(0);

    let only_corpus = |bodies: Vec<String>| -> Vec<String> {
        bodies
            .into_iter()
            .filter(|b| CORPUS.contains(&b.as_str()))
            .collect()
    };

    let mut per_tenant = Vec::new();
    for _ in 0..2 {
        let tenant = fresh_tenant(&store).await;
        let scope = ScopeId::new();
        let actor = ActorId::new();
        let svc = MemoryService::new(Arc::new(store.clone()), Arc::new(CLOCK), Arc::new(stub));
        for body in CORPUS {
            retain_episode(
                svc.store(),
                retain_request(tenant, scope, actor, body, None),
            )
            .await
            .expect("retain");
        }
        svc.reflect(tenant, scope, None).await.expect("reflect");

        let vector_bodies = only_corpus(
            store
                .fetch_vector_candidates(
                    tenant,
                    &[scope],
                    &[],
                    &query_vec,
                    profile.id,
                    VECTOR_CANDIDATE_LIMIT,
                )
                .await
                .expect("vector candidates")
                .into_iter()
                .map(|(unit, _)| unit.body)
                .collect(),
        );
        assert_eq!(
            vector_bodies, sorted_bodies,
            "tied vector candidates must order by body, not insertion/uuid order"
        );

        let recall_bodies = only_corpus(
            store
                .fetch_recall_candidates(tenant, &[scope], &[], &["region".to_string()], 100)
                .await
                .expect("recall candidates")
                .into_iter()
                .map(|unit| unit.body)
                .collect(),
        );
        per_tenant.push((vector_bodies, recall_bodies));
    }

    assert_eq!(
        per_tenant[0], per_tenant[1],
        "identical corpus recalls in identical order across a fresh-UUID re-ingest"
    );
}

/// Sibling of the recall-ordering guard, for the degraded read-your-own-writes
/// path: `fetch_episodes_for_scope` orders by `last_observed_at desc` and cuts
/// at LIMIT. Every episode staged in one transaction shares `last_observed_at`
/// (it is `now()`), so the recency key ties and the LIMIT boundary is decided
/// by the `dedup_key` tie-break — a total, content-derived order. Without it the
/// cut follows physical/insertion order and a fresh-UUID re-ingest surfaces a
/// different subset as degraded citations.
///
/// The two tenants ingest the SAME corpus in OPPOSITE orders and then have every
/// episode's `last_observed_at` collapsed to one instant: pre-fix the tied cut
/// tracks insertion order and the two disagree; the content tie-break makes them
/// agree. This is what makes the assertion a real guard rather than a
/// coincidental pass.
#[tokio::test]
#[ignore = "requires MEMPHANT_TEST_DATABASE_URL"]
async fn episode_scope_cut_is_content_stable_across_reingest() {
    const CORPUS: [&str; 5] = [
        "Episode fact alpha.",
        "Episode fact bravo.",
        "Episode fact charlie.",
        "Episode fact delta.",
        "Episode fact echo.",
    ];
    let store = connect().await;

    let mut per_tenant = Vec::new();
    for order in [[0, 1, 2, 3, 4], [4, 3, 2, 1, 0]] {
        let tenant = fresh_tenant(&store).await;
        let scope = ScopeId::new();
        let actor = ActorId::new();
        for index in order {
            retain_episode(
                &store,
                retain_request(tenant, scope, actor, CORPUS[index], None),
            )
            .await
            .expect("retain episode");
        }
        // Collapse every episode onto one `last_observed_at` in a single
        // statement (all rows get the same `now()`), forcing the recency tie the
        // content key must resolve.
        sqlx::query("update memphant.episode set last_observed_at = now() where tenant_id = $1")
            .bind(tenant.as_uuid())
            .execute(store.pool())
            .await
            .expect("collapse last_observed_at");

        let bodies: Vec<String> = store
            .fetch_episodes_for_scope(tenant, scope, 3)
            .await
            .expect("fetch episodes")
            .into_iter()
            .map(|episode| episode.body)
            .collect();
        per_tenant.push(bodies);
    }

    assert_eq!(per_tenant[0].len(), 3, "the cut returns the limit");
    assert_eq!(
        per_tenant[0], per_tenant[1],
        "the recency LIMIT cut must be content-stable regardless of ingest order / fresh UUIDs"
    );
}

/// Regression: a direct-unit retain into a scope with >1000 pre-existing units
/// must return the just-created unit's id. The response id used to be recovered
/// via `scope_memory_page`, which clamps to 1000 rows ordered by id, so a fresh
/// (larger) unit id fell off the page and `unit_ids` came back empty. The id now
/// comes straight from `reflect_recorded`, independent of scope size.
#[tokio::test]
#[ignore = "requires MEMPHANT_TEST_DATABASE_URL"]
async fn direct_unit_retain_returns_unit_id_past_scope_page_clamp() {
    let store = connect().await;
    let tenant = fresh_tenant(&store).await;
    let scope = ScopeId::new();
    let actor = ActorId::new();

    // Seed 1001 units in one reflect so the scope exceeds the 1000-row clamp.
    let candidates: Vec<ReflectCandidate> = (0..1001)
        .map(|i| ReflectCandidate {
            source_kind: "user".to_string(),
            trust_level: TrustLevel::TrustedUser,
            actor_id: actor,
            subject: Some(format!("seed-subject-{i}")),
            predicate: Some("is".to_string()),
            kind: Some(MemoryKind::Semantic),
            body: format!("seed fact number {i} about widgets"),
            churn_class: None,
            admission_hint: None,
            contextual_chunks: Vec::new(),
            valid_from: None,
            valid_to: None,
        })
        .collect();
    reflect_recorded(
        &store,
        ReflectInput {
            tenant_id: tenant,
            scope_id: scope,
            actor_id: actor,
            episode_id: None,
            resource_id: None,
            job_id: JobId::new(),
            compiler_version: "compiler-pg-seed".to_string(),
            candidates,
        },
        &NoopEmbedding,
        &CLOCK,
    )
    .await
    .expect("seed reflect succeeds");

    // `has_more` past the 1000-row page clamp proves the scope is oversized.
    let seeded = store
        .scope_memory_page(tenant, scope, None, usize::MAX)
        .await
        .expect("count seeded units");
    assert!(
        seeded.has_more,
        "scope must exceed the 1000-row page clamp, got {}",
        seeded.items.len()
    );

    let direct_body = "direct unit past the clamp".to_string();
    let response = service(store)
        .retain(
            tenant,
            RetainEpisodeHttpRequest {
                tenant_id: tenant,
                scope_id: scope,
                actor_id: actor,
                source_kind: "user".to_string(),
                source_trust: TrustLevel::TrustedUser,
                subject_hint: None,
                subject: None,
                predicate: None,
                body: None,
                resource: None,
                unit: Some(RetainUnitPayload {
                    kind: MemoryKind::Semantic,
                    subject: "direct-subject".to_string(),
                    predicate: "records".to_string(),
                    body: direct_body.clone(),
                    churn_class: None,
                }),
                compiler_version: Some("compiler-pg-seed".to_string()),
            },
        )
        .await
        .expect("direct unit retain succeeds");

    assert!(
        !response.unit_ids.is_empty(),
        "response must surface the created unit id regardless of scope size"
    );
    let store = connect().await;
    let resolved = store
        .fetch_units_by_ids(tenant, &response.unit_ids)
        .await
        .expect("resolve returned unit ids");
    assert!(
        resolved.iter().any(|unit| unit.body == direct_body),
        "a returned id must resolve to the just-created direct unit"
    );
}

/// Regression: the write compiler must supersede against the WHOLE scope, not a
/// recency-bounded slice. `reflect_recorded` loads its working set as "all open
/// units in the scope"; when that load was served by `fetch_recall_candidates`
/// (a ranked recall pool that PgStore caps at the 100 most-recent units), a
/// prior semantic unit that had aged past the window was invisible — so a
/// high-trust update failed to supersede it and instead tried to insert a second
/// open unit on the same `subject_key`, violating the scope-subject unique index
/// and hard-failing the write. Only PgStore reproduced it (the in-memory store
/// returns the full scope), so it was invisible to every in-memory test.
#[tokio::test]
#[ignore = "requires MEMPHANT_TEST_DATABASE_URL"]
async fn semantic_update_supersedes_a_unit_aged_past_the_recall_window() {
    const OLD_CLOCK: FixedClock = FixedClock("2026-07-01T00:00:00Z");
    let store = connect().await;
    let tenant = fresh_tenant(&store).await;
    let scope = ScopeId::new();
    let actor = ActorId::new();

    // The target unit, written oldest so recency-ordering buries it.
    reflect_recorded(
        &store,
        ReflectInput {
            tenant_id: tenant,
            scope_id: scope,
            actor_id: actor,
            episode_id: None,
            resource_id: None,
            job_id: JobId::new(),
            compiler_version: "compiler-pg-supersede".to_string(),
            candidates: vec![ReflectCandidate {
                source_kind: "user".to_string(),
                trust_level: TrustLevel::TrustedUser,
                actor_id: actor,
                subject: Some("role".to_string()),
                predicate: Some("is".to_string()),
                kind: Some(MemoryKind::Semantic),
                body: "the user is an admin".to_string(),
                churn_class: None,
                admission_hint: None,
                contextual_chunks: Vec::new(),
                valid_from: None,
                valid_to: None,
            }],
        },
        &NoopEmbedding,
        &OLD_CLOCK,
    )
    .await
    .expect("seed target unit");

    // 105 newer unrelated units push the target out of the most-recent-100.
    let fillers: Vec<ReflectCandidate> = (0..105)
        .map(|i| ReflectCandidate {
            source_kind: "user".to_string(),
            trust_level: TrustLevel::TrustedUser,
            actor_id: actor,
            subject: Some(format!("filler-{i}")),
            predicate: Some("is".to_string()),
            kind: Some(MemoryKind::Semantic),
            body: format!("filler fact number {i} about widgets"),
            churn_class: None,
            admission_hint: None,
            contextual_chunks: Vec::new(),
            valid_from: None,
            valid_to: None,
        })
        .collect();
    reflect_recorded(
        &store,
        ReflectInput {
            tenant_id: tenant,
            scope_id: scope,
            actor_id: actor,
            episode_id: None,
            resource_id: None,
            job_id: JobId::new(),
            compiler_version: "compiler-pg-supersede".to_string(),
            candidates: fillers,
        },
        &NoopEmbedding,
        &CLOCK,
    )
    .await
    .expect("seed fillers");

    // Update the same subject/predicate: must supersede the aged unit, not
    // collide with it on the scope-subject unique index.
    service(store)
        .retain(
            tenant,
            RetainEpisodeHttpRequest {
                tenant_id: tenant,
                scope_id: scope,
                actor_id: actor,
                source_kind: "user".to_string(),
                source_trust: TrustLevel::TrustedUser,
                subject_hint: None,
                subject: None,
                predicate: None,
                body: None,
                resource: None,
                unit: Some(RetainUnitPayload {
                    kind: MemoryKind::Semantic,
                    subject: "role".to_string(),
                    predicate: "is".to_string(),
                    body: "the user is a developer".to_string(),
                    churn_class: None,
                }),
                compiler_version: Some("compiler-pg-supersede".to_string()),
            },
        )
        .await
        .expect("update must supersede the aged unit, not fail on a duplicate subject");

    // Exactly one OPEN semantic unit remains for the subject, carrying the new
    // value; the old one is closed (superseded).
    let store = connect().await;
    let open: Vec<_> = store
        .fetch_scope_open_units(tenant, scope)
        .await
        .expect("fetch open units")
        .into_iter()
        .filter(|unit| unit.body.contains("the user is"))
        .collect();
    assert_eq!(
        open.len(),
        1,
        "the aged unit must be superseded, leaving one open value"
    );
    assert_eq!(open[0].body, "the user is a developer");
}

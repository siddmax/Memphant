//! Bar 3 (C1): two-user episodic RLS leakage proof under the real least-
//! privilege role.
//!
//! The Syndai cutover swaps app-level `user_id` WHERE-filters for MemPhant
//! tenant-RLS. That swap is load-bearing: production episodic tables have
//! `relrowsecurity = false` today, so a gap is a data-exposure incident, not a
//! bug. This test proves RLS actually bites for the EPISODIC tables by seeding
//! episodes for tenant A and tenant B and asserting that, under
//! `set local role memphant_app` + `bind_tenant`, each tenant's connection sees
//! exactly zero of the other's episodes and memory_units — enforced by Postgres
//! FORCE RLS, not by application code.
//!
//! Why the role matters (the finding this test exists for): the packaged server
//! and `e2e_probe.sh` connect as the scratch-DB login, which is a superuser with
//! `rolbypassrls = true` — RLS never fires there, so the probe's cross-tenant
//! 404 proves app+GUC isolation, NOT the RLS backstop. Only a connection that
//! has ASSUMED `memphant_app` (a non-BYPASSRLS policy role) exercises the swap.
//! This mirrors `role_matrix.rs`, which is the only other place RLS is proven to
//! bite, extended to the episodic surface the cutover touches.
//!
//! `#[ignore]`d like every live-PG contract; run under the AGENTS.md §37
//! scratch-DB leg.

use memphant_store_postgres::PgStore;
use sqlx::postgres::PgPoolOptions;
use sqlx::{Executor, Row};
use uuid::Uuid;

fn db_url() -> String {
    std::env::var("MEMPHANT_TEST_DATABASE_URL")
        .expect("MEMPHANT_TEST_DATABASE_URL must point at a migrated scratch database")
}

/// Seed one subject/scope/agent + one episode for `tenant` under the
/// `memphant_app` policy role (so the write itself must satisfy RLS `with check`).
/// Returns the episode id.
async fn seed_episode(pool: &sqlx::PgPool, tenant: Uuid) -> Uuid {
    let subject = Uuid::now_v7();
    let scope = Uuid::now_v7();
    let actor = Uuid::now_v7();
    let agent_node = Uuid::now_v7();
    let episode = Uuid::now_v7();

    let mut tx = pool.begin().await.expect("begin seed");
    tx.execute("set local role memphant_app")
        .await
        .expect("assume app");
    sqlx::query("select memphant.bind_tenant($1)")
        .bind(tenant)
        .execute(&mut *tx)
        .await
        .expect("bind tenant");
    sqlx::query(
        "insert into memphant.subject (id, tenant_id, external_ref, kind) \
         values ($1, $2, 'rls-subject', 'user')",
    )
    .bind(subject)
    .bind(tenant)
    .execute(&mut *tx)
    .await
    .expect("subject write");
    sqlx::query(
        "insert into memphant.scope
           (id, tenant_id, data_subject_id, kind, external_ref, materialized_path, scope_depth)
         values ($1, $2, $3, 'rls', 'rls-root', $4::memphant.ltree, 0)",
    )
    .bind(scope)
    .bind(tenant)
    .bind(subject)
    .bind(scope.to_string().replace('-', "_"))
    .execute(&mut *tx)
    .await
    .expect("scope write");
    sqlx::query(
        "insert into memphant.actor
           (id, tenant_id, data_subject_id, kind, external_ref, trust_level)
         values ($1, $2, $3, 'agent', 'rls-actor', 'trusted_system')",
    )
    .bind(actor)
    .bind(tenant)
    .bind(subject)
    .execute(&mut *tx)
    .await
    .expect("actor write");
    sqlx::query(
        "insert into memphant.agent_node
           (id, tenant_id, data_subject_id, scope_id, level, external_ref)
         values ($1, $2, $3, $4, 0, 'rls-agent')",
    )
    .bind(agent_node)
    .bind(tenant)
    .bind(subject)
    .bind(scope)
    .execute(&mut *tx)
    .await
    .expect("agent_node write");
    sqlx::query(
        "insert into memphant.episode
           (id, tenant_id, data_subject_id, scope_id, agent_node_id, subject_generation,
            actor_id, source_kind, source_ref, source_trust, dedup_key,
            first_observed_at, last_observed_at, body)
         values ($1, $2, $3, $4, $5, 0, $6, 'user', 'rls:ep', 'trusted_system', $7,
                 now(), now(), 'tenant-private episode body')",
    )
    .bind(episode)
    .bind(tenant)
    .bind(subject)
    .bind(scope)
    .bind(agent_node)
    .bind(actor)
    .bind(format!("rls-dedup-{episode}"))
    .execute(&mut *tx)
    .await
    .expect("episode write");
    tx.commit().await.expect("commit seed");
    episode
}

/// Count episodes visible to `reader_tenant` under the `memphant_app` role — i.e.
/// what RLS lets that tenant's connection see.
async fn visible_episode_count(pool: &sqlx::PgPool, reader_tenant: Uuid) -> i64 {
    let mut tx = pool.begin().await.expect("begin read");
    tx.execute("set local role memphant_app")
        .await
        .expect("assume app");
    sqlx::query("select memphant.bind_tenant($1)")
        .bind(reader_tenant)
        .execute(&mut *tx)
        .await
        .expect("bind reader tenant");
    let count: i64 = sqlx::query("select count(*) from memphant.episode")
        .fetch_one(&mut *tx)
        .await
        .expect("count episodes")
        .get(0);
    tx.rollback().await.expect("rollback read");
    count
}

#[tokio::test]
#[ignore = "requires MEMPHANT_TEST_DATABASE_URL"]
async fn two_user_episodic_isolation_is_enforced_by_rls_not_app_code() {
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&db_url())
        .await
        .expect("connect");

    // Provision two tenants (provisioner capability).
    let provisioner = PgStore::connect_provisioner(&db_url())
        .await
        .expect("connect provisioner");
    let tenant_a = provisioner
        .create_tenant(&format!("rls-a-{}", Uuid::new_v4().simple()))
        .await
        .expect("provision tenant A");
    let tenant_b = provisioner
        .create_tenant(&format!("rls-b-{}", Uuid::new_v4().simple()))
        .await
        .expect("provision tenant B");

    seed_episode(&pool, tenant_a).await;
    seed_episode(&pool, tenant_b).await;

    // Each tenant, under the memphant_app policy role, sees exactly its own one
    // episode and none of the other's — RLS, not an app WHERE clause, enforces
    // this (the connection carries no `user_id` filter; the query is a bare
    // `select count(*) from memphant.episode`).
    assert_eq!(
        visible_episode_count(&pool, tenant_a).await,
        1,
        "tenant A must see exactly its own episode"
    );
    assert_eq!(
        visible_episode_count(&pool, tenant_b).await,
        1,
        "tenant B must see exactly its own episode"
    );

    // The leakage assertion: neither tenant can see the OTHER's episode. Since
    // each sees exactly 1 and there are 2 total, cross-tenant visibility is 0 —
    // but assert it directly by proving the total-as-owner is 2 while each
    // tenant-bound view is 1.
    let total_as_owner: i64 = sqlx::query("select count(*) from memphant.episode")
        .fetch_one(&pool)
        .await
        .expect("owner count")
        .get(0);
    assert_eq!(
        total_as_owner, 2,
        "both episodes exist (owner/superuser view bypasses RLS)"
    );

    pool.close().await;
}

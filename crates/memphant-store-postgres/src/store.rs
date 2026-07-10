//! Durable `MemoryStore` implementation over Postgres (sqlx 0.9, runtime
//! queries) against the real 001+002 schema: composite `(tenant_id, id)`
//! primary keys, the reused `job_state` queue, `body_tsv` FTS,
//! `forgotten_source` tombstones and `api_key.max_trust` ceilings.

use std::collections::{HashMap, HashSet};

use memphant_core::{
    ApiKeyRow, CompiledWrite, CorrectOutcome, CorrectionWrite, EmbeddingRow, ForgetOutcome,
    ForgetWrite, JobFilter, MemoryStore, ReflectJobRow, ReviewEventRow, ScopePage, StoreError,
};
use memphant_types::{
    ActorId, ContextualChunk, CorrectResult, EdgeId, EpisodeId, ForgetTarget, JobId, MemoryKind,
    NewEpisode, NewMemoryEdge, NewMemoryUnit, NewResource, QueuedReflectJob, ReflectJob,
    ReflectJobKind, ReflectTrace, ResourceId, RetainOutcome, RetrievalTrace, ScopeId,
    StoredEpisode, StoredMemoryEdge, StoredMemoryUnit, StoredResource, TenantId, TraceId,
    TrustLevel, UnitId,
};
use serde::Serialize;
use serde::de::DeserializeOwned;
use sqlx::postgres::{PgPoolOptions, PgRow};
use sqlx::{AssertSqlSafe, PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

/// RFC 3339 UTC projection used for every timestamptz read; writes bind RFC
/// 3339 strings and cast `::timestamptz`.
const TS_FMT: &str = r#"'YYYY-MM-DD"T"HH24:MI:SS.US"Z"'"#;

fn ts(column: &str) -> String {
    format!("to_char({column} at time zone 'utc', {TS_FMT})")
}

fn backend(error: sqlx::Error) -> StoreError {
    StoreError::Backend(error.to_string())
}

fn enum_str<T: Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .expect("unit enums serialize to strings")
}

fn enum_from_str<T: DeserializeOwned>(value: &str) -> Result<T, StoreError> {
    serde_json::from_value(serde_json::Value::String(value.to_string()))
        .map_err(|error| StoreError::Backend(format!("bad enum value {value}: {error}")))
}

fn vec_literal(vec: &[f32]) -> String {
    let mut out = String::from("[");
    for (index, value) in vec.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str(&value.to_string());
    }
    out.push(']');
    out
}

/// Owned bind values for dynamically assembled queries.
enum Bind {
    Uuid(Uuid),
    UuidOpt(Option<Uuid>),
    UuidVec(Vec<Uuid>),
    Text(String),
    TextVec(Vec<String>),
    I64(i64),
}

#[derive(Clone)]
pub struct PgStore {
    pool: PgPool,
}

pub struct PgTxn {
    tx: Transaction<'static, Postgres>,
}

impl PgStore {
    /// Connects and pings; refuses to construct against an unreachable
    /// database.
    pub async fn connect(database_url: &str) -> Result<Self, StoreError> {
        let pool = PgPoolOptions::new()
            .max_connections(8)
            .connect(database_url)
            .await
            .map_err(backend)?;
        sqlx::query("select 1")
            .execute(&pool)
            .await
            .map_err(backend)?;
        Ok(Self { pool })
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    // ---- Admin surface (tenants + API keys; used by `memphant admin`). ----

    pub async fn create_tenant(&self, name: &str) -> Result<Uuid, StoreError> {
        let id = Uuid::now_v7();
        sqlx::query(
            "insert into memphant.tenant (id, slug, plan, region) values ($1, $2, 'dev', 'local')",
        )
        .bind(id)
        .bind(name)
        .execute(&self.pool)
        .await
        .map_err(backend)?;
        Ok(id)
    }

    pub async fn create_api_key(
        &self,
        tenant: Uuid,
        key_hash: &str,
        label: &str,
        max_trust: TrustLevel,
    ) -> Result<Uuid, StoreError> {
        let id = Uuid::now_v7();
        sqlx::query(
            "insert into memphant.api_key (id, tenant_id, key_hash, label, max_trust)
             values ($1, $2, $3, $4, $5)",
        )
        .bind(id)
        .bind(tenant)
        .bind(key_hash)
        .bind(label)
        .bind(enum_str(&max_trust))
        .execute(&self.pool)
        .await
        .map_err(backend)?;
        Ok(id)
    }

    pub async fn revoke_api_key(&self, id: Uuid) -> Result<bool, StoreError> {
        let result = sqlx::query(
            "update memphant.api_key set revoked_at = now() where id = $1 and revoked_at is null",
        )
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(backend)?;
        Ok(result.rows_affected() > 0)
    }

    // ---- Provisioning upserts (client-supplied scope/actor UUIDs never
    //      require pre-registration; rows are minted on first write). ----

    async fn ensure_tenant(
        tx: &mut Transaction<'static, Postgres>,
        tenant: TenantId,
    ) -> Result<(), StoreError> {
        sqlx::query(
            "insert into memphant.tenant (id, slug, plan, region)
             values ($1, $1::text, 'dev', 'local')
             on conflict (id) do nothing",
        )
        .bind(tenant.as_uuid())
        .execute(&mut **tx)
        .await
        .map_err(backend)?;
        Ok(())
    }

    async fn ensure_scope(
        tx: &mut Transaction<'static, Postgres>,
        tenant: TenantId,
        scope: ScopeId,
    ) -> Result<(), StoreError> {
        sqlx::query(
            "insert into memphant.scope (id, tenant_id, kind, external_ref, materialized_path, scope_depth)
             values ($1, $2, 'external', $1::text, text2ltree(replace($1::text, '-', '_')), 1)
             on conflict (tenant_id, id) do nothing",
        )
        .bind(scope.as_uuid())
        .bind(tenant.as_uuid())
        .execute(&mut **tx)
        .await
        .map_err(backend)?;
        Ok(())
    }

    async fn ensure_actor(
        tx: &mut Transaction<'static, Postgres>,
        tenant: TenantId,
        actor: ActorId,
    ) -> Result<(), StoreError> {
        sqlx::query(
            "insert into memphant.actor (id, tenant_id, kind, external_ref, trust_level)
             values ($1, $2, 'agent', $1::text, 'unverified_tool')
             on conflict (tenant_id, id) do nothing",
        )
        .bind(actor.as_uuid())
        .bind(tenant.as_uuid())
        .execute(&mut **tx)
        .await
        .map_err(backend)?;
        Ok(())
    }

    fn unit_select(where_clause: &str, tail: &str) -> String {
        format!(
            "select id, scope_id, kind, state, subject_key, body, trust_level, churn_class,
                    {freshness} as freshness_due_at, actor_id, source_kind, source_episode_id,
                    source_resource_id, deletion_generation, payload,
                    {valid_from} as valid_from, {valid_to} as valid_to,
                    {tx_from} as transaction_from, {tx_to} as transaction_to,
                    difficulty, stability_days, {reinforced} as last_reinforced_at,
                    reinforcement_count, tenant_id
             from memphant.memory_unit where {where_clause} {tail}",
            freshness = ts("freshness_due_at"),
            valid_from = ts("valid_from"),
            valid_to = ts("valid_to"),
            tx_from = ts("transaction_from"),
            tx_to = ts("transaction_to"),
            reinforced = ts("last_reinforced_at"),
        )
    }

    fn unit_from_row(row: &PgRow) -> Result<StoredMemoryUnit, StoreError> {
        let payload: serde_json::Value = row.try_get("payload").map_err(backend)?;
        let contextual_chunks: Vec<ContextualChunk> = payload
            .get("contextual_chunks")
            .cloned()
            .map(serde_json::from_value)
            .transpose()
            .map_err(|error| StoreError::Backend(error.to_string()))?
            .unwrap_or_default();
        Ok(StoredMemoryUnit {
            id: UnitId::from_u128(row.try_get::<Uuid, _>("id").map_err(backend)?.as_u128()),
            tenant_id: TenantId::from_u128(
                row.try_get::<Uuid, _>("tenant_id")
                    .map_err(backend)?
                    .as_u128(),
            ),
            scope_id: ScopeId::from_u128(
                row.try_get::<Uuid, _>("scope_id")
                    .map_err(backend)?
                    .as_u128(),
            ),
            kind: enum_from_str(row.try_get::<String, _>("kind").map_err(backend)?.as_str())?,
            state: enum_from_str(row.try_get::<String, _>("state").map_err(backend)?.as_str())?,
            subject_key: row.try_get("subject_key").map_err(backend)?,
            body: row.try_get("body").map_err(backend)?,
            trust_level: enum_from_str(
                row.try_get::<String, _>("trust_level")
                    .map_err(backend)?
                    .as_str(),
            )?,
            churn_class: row.try_get("churn_class").map_err(backend)?,
            freshness_due_at: row.try_get("freshness_due_at").map_err(backend)?,
            actor_id: row
                .try_get::<Option<Uuid>, _>("actor_id")
                .map_err(backend)?
                .map(|id| ActorId::from_u128(id.as_u128())),
            source_kind: row.try_get("source_kind").map_err(backend)?,
            source_episode_id: row
                .try_get::<Option<Uuid>, _>("source_episode_id")
                .map_err(backend)?
                .map(|id| EpisodeId::from_u128(id.as_u128())),
            source_resource_id: row
                .try_get::<Option<Uuid>, _>("source_resource_id")
                .map_err(backend)?
                .map(|id| ResourceId::from_u128(id.as_u128())),
            deletion_generation: row
                .try_get::<Option<i64>, _>("deletion_generation")
                .map_err(backend)?
                .map(|generation| generation as u64),
            contextual_chunks,
            valid_from: row.try_get("valid_from").map_err(backend)?,
            valid_to: row.try_get("valid_to").map_err(backend)?,
            transaction_from: row.try_get("transaction_from").map_err(backend)?,
            transaction_to: row.try_get("transaction_to").map_err(backend)?,
            difficulty: row.try_get("difficulty").map_err(backend)?,
            stability_days: row.try_get("stability_days").map_err(backend)?,
            last_reinforced_at: row.try_get("last_reinforced_at").map_err(backend)?,
            reinforcement_count: row
                .try_get::<i32, _>("reinforcement_count")
                .map_err(backend)? as u32,
        })
    }

    fn episode_from_row(row: &PgRow) -> Result<StoredEpisode, StoreError> {
        Ok(StoredEpisode {
            id: EpisodeId::from_u128(row.try_get::<Uuid, _>("id").map_err(backend)?.as_u128()),
            tenant_id: TenantId::from_u128(
                row.try_get::<Uuid, _>("tenant_id")
                    .map_err(backend)?
                    .as_u128(),
            ),
            scope_id: ScopeId::from_u128(
                row.try_get::<Uuid, _>("scope_id")
                    .map_err(backend)?
                    .as_u128(),
            ),
            actor_id: ActorId::from_u128(
                row.try_get::<Uuid, _>("actor_id")
                    .map_err(backend)?
                    .as_u128(),
            ),
            source_kind: row.try_get("source_kind").map_err(backend)?,
            source_trust: enum_from_str(
                row.try_get::<String, _>("source_trust")
                    .map_err(backend)?
                    .as_str(),
            )?,
            dedup_key: row.try_get("dedup_key").map_err(backend)?,
            body: row
                .try_get::<Option<String>, _>("body")
                .map_err(backend)?
                .unwrap_or_default(),
            observation_count: row
                .try_get::<i32, _>("observation_count")
                .map_err(backend)? as u32,
        })
    }

    async fn insert_unit(
        tx: &mut Transaction<'static, Postgres>,
        unit: &StoredMemoryUnit,
    ) -> Result<(), StoreError> {
        Self::ensure_scope(tx, unit.tenant_id, unit.scope_id).await?;
        let payload = serde_json::json!({ "contextual_chunks": unit.contextual_chunks });
        sqlx::query(
            "insert into memphant.memory_unit
               (id, tenant_id, scope_id, kind, state, subject_key, body, payload, trust_level,
                valid_from, valid_to, transaction_from, transaction_to, difficulty,
                stability_days, last_reinforced_at, reinforcement_count, freshness_due_at,
                deletion_generation, actor_id, source_kind, source_episode_id,
                source_resource_id, churn_class)
             values ($1, $2, $3, $4, $5, $6, $7, $8, $9,
                     $10::timestamptz, $11::timestamptz, coalesce($12::timestamptz, now()),
                     $13::timestamptz, $14, $15, $16::timestamptz, $17, $18::timestamptz,
                     $19, $20, $21, $22, $23, $24)",
        )
        .bind(unit.id.as_uuid())
        .bind(unit.tenant_id.as_uuid())
        .bind(unit.scope_id.as_uuid())
        .bind(enum_str(&unit.kind))
        .bind(enum_str(&unit.state))
        .bind(&unit.subject_key)
        .bind(&unit.body)
        .bind(payload)
        .bind(enum_str(&unit.trust_level))
        .bind(&unit.valid_from)
        .bind(&unit.valid_to)
        .bind(&unit.transaction_from)
        .bind(&unit.transaction_to)
        .bind(unit.difficulty)
        .bind(unit.stability_days)
        .bind(&unit.last_reinforced_at)
        .bind(unit.reinforcement_count as i32)
        .bind(&unit.freshness_due_at)
        .bind(unit.deletion_generation.map(|generation| generation as i64))
        .bind(unit.actor_id.map(|id| id.as_uuid()))
        .bind(&unit.source_kind)
        .bind(unit.source_episode_id.map(|id| id.as_uuid()))
        .bind(unit.source_resource_id.map(|id| id.as_uuid()))
        .bind(&unit.churn_class)
        .execute(&mut **tx)
        .await
        .map_err(backend)?;
        Ok(())
    }

    async fn insert_edge(
        tx: &mut Transaction<'static, Postgres>,
        edge: &StoredMemoryEdge,
    ) -> Result<(), StoreError> {
        sqlx::query(
            "insert into memphant.memory_edge (id, tenant_id, scope_id, src_id, dst_id, kind)
             values ($1, $2, $3, $4, $5, $6)
             on conflict (tenant_id, src_id, dst_id, kind) do nothing",
        )
        .bind(edge.id.as_uuid())
        .bind(edge.tenant_id.as_uuid())
        .bind(edge.scope_id.as_uuid())
        .bind(edge.src_id.as_uuid())
        .bind(edge.dst_id.as_uuid())
        .bind(enum_str(&edge.kind))
        .execute(&mut **tx)
        .await
        .map_err(backend)?;
        Ok(())
    }

    async fn fetch_units_where(
        &self,
        where_clause: &str,
        tail: &str,
        binds: Vec<Bind>,
    ) -> Result<Vec<StoredMemoryUnit>, StoreError> {
        let sql = Self::unit_select(where_clause, tail);
        let mut query = sqlx::query(AssertSqlSafe(sql.as_str()));
        for bind in binds {
            query = match bind {
                Bind::Uuid(value) => query.bind(value),
                Bind::UuidOpt(value) => query.bind(value),
                Bind::UuidVec(value) => query.bind(value),
                Bind::Text(value) => query.bind(value),
                Bind::TextVec(value) => query.bind(value),
                Bind::I64(value) => query.bind(value),
            };
        }
        let rows = query.fetch_all(&self.pool).await.map_err(backend)?;
        rows.iter().map(Self::unit_from_row).collect()
    }

    /// Deletes composition-derived dependents of the given source units;
    /// returns the ids transitioned.
    async fn delete_composed_dependents(
        tx: &mut Transaction<'static, Postgres>,
        tenant: TenantId,
        source_ids: &[Uuid],
        generation: i64,
    ) -> Result<Vec<UnitId>, StoreError> {
        let rows = sqlx::query(
            "update memphant.memory_unit set state = 'deleted', deletion_generation = $3,
                    transaction_to = now()
             where tenant_id = $1 and state <> 'deleted' and source_kind = 'composition'
               and id in (select src_id from memphant.memory_edge
                          where tenant_id = $1 and kind = 'derived_from' and dst_id = any($2))
             returning id",
        )
        .bind(tenant.as_uuid())
        .bind(source_ids)
        .bind(generation)
        .fetch_all(&mut **tx)
        .await
        .map_err(backend)?;
        rows.iter()
            .map(|row| {
                Ok(UnitId::from_u128(
                    row.try_get::<Uuid, _>("id").map_err(backend)?.as_u128(),
                ))
            })
            .collect()
    }
}

impl MemoryStore for PgStore {
    type Txn = PgTxn;

    async fn begin(&self) -> Self::Txn {
        PgTxn {
            tx: self.pool.begin().await.expect("begin postgres transaction"),
        }
    }

    async fn commit(&self, tx: Self::Txn) -> Result<(), StoreError> {
        tx.tx.commit().await.map_err(backend)
    }

    async fn stage_episode(
        &self,
        tx: &mut Self::Txn,
        episode: NewEpisode,
    ) -> Result<RetainOutcome, StoreError> {
        Self::ensure_tenant(&mut tx.tx, episode.tenant_id).await?;
        Self::ensure_scope(&mut tx.tx, episode.tenant_id, episode.scope_id).await?;
        Self::ensure_actor(&mut tx.tx, episode.tenant_id, episode.actor_id).await?;
        let row = sqlx::query(
            "insert into memphant.episode
               (id, tenant_id, scope_id, actor_id, source_kind, source_trust, dedup_key, body,
                first_observed_at, last_observed_at)
             values ($1, $2, $3, $4, $5, $6, $7, $8, now(), now())
             on conflict (tenant_id, scope_id, dedup_key) do update
               set observation_count = memphant.episode.observation_count + 1,
                   last_observed_at = now()
             returning id, observation_count, (xmax = 0) as inserted",
        )
        .bind(Uuid::now_v7())
        .bind(episode.tenant_id.as_uuid())
        .bind(episode.scope_id.as_uuid())
        .bind(episode.actor_id.as_uuid())
        .bind(&episode.source_kind)
        .bind(enum_str(&episode.source_trust))
        .bind(&episode.dedup_key)
        .bind(&episode.body)
        .fetch_one(&mut *tx.tx)
        .await
        .map_err(backend)?;
        let inserted: bool = row.try_get("inserted").map_err(backend)?;
        Ok(RetainOutcome {
            episode_id: EpisodeId::from_u128(
                row.try_get::<Uuid, _>("id").map_err(backend)?.as_u128(),
            ),
            dedup: memphant_types::DedupOutcome {
                matched: !inserted,
                observation_count: row
                    .try_get::<i32, _>("observation_count")
                    .map_err(backend)? as u32,
            },
        })
    }

    async fn stage_memory_unit(
        &self,
        tx: &mut Self::Txn,
        unit: NewMemoryUnit,
    ) -> Result<UnitId, StoreError> {
        Self::ensure_tenant(&mut tx.tx, unit.tenant_id).await?;
        let id = UnitId::new();
        let stored = StoredMemoryUnit {
            id,
            tenant_id: unit.tenant_id,
            scope_id: unit.scope_id,
            kind: unit.kind,
            state: unit.state,
            subject_key: unit.subject_key,
            body: unit.body,
            trust_level: unit.trust_level,
            churn_class: unit.churn_class,
            freshness_due_at: unit.freshness_due_at,
            actor_id: unit.actor_id,
            source_kind: unit.source_kind,
            source_episode_id: unit.source_episode_id,
            source_resource_id: unit.source_resource_id,
            deletion_generation: unit.deletion_generation,
            contextual_chunks: unit.contextual_chunks,
            valid_from: unit.valid_from,
            valid_to: unit.valid_to,
            transaction_from: unit.transaction_from,
            transaction_to: unit.transaction_to,
            difficulty: None,
            stability_days: None,
            last_reinforced_at: None,
            reinforcement_count: 0,
        };
        Self::insert_unit(&mut tx.tx, &stored).await?;
        Ok(id)
    }

    async fn stage_resource(
        &self,
        tx: &mut Self::Txn,
        resource: NewResource,
    ) -> Result<ResourceId, StoreError> {
        Self::ensure_tenant(&mut tx.tx, resource.tenant_id).await?;
        Self::ensure_scope(&mut tx.tx, resource.tenant_id, resource.scope_id).await?;
        Self::ensure_actor(&mut tx.tx, resource.tenant_id, resource.actor_id).await?;
        let id = ResourceId::new();
        sqlx::query(
            "insert into memphant.resource
               (id, tenant_id, scope_id, kind, uri, content_hash, actor_id, mime_type,
                revision, body, source_trust)
             values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
        )
        .bind(id.as_uuid())
        .bind(resource.tenant_id.as_uuid())
        .bind(resource.scope_id.as_uuid())
        .bind(enum_str(&resource.kind))
        .bind(&resource.uri)
        .bind(&resource.content_hash)
        .bind(resource.actor_id.as_uuid())
        .bind(&resource.mime_type)
        .bind(&resource.revision)
        .bind(&resource.body)
        .bind(enum_str(&resource.source_trust))
        .execute(&mut *tx.tx)
        .await
        .map_err(backend)?;
        Ok(id)
    }

    async fn stage_memory_edge(
        &self,
        tx: &mut Self::Txn,
        edge: NewMemoryEdge,
    ) -> Result<EdgeId, StoreError> {
        let id = EdgeId::new();
        Self::insert_edge(
            &mut tx.tx,
            &StoredMemoryEdge {
                id,
                tenant_id: edge.tenant_id,
                scope_id: edge.scope_id,
                src_id: edge.src_id,
                dst_id: edge.dst_id,
                kind: edge.kind,
            },
        )
        .await?;
        Ok(id)
    }

    async fn enqueue_reflect(
        &self,
        tx: &mut Self::Txn,
        job: ReflectJob,
    ) -> Result<JobId, StoreError> {
        let id = JobId::new();
        let (job_type, target) = match job.kind {
            ReflectJobKind::ReflectEpisode => (
                "reflect_episode",
                job.episode_id.map(|episode| episode.as_uuid()),
            ),
            ReflectJobKind::ReflectResource => (
                "reflect_resource",
                job.resource_id.map(|resource| resource.as_uuid()),
            ),
        };
        let target = target.ok_or(StoreError::NotFound("reflect job target"))?;
        sqlx::query(
            "insert into memphant.job_state
               (id, tenant_id, job_type, target_id, compiler_version, state, scope_id,
                subject, predicate)
             values ($1, $2, $3, $4, $5, 'queued', $6, $7, $8)
             on conflict (tenant_id, job_type, target_id, compiler_version) do update
               set state = case when memphant.job_state.state in ('done', 'dead')
                                then 'queued' else memphant.job_state.state end,
                   run_after = now()",
        )
        .bind(id.as_uuid())
        .bind(job.tenant_id.as_uuid())
        .bind(job_type)
        .bind(target)
        .bind(&job.compiler_version)
        .bind(job.scope_id.as_uuid())
        .bind(&job.subject)
        .bind(&job.predicate)
        .execute(&mut *tx.tx)
        .await
        .map_err(backend)?;
        Ok(id)
    }

    async fn fetch_recall_candidates(
        &self,
        tenant: TenantId,
        scopes: &[ScopeId],
        kinds: &[MemoryKind],
        query_terms: &[String],
        query_vec: Option<&[f32]>,
        limit: usize,
    ) -> Result<Vec<StoredMemoryUnit>, StoreError> {
        let scope_uuids: Vec<Uuid> = scopes.iter().map(|scope| scope.as_uuid()).collect();
        let kind_strs: Vec<String> = kinds.iter().map(enum_str).collect();
        let base = "tenant_id = $1 and scope_id = any($2)
                    and (cardinality($3::text[]) = 0 or kind = any($3))
                    and transaction_to is null";
        let mut seen: HashSet<Uuid> = HashSet::new();
        let mut units = Vec::new();
        let mut extend = |fetched: Vec<StoredMemoryUnit>| {
            for unit in fetched {
                if seen.insert(unit.id.as_uuid()) {
                    units.push(unit);
                }
            }
        };

        // Family 1: FTS top-N over body_tsv (websearch, OR-joined terms).
        if !query_terms.is_empty() {
            let websearch = query_terms.join(" or ");
            let fetched = self
                .fetch_units_where(
                    &format!(
                        "{base} and body_tsv @@ websearch_to_tsquery('english', $4)"
                    ),
                    "order by ts_rank_cd(body_tsv, websearch_to_tsquery('english', $4)) desc limit 200",
                    vec![
                        Bind::Uuid(tenant.as_uuid()),
                        Bind::UuidVec(scope_uuids.clone()),
                        Bind::TextVec(kind_strs.clone()),
                        Bind::Text(websearch.clone()),
                    ],
                )
                .await?;
            extend(fetched);
        }

        // Family 2: most-recent-M per scope.
        for scope in &scope_uuids {
            let fetched = self
                .fetch_units_where(
                    &base.replace("scope_id = any($2)", "scope_id = $2"),
                    "order by transaction_from desc limit 100",
                    vec![
                        Bind::Uuid(tenant.as_uuid()),
                        Bind::Uuid(*scope),
                        Bind::TextVec(kind_strs.clone()),
                    ],
                )
                .await?;
            extend(fetched);
        }

        // Family 3: exact-subject matches.
        if !query_terms.is_empty() {
            let fetched = self
                .fetch_units_where(
                    &format!(
                        "{base} and subject_key is not null
                         and exists (select 1 from unnest($4::text[]) term
                                     where memphant.memory_unit.subject_key ilike '%' || term || '%')"
                    ),
                    "limit 200",
                    vec![
                        Bind::Uuid(tenant.as_uuid()),
                        Bind::UuidVec(scope_uuids.clone()),
                        Bind::TextVec(kind_strs.clone()),
                        Bind::TextVec(query_terms.to_vec()),
                    ],
                )
                .await?;
            extend(fetched);
        }

        // Family 4: vector top-K (only when a real embedding was provided).
        if let Some(vec) = query_vec.filter(|vec| !vec.is_empty()) {
            let sql = format!(
                "select unit.* from ({inner}) unit
                 join memphant.embedding embedding
                   on embedding.tenant_id = $1 and embedding.memory_unit_id = unit.id
                 order by embedding.vec <=> $4::halfvec limit 32",
                inner = Self::unit_select(base, "")
            );
            let rows = sqlx::query(AssertSqlSafe(sql.as_str()))
                .bind(tenant.as_uuid())
                .bind(scope_uuids.clone())
                .bind(kind_strs.clone())
                .bind(vec_literal(vec))
                .fetch_all(&self.pool)
                .await
                .map_err(backend)?;
            let fetched: Result<Vec<_>, _> = rows.iter().map(Self::unit_from_row).collect();
            extend(fetched?);
        }

        units.truncate(limit.min(1_000));
        Ok(units)
    }

    async fn fetch_units_by_ids(
        &self,
        tenant: TenantId,
        ids: &[UnitId],
    ) -> Result<Vec<StoredMemoryUnit>, StoreError> {
        let uuids: Vec<Uuid> = ids.iter().map(|id| id.as_uuid()).collect();
        self.fetch_units_where(
            "tenant_id = $1 and id = any($2)",
            "",
            vec![Bind::Uuid(tenant.as_uuid()), Bind::UuidVec(uuids)],
        )
        .await
    }

    async fn fetch_edges(
        &self,
        tenant: TenantId,
        unit_ids: &[UnitId],
    ) -> Result<Vec<StoredMemoryEdge>, StoreError> {
        let uuids: Vec<Uuid> = unit_ids.iter().map(|id| id.as_uuid()).collect();
        let rows = sqlx::query(
            "select id, tenant_id, scope_id, src_id, dst_id, kind from memphant.memory_edge
             where tenant_id = $1 and (src_id = any($2) or dst_id = any($2))",
        )
        .bind(tenant.as_uuid())
        .bind(uuids)
        .fetch_all(&self.pool)
        .await
        .map_err(backend)?;
        rows.iter()
            .map(|row| {
                Ok(StoredMemoryEdge {
                    id: EdgeId::from_u128(row.try_get::<Uuid, _>("id").map_err(backend)?.as_u128()),
                    tenant_id: TenantId::from_u128(
                        row.try_get::<Uuid, _>("tenant_id")
                            .map_err(backend)?
                            .as_u128(),
                    ),
                    scope_id: ScopeId::from_u128(
                        row.try_get::<Uuid, _>("scope_id")
                            .map_err(backend)?
                            .as_u128(),
                    ),
                    src_id: UnitId::from_u128(
                        row.try_get::<Uuid, _>("src_id").map_err(backend)?.as_u128(),
                    ),
                    dst_id: UnitId::from_u128(
                        row.try_get::<Uuid, _>("dst_id").map_err(backend)?.as_u128(),
                    ),
                    kind: enum_from_str(
                        row.try_get::<String, _>("kind").map_err(backend)?.as_str(),
                    )?,
                })
            })
            .collect()
    }

    async fn fetch_review_events(
        &self,
        tenant: TenantId,
        unit_ids: &[UnitId],
    ) -> Result<Vec<ReviewEventRow>, StoreError> {
        let uuids: Vec<Uuid> = unit_ids.iter().map(|id| id.as_uuid()).collect();
        let rows = sqlx::query(
            "select event.id, event.trace_id, event.caller_id, event.outcome,
                    coalesce(array_agg(unit.memory_unit_id)
                             filter (where unit.memory_unit_id is not null), '{}') as used_ids
             from memphant.review_event event
             left join memphant.review_event_unit unit on unit.review_event_id = event.id
             where event.tenant_id = $1
             group by event.id, event.trace_id, event.caller_id, event.outcome
             having count(unit.memory_unit_id) = 0
                 or bool_or(unit.memory_unit_id = any($2))",
        )
        .bind(tenant.as_uuid())
        .bind(uuids)
        .fetch_all(&self.pool)
        .await
        .map_err(backend)?;
        rows.iter()
            .map(|row| {
                Ok(ReviewEventRow {
                    tenant_id: tenant,
                    trace_id: TraceId::from_u128(
                        row.try_get::<Uuid, _>("trace_id")
                            .map_err(backend)?
                            .as_u128(),
                    ),
                    caller_id: row.try_get("caller_id").map_err(backend)?,
                    used_ids: row
                        .try_get::<Vec<Uuid>, _>("used_ids")
                        .map_err(backend)?
                        .into_iter()
                        .map(|id| UnitId::from_u128(id.as_u128()))
                        .collect(),
                    outcome: enum_from_str(
                        row.try_get::<String, _>("outcome")
                            .map_err(backend)?
                            .as_str(),
                    )?,
                })
            })
            .collect()
    }

    async fn fetch_episodes_for_scope(
        &self,
        tenant: TenantId,
        scope: ScopeId,
        limit: usize,
    ) -> Result<Vec<StoredEpisode>, StoreError> {
        let rows = sqlx::query(
            "select id, tenant_id, scope_id, actor_id, source_kind, source_trust, dedup_key,
                    body, observation_count
             from memphant.episode
             where tenant_id = $1 and scope_id = $2 and deletion_generation is null
             order by last_observed_at desc limit $3",
        )
        .bind(tenant.as_uuid())
        .bind(scope.as_uuid())
        .bind(limit.min(1_000) as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(backend)?;
        rows.iter().map(Self::episode_from_row).collect()
    }

    async fn pending_job_count(
        &self,
        tenant: TenantId,
        scope: ScopeId,
    ) -> Result<usize, StoreError> {
        let count: i64 = sqlx::query_scalar(
            "select count(*) from memphant.job_state
             where tenant_id = $1 and scope_id = $2 and state in ('queued', 'running')",
        )
        .bind(tenant.as_uuid())
        .bind(scope.as_uuid())
        .fetch_one(&self.pool)
        .await
        .map_err(backend)?;
        Ok(count as usize)
    }

    async fn fetch_episode(
        &self,
        tenant: TenantId,
        id: EpisodeId,
    ) -> Result<Option<StoredEpisode>, StoreError> {
        let row = sqlx::query(
            "select id, tenant_id, scope_id, actor_id, source_kind, source_trust, dedup_key,
                    body, observation_count
             from memphant.episode
             where tenant_id = $1 and id = $2 and deletion_generation is null",
        )
        .bind(tenant.as_uuid())
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(backend)?;
        row.as_ref().map(Self::episode_from_row).transpose()
    }

    async fn fetch_resource(
        &self,
        tenant: TenantId,
        id: ResourceId,
    ) -> Result<Option<StoredResource>, StoreError> {
        let row = sqlx::query(
            "select id, tenant_id, scope_id, coalesce(actor_id, tenant_id) as actor_id, kind,
                    uri, content_hash, mime_type, revision, body, source_trust, extractor_state
             from memphant.resource where tenant_id = $1 and id = $2",
        )
        .bind(tenant.as_uuid())
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(backend)?;
        let Some(row) = row else { return Ok(None) };
        Ok(Some(StoredResource {
            id: ResourceId::from_u128(row.try_get::<Uuid, _>("id").map_err(backend)?.as_u128()),
            tenant_id: TenantId::from_u128(
                row.try_get::<Uuid, _>("tenant_id")
                    .map_err(backend)?
                    .as_u128(),
            ),
            scope_id: ScopeId::from_u128(
                row.try_get::<Uuid, _>("scope_id")
                    .map_err(backend)?
                    .as_u128(),
            ),
            actor_id: ActorId::from_u128(
                row.try_get::<Uuid, _>("actor_id")
                    .map_err(backend)?
                    .as_u128(),
            ),
            uri: row.try_get("uri").map_err(backend)?,
            kind: enum_from_str(row.try_get::<String, _>("kind").map_err(backend)?.as_str())
                .unwrap_or_default(),
            content_hash: row.try_get("content_hash").map_err(backend)?,
            mime_type: row
                .try_get::<Option<String>, _>("mime_type")
                .map_err(backend)?
                .unwrap_or_default(),
            revision: row.try_get("revision").map_err(backend)?,
            body: row.try_get("body").map_err(backend)?,
            source_trust: enum_from_str(
                row.try_get::<String, _>("source_trust")
                    .map_err(backend)?
                    .as_str(),
            )
            .unwrap_or(TrustLevel::Quarantined),
            extractor_state: enum_from_str(
                row.try_get::<String, _>("extractor_state")
                    .map_err(backend)?
                    .as_str(),
            )?,
        }))
    }

    async fn apply_correction(
        &self,
        tenant: TenantId,
        correction: CorrectionWrite,
    ) -> Result<CorrectOutcome, StoreError> {
        let mut tx = self.pool.begin().await.map_err(backend)?;
        let old_id = correction.selector.memory_unit_id;
        let sql = Self::unit_select(
            "tenant_id = $1 and id = $2 and scope_id = $3 and state <> 'deleted'",
            "for update",
        );
        let row = sqlx::query(AssertSqlSafe(sql.as_str()))
            .bind(tenant.as_uuid())
            .bind(old_id.as_uuid())
            .bind(correction.scope_id.as_uuid())
            .fetch_optional(&mut *tx)
            .await
            .map_err(backend)?
            .ok_or(StoreError::NotFound("memory_unit"))?;
        let old_unit = Self::unit_from_row(&row)?;
        let is_retroactive =
            correction.correction.valid_from.is_some() || correction.correction.valid_to.is_some();

        sqlx::query(
            "update memphant.memory_unit set state = 'superseded', transaction_to = $3::timestamptz
             where tenant_id = $1 and id = $2",
        )
        .bind(tenant.as_uuid())
        .bind(old_id.as_uuid())
        .bind(&correction.now)
        .execute(&mut *tx)
        .await
        .map_err(backend)?;

        let new_id = UnitId::new();
        let mut replacement = old_unit.clone();
        replacement.id = new_id;
        replacement.body = correction.correction.value.clone();
        replacement.state = memphant_types::UnitState::Active;
        replacement.actor_id = Some(correction.actor_id);
        replacement.deletion_generation = None;
        replacement.valid_from = correction.correction.valid_from.clone();
        replacement.valid_to = correction.correction.valid_to.clone();
        replacement.transaction_from = Some(correction.now.clone());
        replacement.transaction_to = None;
        Self::ensure_actor(&mut tx, tenant, correction.actor_id).await?;
        Self::insert_unit(&mut tx, &replacement).await?;
        Self::insert_edge(
            &mut tx,
            &StoredMemoryEdge {
                id: EdgeId::new(),
                tenant_id: tenant,
                scope_id: correction.scope_id,
                src_id: new_id,
                dst_id: old_id,
                kind: memphant_types::MemoryEdgeKind::Supersedes,
            },
        )
        .await?;

        // Expire composition-derived dependents of the superseded unit.
        sqlx::query(
            "update memphant.memory_unit set state = 'expired', transaction_to = $3::timestamptz
             where tenant_id = $1 and state <> 'deleted' and transaction_to is null
               and source_kind = 'composition'
               and id in (select src_id from memphant.memory_edge
                          where tenant_id = $1 and kind = 'derived_from' and dst_id = $2)",
        )
        .bind(tenant.as_uuid())
        .bind(old_id.as_uuid())
        .bind(&correction.now)
        .execute(&mut *tx)
        .await
        .map_err(backend)?;

        tx.commit().await.map_err(backend)?;
        Ok(CorrectResult {
            correction_id: format!("cor_{}", new_id.as_uuid()),
            superseded: vec![old_id],
            created: vec![new_id],
            correction_kind: if is_retroactive {
                "retroactive".to_string()
            } else {
                "current".to_string()
            },
            trace_ref: None,
        })
    }

    async fn apply_forget(
        &self,
        tenant: TenantId,
        forget: ForgetWrite,
    ) -> Result<ForgetOutcome, StoreError> {
        let mut tx = self.pool.begin().await.map_err(backend)?;
        Self::ensure_tenant(&mut tx, tenant).await?;
        Self::ensure_scope(&mut tx, tenant, forget.scope_id).await?;
        Self::ensure_actor(&mut tx, tenant, forget.actor_id).await?;

        let (source_kind, source_id, unit_filter) = match forget.target {
            ForgetTarget::MemoryUnit(id) => ("memory_unit", id.as_uuid(), "id = $4"),
            ForgetTarget::Episode(id) => ("episode", id.as_uuid(), "source_episode_id = $4"),
            ForgetTarget::Resource(id) => ("resource", id.as_uuid(), "source_resource_id = $4"),
        };

        // Durable tombstone: blocks re-derivation in persist_compiled_units.
        sqlx::query(
            "insert into memphant.forgotten_source (tenant_id, source_kind, source_id)
             values ($1, $2, $3) on conflict do nothing",
        )
        .bind(tenant.as_uuid())
        .bind(source_kind)
        .bind(source_id)
        .execute(&mut *tx)
        .await
        .map_err(backend)?;

        let generation: i64 = sqlx::query_scalar(
            "insert into memphant.deletion_generation
               (tenant_id, scope_id, requested_by, state, completed_at)
             values ($1, $2, $3, 'completed', now()) returning id",
        )
        .bind(tenant.as_uuid())
        .bind(forget.scope_id.as_uuid())
        .bind(forget.actor_id.as_uuid())
        .fetch_one(&mut *tx)
        .await
        .map_err(backend)?;

        if let ForgetTarget::Episode(episode_id) = forget.target {
            sqlx::query(
                "update memphant.episode set deletion_generation = $3
                 where tenant_id = $1 and id = $2",
            )
            .bind(tenant.as_uuid())
            .bind(episode_id.as_uuid())
            .bind(generation)
            .execute(&mut *tx)
            .await
            .map_err(backend)?;
        }

        let rows = sqlx::query(AssertSqlSafe(
            format!(
                "update memphant.memory_unit
             set state = 'deleted', deletion_generation = $3, transaction_to = now()
             where tenant_id = $1 and scope_id = $2 and state <> 'deleted' and {unit_filter}
             returning id"
            )
            .as_str(),
        ))
        .bind(tenant.as_uuid())
        .bind(forget.scope_id.as_uuid())
        .bind(generation)
        .bind(source_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(backend)?;
        let mut invalidated: Vec<UnitId> = rows
            .iter()
            .map(|row| {
                Ok::<_, StoreError>(UnitId::from_u128(
                    row.try_get::<Uuid, _>("id").map_err(backend)?.as_u128(),
                ))
            })
            .collect::<Result<_, _>>()?;

        let invalidated_uuids: Vec<Uuid> = invalidated.iter().map(|id| id.as_uuid()).collect();
        invalidated.extend(
            Self::delete_composed_dependents(&mut tx, tenant, &invalidated_uuids, generation)
                .await?,
        );

        // Forgotten embeddings are hard-deleted with their units.
        let all_uuids: Vec<Uuid> = invalidated.iter().map(|id| id.as_uuid()).collect();
        sqlx::query(
            "delete from memphant.embedding where tenant_id = $1 and memory_unit_id = any($2)",
        )
        .bind(tenant.as_uuid())
        .bind(all_uuids)
        .execute(&mut *tx)
        .await
        .map_err(backend)?;

        tx.commit().await.map_err(backend)?;
        Ok(ForgetOutcome {
            deletion_generation: generation as u64,
            invalidated_units: invalidated,
        })
    }

    async fn record_review_events(
        &self,
        tenant: TenantId,
        events: Vec<ReviewEventRow>,
    ) -> Result<(), StoreError> {
        let mut tx = self.pool.begin().await.map_err(backend)?;
        Self::ensure_tenant(&mut tx, tenant).await?;
        for event in events {
            let inserted: Option<Uuid> = sqlx::query_scalar(
                "insert into memphant.review_event (tenant_id, trace_id, caller_id, outcome)
                 values ($1, $2, $3, $4)
                 on conflict (trace_id, caller_id) do nothing
                 returning id",
            )
            .bind(tenant.as_uuid())
            .bind(event.trace_id.as_uuid())
            .bind(&event.caller_id)
            .bind(enum_str(&event.outcome))
            .fetch_optional(&mut *tx)
            .await
            .map_err(backend)?;
            if let Some(event_id) = inserted {
                for unit_id in &event.used_ids {
                    sqlx::query(
                        "insert into memphant.review_event_unit
                           (review_event_id, tenant_id, memory_unit_id)
                         values ($1, $2, $3) on conflict do nothing",
                    )
                    .bind(event_id)
                    .bind(tenant.as_uuid())
                    .bind(unit_id.as_uuid())
                    .execute(&mut *tx)
                    .await
                    .map_err(backend)?;
                }
            }
        }
        tx.commit().await.map_err(backend)
    }

    async fn store_trace(&self, tenant: TenantId, trace: RetrievalTrace) -> Result<(), StoreError> {
        let mut tx = self.pool.begin().await.map_err(backend)?;
        Self::ensure_tenant(&mut tx, tenant).await?;
        Self::ensure_scope(&mut tx, tenant, trace.scope_id).await?;
        let document =
            serde_json::to_value(&trace).map_err(|error| StoreError::Backend(error.to_string()))?;
        sqlx::query(
            "insert into memphant.retrieval_trace
               (id, tenant_id, scope_id, query_hash, mode, channels, candidates, dropped,
                citations, filter_selectivity, consolidation_lag_ms, config_hash, trace)
             values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
             on conflict (tenant_id, id) do nothing",
        )
        .bind(trace.id.as_uuid())
        .bind(tenant.as_uuid())
        .bind(trace.scope_id.as_uuid())
        .bind(&trace.query_hash)
        .bind(enum_str(&trace.mode_executed))
        .bind(serde_json::to_value(&trace.channel_runs).unwrap_or_default())
        .bind(serde_json::to_value(&trace.candidates).unwrap_or_default())
        .bind(serde_json::to_value(&trace.dropped_items).unwrap_or_default())
        .bind(serde_json::to_value(&trace.citations).unwrap_or_default())
        .bind(trace.filter_selectivity)
        .bind(trace.consolidation_lag_ms as i64)
        .bind(&trace.engine_version)
        .bind(document)
        .execute(&mut *tx)
        .await
        .map_err(backend)?;
        tx.commit().await.map_err(backend)
    }

    async fn trace_by_id(
        &self,
        tenant: TenantId,
        id: TraceId,
    ) -> Result<Option<RetrievalTrace>, StoreError> {
        let document: Option<serde_json::Value> = sqlx::query_scalar(
            "select trace from memphant.retrieval_trace where tenant_id = $1 and id = $2",
        )
        .bind(tenant.as_uuid())
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(backend)?;
        document
            .map(serde_json::from_value)
            .transpose()
            .map_err(|error| StoreError::Backend(error.to_string()))
    }

    async fn scope_memory_page(
        &self,
        tenant: TenantId,
        scope: ScopeId,
        cursor: Option<UnitId>,
        limit: usize,
    ) -> Result<ScopePage, StoreError> {
        let limit = limit.clamp(1, 1_000);
        let fetched = self
            .fetch_units_where(
                "tenant_id = $1 and scope_id = $2 and ($3::uuid is null or id > $3)",
                "order by id limit $4",
                vec![
                    Bind::Uuid(tenant.as_uuid()),
                    Bind::Uuid(scope.as_uuid()),
                    Bind::UuidOpt(cursor.map(|cursor| cursor.as_uuid())),
                    Bind::I64((limit + 1) as i64),
                ],
            )
            .await?;
        let has_more = fetched.len() > limit;
        let mut items = fetched;
        items.truncate(limit);
        let next_cursor = has_more.then(|| items.last().map(|unit| unit.id)).flatten();
        Ok(ScopePage {
            items,
            next_cursor,
            has_more,
        })
    }

    async fn claim_reflect_jobs(
        &self,
        filter: JobFilter,
        limit: usize,
    ) -> Result<Vec<ReflectJobRow>, StoreError> {
        // Dead-letter sweep first: exhausted jobs are never re-claimed.
        sqlx::query(
            "update memphant.job_state set state = 'dead'
             where state not in ('done', 'dead') and attempts >= 5",
        )
        .execute(&self.pool)
        .await
        .map_err(backend)?;

        let rows = sqlx::query(
            "update memphant.job_state job
             set state = 'running', claimed_at = now(), attempts = job.attempts + 1
             where (job.tenant_id, job.id) in (
               select tenant_id, id from memphant.job_state
               where state in ('queued', 'running') and attempts < 5
                 and run_after <= now()
                 and (claimed_at is null or claimed_at < now() - interval '5 minutes')
                 and ($1::uuid is null or tenant_id = $1)
                 and ($2::uuid is null or scope_id = $2)
                 and scope_id is not null
               order by created_at
               for update skip locked
               limit $3)
             returning job.id, job.tenant_id, job.scope_id, job.job_type, job.target_id,
                       job.compiler_version, job.subject, job.predicate, job.attempts",
        )
        .bind(filter.tenant.map(|tenant| tenant.as_uuid()))
        .bind(filter.scope.map(|scope| scope.as_uuid()))
        .bind(limit.min(1_000) as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(backend)?;

        rows.iter()
            .map(|row| {
                let job_type: String = row.try_get("job_type").map_err(backend)?;
                let target: Uuid = row.try_get("target_id").map_err(backend)?;
                let (kind, episode_id, resource_id) = match job_type.as_str() {
                    "reflect_resource" => (
                        ReflectJobKind::ReflectResource,
                        None,
                        Some(ResourceId::from_u128(target.as_u128())),
                    ),
                    _ => (
                        ReflectJobKind::ReflectEpisode,
                        Some(EpisodeId::from_u128(target.as_u128())),
                        None,
                    ),
                };
                Ok(ReflectJobRow {
                    job: QueuedReflectJob {
                        id: JobId::from_u128(
                            row.try_get::<Uuid, _>("id").map_err(backend)?.as_u128(),
                        ),
                        tenant_id: TenantId::from_u128(
                            row.try_get::<Uuid, _>("tenant_id")
                                .map_err(backend)?
                                .as_u128(),
                        ),
                        scope_id: ScopeId::from_u128(
                            row.try_get::<Uuid, _>("scope_id")
                                .map_err(backend)?
                                .as_u128(),
                        ),
                        episode_id,
                        resource_id,
                        kind,
                        compiler_version: row.try_get("compiler_version").map_err(backend)?,
                        subject: row.try_get("subject").map_err(backend)?,
                        predicate: row.try_get("predicate").map_err(backend)?,
                    },
                    attempts: row.try_get::<i32, _>("attempts").map_err(backend)? as u32,
                })
            })
            .collect()
    }

    async fn complete_reflect_job(&self, id: JobId) -> Result<(), StoreError> {
        sqlx::query("update memphant.job_state set state = 'done' where id = $1")
            .bind(id.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(backend)?;
        Ok(())
    }

    async fn persist_compiled_units(
        &self,
        tenant: TenantId,
        write: CompiledWrite,
    ) -> Result<(), StoreError> {
        let mut tx = self.pool.begin().await.map_err(backend)?;
        Self::ensure_tenant(&mut tx, tenant).await?;
        Self::ensure_scope(&mut tx, tenant, write.scope_id).await?;

        // Idempotency: one compilation per (job_id, compiler_version).
        let already: Option<serde_json::Value> = sqlx::query_scalar(
            "select result from memphant.job_state
             where tenant_id = $1 and id = $2 and compiler_version = $3 and result is not null",
        )
        .bind(tenant.as_uuid())
        .bind(write.job_id.as_uuid())
        .bind(&write.compiler_version)
        .fetch_optional(&mut *tx)
        .await
        .map_err(backend)?;
        if already.is_some() {
            tx.commit().await.map_err(backend)?;
            return Ok(());
        }

        // Apply state transitions BEFORE inserts so the partial unique
        // scope-subject index never sees two open semantic generations.
        for update in &write.unit_updates {
            sqlx::query(
                "update memphant.memory_unit set state = $3, transaction_to = $4::timestamptz
                 where tenant_id = $1 and id = $2",
            )
            .bind(tenant.as_uuid())
            .bind(update.id.as_uuid())
            .bind(enum_str(&update.state))
            .bind(&update.transaction_to)
            .execute(&mut *tx)
            .await
            .map_err(backend)?;
        }

        // Forgotten-source tombstones durably block re-derivation.
        let tombstones: Vec<(String, Uuid)> = sqlx::query(
            "select source_kind, source_id from memphant.forgotten_source where tenant_id = $1",
        )
        .bind(tenant.as_uuid())
        .fetch_all(&mut *tx)
        .await
        .map_err(backend)?
        .iter()
        .map(|row| {
            Ok::<_, StoreError>((
                row.try_get::<String, _>("source_kind").map_err(backend)?,
                row.try_get::<Uuid, _>("source_id").map_err(backend)?,
            ))
        })
        .collect::<Result<_, _>>()?;
        let forbidden: HashSet<(&str, Uuid)> = tombstones
            .iter()
            .map(|(kind, id)| (kind.as_str(), *id))
            .collect();
        let is_forgotten = |unit: &StoredMemoryUnit| {
            unit.source_episode_id
                .is_some_and(|id| forbidden.contains(&("episode", id.as_uuid())))
                || unit
                    .source_resource_id
                    .is_some_and(|id| forbidden.contains(&("resource", id.as_uuid())))
                || forbidden.contains(&("memory_unit", unit.id.as_uuid()))
        };

        let mut admitted_ids: HashSet<UnitId> = HashSet::new();
        for unit in &write.new_units {
            if is_forgotten(unit) {
                continue;
            }
            let mut actors: Vec<ActorId> = unit.actor_id.into_iter().collect();
            actors.dedup();
            for actor in actors {
                Self::ensure_actor(&mut tx, tenant, actor).await?;
            }
            Self::insert_unit(&mut tx, unit).await?;
            admitted_ids.insert(unit.id);
        }

        // Edges may only reference admitted or pre-existing units.
        let mut existing: HashMap<Uuid, bool> = HashMap::new();
        for edge in &write.new_edges {
            let mut endpoints_ok = true;
            for endpoint in [edge.src_id, edge.dst_id] {
                if admitted_ids.contains(&endpoint) {
                    continue;
                }
                let known = match existing.get(&endpoint.as_uuid()) {
                    Some(known) => *known,
                    None => {
                        let found: Option<Uuid> = sqlx::query_scalar(
                            "select id from memphant.memory_unit where tenant_id = $1 and id = $2",
                        )
                        .bind(tenant.as_uuid())
                        .bind(endpoint.as_uuid())
                        .fetch_optional(&mut *tx)
                        .await
                        .map_err(backend)?;
                        let known = found.is_some();
                        existing.insert(endpoint.as_uuid(), known);
                        known
                    }
                };
                if !known {
                    endpoints_ok = false;
                    break;
                }
            }
            if endpoints_ok {
                Self::insert_edge(&mut tx, edge).await?;
            }
        }

        // Store the compiled trace as the idempotency record on the job row
        // (insert a synthetic row for direct writes with no queued job).
        let trace_json = serde_json::to_value(&write.trace)
            .map_err(|error| StoreError::Backend(error.to_string()))?;
        let updated = sqlx::query(
            "update memphant.job_state set result = $4
             where tenant_id = $1 and id = $2 and compiler_version = $3",
        )
        .bind(tenant.as_uuid())
        .bind(write.job_id.as_uuid())
        .bind(&write.compiler_version)
        .bind(&trace_json)
        .execute(&mut *tx)
        .await
        .map_err(backend)?;
        if updated.rows_affected() == 0 {
            sqlx::query(
                "insert into memphant.job_state
                   (id, tenant_id, job_type, target_id, compiler_version, state, scope_id, result)
                 values ($1, $2, 'direct', $1, $3, 'done', $4, $5)
                 on conflict (tenant_id, job_type, target_id, compiler_version) do update
                   set result = excluded.result",
            )
            .bind(write.job_id.as_uuid())
            .bind(tenant.as_uuid())
            .bind(&write.compiler_version)
            .bind(write.scope_id.as_uuid())
            .bind(&trace_json)
            .execute(&mut *tx)
            .await
            .map_err(backend)?;
        }

        tx.commit().await.map_err(backend)
    }

    async fn fetch_reflect_trace(
        &self,
        tenant: TenantId,
        job_id: JobId,
        compiler_version: &str,
    ) -> Result<Option<ReflectTrace>, StoreError> {
        let document: Option<serde_json::Value> = sqlx::query_scalar(
            "select result from memphant.job_state
             where tenant_id = $1 and id = $2 and compiler_version = $3 and result is not null",
        )
        .bind(tenant.as_uuid())
        .bind(job_id.as_uuid())
        .bind(compiler_version)
        .fetch_optional(&self.pool)
        .await
        .map_err(backend)?;
        document
            .map(serde_json::from_value)
            .transpose()
            .map_err(|error| StoreError::Backend(error.to_string()))
    }

    async fn upsert_embeddings(
        &self,
        tenant: TenantId,
        rows: Vec<EmbeddingRow>,
    ) -> Result<(), StoreError> {
        if rows.is_empty() {
            return Ok(());
        }
        let mut tx = self.pool.begin().await.map_err(backend)?;
        for row in rows {
            if row.vec.is_empty() {
                continue;
            }
            sqlx::query(
                "insert into memphant.embedding (tenant_id, memory_unit_id, embedding_profile_id, vec)
                 values ($1, $2, $3, $4::halfvec)
                 on conflict (tenant_id, memory_unit_id, embedding_profile_id)
                   do update set vec = excluded.vec",
            )
            .bind(tenant.as_uuid())
            .bind(row.memory_unit_id.as_uuid())
            .bind(row.embedding_profile_id)
            .bind(vec_literal(&row.vec))
            .execute(&mut *tx)
            .await
            .map_err(backend)?;
        }
        tx.commit().await.map_err(backend)
    }

    async fn lookup_api_key(&self, key_hash: &str) -> Result<Option<ApiKeyRow>, StoreError> {
        let row = sqlx::query(
            "select id, tenant_id, key_hash, label, max_trust, (revoked_at is not null) as revoked
             from memphant.api_key where key_hash = $1",
        )
        .bind(key_hash)
        .fetch_optional(&self.pool)
        .await
        .map_err(backend)?;
        let Some(row) = row else { return Ok(None) };
        Ok(Some(ApiKeyRow {
            id: row.try_get("id").map_err(backend)?,
            tenant_id: TenantId::from_u128(
                row.try_get::<Uuid, _>("tenant_id")
                    .map_err(backend)?
                    .as_u128(),
            ),
            key_hash: row.try_get("key_hash").map_err(backend)?,
            label: row.try_get("label").map_err(backend)?,
            max_trust: enum_from_str(
                row.try_get::<String, _>("max_trust")
                    .map_err(backend)?
                    .as_str(),
            )?,
            revoked: row.try_get("revoked").map_err(backend)?,
        }))
    }

    async fn ping(&self) -> Result<(), StoreError> {
        sqlx::query("select 1")
            .execute(&self.pool)
            .await
            .map_err(backend)?;
        Ok(())
    }

    async fn dead_letter_count(&self) -> Result<u64, StoreError> {
        let count: i64 =
            sqlx::query_scalar("select count(*) from memphant.job_state where state = 'dead'")
                .fetch_one(&self.pool)
                .await
                .map_err(backend)?;
        Ok(count as u64)
    }
}

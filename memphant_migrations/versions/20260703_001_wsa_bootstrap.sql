create schema if not exists memphant;

create extension if not exists vector;
create extension if not exists pg_trgm;
create extension if not exists ltree;
create extension if not exists btree_gist;

do $$
begin
  if not exists (select 1 from pg_roles where rolname = 'memphant_app') then
    create role memphant_app nologin;
  end if;
  if not exists (select 1 from pg_roles where rolname = 'memphant_cron') then
    create role memphant_cron nologin;
  end if;
  if not exists (select 1 from pg_roles where rolname = 'memphant_readonly') then
    create role memphant_readonly nologin;
  end if;
end
$$;

alter role memphant_app set statement_timeout = '30s';
alter role memphant_app set lock_timeout = '5s';
alter role memphant_app set idle_in_transaction_session_timeout = '30s';
alter role memphant_cron set statement_timeout = '5min';
alter role memphant_cron set lock_timeout = '5s';
alter role memphant_cron set idle_in_transaction_session_timeout = '30s';
alter role memphant_readonly set statement_timeout = '30s';
alter role memphant_readonly set lock_timeout = '5s';
alter role memphant_readonly set idle_in_transaction_session_timeout = '30s';

revoke all on schema memphant from public;
do $$
begin
  if exists (select 1 from pg_roles where rolname = 'anon') then
    execute 'revoke all on schema memphant from anon';
  end if;
  if exists (select 1 from pg_roles where rolname = 'authenticated') then
    execute 'revoke all on schema memphant from authenticated';
  end if;
  if exists (select 1 from pg_roles where rolname = 'authenticator') then
    execute 'revoke all on schema memphant from authenticator';
  end if;
end
$$;
grant usage on schema memphant to memphant_app, memphant_cron, memphant_readonly;

create or replace function memphant.current_tenant_id()
returns uuid
language sql
stable
set search_path = memphant, pg_catalog
as $$
  select nullif(current_setting('memphant.tenant_id', true), '')::uuid
$$;

create or replace function memphant.set_updated_at()
returns trigger
language plpgsql
set search_path = memphant, pg_catalog
as $$
begin
  new.updated_at = now();
  return new;
end
$$;

create table if not exists memphant.schema_migrations (
  version text primary key,
  schema_compat_revision text not null,
  migration_kind text not null default 'additive'
    check (migration_kind in ('additive', 'breaking', 'rewrite')),
  applied_at timestamptz not null default now()
);

create table if not exists memphant.tenant (
  id uuid primary key,
  slug text not null unique,
  plan text not null,
  region text not null,
  schema_compat_revision text not null default '20260703_001_wsa_bootstrap',
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now()
);

create table if not exists memphant.subject (
  id uuid not null,
  tenant_id uuid not null,
  external_ref text not null,
  kind text not null,
  privacy_policy jsonb not null default '{}'::jsonb,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  primary key (tenant_id, id),
  unique (tenant_id, external_ref)
);

create table if not exists memphant.actor (
  id uuid not null,
  tenant_id uuid not null,
  kind text not null check (kind in ('user','agent','tool','web','system')),
  external_ref text not null,
  trust_level text not null,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  primary key (tenant_id, id),
  unique (tenant_id, kind, external_ref)
);

create table if not exists memphant.scope (
  id uuid not null,
  tenant_id uuid not null,
  parent_scope_id uuid,
  kind text not null,
  external_ref text,
  materialized_path ltree not null,
  scope_depth smallint not null check (scope_depth <= 32),
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  primary key (tenant_id, id),
  foreign key (tenant_id, parent_scope_id) references memphant.scope (tenant_id, id)
);

create table if not exists memphant.scope_policy (
  id uuid not null,
  tenant_id uuid not null,
  scope_id uuid not null,
  kind text not null check (kind in ('episodic','semantic','procedural','belief','resource')),
  direction text not null check (direction in ('inherit','grant')),
  min_level smallint not null check (min_level between 0 and 64),
  grantee_scope_id uuid,
  admit boolean not null default true,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  primary key (tenant_id, id),
  foreign key (tenant_id, scope_id) references memphant.scope (tenant_id, id),
  foreign key (tenant_id, grantee_scope_id) references memphant.scope (tenant_id, id),
  check ((direction = 'grant') = (grantee_scope_id is not null)),
  check (grantee_scope_id is distinct from scope_id)
);

create table if not exists memphant.agent_node (
  id uuid not null,
  tenant_id uuid not null,
  scope_id uuid not null,
  parent_agent_node_id uuid,
  level smallint not null check (level between 0 and 64),
  external_ref text not null,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  primary key (tenant_id, id),
  foreign key (tenant_id, scope_id) references memphant.scope (tenant_id, id),
  foreign key (tenant_id, parent_agent_node_id) references memphant.agent_node (tenant_id, id)
);

create table if not exists memphant.episode (
  id uuid not null,
  tenant_id uuid not null,
  scope_id uuid not null,
  actor_id uuid not null,
  agent_node_id uuid,
  source_kind text not null check (source_kind in ('user','agent','tool','web','resource','system')),
  source_trust text not null,
  dedup_key text not null,
  observation_count integer not null default 1 check (observation_count >= 1),
  retention_tier text not null default 'hot' check (retention_tier in ('hot','warm','cold')),
  blob_hash text,
  body text,
  first_observed_at timestamptz not null,
  last_observed_at timestamptz not null,
  transaction_from timestamptz not null default now(),
  deletion_generation bigint,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  primary key (tenant_id, id),
  unique (tenant_id, scope_id, dedup_key),
  foreign key (tenant_id, scope_id) references memphant.scope (tenant_id, id),
  foreign key (tenant_id, actor_id) references memphant.actor (tenant_id, id),
  foreign key (tenant_id, agent_node_id) references memphant.agent_node (tenant_id, id)
);

create table if not exists memphant.resource (
  id uuid not null,
  tenant_id uuid not null,
  scope_id uuid not null,
  kind text not null,
  uri text not null,
  content_hash text not null,
  acl jsonb not null default '{}'::jsonb,
  extractor_state text not null default 'registered'
    check (extractor_state in ('registered','fetching','extracting','chunked','embedded','failed','stale')),
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  primary key (tenant_id, id),
  foreign key (tenant_id, scope_id) references memphant.scope (tenant_id, id)
);

create table if not exists memphant.memory_unit (
  id uuid not null,
  tenant_id uuid not null,
  scope_id uuid not null,
  kind text not null check (kind in ('episodic','semantic','procedural','belief','resource')),
  state text not null check (state in (
    'captured','extracted','candidate','active','superseded',
    'invalidated','deleted','quarantined','expired','validated','retired'
  )),
  subject_key text,
  body text not null,
  payload jsonb not null default '{}'::jsonb,
  confidence real check (confidence between 0 and 1),
  trust_level text not null,
  valid_from timestamptz,
  valid_to timestamptz,
  observed_at timestamptz,
  transaction_from timestamptz not null default now(),
  transaction_to timestamptz,
  difficulty real check (difficulty between 0 and 10),
  stability_days real,
  last_reinforced_at timestamptz,
  reinforcement_count integer not null default 0 check (reinforcement_count >= 0),
  desired_retention real not null default 0.9 check (desired_retention between 0 and 1),
  last_confirmed_at timestamptz,
  freshness_due_at timestamptz,
  deletion_generation bigint,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  primary key (tenant_id, id),
  foreign key (tenant_id, scope_id) references memphant.scope (tenant_id, id)
);

create table if not exists memphant.memory_edge (
  id uuid not null,
  tenant_id uuid not null,
  scope_id uuid not null,
  src_id uuid not null,
  dst_id uuid not null,
  kind text not null check (kind in (
    'supersedes','contradicts','derived_from','cites','same_subject','depends_on'
  )),
  observed boolean not null default false,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  primary key (tenant_id, id),
  unique (tenant_id, src_id, dst_id, kind),
  foreign key (tenant_id, scope_id) references memphant.scope (tenant_id, id),
  foreign key (tenant_id, src_id) references memphant.memory_unit (tenant_id, id),
  foreign key (tenant_id, dst_id) references memphant.memory_unit (tenant_id, id)
);

create table if not exists memphant.embedding_profile (
  id uuid not null,
  tenant_id uuid not null,
  provider text not null,
  model text not null,
  dimensions integer not null check (dimensions > 0 and dimensions <= 16000),
  distance text not null check (distance in ('cosine','l2','inner_product')),
  version text not null,
  index_strategy text not null check (index_strategy in ('hnsw_full','hnsw_subvector','hnsw_binary','exact')),
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  primary key (tenant_id, id),
  unique (tenant_id, provider, model, dimensions, version, index_strategy)
);

create table if not exists memphant.embedding (
  tenant_id uuid not null,
  memory_unit_id uuid not null,
  embedding_profile_id uuid not null,
  vec halfvec,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  primary key (tenant_id, memory_unit_id, embedding_profile_id),
  foreign key (tenant_id, memory_unit_id) references memphant.memory_unit (tenant_id, id),
  foreign key (tenant_id, embedding_profile_id) references memphant.embedding_profile (tenant_id, id)
);

create table if not exists memphant.citation (
  id uuid not null,
  tenant_id uuid not null,
  memory_unit_id uuid not null,
  episode_id uuid,
  resource_id uuid,
  span jsonb,
  quote_hash text,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  primary key (tenant_id, id),
  foreign key (tenant_id, memory_unit_id) references memphant.memory_unit (tenant_id, id),
  foreign key (tenant_id, episode_id) references memphant.episode (tenant_id, id),
  foreign key (tenant_id, resource_id) references memphant.resource (tenant_id, id)
);

create table if not exists memphant.trust_event (
  id uuid not null,
  tenant_id uuid not null,
  target_kind text not null check (target_kind in ('episode','memory_unit','resource','actor','source')),
  target_id uuid not null,
  level text not null,
  decision text not null,
  reason_code text not null,
  corroborating_sources jsonb,
  policy_version text not null,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  primary key (tenant_id, id)
);

create table if not exists memphant.retrieval_trace (
  id uuid not null,
  tenant_id uuid not null,
  scope_id uuid not null,
  query_hash text not null,
  mode text not null,
  channels jsonb not null default '[]'::jsonb,
  candidates jsonb not null default '[]'::jsonb,
  dropped jsonb not null default '[]'::jsonb,
  citations jsonb not null default '[]'::jsonb,
  filter_selectivity real,
  consolidation_lag_ms bigint,
  config_hash text not null,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  primary key (tenant_id, id),
  foreign key (tenant_id, scope_id) references memphant.scope (tenant_id, id)
);

create table if not exists memphant.deletion_generation (
  id bigint generated always as identity,
  tenant_id uuid not null,
  scope_id uuid,
  requested_by uuid not null,
  state text not null check (state in ('requested','tombstoned','compacting','completed','failed')),
  completed_at timestamptz,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  primary key (tenant_id, id),
  foreign key (tenant_id, scope_id) references memphant.scope (tenant_id, id),
  foreign key (tenant_id, requested_by) references memphant.actor (tenant_id, id)
);

create table if not exists memphant.job_state (
  id uuid not null,
  tenant_id uuid not null,
  job_type text not null,
  target_id uuid not null,
  compiler_version text not null,
  state text not null check (state in ('queued','running','done','failed')),
  attempts integer not null default 0 check (attempts >= 0),
  run_after timestamptz not null default now(),
  last_error text,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  primary key (tenant_id, id),
  unique (tenant_id, job_type, target_id, compiler_version)
);

create table if not exists memphant.blob_ledger (
  tenant_id uuid not null,
  content_hash text not null,
  state text not null check (state in ('present','collected')),
  byte_len bigint not null check (byte_len >= 0),
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  primary key (tenant_id, content_hash)
);

create table if not exists memphant.belief_observation (
  id uuid not null,
  tenant_id uuid not null,
  memory_unit_id uuid not null,
  source_event_id text not null,
  evidence text not null,
  direction text not null check (direction in ('confirm','disconfirm')),
  observed_at timestamptz not null,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  primary key (tenant_id, id),
  unique (tenant_id, memory_unit_id, source_event_id),
  foreign key (tenant_id, memory_unit_id) references memphant.memory_unit (tenant_id, id)
);

create table if not exists memphant.review_event (
  id uuid not null,
  tenant_id uuid not null,
  memory_unit_id uuid not null,
  source_event_id text not null,
  outcome text not null,
  observed_at timestamptz not null,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  primary key (tenant_id, id),
  unique (tenant_id, memory_unit_id, source_event_id),
  foreign key (tenant_id, memory_unit_id) references memphant.memory_unit (tenant_id, id)
);

create table if not exists memphant.scope_block (
  id uuid not null,
  tenant_id uuid not null,
  scope_id uuid not null,
  content text not null,
  token_limit integer not null default 300 check (token_limit > 0),
  version integer not null check (version > 0),
  updated_by_actor_id uuid not null,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  primary key (tenant_id, id),
  unique (tenant_id, scope_id, version),
  foreign key (tenant_id, scope_id) references memphant.scope (tenant_id, id),
  foreign key (tenant_id, updated_by_actor_id) references memphant.actor (tenant_id, id)
);

create index if not exists memphant_subject_tenant_external_idx on memphant.subject (tenant_id, external_ref);
create index if not exists memphant_actor_tenant_external_idx on memphant.actor (tenant_id, kind, external_ref);
create index if not exists memphant_scope_tenant_parent_idx on memphant.scope (tenant_id, parent_scope_id);
create index if not exists memphant_scope_tenant_path_idx on memphant.scope using gist (tenant_id, materialized_path gist_ltree_ops(siglen=100));
create index if not exists memphant_scope_policy_tenant_inherit_idx on memphant.scope_policy (tenant_id, scope_id, kind) where direction = 'inherit';
create index if not exists memphant_scope_policy_tenant_grant_idx on memphant.scope_policy (tenant_id, grantee_scope_id, kind) where direction = 'grant';
create index if not exists memphant_agent_node_tenant_scope_idx on memphant.agent_node (tenant_id, scope_id);
create index if not exists memphant_agent_node_tenant_parent_idx on memphant.agent_node (tenant_id, parent_agent_node_id);
create index if not exists memphant_episode_tenant_scope_source_idx on memphant.episode (tenant_id, scope_id, source_kind, last_observed_at);
create index if not exists memphant_episode_tenant_actor_idx on memphant.episode (tenant_id, actor_id);
create index if not exists memphant_episode_tenant_agent_node_idx on memphant.episode (tenant_id, agent_node_id);
create index if not exists memphant_episode_tenant_retention_idx on memphant.episode (tenant_id, retention_tier) where retention_tier <> 'hot';
create index if not exists memphant_resource_tenant_scope_idx on memphant.resource (tenant_id, scope_id);
create index if not exists memphant_memory_unit_tenant_live_idx on memphant.memory_unit (tenant_id, scope_id, kind, valid_to) where state = 'active' and transaction_to is null;
create index if not exists memphant_memory_unit_tenant_subject_idx on memphant.memory_unit (tenant_id, scope_id, subject_key) where state = 'active' and transaction_to is null;
create unique index if not exists memphant_memory_unit_tenant_open_subject_idx on memphant.memory_unit (tenant_id, subject_key) where transaction_to is null and kind in ('semantic','belief');
create index if not exists memphant_memory_edge_tenant_dst_idx on memphant.memory_edge (tenant_id, scope_id, dst_id, kind);
create index if not exists memphant_memory_edge_tenant_dst_fk_idx on memphant.memory_edge (tenant_id, dst_id);
create index if not exists memphant_embedding_profile_tenant_model_idx on memphant.embedding_profile (tenant_id, provider, model, version);
create index if not exists memphant_embedding_tenant_profile_idx on memphant.embedding (tenant_id, embedding_profile_id);
create index if not exists memphant_citation_tenant_unit_idx on memphant.citation (tenant_id, memory_unit_id);
create index if not exists memphant_citation_tenant_episode_idx on memphant.citation (tenant_id, episode_id);
create index if not exists memphant_citation_tenant_resource_idx on memphant.citation (tenant_id, resource_id);
create index if not exists memphant_trust_event_tenant_target_idx on memphant.trust_event (tenant_id, target_kind, target_id, created_at);
create index if not exists memphant_retrieval_trace_tenant_scope_idx on memphant.retrieval_trace (tenant_id, scope_id, created_at);
create index if not exists memphant_deletion_generation_tenant_scope_idx on memphant.deletion_generation (tenant_id, scope_id, state);
create index if not exists memphant_deletion_generation_tenant_actor_idx on memphant.deletion_generation (tenant_id, requested_by);
create index if not exists memphant_job_state_tenant_run_idx on memphant.job_state (tenant_id, state, run_after);
create index if not exists memphant_blob_ledger_tenant_state_idx on memphant.blob_ledger (tenant_id, state, created_at);
create index if not exists memphant_belief_observation_tenant_unit_idx on memphant.belief_observation (tenant_id, memory_unit_id, observed_at);
create index if not exists memphant_review_event_tenant_unit_idx on memphant.review_event (tenant_id, memory_unit_id, observed_at);
create index if not exists memphant_scope_block_tenant_scope_idx on memphant.scope_block (tenant_id, scope_id, version);
create index if not exists memphant_scope_block_tenant_actor_idx on memphant.scope_block (tenant_id, updated_by_actor_id);

alter table memphant.tenant enable row level security;
alter table memphant.subject enable row level security;
alter table memphant.actor enable row level security;
alter table memphant.scope enable row level security;
alter table memphant.scope_policy enable row level security;
alter table memphant.agent_node enable row level security;
alter table memphant.episode enable row level security;
alter table memphant.resource enable row level security;
alter table memphant.memory_unit enable row level security;
alter table memphant.memory_edge enable row level security;
alter table memphant.embedding_profile enable row level security;
alter table memphant.embedding enable row level security;
alter table memphant.citation enable row level security;
alter table memphant.trust_event enable row level security;
alter table memphant.retrieval_trace enable row level security;
alter table memphant.deletion_generation enable row level security;
alter table memphant.job_state enable row level security;
alter table memphant.blob_ledger enable row level security;
alter table memphant.belief_observation enable row level security;
alter table memphant.review_event enable row level security;
alter table memphant.scope_block enable row level security;

do $$
declare
  rel record;
begin
  for rel in
    select tablename from pg_tables where schemaname = 'memphant'
  loop
    execute format(
      'drop policy if exists memphant_%s_tenant_isolation on memphant.%I',
      rel.tablename,
      rel.tablename
    );
  end loop;
  execute 'drop policy if exists memphant_tenant_isolation on memphant.tenant';
end
$$;

create policy memphant_tenant_isolation on memphant.tenant
  for all to memphant_app, memphant_cron
  using (id = memphant.current_tenant_id())
  with check (id = memphant.current_tenant_id());

create policy memphant_subject_tenant_isolation on memphant.subject for all to memphant_app, memphant_cron using (tenant_id = memphant.current_tenant_id()) with check (tenant_id = memphant.current_tenant_id());
create policy memphant_actor_tenant_isolation on memphant.actor for all to memphant_app, memphant_cron using (tenant_id = memphant.current_tenant_id()) with check (tenant_id = memphant.current_tenant_id());
create policy memphant_scope_tenant_isolation on memphant.scope for all to memphant_app, memphant_cron using (tenant_id = memphant.current_tenant_id()) with check (tenant_id = memphant.current_tenant_id());
create policy memphant_scope_policy_tenant_isolation on memphant.scope_policy for all to memphant_app, memphant_cron using (tenant_id = memphant.current_tenant_id()) with check (tenant_id = memphant.current_tenant_id());
create policy memphant_agent_node_tenant_isolation on memphant.agent_node for all to memphant_app, memphant_cron using (tenant_id = memphant.current_tenant_id()) with check (tenant_id = memphant.current_tenant_id());
create policy memphant_episode_tenant_isolation on memphant.episode for all to memphant_app, memphant_cron using (tenant_id = memphant.current_tenant_id()) with check (tenant_id = memphant.current_tenant_id());
create policy memphant_resource_tenant_isolation on memphant.resource for all to memphant_app, memphant_cron using (tenant_id = memphant.current_tenant_id()) with check (tenant_id = memphant.current_tenant_id());
create policy memphant_memory_unit_tenant_isolation on memphant.memory_unit for all to memphant_app, memphant_cron using (tenant_id = memphant.current_tenant_id()) with check (tenant_id = memphant.current_tenant_id());
create policy memphant_memory_edge_tenant_isolation on memphant.memory_edge for all to memphant_app, memphant_cron using (tenant_id = memphant.current_tenant_id()) with check (tenant_id = memphant.current_tenant_id());
create policy memphant_embedding_profile_tenant_isolation on memphant.embedding_profile for all to memphant_app, memphant_cron using (tenant_id = memphant.current_tenant_id()) with check (tenant_id = memphant.current_tenant_id());
create policy memphant_embedding_tenant_isolation on memphant.embedding for all to memphant_app, memphant_cron using (tenant_id = memphant.current_tenant_id()) with check (tenant_id = memphant.current_tenant_id());
create policy memphant_citation_tenant_isolation on memphant.citation for all to memphant_app, memphant_cron using (tenant_id = memphant.current_tenant_id()) with check (tenant_id = memphant.current_tenant_id());
create policy memphant_trust_event_tenant_isolation on memphant.trust_event for all to memphant_app, memphant_cron using (tenant_id = memphant.current_tenant_id()) with check (tenant_id = memphant.current_tenant_id());
create policy memphant_retrieval_trace_tenant_isolation on memphant.retrieval_trace for all to memphant_app, memphant_cron using (tenant_id = memphant.current_tenant_id()) with check (tenant_id = memphant.current_tenant_id());
create policy memphant_deletion_generation_tenant_isolation on memphant.deletion_generation for all to memphant_app, memphant_cron using (tenant_id = memphant.current_tenant_id()) with check (tenant_id = memphant.current_tenant_id());
create policy memphant_job_state_tenant_isolation on memphant.job_state for all to memphant_app, memphant_cron using (tenant_id = memphant.current_tenant_id()) with check (tenant_id = memphant.current_tenant_id());
create policy memphant_blob_ledger_tenant_isolation on memphant.blob_ledger for all to memphant_app, memphant_cron using (tenant_id = memphant.current_tenant_id()) with check (tenant_id = memphant.current_tenant_id());
create policy memphant_belief_observation_tenant_isolation on memphant.belief_observation for all to memphant_app, memphant_cron using (tenant_id = memphant.current_tenant_id()) with check (tenant_id = memphant.current_tenant_id());
create policy memphant_review_event_tenant_isolation on memphant.review_event for all to memphant_app, memphant_cron using (tenant_id = memphant.current_tenant_id()) with check (tenant_id = memphant.current_tenant_id());
create policy memphant_scope_block_tenant_isolation on memphant.scope_block for all to memphant_app, memphant_cron using (tenant_id = memphant.current_tenant_id()) with check (tenant_id = memphant.current_tenant_id());

grant select, insert, update, delete on all tables in schema memphant to memphant_app;
grant select, insert, update, delete on all tables in schema memphant to memphant_cron;
grant select on memphant.tenant, memphant.schema_migrations to memphant_readonly;

do $$
declare
  rel record;
begin
  for rel in
    select c.table_name as tablename
    from information_schema.columns c
    where c.table_schema = 'memphant'
      and c.column_name = 'updated_at'
  loop
    execute format('drop trigger if exists set_updated_at on memphant.%I', rel.tablename);
    execute format(
      'create trigger set_updated_at before update on memphant.%I for each row execute function memphant.set_updated_at()',
      rel.tablename
    );
  end loop;
end
$$;

insert into memphant.schema_migrations (version, schema_compat_revision, migration_kind)
values ('20260703_001_wsa_bootstrap', '20260703_001_wsa_bootstrap', 'additive')
on conflict (version) do update
set schema_compat_revision = excluded.schema_compat_revision,
    migration_kind = excluded.migration_kind;

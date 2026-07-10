-- migration_kind: rewrite
-- 20260709_002_runtime_reconciliation
-- Reconciles the runtime Rust types with the DDL, adds tri-domain resource
-- identity, forget tombstones, api keys, and the rewritten review_event shape.
-- NOTE (verified against 001): episode/resource/memory_unit have COMPOSITE PKs
-- (tenant_id, id). Every FK below is therefore a composite (tenant_id, <col>)
-- pair — bare `references t(id)` fails.

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

alter table memphant.resource
  add column if not exists actor_id uuid,
  add column if not exists mime_type text,
  add column if not exists revision text,
  add column if not exists body text,
  add column if not exists source_trust text not null default 'untrusted';

create table if not exists memphant.forgotten_source (
  tenant_id uuid not null references memphant.tenant(id),
  source_kind text not null check (source_kind in ('episode','resource','memory_unit')),
  source_id uuid not null,
  forgotten_at timestamptz not null default now(),
  primary key (tenant_id, source_kind, source_id)
);
alter table memphant.forgotten_source enable row level security;
create index if not exists memphant_forgotten_source_tenant_kind_idx
  on memphant.forgotten_source (tenant_id, source_kind, forgotten_at);

alter table memphant.memory_unit
  add column if not exists actor_id uuid,
  add column if not exists source_kind text,
  add column if not exists source_episode_id uuid,
  add column if not exists source_resource_id uuid,
  add column if not exists churn_class text;
alter table memphant.memory_unit
  add constraint memory_unit_source_episode_fk
    foreign key (tenant_id, source_episode_id) references memphant.episode(tenant_id, id),
  add constraint memory_unit_source_resource_fk
    foreign key (tenant_id, source_resource_id) references memphant.resource(tenant_id, id);
create index if not exists memphant_memory_unit_tenant_source_episode_idx
  on memphant.memory_unit (tenant_id, source_episode_id);
create index if not exists memphant_memory_unit_tenant_source_resource_idx
  on memphant.memory_unit (tenant_id, source_resource_id);

-- 001 already has freshness_due_at timestamptz; the Rust field becomes
-- freshness_due_at: Option<String> instead of freshness_due: bool.

-- Replace the tenant-wide open-subject uniqueness with scope-bound uniqueness.
-- Belief/candidate kinds are dropped from the unique index — beliefs may hold
-- multiple same-subject generations; supersedence for them is compiler policy.
drop index memphant.memphant_memory_unit_tenant_open_subject_idx;
create unique index memphant_memory_unit_scope_subject_idx
  on memphant.memory_unit (tenant_id, scope_id, subject_key)
  where transaction_to is null and kind = 'semantic';

-- Rewrite review_event to match the Rust ReviewEvent shape
-- (trace_id, caller_id, used_ids, outcome).
drop table memphant.review_event;
create table if not exists memphant.review_event (
  id uuid primary key default gen_random_uuid(),
  tenant_id uuid not null references memphant.tenant(id),
  trace_id uuid not null,
  caller_id text not null,
  outcome text not null check (outcome in ('success','failure','corrected','ignored')),
  created_at timestamptz not null default now(),
  unique (trace_id, caller_id)
);
create table if not exists memphant.review_event_unit (
  review_event_id uuid not null references memphant.review_event(id) on delete cascade,
  tenant_id uuid not null,
  memory_unit_id uuid not null,
  primary key (review_event_id, memory_unit_id),
  foreign key (tenant_id, memory_unit_id) references memphant.memory_unit(tenant_id, id)
);
alter table memphant.review_event enable row level security;
alter table memphant.review_event_unit enable row level security;
create index if not exists memphant_review_event_tenant_trace_idx
  on memphant.review_event (tenant_id, trace_id);
create index if not exists memphant_review_event_unit_tenant_unit_idx
  on memphant.review_event_unit (tenant_id, memory_unit_id);

create table if not exists memphant.api_key (
  id uuid primary key default gen_random_uuid(),
  tenant_id uuid not null references memphant.tenant(id),
  key_hash text not null unique,
  label text not null default '',
  max_trust text not null default 'trusted_user',
  created_at timestamptz not null default now(),
  revoked_at timestamptz
);
alter table memphant.api_key enable row level security;
create index if not exists memphant_api_key_tenant_idx
  on memphant.api_key (tenant_id, created_at);

-- Job queue: reuse the existing job_state table from 001. It already carries
-- attempts int; add claimed_at and the dead-letter state.
alter table memphant.job_state
  add column if not exists claimed_at timestamptz;
alter table memphant.job_state
  drop constraint job_state_state_check;
alter table memphant.job_state
  add constraint job_state_state_check
    check (state in ('queued','running','done','failed','dead'));

-- Full-text search support for the lexical recall channel.
alter table memphant.memory_unit
  add column if not exists body_tsv tsvector
    generated always as (to_tsvector('english', coalesce(body,''))) stored;
create index if not exists memphant_memory_unit_body_tsv_idx
  on memphant.memory_unit using gin (body_tsv);

-- Replicate 001's grant/policy pattern for every new table (001's
-- `grant on all tables` predates them; dropping review_event dropped its
-- grants and policy).
drop policy if exists memphant_forgotten_source_tenant_isolation on memphant.forgotten_source;
create policy memphant_forgotten_source_tenant_isolation on memphant.forgotten_source for all to memphant_app, memphant_cron using (tenant_id = memphant.current_tenant_id()) with check (tenant_id = memphant.current_tenant_id());
drop policy if exists memphant_review_event_tenant_isolation on memphant.review_event;
create policy memphant_review_event_tenant_isolation on memphant.review_event for all to memphant_app, memphant_cron using (tenant_id = memphant.current_tenant_id()) with check (tenant_id = memphant.current_tenant_id());
drop policy if exists memphant_review_event_unit_tenant_isolation on memphant.review_event_unit;
create policy memphant_review_event_unit_tenant_isolation on memphant.review_event_unit for all to memphant_app, memphant_cron using (tenant_id = memphant.current_tenant_id()) with check (tenant_id = memphant.current_tenant_id());
drop policy if exists memphant_api_key_tenant_isolation on memphant.api_key;
create policy memphant_api_key_tenant_isolation on memphant.api_key for all to memphant_app, memphant_cron using (tenant_id = memphant.current_tenant_id()) with check (tenant_id = memphant.current_tenant_id());

grant select, insert, update, delete
  on memphant.forgotten_source, memphant.review_event, memphant.review_event_unit, memphant.api_key
  to memphant_app;
grant select, insert, update, delete
  on memphant.forgotten_source, memphant.review_event, memphant.review_event_unit, memphant.api_key
  to memphant_cron;

insert into memphant.schema_migrations (version, schema_compat_revision, migration_kind)
values ('20260709_002_runtime_reconciliation', '20260709_002_runtime_reconciliation', 'rewrite')
on conflict (version) do update
set schema_compat_revision = excluded.schema_compat_revision,
    migration_kind = excluded.migration_kind;

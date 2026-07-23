alter table memphant.mutation_ledger
  drop constraint mutation_ledger_verb_check,
  add constraint mutation_ledger_verb_check
    check (verb in ('retain','reflect','correct','forget','mark','file_sync','erase_subject'));

insert into memphant.schema_migrations (version, schema_compat_revision, migration_kind)
values (
  '20260723_002_file_sync_mutation_verb',
  '20260723_002_file_sync_mutation_verb',
  'breaking'
)
on conflict (version) do update
set schema_compat_revision = excluded.schema_compat_revision,
    migration_kind = excluded.migration_kind;

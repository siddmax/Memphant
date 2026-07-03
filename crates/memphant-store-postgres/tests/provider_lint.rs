use memphant_store_postgres::{Provider, lint_migration_sql, lint_migrations};

#[test]
fn bundled_wsa_migration_passes_all_provider_lints() {
    for provider in [Provider::PlainPostgres, Provider::Supabase, Provider::Neon] {
        lint_migrations(provider).expect("bundled migration should pass");
    }
}

#[test]
fn provider_lint_rejects_browser_role_grants() {
    let bad_sql = r#"
        create table if not exists memphant.memory_unit (
          id uuid not null,
          tenant_id uuid not null,
          scope_id uuid not null,
          primary key (tenant_id, id)
        );
        alter table memphant.memory_unit enable row level security;
        create index if not exists memphant_memory_unit_tenant_idx on memphant.memory_unit (tenant_id);
        grant select on memphant.memory_unit to authenticated;
    "#;

    let error = lint_migration_sql(bad_sql, Provider::Supabase).expect_err("grant must fail");

    assert!(error.to_string().contains("browser_role_grant"));
}

#[test]
fn provider_lint_rejects_missing_rls() {
    let bad_sql = r#"
        create table if not exists memphant.memory_unit (
          id uuid not null,
          tenant_id uuid not null,
          scope_id uuid not null,
          primary key (tenant_id, id)
        );
        create index if not exists memphant_memory_unit_tenant_idx on memphant.memory_unit (tenant_id);
    "#;

    let error = lint_migration_sql(bad_sql, Provider::PlainPostgres).expect_err("RLS must fail");

    assert!(error.to_string().contains("memory_unit:missing_rls"));
}

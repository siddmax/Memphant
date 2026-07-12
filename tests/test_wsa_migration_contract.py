from __future__ import annotations

import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
MIGRATIONS = ROOT / "memphant_migrations" / "versions"
BOOTSTRAP = MIGRATIONS / "20260703_001_wsa_bootstrap.sql"
RECONCILIATION = MIGRATIONS / "20260709_002_runtime_reconciliation.sql"

REQUIRED_TABLES = {
    "tenant",
    "subject",
    "actor",
    "agent_node",
    "scope",
    "scope_policy",
    "episode",
    "resource",
    "memory_unit",
    "memory_edge",
    "embedding_profile",
    "embedding",
    "citation",
    "trust_event",
    "event_outbox",
    "retrieval_trace",
    "deletion_generation",
    "job_state",
    "blob_ledger",
    "belief_observation",
    "review_event",
    "scope_block",
    "schema_migrations",
}

TENANT_SCOPED_TABLES = REQUIRED_TABLES - {"schema_migrations"}


def sql_text() -> str:
    return BOOTSTRAP.read_text(encoding="utf-8")


def test_wsa_bootstrap_migration_declares_required_tables() -> None:
    sql = sql_text()

    missing = [
        table
        for table in sorted(REQUIRED_TABLES)
        if f"create table if not exists memphant.{table}" not in sql.lower()
    ]

    assert missing == []


def test_migration_boundary_script_rejects_public_syndai_and_drop_table() -> None:
    result = subprocess.run(
        ["python3", "scripts/check_memphant_migration_boundary.py"],
        cwd=ROOT,
        text=True,
        capture_output=True,
        check=False,
    )

    assert result.returncode == 0, result.stdout + result.stderr
    assert "migration_boundary=clean" in result.stdout


def test_migration_contract_script_accepts_wsa_bootstrap() -> None:
    result = subprocess.run(
        ["python3", "scripts/check_memphant_migration_contract.py"],
        cwd=ROOT,
        text=True,
        capture_output=True,
        check=False,
    )

    assert result.returncode == 0, result.stdout + result.stderr
    assert "migration_contract=clean" in result.stdout


def test_migration_class_script_accepts_additive_bootstrap() -> None:
    result = subprocess.run(
        ["python3", "scripts/check_memphant_migration_class.py"],
        cwd=ROOT,
        text=True,
        capture_output=True,
        check=False,
    )

    assert result.returncode == 0, result.stdout + result.stderr
    assert "migration_class=clean" in result.stdout


def test_apply_runner_dry_run_reports_ordered_bootstrap() -> None:
    result = subprocess.run(
        [
            "python3",
            "scripts/apply_memphant_migrations.py",
            "--database-url",
            "postgres://memphant.invalid/memphant",
            "--dry-run",
        ],
        cwd=ROOT,
        text=True,
        capture_output=True,
        check=False,
    )

    assert result.returncode == 0, result.stdout + result.stderr
    assert "migration_plan=2" in result.stdout
    assert "20260703_001_wsa_bootstrap.sql" in result.stdout
    assert "20260709_002_runtime_reconciliation.sql" in result.stdout


def test_live_catalog_check_requires_database_url() -> None:
    result = subprocess.run(
        ["python3", "scripts/check_memphant_live_catalog.py"],
        cwd=ROOT,
        text=True,
        capture_output=True,
        check=False,
    )

    assert result.returncode == 2
    assert "--database-url" in result.stderr


def test_wsa_bootstrap_has_schema_migration_compat_floor() -> None:
    sql = sql_text().lower()

    assert "memphant.schema_migrations" in sql
    assert "schema_compat_revision" in sql
    assert "version text primary key" in sql


def test_tenant_scoped_tables_have_rls_and_tenant_indexes() -> None:
    sql = sql_text().lower()

    missing_tenant = [
        table
        for table in sorted(TENANT_SCOPED_TABLES)
        if table != "tenant" and f"create table if not exists memphant.{table}" in sql
        and "tenant_id" not in _table_block(sql, table)
    ]
    missing_rls = [
        table
        for table in sorted(TENANT_SCOPED_TABLES)
        if f"alter table memphant.{table} enable row level security" not in sql
    ]
    missing_index = [
        table
        for table in sorted(TENANT_SCOPED_TABLES)
        if table != "tenant" and f"create index if not exists memphant_{table}_tenant" not in sql
    ]

    assert missing_tenant == []
    assert missing_rls == []
    assert missing_index == []


def test_wsa_bootstrap_locks_down_browser_roles() -> None:
    sql = sql_text().lower()

    forbidden_grants = [
        "grant select on",
        "grant insert on",
        "grant update on",
        "grant delete on",
        "grant all on",
    ]
    browser_roles = ["anon", "authenticated", "authenticator"]

    for grant in forbidden_grants:
        for role in browser_roles:
            assert f"{grant} memphant." not in sql or f" to {role}" not in sql

    for role in browser_roles:
        assert f"revoke all on schema memphant from {role}" in sql


def _reconciliation_sql() -> str:
    return RECONCILIATION.read_text(encoding="utf-8")


def test_runtime_reconciliation_declares_rewrite_header() -> None:
    first_lines = _reconciliation_sql().splitlines()[:5]
    assert any(
        line.strip() == "-- migration_kind: rewrite" for line in first_lines
    )


def test_runtime_reconciliation_replaces_open_subject_index_with_scope_bound() -> None:
    sql = _reconciliation_sql().lower()

    # The old tenant-open-subject index must be dropped by its REAL name and
    # NOT behind `if exists` — a silent no-op must fail loudly at apply time.
    assert "drop index memphant.memphant_memory_unit_tenant_open_subject_idx" in sql
    assert "drop index if exists memphant.memphant_memory_unit_tenant_open_subject_idx" not in sql

    start = sql.index("create unique index memphant_memory_unit_scope_subject_idx")
    stanza = sql[start : sql.index(";", start)]
    assert "scope_id" in stanza
    assert "kind = 'semantic'" in stanza
    assert "transaction_to is null" in stanza


def test_runtime_reconciliation_adds_api_key_and_forgotten_source_with_rls() -> None:
    sql = _reconciliation_sql().lower()

    for table in ("api_key", "forgotten_source"):
        assert f"create table if not exists memphant.{table}" in sql
        assert f"alter table memphant.{table} enable row level security" in sql
        assert f"create policy memphant_{table}_tenant_isolation" in sql

    api_key_block = _table_block(sql, "api_key")
    assert "key_hash text not null unique" in api_key_block
    assert "max_trust" in api_key_block
    assert "revoked_at" in api_key_block

    forgotten_block = _table_block(sql, "forgotten_source")
    assert "source_kind text not null check (source_kind in ('episode','resource','memory_unit'))" in forgotten_block
    assert "primary key (tenant_id, source_kind, source_id)" in forgotten_block


def test_runtime_reconciliation_rewrites_review_event_with_join_table() -> None:
    sql = _reconciliation_sql().lower()

    assert "drop table memphant.review_event" in sql
    review_block = _table_block(sql, "review_event")
    assert "trace_id uuid not null" in review_block
    assert "caller_id text not null" in review_block
    assert "unique (trace_id, caller_id)" in review_block

    review_pk = _table_block(sql, "review_event")
    assert "primary key (tenant_id, id)" in review_pk

    join_block = _table_block(sql, "review_event_unit")
    assert "review_event_id uuid not null" in join_block
    assert (
        "foreign key (tenant_id, review_event_id)\n    references memphant.review_event(tenant_id, id) on delete cascade"
        in join_block
    )
    assert "primary key (review_event_id, memory_unit_id)" in join_block
    assert "foreign key (tenant_id, memory_unit_id) references memphant.memory_unit(tenant_id, id)" in join_block
    assert "alter table memphant.review_event_unit enable row level security" in sql


def test_runtime_reconciliation_extends_job_state_instead_of_new_table() -> None:
    sql = _reconciliation_sql().lower()

    assert "create table if not exists memphant.reflect_job" not in sql
    assert "alter table memphant.job_state" in sql
    assert "add column if not exists claimed_at timestamptz" in sql
    assert "'dead'" in sql


def test_boundary_checker_allows_drops_only_under_rewrite_header(tmp_path: Path) -> None:
    def run_boundary(sql: str) -> subprocess.CompletedProcess[str]:
        migrations = tmp_path / "versions"
        migrations.mkdir(exist_ok=True)
        for stale in migrations.glob("*.sql"):
            stale.unlink()
        (migrations / "20990101_900_case.sql").write_text(sql, encoding="utf-8")
        return subprocess.run(
            [
                "python3",
                "scripts/check_memphant_migration_boundary.py",
                "--migrations-dir",
                str(migrations),
            ],
            cwd=ROOT,
            text=True,
            capture_output=True,
            check=False,
        )

    denied = run_boundary("drop table memphant.review_event;\n")
    assert denied.returncode == 1
    assert "drop_table_without_rewrite_header" in denied.stdout

    denied_index = run_boundary("drop index memphant.some_idx;\n")
    assert denied_index.returncode == 1
    assert "drop_index_without_rewrite_header" in denied_index.stdout

    allowed = run_boundary(
        "-- migration_kind: rewrite\n"
        "drop table memphant.review_event;\n"
        "drop index memphant.some_idx;\n"
    )
    assert allowed.returncode == 0, allowed.stdout + allowed.stderr
    assert "migration_boundary=clean" in allowed.stdout

    still_denied = run_boundary(
        "-- migration_kind: rewrite\n"
        "create table public.leak (id int);\n"
    )
    assert still_denied.returncode == 1
    assert "public_schema_reference" in still_denied.stdout


def _table_block(sql: str, table: str) -> str:
    marker = f"create table if not exists memphant.{table}"
    start = sql.index(marker)
    next_marker = sql.find("create table if not exists", start + len(marker))
    if next_marker == -1:
        return sql[start:]
    return sql[start:next_marker]

from __future__ import annotations

import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
MIGRATIONS = ROOT / "memphant_migrations" / "versions"
BOOTSTRAP = MIGRATIONS / "20260703_001_wsa_bootstrap.sql"

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
    assert "migration_plan=1" in result.stdout
    assert "20260703_001_wsa_bootstrap.sql" in result.stdout


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


def _table_block(sql: str, table: str) -> str:
    marker = f"create table if not exists memphant.{table}"
    start = sql.index(marker)
    next_marker = sql.find("create table if not exists", start + len(marker))
    if next_marker == -1:
        return sql[start:]
    return sql[start:next_marker]

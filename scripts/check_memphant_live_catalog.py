from __future__ import annotations

import argparse
import json
import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]

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
    "retrieval_trace",
    "deletion_generation",
    "job_state",
    "blob_ledger",
    "belief_observation",
    "review_event",
    "scope_block",
    "schema_migrations",
}

TENANT_RLS_TABLES = REQUIRED_TABLES - {"schema_migrations"}


def psql_json(database_url: str, sql: str) -> list[dict[str, object]]:
    result = subprocess.run(
        [
            "psql",
            "--no-psqlrc",
            "--set",
            "ON_ERROR_STOP=1",
            "--quiet",
            "--tuples-only",
            "--no-align",
            "--dbname",
            database_url,
            "--command",
            f"select coalesce(json_agg(row_to_json(q)), '[]'::json) from ({sql}) q;",
        ],
        cwd=ROOT,
        text=True,
        capture_output=True,
        check=False,
    )
    if result.returncode != 0:
        raise RuntimeError(result.stdout + result.stderr)
    return json.loads(result.stdout.strip())


def semver_at_least(actual: str, minimum: str) -> bool:
    def parts(value: str) -> tuple[int, int, int]:
        parsed = [int(part) for part in value.split(".")]
        return tuple((parsed + [0, 0, 0])[:3])  # type: ignore[return-value]

    return parts(actual) >= parts(minimum)


def main() -> int:
    parser = argparse.ArgumentParser(description="Check a live MemPhant Postgres catalog.")
    parser.add_argument("--database-url", required=True)
    parser.add_argument("--min-postgres-version", type=int, default=17)
    parser.add_argument("--min-pgvector-version", default="0.8.4")
    args = parser.parse_args()

    findings: list[str] = []

    server = psql_json(
        args.database_url,
        "select current_setting('server_version_num')::int as server_version_num",
    )[0]
    if int(server["server_version_num"]) < args.min_postgres_version * 10000:
        findings.append(
            f"postgres_version:below_floor:{server['server_version_num']}"
        )

    extensions = {
        row["extname"]: row["extversion"]
        for row in psql_json(
            args.database_url,
            "select extname, extversion from pg_extension where extname in ('vector','pg_trgm','ltree','btree_gist')",
        )
    }
    for extension in ("vector", "pg_trgm", "ltree", "btree_gist"):
        if extension not in extensions:
            findings.append(f"{extension}:missing_extension")
    vector_version = str(extensions.get("vector", "0.0.0"))
    if not semver_at_least(vector_version, args.min_pgvector_version):
        findings.append(f"vector:below_floor:{vector_version}")

    tables = {
        row["tablename"]
        for row in psql_json(
            args.database_url,
            "select tablename from pg_tables where schemaname = 'memphant'",
        )
    }
    for table in sorted(REQUIRED_TABLES - tables):
        findings.append(f"{table}:missing_table")

    rls_tables = {
        row["relname"]
        for row in psql_json(
            args.database_url,
            """
            select c.relname
            from pg_class c
            join pg_namespace n on n.oid = c.relnamespace
            where n.nspname = 'memphant'
              and c.relkind = 'r'
              and c.relrowsecurity
            """,
        )
    }
    for table in sorted(TENANT_RLS_TABLES - rls_tables):
        findings.append(f"{table}:missing_rls")

    browser_grants = psql_json(
        args.database_url,
        """
        select grantee, table_name, privilege_type
        from information_schema.table_privileges
        where table_schema = 'memphant'
          and grantee in ('anon','authenticated','authenticator')
        """,
    )
    for row in browser_grants:
        findings.append(
            f"{row['table_name']}:{row['grantee']}:browser_role_grant:{row['privilege_type']}"
        )

    function_search_path = psql_json(
        args.database_url,
        """
        select p.proname, p.proconfig::text as proconfig
        from pg_proc p
        join pg_namespace n on n.oid = p.pronamespace
        where n.nspname = 'memphant'
          and p.proname in ('current_tenant_id','set_updated_at')
        """,
    )
    for row in function_search_path:
        if "search_path=memphant, pg_catalog" not in str(row["proconfig"]):
            findings.append(f"{row['proname']}:missing_search_path")
    if len(function_search_path) != 2:
        findings.append("functions:missing_search_path_checked_functions")

    tenant_indexes = {
        row["tablename"]
        for row in psql_json(
            args.database_url,
            """
            select tablename
            from pg_indexes
            where schemaname = 'memphant'
              and indexname like 'memphant_%_tenant%'
            """,
        )
    }
    for table in sorted(TENANT_RLS_TABLES - {"tenant"} - tenant_indexes):
        findings.append(f"{table}:missing_tenant_index")

    missing_fk_indexes = psql_json(
        args.database_url,
        """
        with fk as (
          select conrelid, conname, conkey
          from pg_constraint
          where contype = 'f'
            and connamespace = 'memphant'::regnamespace
        ),
        indexed as (
          select indrelid, indkey::int2[] as indkey
          from pg_index
          where indisvalid
        )
        select conrelid::regclass::text as table_name, conname
        from fk
        where not exists (
          select 1
          from indexed
          where indexed.indrelid = fk.conrelid
            and indexed.indkey[0:array_length(fk.conkey, 1) - 1] = fk.conkey
        )
        order by table_name, conname
        """,
    )
    for row in missing_fk_indexes:
        findings.append(f"{row['table_name']}:{row['conname']}:missing_fk_index")

    migrations = psql_json(
        args.database_url,
        """
        select version, schema_compat_revision, migration_kind
        from memphant.schema_migrations
        where version = '20260703_001_wsa_bootstrap'
        """,
    )
    if not migrations:
        findings.append("schema_migrations:missing_wsa_bootstrap")
    elif migrations[0]["schema_compat_revision"] != "20260703_001_wsa_bootstrap":
        findings.append("schema_migrations:wrong_schema_compat_revision")

    if findings:
        print("live_catalog=dirty")
        for finding in findings:
            print(finding)
        return 1

    print("live_catalog=clean")
    print(f"postgres_version_num={server['server_version_num']}")
    print(f"vector_version={vector_version}")
    print(f"memphant_tables={len(tables)}")
    print(f"rls_tables={len(rls_tables)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

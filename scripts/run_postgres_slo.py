#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
TENANT_ID = "00000000-0000-4000-8000-000000000001"
SCOPE_ID = "00000000-0000-4000-8000-000000000002"


def run_psql(database_url: str, sql: str) -> str:
    result = subprocess.run(
        ["psql", "-qAtX", "--set", "ON_ERROR_STOP=1", "--dbname", database_url, "--command", sql],
        cwd=ROOT,
        text=True,
        capture_output=True,
        check=True,
    )
    return result.stdout.strip()


def apply_migrations(database_url: str) -> None:
    subprocess.run(
        ["python3", "scripts/apply_memphant_migrations.py", "--database-url", database_url],
        cwd=ROOT,
        check=True,
    )


def measurement_sql(seed_units: int, repeats: int) -> str:
    return f"""
insert into memphant.tenant (id, slug, plan, region)
values ('{TENANT_ID}', 'slo-local', 'dev', 'local')
on conflict (id) do update set slug = excluded.slug;

insert into memphant.scope (id, tenant_id, kind, external_ref, materialized_path, scope_depth)
values ('{SCOPE_ID}', '{TENANT_ID}', 'agent', 'slo-local', 'slo_local', 0)
on conflict (tenant_id, id) do nothing;

delete from memphant.memory_unit where tenant_id = '{TENANT_ID}';

insert into memphant.memory_unit (
  id, tenant_id, scope_id, kind, state, subject_key, body, trust_level, observed_at
)
select
  ('00000000-0000-4000-8000-' || lpad(to_hex(g), 12, '0'))::uuid,
  '{TENANT_ID}'::uuid,
  '{SCOPE_ID}'::uuid,
  'semantic',
  'active',
  'bench_subject_' || g,
  'Postgres SLO sampled memory unit ' || g || ' for bench_subject_' || g,
  'trusted_system',
  now()
from generate_series(1, {seed_units}) as g;

analyze memphant.memory_unit;

create temp table memphant_slo_measurements (elapsed_ms double precision) on commit drop;

do $$
declare
  started timestamptz;
  finished timestamptz;
  idx integer;
begin
  for idx in 1..{repeats} loop
    started := clock_timestamp();
    perform id
    from memphant.memory_unit
    where tenant_id = '{TENANT_ID}'
      and scope_id = '{SCOPE_ID}'
      and subject_key = 'bench_subject_' || (((idx - 1) % {seed_units}) + 1)
      and state = 'active'
      and transaction_to is null
    order by created_at desc
    limit 12;
    finished := clock_timestamp();
    insert into memphant_slo_measurements
    values (extract(epoch from finished - started) * 1000.0);
  end loop;
end
$$;

select json_build_object(
  'p50_ms', percentile_cont(0.5) within group (order by elapsed_ms),
  'p95_ms', percentile_cont(0.95) within group (order by elapsed_ms),
  'max_ms', max(elapsed_ms),
  'repeat_count', count(*)
) from memphant_slo_measurements;
"""


def explain_sql() -> str:
    return f"""
explain (analyze, buffers, format json)
select id
from memphant.memory_unit
where tenant_id = '{TENANT_ID}'
  and scope_id = '{SCOPE_ID}'
  and subject_key = 'bench_subject_7'
  and state = 'active'
  and transaction_to is null
order by created_at desc
limit 12;
"""


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--database-url", required=True)
    parser.add_argument("--output", required=True)
    parser.add_argument("--seeded-units", type=int, default=1000)
    parser.add_argument("--repeats", type=int, default=30)
    parser.add_argument("--apply-migrations", action="store_true")
    args = parser.parse_args()

    if args.apply_migrations:
        apply_migrations(args.database_url)

    metrics = json.loads(run_psql(args.database_url, measurement_sql(args.seeded_units, args.repeats)))
    explain = json.loads(run_psql(args.database_url, explain_sql()))
    output = {
        "store_backend": "postgres",
        "seeded_units": args.seeded_units,
        "repeat_count": metrics["repeat_count"],
        "p50_ms": metrics["p50_ms"],
        "p95_ms": metrics["p95_ms"],
        "max_ms": metrics["max_ms"],
        "thresholds_ms": {"p50_lt": 200, "p95_lt": 500},
        "slowest_query_explain": explain,
        "status": "pass" if metrics["p50_ms"] < 200 and metrics["p95_ms"] < 500 else "fail",
    }
    out = Path(args.output)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(output, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(f"postgres_slo={output['status']} p50_ms={output['p50_ms']:.3f} p95_ms={output['p95_ms']:.3f}")
    return 0 if output["status"] == "pass" else 1


if __name__ == "__main__":
    raise SystemExit(main())

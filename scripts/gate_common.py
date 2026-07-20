#!/usr/bin/env python3
"""Shared corpus + provenance helpers for the Syndai replacement gate (W10).

Imported by the miner (``gate_mine_goldens.py``) and BOTH engine runners
(``gate_run_syndai.py``, ``gate_run_memphant.py``) so the corpus is sectionized
identically, the golden set is read identically, and provenance is graded
identically for both engines (the brief's requirement 4). ``gate_run_syndai.py``
runs under the Syndai backend's interpreter and imports this by inserting
``/Users/sidsharma/Memphant/scripts`` on ``sys.path`` — everything here is
stdlib-only (plus ``run_reader`` for containment, which is itself stdlib-only).

Ingest granularity decision (recorded here because both runners depend on it):
Syndai ingests each markdown FILE and its real sectionizer/chunker splits it;
MemPhant's resource channel does NOT auto-chunk (one resource body -> one
whole-document unit). To compare the two engines at the SAME section
granularity, the gate pre-splits the corpus into markdown sections with the one
parser below and ingests each section as one unit in EACH engine. Provenance is
then "does a retrieved item's body contain the gold span" — identical for both
engines, independent of either engine's internal provenance metadata.
"""

from __future__ import annotations

import hashlib
import importlib.util
import json
import math
import os
import random
import re
import statistics
import subprocess
from pathlib import Path
from urllib.parse import urlsplit, urlunsplit

MEMPHANT_ROOT = Path(__file__).resolve().parents[1]
CORPUS_MANIFEST_PATH = (
    MEMPHANT_ROOT / "benchmarks" / "manifests" / "syndai_docs_gate.lock.json"
)

# The SDD process scaffolding (plans/audits/briefs/research) under
# docs/superpowers/ is not product documentation; the gate corpus is the real
# product/architecture/spec docs.
EXCLUDE_PREFIXES = ("docs/superpowers/",)
CORPUS_GLOBS = ("docs/*.md", "docs/**/*.md")

# A leaf section must have at least this much of its own content to seed a
# verbatim-span question (miner). The haystack (runner ingest) keeps every
# section with any non-empty content so no gold section is ever missing.
SECTION_MIN_CHARS = 240
SECTION_MAX_CHARS = 3200

HEADING_RE = re.compile(r"^(#{1,6})[ \t]+(.*?)[ \t]*#*[ \t]*$")
DISPOSABLE_DATABASE_ENV = "MEMPHANT_SYNDAI_GATE_SCRATCH_DB"
DISPOSABLE_TEMPLATE_ENV = "MEMPHANT_SYNDAI_GATE_TEMPLATE_DB"


def _load_run_reader():
    spec = importlib.util.spec_from_file_location(
        "run_reader", MEMPHANT_ROOT / "scripts" / "run_reader.py"
    )
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(module)
    return module


_RUN_READER = _load_run_reader()
normalize = _RUN_READER.normalize
contains_gold = _RUN_READER.contains_gold


def sha256_hex(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def json_fingerprint(value) -> str:
    return sha256_hex(
        json.dumps(value, sort_keys=True, separators=(",", ":")).encode()
    )


def file_sha256(path: Path) -> str:
    return sha256_hex(path.read_bytes())


def disposable_database_urls(base_url: str, scratch_name: str) -> tuple[str, str, str]:
    """Return (maintenance, scratch, template) for a strictly local Postgres URL."""
    normalized = base_url.replace("postgresql+asyncpg://", "postgresql://", 1)
    parsed = urlsplit(normalized)
    if parsed.scheme not in ("postgres", "postgresql") or parsed.hostname not in (
        "localhost",
        "127.0.0.1",
        "::1",
    ):
        raise RuntimeError("Syndai gate requires a local Postgres database URL")
    template = parsed.path.lstrip("/")
    if not template or "/" in template:
        raise RuntimeError("Syndai gate base database name is invalid")
    if not re.fullmatch(r"syndai_gate_[a-z0-9_]+", scratch_name):
        raise RuntimeError("Syndai gate scratch database name is invalid")
    maintenance = urlunsplit((parsed.scheme, parsed.netloc, "/postgres", parsed.query, ""))
    scratch = urlunsplit((parsed.scheme, parsed.netloc, "/" + scratch_name, parsed.query, ""))
    return maintenance, scratch, template


def run_in_disposable_database(
    base_url: str,
    scratch_name: str,
    command: list[str],
    environment: dict[str, str] | None = None,
) -> int:
    """Clone a local database, run one child, and force-drop it on every exit."""
    maintenance, scratch_url, template = disposable_database_urls(base_url, scratch_name)
    create = subprocess.run(
        ["createdb", f"--maintenance-db={maintenance}", f"--template={template}", scratch_name],
        capture_output=True,
        text=True,
    )
    if create.returncode != 0:
        raise RuntimeError(f"create disposable database failed: {create.stderr.strip()}")
    child_env = dict(os.environ if environment is None else environment)
    child_env["DATABASE_URL"] = scratch_url
    child_env[DISPOSABLE_DATABASE_ENV] = scratch_name
    child_env[DISPOSABLE_TEMPLATE_ENV] = template
    child_result = None
    child_error = None
    cleanup_error = None
    try:
        child_result = subprocess.run(command, env=child_env)
    except BaseException as error:
        child_error = error
    finally:
        drop = subprocess.run(
            ["dropdb", f"--maintenance-db={maintenance}", "--force", "--if-exists", scratch_name],
            capture_output=True,
            text=True,
        )
        if drop.returncode != 0:
            cleanup_error = drop.stderr.strip()
    if cleanup_error is not None:
        raise RuntimeError(f"drop disposable database failed: {cleanup_error}") from child_error
    if child_error is not None:
        raise child_error
    assert child_result is not None
    return child_result.returncode


def validate_disposable_database_child(database_url: str, scratch_name: str) -> None:
    _, expected, template = disposable_database_urls(database_url, scratch_name)
    normalized = database_url.replace("postgresql+asyncpg://", "postgresql://", 1)
    if template != scratch_name or urlsplit(normalized) != urlsplit(expected):
        raise RuntimeError("Syndai gate child is not connected to its disposable database")


def tracked_tree_identity(root: Path) -> dict:
    """Bind a run to the exact tracked source bytes, including dirty edits."""
    result = subprocess.run(
        ["git", "-C", str(root), "ls-files", "-z"], capture_output=True
    )
    if result.returncode != 0:
        raise RuntimeError(f"git ls-files failed in {root}")
    digest = hashlib.sha256()
    for raw in sorted(part for part in result.stdout.split(b"\0") if part):
        rel = raw.decode()
        path = root / rel
        data = path.read_bytes() if path.is_file() else None
        digest.update(len(raw).to_bytes(8, "big"))
        digest.update(raw)
        if data is None:
            digest.update(b"\xffdeleted")
        else:
            digest.update(len(data).to_bytes(8, "big"))
            digest.update(data)
    identity = {
        "git_commit": git_commit(root),
        "tracked_tree_sha256": digest.hexdigest(),
    }
    identity["sha256"] = json_fingerprint(identity)
    return identity


def generation_identity(*, root: Path, files: dict[str, Path]) -> dict:
    identity = {
        "source_tree": tracked_tree_identity(root),
        "files": {name: file_sha256(path) for name, path in sorted(files.items())},
    }
    identity["sha256"] = json_fingerprint(identity)
    return identity


def sql_sources_identity(*roots: Path) -> dict:
    payload = {}
    for root in roots:
        payload.update(
            {
                f"{root.name}/{path.relative_to(root)}": file_sha256(path)
                for path in sorted(root.rglob("*.sql"))
                if path.is_file()
            }
        )
    return {"files": payload, "sha256": json_fingerprint(payload)}


DATABASE_CATALOG_IDENTITY_SQL = r"""
with user_namespaces as (
  select oid, nspname from pg_namespace
  where nspname not in ('pg_catalog', 'information_schema')
    and nspname !~ '^pg_toast' and nspname !~ '^pg_temp_'
), facts as (
  select jsonb_build_object(
    'kind','relation','schema',n.nspname,'name',c.relname,'relkind',c.relkind,
    'persistence',c.relpersistence,'rls',c.relrowsecurity,
    'force_rls',c.relforcerowsecurity,'options',coalesce(to_jsonb(c.reloptions),'[]'::jsonb),
    'partition_key',pg_get_partkeydef(c.oid),
    'partition_bound',pg_get_expr(c.relpartbound,c.oid,true)
  )::text as fact
  from pg_class c join user_namespaces n on n.oid=c.relnamespace
  where c.relkind in ('r','p','v','m','S','f')
  union all
  select jsonb_build_object(
    'kind','column','schema',n.nspname,'relation',c.relname,'position',a.attnum,
    'name',a.attname,'type',format_type(a.atttypid,a.atttypmod),
    'not_null',a.attnotnull,'identity',a.attidentity,'generated',a.attgenerated,
    'default',pg_get_expr(d.adbin,d.adrelid),
    'collation',case when a.attcollation=0 then null else a.attcollation::regcollation::text end
  )::text
  from pg_attribute a join pg_class c on c.oid=a.attrelid
  join user_namespaces n on n.oid=c.relnamespace
  left join pg_attrdef d on d.adrelid=a.attrelid and d.adnum=a.attnum
  where a.attnum>0 and not a.attisdropped and c.relkind in ('r','p','v','m','f')
  union all
  select jsonb_build_object(
    'kind','constraint','schema',n.nspname,'relation',c.relname,
    'name',con.conname,'type',con.contype,'definition',pg_get_constraintdef(con.oid,true)
  )::text
  from pg_constraint con join pg_class c on c.oid=con.conrelid
  join user_namespaces n on n.oid=c.relnamespace
  union all
  select jsonb_build_object(
    'kind','index','schema',n.nspname,'relation',c.relname,
    'name',i.relname,'definition',pg_get_indexdef(i.oid)
  )::text
  from pg_index x join pg_class i on i.oid=x.indexrelid
  join pg_class c on c.oid=x.indrelid join user_namespaces n on n.oid=c.relnamespace
  union all
  select jsonb_build_object(
    'kind','view','schema',n.nspname,'name',c.relname,
    'definition',pg_get_viewdef(c.oid,true)
  )::text
  from pg_class c join user_namespaces n on n.oid=c.relnamespace
  where c.relkind in ('v','m')
  union all
  select jsonb_build_object(
    'kind','function','schema',n.nspname,'name',p.proname,
    'identity_args',pg_get_function_identity_arguments(p.oid),
    'definition',pg_get_functiondef(p.oid)
  )::text
  from pg_proc p join user_namespaces n on n.oid=p.pronamespace
  where p.prokind in ('f','p')
  union all
  select jsonb_build_object(
    'kind','trigger','schema',n.nspname,'relation',c.relname,
    'name',t.tgname,'definition',pg_get_triggerdef(t.oid,true)
  )::text
  from pg_trigger t join pg_class c on c.oid=t.tgrelid
  join user_namespaces n on n.oid=c.relnamespace where not t.tgisinternal
  union all
  select jsonb_build_object(
    'kind','policy','schema',n.nspname,'relation',c.relname,'name',p.polname,
    'command',p.polcmd,'permissive',p.polpermissive,'roles',p.polroles::text,
    'using',pg_get_expr(p.polqual,p.polrelid),
    'check',pg_get_expr(p.polwithcheck,p.polrelid)
  )::text
  from pg_policy p join pg_class c on c.oid=p.polrelid
  join user_namespaces n on n.oid=c.relnamespace
  union all
  select jsonb_build_object(
    'kind','type','schema',n.nspname,'name',t.typname,'type_kind',t.typtype,
    'not_null',t.typnotnull,'base_type',case when t.typbasetype=0 then null else format_type(t.typbasetype,null) end,
    'default',t.typdefault
  )::text
  from pg_type t join user_namespaces n on n.oid=t.typnamespace
  where t.typtype in ('d','e','r') and t.typcategory <> 'A'
  union all
  select jsonb_build_object(
    'kind','enum','schema',n.nspname,'type',t.typname,
    'sort',e.enumsortorder,'label',e.enumlabel
  )::text
  from pg_enum e join pg_type t on t.oid=e.enumtypid
  join user_namespaces n on n.oid=t.typnamespace
  union all
  select jsonb_build_object(
    'kind','sequence','schema',n.nspname,'name',c.relname,
    'type',format_type(s.seqtypid,null),'start',s.seqstart,'increment',s.seqincrement,
    'min',s.seqmin,'max',s.seqmax,'cache',s.seqcache,'cycle',s.seqcycle
  )::text
  from pg_sequence s join pg_class c on c.oid=s.seqrelid
  join user_namespaces n on n.oid=c.relnamespace
)
select fact from facts order by fact
""".strip()


def _database_facts(database_url: str, sql: str, label: str) -> bytes:
    result = subprocess.run(
        [
            "psql", "--no-psqlrc", "--tuples-only", "--no-align",
            "--set", "ON_ERROR_STOP=1", "--dbname", database_url,
            "--command", sql,
        ],
        capture_output=True,
    )
    if result.returncode != 0:
        stderr = result.stderr.decode(errors="replace").strip()
        raise RuntimeError(f"database {label} identity query failed: {stderr}")
    return result.stdout


def database_schema_identity(database_url: str, ledger_query: str) -> dict:
    schema_facts = _database_facts(
        database_url, DATABASE_CATALOG_IDENTITY_SQL, "schema catalog"
    )
    extension_and_migration_facts = _database_facts(
        database_url,
        (
            "select 'extension:' || extname || '=' || extversion "
            "from pg_extension union all " + ledger_query + " order by 1"
        ),
        "extension/migration",
    )
    identity = {
        "schema_sha256": sha256_hex(schema_facts),
        "extensions_and_migrations_sha256": sha256_hex(
            extension_and_migration_facts
        ),
    }
    identity["sha256"] = json_fingerprint(identity)
    return identity


def provenance_fingerprint(report: dict) -> str:
    return json_fingerprint(
        {key: value for key, value in report.items() if key != "provenance_sha256"}
    )


def finalize_provenance_report(report: dict, evidence_path: Path) -> dict:
    report["evidence_sha256"] = file_sha256(evidence_path)
    report["provenance_sha256"] = provenance_fingerprint(report)
    return report


def _percentile(values: list[float], percentile: int) -> float:
    if len(values) == 1:
        return values[0]
    return float(
        statistics.quantiles(values, n=100, method="inclusive")[percentile - 1]
    )


def validate_provenance_report(report: dict) -> dict[str, dict]:
    """Strict shared promotion schema for both engine runners and comparators."""
    rows = report.get("per_question")
    if not isinstance(rows, list) or not rows:
        raise ValueError("provenance per_question is incomplete")
    expected_n = report.get("expected_n")
    if type(expected_n) is not int or expected_n != len(rows):
        raise ValueError("provenance expected_n does not match rows")
    if report.get("golden_count") != expected_n:
        raise ValueError("provenance golden_count does not match expected_n")
    runtime = report.get("runtime_config")
    if not isinstance(runtime, dict) or not runtime:
        raise ValueError("provenance runtime_config is missing")
    if report.get("runtime_config_fingerprint") != json_fingerprint(runtime):
        raise ValueError("provenance runtime fingerprint is invalid")
    identity = runtime.get("generation_identity")
    if (
        not isinstance(identity, dict)
        or identity.get("sha256")
        != json_fingerprint({key: value for key, value in identity.items() if key != "sha256"})
    ):
        raise ValueError("provenance generation identity is invalid")
    source_tree = identity.get("source_tree")
    files = identity.get("files")
    if (
        not isinstance(source_tree, dict)
        or not re.fullmatch(r"[0-9a-f]{40,64}", str(source_tree.get("git_commit", "")))
        or not re.fullmatch(r"[0-9a-f]{64}", str(source_tree.get("tracked_tree_sha256", "")))
        or source_tree.get("sha256")
        != json_fingerprint(
            {key: value for key, value in source_tree.items() if key != "sha256"}
        )
        or not isinstance(files, dict)
        or not files
        or any(not re.fullmatch(r"[0-9a-f]{64}", str(value)) for value in files.values())
    ):
        raise ValueError("provenance source tree or executable hashes are invalid")
    database = identity.get("database")
    migrations = identity.get("migration_sources")
    if (
        not isinstance(database, dict)
        or database.get("sha256")
        != json_fingerprint({key: value for key, value in database.items() if key != "sha256"})
        or not re.fullmatch(r"[0-9a-f]{64}", str(database.get("schema_sha256", "")))
        or not re.fullmatch(
            r"[0-9a-f]{64}", str(database.get("extensions_and_migrations_sha256", ""))
        )
        or not isinstance(migrations, dict)
        or migrations.get("sha256") != json_fingerprint(migrations.get("files"))
        or not isinstance(migrations.get("files"), dict)
    ):
        raise ValueError("provenance database or migration identity is invalid")
    packer = runtime.get("evidence_packer")
    if (
        not isinstance(packer, dict)
        or packer.get("sha256")
        != json_fingerprint({key: value for key, value in packer.items() if key != "sha256"})
        or type(runtime.get("k")) is not int
        or runtime["k"] <= 0
        or type(runtime.get("budget_tokens")) is not int
        or runtime["budget_tokens"] <= 0
    ):
        raise ValueError("provenance evidence packer configuration is invalid")
    for field in ("golden_revision", "corpus_revision"):
        value = report.get(field)
        if (
            not isinstance(value, str)
            or not re.fullmatch(r"sha256:[0-9a-f]{64}", value)
            or runtime.get(field) != value
        ):
            raise ValueError(f"provenance {field} is invalid")
    if report.get("golden_revision") != "sha256:" + str(report.get("golden_sha256")):
        raise ValueError("provenance golden sha256 is inconsistent")
    for field in ("evidence_sha256", "provenance_sha256"):
        if not isinstance(report.get(field), str) or not re.fullmatch(
            r"[0-9a-f]{64}", report[field]
        ):
            raise ValueError(f"provenance {field} is invalid")
    if report["provenance_sha256"] != provenance_fingerprint(report):
        raise ValueError("provenance report fingerprint is invalid")

    by_id: dict[str, dict] = {}
    latencies: list[float] = []
    for row in rows:
        question_id = row.get("question_id") if isinstance(row, dict) else None
        if not isinstance(question_id, str) or not question_id or question_id in by_id:
            raise ValueError("provenance has missing or duplicate question IDs")
        for depth in (5, 10):
            if type(row.get(f"hit_at_{depth}")) is not bool:
                raise ValueError(f"provenance {question_id} has invalid hit_at_{depth}")
        for health in ("degraded", "fallback", "skipped"):
            if row.get(health) is not False:
                raise ValueError(f"provenance {question_id} is {health}")
        if row.get("failure", "none") != "none" or any(
            value
            for key, value in row.items()
            if key.endswith("_error")
        ):
            raise ValueError(f"provenance {question_id} has a retrieval failure")
        latency = row.get("recall_e2e_ms")
        if isinstance(latency, bool) or not isinstance(latency, (int, float)) or latency < 0:
            raise ValueError(f"provenance {question_id} has invalid latency")
        latencies.append(float(latency))
        if (
            row.get("evidence_packer_sha256") != packer["sha256"]
            or row.get("evidence_budget_tokens") != runtime["budget_tokens"]
            or type(row.get("evidence_packed_tokens")) is not int
            or not 0 <= row["evidence_packed_tokens"] <= runtime["budget_tokens"]
            or row.get("evidence_truncated_items") not in (0, 1)
            or type(row.get("evidence_dropped_items")) is not int
            or row["evidence_dropped_items"] < 0
            or type(row.get("packed_items")) is not int
            or not 0 <= row["packed_items"] <= runtime["k"]
        ):
            raise ValueError(f"provenance {question_id} has invalid evidence packing facts")
        if report.get("engine") == "syndai" and (
            runtime.get("reranking_enabled") is not True
            or row.get("reranker_configured") is not True
            or row.get("reranker_observed") is not True
        ):
            raise ValueError(f"provenance {question_id} lacks observed Syndai reranker success")
        by_id[question_id] = row

    for field in ("degraded_count", "fallback_count", "skipped_count", "reranker_failure_count"):
        if report.get(field) != 0:
            raise ValueError(f"provenance has nonzero {field}")
    for depth in (5, 10):
        observed = sum(row[f"hit_at_{depth}"] for row in rows) / len(rows)
        if not isinstance(report.get(f"recall_at_{depth}"), (int, float)) or not math.isclose(
            float(report[f"recall_at_{depth}"]), observed, abs_tol=1e-12
        ):
            raise ValueError(f"provenance recall_at_{depth} is inconsistent")
    aggregates = {
        "recall_e2e_ms_p50": float(statistics.median(latencies)),
        "recall_e2e_ms_p95": _percentile(latencies, 95),
    }
    for field, observed in aggregates.items():
        if not isinstance(report.get(field), (int, float)) or not math.isclose(
            float(report[field]), observed, abs_tol=1e-9
        ):
            raise ValueError(f"provenance {field} is inconsistent")
    ceiling = report.get("recall_e2e_p95_ceiling_ms")
    if isinstance(ceiling, bool) or not isinstance(ceiling, (int, float)) or ceiling <= 0:
        raise ValueError("provenance latency ceiling is invalid")
    if report.get("recall_e2e_p95_within_ceiling") is not True or aggregates[
        "recall_e2e_ms_p95"
    ] > float(ceiling):
        raise ValueError("provenance exceeds its hard latency ceiling")
    return by_id


def golden_source_clusters(path: Path) -> tuple[str, dict[str, str]]:
    revision = "sha256:" + file_sha256(path)
    clusters: dict[str, str] = {}
    for line_number, line in enumerate(path.read_text().splitlines(), 1):
        if not line.strip():
            continue
        row = json.loads(line)
        question_id = row.get("question_id")
        provenance = row.get("provenance")
        if not isinstance(question_id, str) or not question_id or question_id in clusters:
            raise ValueError(f"golden line {line_number} has invalid question_id")
        if not isinstance(provenance, list) or not provenance:
            raise ValueError(f"golden {question_id} lacks pinned provenance")
        if any(
            not isinstance(item, dict)
            or not isinstance(item.get("file"), str)
            or not item["file"]
            for item in provenance
        ):
            raise ValueError(f"golden {question_id} has unclusterable provenance")
        files = sorted({item["file"] for item in provenance})
        clusters[question_id] = "\x1f".join(files)
    if not clusters:
        raise ValueError("golden is empty")
    return revision, clusters


def cluster_bootstrap_ci(
    values_by_cluster: dict[str, list[float]], *, resamples: int, seed: int
) -> dict:
    if resamples < 1000 or not values_by_cluster:
        raise ValueError("cluster bootstrap requires clusters and at least 1000 resamples")
    cluster_ids = sorted(values_by_cluster)
    observed = [value for key in cluster_ids for value in values_by_cluster[key]]
    rng = random.Random(seed)
    means = []
    for _ in range(resamples):
        selected = [rng.choice(cluster_ids) for _ in cluster_ids]
        values = [value for key in selected for value in values_by_cluster[key]]
        means.append(sum(values) / len(values))
    means.sort()
    low = means[min(int(resamples * 0.025), resamples - 1)]
    high = means[min(max(math.ceil(resamples * 0.975) - 1, 0), resamples - 1)]
    return {
        "mean": sum(observed) / len(observed),
        "ci95_low": low,
        "ci95_high": high,
        "ci_excludes_zero": low > 0 or high < 0,
        "resamples": resamples,
        "seed": seed,
    }


def golden_lock_path(golden_path: Path) -> Path:
    """The lock JSON for a golden JSONL, by the miner's naming convention:
    ``<stem>.jsonl`` -> ``<stem>.lock.json`` (holds for every golden set —
    v1 ``syndai_docs_golden.jsonl``, v2 ``syndai_docs_golden_v2.jsonl``, and
    any future vN). SINGLE SOURCE OF TRUTH for both engine runners
    (``gate_run_memphant.py``, ``gate_run_syndai.py``) so the lock path is
    derived identically instead of drifting into a per-script hardcoded
    constant — which is exactly how ``gate_run_syndai.py``'s old
    ``GOLDEN_LOCK_PATH`` module constant went stale: it stayed pinned to v1's
    lock file even when ``--golden`` was pointed at v2, so a v2 run would
    "verify" against the wrong lock and silently accept a drifted golden
    set."""
    return golden_path.with_name(golden_path.stem + ".lock.json")


def breadcrumb_prefix(heading_path: list[str]) -> str:
    """Syndai's deterministic context-prefix convention, byte-identical to
    ``processing_chunks.py:84``'s ``_deterministic_context_prefix``
    (Syndai backend, not vendored here):

        if not chunk.heading_hierarchy:
            return None
        return "Section path: " + " > ".join(chunk.heading_hierarchy)

    ``_build_embedding_texts`` then joins a non-None prefix to the chunk
    content with a blank line (``f"{prefix}\\n\\n{chunk.content}"``); this
    helper returns that already-joined prefix (including the trailing blank
    line) so callers can simply prepend it to a body verbatim, or ``""`` when
    ``heading_path`` is empty — mirroring Syndai's plain truthiness check on
    the list, NOT a check against any particular sentinel string.

    NOTE (R1-T1 empty-heading-path finding): ``gate_common``'s own
    ``parse_sections`` never actually produces an empty ``heading_path``.
    Headerless content before a file's first heading (or a headingless file
    entirely) gets the placeholder ``["(preamble)"]`` instead, so no corpus
    text is ever dropped from the haystack. That makes this function's
    ``not heading_path`` branch unreachable from a real ``Section`` in this
    harness today — every ``Section``, including preamble ones, gets a
    "Section path: (preamble)" breadcrumb under ``--breadcrumb``. The branch
    is still implemented (rather than assuming non-empty) for byte-identical
    parity with Syndai's own check and for any future caller that CAN pass
    an empty list."""
    if not heading_path:
        return ""
    return "Section path: " + " > ".join(heading_path) + "\n\n"


def git_commit(root: Path) -> str:
    result = subprocess.run(
        ["git", "-C", str(root), "rev-parse", "HEAD"],
        capture_output=True,
        text=True,
    )
    return result.stdout.strip() if result.returncode == 0 else "unknown"


def list_corpus_files(root: Path) -> list[str]:
    """Sorted git-tracked corpus paths relative to root; tracked-only
    auto-excludes gitignored/generated files, superpowers dropped explicitly."""
    result = subprocess.run(
        ["git", "-C", str(root), "ls-files", *CORPUS_GLOBS],
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        raise RuntimeError(f"git ls-files failed in {root}: {result.stderr.strip()}")
    files = [
        line
        for line in result.stdout.splitlines()
        if line.strip()
        and not any(line.startswith(prefix) for prefix in EXCLUDE_PREFIXES)
    ]
    return sorted(set(files))


def bucket_of(rel_path: str) -> str:
    """Top-level docs directory bucket (docs/FOO.md -> 'root',
    docs/specs/... -> 'specs')."""
    parts = rel_path.split("/")
    if len(parts) <= 2:
        return "root"
    return parts[1]


def slugify(text: str) -> str:
    slug = re.sub(r"[^a-z0-9]+", "-", text.lower()).strip("-")
    return slug or "section"


class Section:
    __slots__ = ("rel_path", "bucket", "heading_path", "char_start", "char_end", "body")

    def __init__(self, rel_path, bucket, heading_path, char_start, char_end, body):
        self.rel_path = rel_path
        self.bucket = bucket
        self.heading_path = heading_path
        self.char_start = char_start
        self.char_end = char_end
        self.body = body

    @property
    def content(self) -> str:
        """Section text without the heading line itself."""
        newline = self.body.find("\n")
        return self.body[newline + 1 :] if newline != -1 else ""

    @property
    def content_char_start(self) -> int:
        newline = self.body.find("\n")
        return self.char_start + (newline + 1 if newline != -1 else 0)

    def key(self) -> str:
        return f"{self.rel_path}::{' > '.join(self.heading_path)}::{self.char_start}"

    def uri(self) -> str:
        """Stable per-section URI used as the MemPhant resource uri and the
        Syndai source name: doc://path#h1--h2--h3."""
        anchor = "--".join(slugify(h) for h in self.heading_path)
        return f"doc://{self.rel_path}#{anchor}::{self.char_start}"


def parse_sections(rel_path: str, text: str) -> list[Section]:
    """Split a markdown file into leaf sections: each heading plus its own text
    up to the next heading of ANY level. Records absolute char spans and the
    full ancestor heading breadcrumb. Fenced code blocks are respected so a
    ``#`` inside a fence is never read as a heading. Any preamble before the
    first heading is emitted as a headless '(preamble)' section so no corpus
    text is dropped from the haystack."""
    bucket = bucket_of(rel_path)
    lines = text.split("\n")
    offsets: list[int] = []
    running = 0
    for line in lines:
        offsets.append(running)
        running += len(line) + 1

    heading_stack: list[tuple[int, str]] = []
    sections: list[Section] = []
    in_fence = False
    fence_marker = ""
    pending_start_line: int | None = None
    pending_path: list[str] = []

    def emit(start_line: int, end_line: int, path: list[str]) -> None:
        char_start = offsets[start_line]
        char_end = offsets[end_line] if end_line < len(offsets) else len(text)
        body = text[char_start:char_end].rstrip("\n")
        if body.strip():
            sections.append(
                Section(rel_path, bucket, list(path), char_start, char_end, body)
            )

    # Preamble before the first heading.
    first_heading_line = None
    scan_fence = False
    scan_marker = ""
    for i, line in enumerate(lines):
        stripped = line.lstrip()
        if stripped.startswith("```") or stripped.startswith("~~~"):
            marker = stripped[:3]
            if not scan_fence:
                scan_fence, scan_marker = True, marker
            elif marker == scan_marker:
                scan_fence, scan_marker = False, ""
            continue
        if scan_fence:
            continue
        if HEADING_RE.match(line):
            first_heading_line = i
            break
    if first_heading_line is None:
        emit(0, len(lines), ["(preamble)"])
        return sections
    if first_heading_line > 0:
        emit(0, first_heading_line, ["(preamble)"])

    for i, line in enumerate(lines):
        stripped = line.lstrip()
        if stripped.startswith("```") or stripped.startswith("~~~"):
            marker = stripped[:3]
            if not in_fence:
                in_fence, fence_marker = True, marker
            elif marker == fence_marker:
                in_fence, fence_marker = False, ""
            continue
        if in_fence:
            continue
        match = HEADING_RE.match(line)
        if not match:
            continue
        level = len(match.group(1))
        title = match.group(2).strip()
        if pending_start_line is not None:
            emit(pending_start_line, i, pending_path)
        while heading_stack and heading_stack[-1][0] >= level:
            heading_stack.pop()
        heading_stack.append((level, title))
        pending_start_line = i
        pending_path = [t for _, t in heading_stack]

    if pending_start_line is not None:
        emit(pending_start_line, len(lines), pending_path)
    return sections


def all_sections(root: Path, files: list[str]) -> list[Section]:
    out: list[Section] = []
    for rel in files:
        text = (root / rel).read_text(encoding="utf-8", errors="replace")
        out.extend(parse_sections(rel, text))
    return out


def candidate_sections(sections: list[Section]) -> list[Section]:
    """Sections substantial enough to mine a verbatim-span question from."""
    out = []
    for section in sections:
        content = section.content.strip()
        if len(content) < SECTION_MIN_CHARS:
            continue
        if len(section.body) > SECTION_MAX_CHARS:
            continue
        out.append(section)
    return out


def corpus_revision(sections: list[Section]) -> str:
    """Content-and-boundary pin shared by both docs-gate runners."""
    digest = hashlib.sha256()
    for section in sections:
        row = [
            section.rel_path,
            section.heading_path,
            section.char_start,
            section.char_end,
            section.body,
        ]
        encoded = json.dumps(row, ensure_ascii=False, separators=(",", ":")).encode()
        digest.update(len(encoded).to_bytes(8, "big"))
        digest.update(encoded)
    return "sha256:" + digest.hexdigest()


def verify_corpus_contract(root: Path, files: list[str], manifest: dict) -> list[Section]:
    """Verify the exact full docs input and return all parsed leaf sections.

    ``candidate_sections`` is intentionally absent: its size filter is only a
    golden-mining eligibility rule, never an indexing rule.
    """
    expected_files = manifest.get("files")
    if not isinstance(expected_files, dict) or set(files) != set(expected_files):
        raise RuntimeError("corpus file set mismatch: refusing skipped, missing, or new files")
    if manifest.get("file_count") != len(files):
        raise RuntimeError("corpus file count mismatch")
    total_bytes = 0
    for rel in files:
        path = root / rel
        if not path.is_file():
            raise RuntimeError(f"corpus file missing: {rel}")
        data = path.read_bytes()
        expected = expected_files[rel]
        if len(data) != expected.get("bytes") or sha256_hex(data) != expected.get("sha256"):
            raise RuntimeError(f"corpus content mismatch: {rel}")
        total_bytes += len(data)
    if total_bytes != manifest.get("total_bytes"):
        raise RuntimeError("corpus total byte count mismatch")
    if manifest.get("sectionizer") != "markdown_heading_leaf_v1":
        raise RuntimeError("unsupported corpus sectionizer")

    sections = all_sections(root, files)
    if len(sections) != manifest.get("section_count"):
        raise RuntimeError("corpus section count mismatch")
    if sum(len(section.body) for section in sections) != manifest.get("section_chars"):
        raise RuntimeError("corpus section character count mismatch")
    actual_revision = corpus_revision(sections)
    if actual_revision != manifest.get("section_revision"):
        raise RuntimeError("corpus section revision mismatch")
    return sections


def load_pinned_corpus(
    root: Path, manifest_path: Path = CORPUS_MANIFEST_PATH
) -> tuple[list[str], list[Section], dict]:
    manifest = json.loads(manifest_path.read_text())
    files = list_corpus_files(root)
    sections = verify_corpus_contract(root, files, manifest)
    return files, sections, manifest


# --- golden set + provenance -------------------------------------------------


def load_goldens(path: Path) -> list[dict]:
    return [
        json.loads(line)
        for line in path.read_text().split("\n")
        if line.strip()
    ]


NEGATIVE_CASE_KINDS = {
    "unrelated",
    "lexical_collision",
    "plausible_absent",
    "wrong_tenant",
    "wrong_user",
    "wrong_project",
    "wrong_agent",
    "post_snapshot",
    "stale_superseded_only",
    "answerable_but_unsupported",
}
NEGATIVE_SCOPES = {
    "active",
    "other_tenant",
    "other_user",
    "other_project",
    "other_agent",
}


def load_negative_cases(
    path: Path,
    lock_path: Path,
    *,
    disjoint_question_ids: set[str] | None = None,
) -> list[dict]:
    """Load the exposed negative slice and reject any schema or lock drift."""
    lock = json.loads(lock_path.read_text())
    positive_dir = MEMPHANT_ROOT / "benchmarks" / "data"
    positive_locks = {
        "v1": file_sha256(positive_dir / "syndai_docs_golden.lock.json"),
        "v2": file_sha256(positive_dir / "syndai_docs_golden_v2.lock.json"),
    }
    corpus_revision = json.loads(CORPUS_MANIFEST_PATH.read_text())["section_revision"]
    if lock != {
        "schema": "syndai-docs-negative-v1",
        "created_at": "2026-07-13T00:00:00Z",
        "provenance": "hand-authored exposed-development synthetic restraint cases",
        "semantic_disjointness": (
            "synthetic queries are distinct from positive questions; unique nonsense "
            "canaries are absent from positive goldens"
        ),
        "corpus_revision": corpus_revision,
        "positive_lock_sha256": positive_locks,
        "sha256": file_sha256(path),
        "count": 10,
        "case_kinds": sorted(NEGATIVE_CASE_KINDS),
    }:
        raise ValueError("negative slice lock is missing, stale, or malformed")
    rows = load_goldens(path)
    if len(rows) != lock["count"]:
        raise ValueError("negative slice count does not match lock")

    ids: set[str] = set()
    canaries: set[str] = set()
    seen_kinds: set[str] = set()
    for row in rows:
        if set(row) != {
            "case_id", "case_kind", "ingest", "query", "gold", "forbidden", "expect"
        }:
            raise ValueError("negative case has unknown or missing fields")
        case_id = row["case_id"]
        case_kind = row["case_kind"]
        if (
            not isinstance(case_id, str)
            or not re.fullmatch(r"syndai_docs_neg_[a-z0-9_]+", case_id)
            or case_id in ids
        ):
            raise ValueError("negative case has an invalid or duplicate case_id")
        if case_kind not in NEGATIVE_CASE_KINDS or case_kind in seen_kinds:
            raise ValueError("negative case inventory is invalid or duplicated")
        ids.add(case_id)
        seen_kinds.add(case_kind)

        query = row["query"]
        if set(query) != {"question", "transaction_as_of", "valid_at"} or not isinstance(
            query["question"], str
        ) or not query["question"].strip():
            raise ValueError(f"negative case {case_id} has an invalid query")
        for field in ("transaction_as_of", "valid_at"):
            if query[field] is not None and not re.fullmatch(
                r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z", str(query[field])
            ):
                raise ValueError(f"negative case {case_id} has invalid {field}")

        if row["gold"] != {"answer": "ABSTAIN"} or row["expect"] != {
            "abstain": True,
            "max_forbidden_hits": 0,
        }:
            raise ValueError(f"negative case {case_id} has invalid evaluator labels")
        forbidden = row["forbidden"]
        if (
            not isinstance(forbidden, list)
            or len(forbidden) != 1
            or not re.fullmatch(r"MPH_NEG_[A-Z0-9_]+", str(forbidden[0]))
            or forbidden[0] in canaries
        ):
            raise ValueError(f"negative case {case_id} has invalid or duplicate canaries")
        canaries.add(forbidden[0])

        documents = row["ingest"]
        if not isinstance(documents, list):
            raise ValueError(f"negative case {case_id} ingest must be a list")
        for document in documents:
            if set(document) != {
                "document_id", "body", "scope", "valid_from", "valid_to"
            }:
                raise ValueError(f"negative case {case_id} has malformed ingest fields")
            if (
                not isinstance(document["document_id"], str)
                or not document["document_id"]
                or not isinstance(document["body"], str)
                or not document["body"].strip()
                or document["scope"] not in NEGATIVE_SCOPES
            ):
                raise ValueError(f"negative case {case_id} has malformed ingest content")
            for field in ("valid_from", "valid_to"):
                if document[field] is not None and not re.fullmatch(
                    r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z", str(document[field])
                ):
                    raise ValueError(f"negative case {case_id} has invalid {field}")
    if seen_kinds != NEGATIVE_CASE_KINDS:
        raise ValueError("negative case inventory is incomplete")
    if disjoint_question_ids and ids.intersection(disjoint_question_ids):
        raise ValueError("negative case IDs overlap positive question IDs")
    by_kind = {row["case_kind"]: row for row in rows}
    scoped = {
        "unrelated": "active",
        "lexical_collision": "active",
        "wrong_tenant": "other_tenant",
        "wrong_user": "other_user",
        "wrong_project": "other_project",
        "wrong_agent": "other_agent",
        "post_snapshot": "active",
        "stale_superseded_only": "active",
    }
    for kind, scope in scoped.items():
        case = by_kind[kind]
        if (
            len(case["ingest"]) != 1
            or case["ingest"][0]["scope"] != scope
            or case["forbidden"][0] not in case["ingest"][0]["body"]
        ):
            raise ValueError(f"negative {kind} ingest semantics are invalid")
    for kind in ("plausible_absent", "answerable_but_unsupported"):
        if by_kind[kind]["ingest"]:
            raise ValueError(f"negative {kind} must have no supporting ingest")
    if any(
        case["query"]["transaction_as_of"] is not None
        for kind, case in by_kind.items()
        if kind != "post_snapshot"
    ):
        raise ValueError("only post_snapshot may set transaction_as_of")
    post_snapshot = by_kind["post_snapshot"]
    if post_snapshot["query"]["transaction_as_of"] is None:
        raise ValueError("post_snapshot requires a historical transaction_as_of")
    stale = by_kind["stale_superseded_only"]
    if (
        stale["ingest"][0]["valid_to"] is None
        or stale["query"]["valid_at"] is None
        or stale["query"]["valid_at"] <= stale["ingest"][0]["valid_to"]
    ):
        raise ValueError("stale_superseded_only requires an expired valid interval")
    significant = lambda value: {word for word in re.findall(r"[a-z]+", value.lower()) if len(word) >= 6}
    unrelated_overlap = significant(by_kind["unrelated"]["query"]["question"]) & significant(
        by_kind["unrelated"]["ingest"][0]["body"]
    )
    lexical_overlap = significant(by_kind["lexical_collision"]["query"]["question"]) & significant(
        by_kind["lexical_collision"]["ingest"][0]["body"]
    )
    if unrelated_overlap or len(lexical_overlap) < 3:
        raise ValueError("unrelated and lexical_collision semantics are invalid")
    positive_rows = [
        row
        for filename in ("syndai_docs_golden.jsonl", "syndai_docs_golden_v2.jsonl")
        for row in load_goldens(positive_dir / filename)
    ]
    positive_questions = {normalize(row["question"]) for row in positive_rows}
    positive_bytes = b"".join(
        (positive_dir / filename).read_bytes()
        for filename in ("syndai_docs_golden.jsonl", "syndai_docs_golden_v2.jsonl")
    )
    if any(normalize(row["query"]["question"]) in positive_questions for row in rows) or any(
        canary.encode() in positive_bytes for canary in canaries
    ):
        raise ValueError("negative slice is not semantically disjoint from positive goldens")
    return rows


def negative_ingest_projection(case: dict) -> list[dict]:
    """Return runtime ingest inputs only; evaluator labels stay out."""
    return [
        {
            key: document[key]
            for key in ("document_id", "body", "scope", "valid_from", "valid_to")
        }
        for document in case["ingest"]
    ]


def negative_query_projection(case: dict) -> dict:
    """Return runtime query inputs only; evaluator labels stay out."""
    return {
        key: case["query"][key]
        for key in ("question", "transaction_as_of", "valid_at")
    }


def negative_result_row(
    case: dict,
    evidence_bodies: list[str],
    *,
    supported: bool,
    unsupported_reason: str | None = None,
) -> dict:
    if supported == (unsupported_reason is not None):
        raise ValueError("negative runtime support and unsupported_reason disagree")
    forbidden_hits = sum(
        1
        for forbidden in case["forbidden"]
        if any(forbidden in body for body in evidence_bodies)
    )
    return {
        "case_id": case["case_id"],
        "case_kind": case["case_kind"],
        "supported": supported,
        "unsupported_reason": unsupported_reason,
        "forbidden_hits": forbidden_hits,
        "passed": supported and forbidden_hits <= case["expect"]["max_forbidden_hits"],
    }


def negative_evidence_row(case: dict, evidence_bodies: list[str], *, k: int) -> dict:
    items = [
        {"rank": rank + 1, "session_id": None, "body": body}
        for rank, body in enumerate(evidence_bodies[:k])
    ]
    return {
        "question_id": case["case_id"],
        "question_type": case["case_kind"],
        "is_abstention": True,
        "question": case["query"]["question"],
        "question_date": case["query"]["valid_at"],
        "gold_answer": case["gold"]["answer"],
        "abstained": len(items) == 0,
        "granularity": "section",
        "k": k,
        "evidence": items,
    }


def negative_report(rows: list[dict]) -> dict:
    unsupported = sum(row["supported"] is not True for row in rows)
    forbidden_hits = sum(row["forbidden_hits"] for row in rows)
    return {
        "negative_case_count": len(rows),
        "negative_forbidden_hit_count": forbidden_hits,
        "negative_forbidden_hit_rate": forbidden_hits / len(rows) if rows else 0.0,
        "negative_unsupported_count": unsupported,
        "negative_promotion_eligible": bool(rows) and unsupported == 0 and all(
            row["passed"] is True for row in rows
        ),
        "negative_per_case": rows,
    }


def required_spans(golden: dict) -> list[str]:
    """The verbatim spans a retrieval engine must surface for this question:
    the single answer span, or (multi-hop) the bridge span AND the answer
    span."""
    return [entry["span"] for entry in golden["provenance"]]


def provenance_hit(golden: dict, evidence_bodies: list[str], k: int) -> bool:
    """Span-level provenance hit within the top-k retrieved bodies, graded
    identically for every engine: a required span counts as covered if ANY
    top-k body contains it (normalized word-boundary containment, the same
    function run_reader uses to grade answers). Single-hop hits when its one
    span is covered; multi-hop hits only when BOTH required spans are covered
    across the union of the top-k bodies."""
    top = evidence_bodies[:k]
    for span in required_spans(golden):
        if not any(contains_gold(body, span) for body in top):
            return False
    return True


def evidence_row(golden: dict, evidence_bodies: list[str], k: int) -> dict:
    """Assemble the run_reader.py evidence-row for a golden + retrieved bodies.
    Matches the QaEvidenceRow contract that bench_lme emits (question_id,
    question_type, is_abstention, question, question_date, gold_answer,
    abstained, granularity, k, evidence[{rank, session_id, body}])."""
    items = [
        {"rank": rank + 1, "session_id": None, "body": body}
        for rank, body in enumerate(evidence_bodies[:k])
    ]
    return {
        "question_id": golden["question_id"],
        "question_type": golden["question_type"],
        "is_abstention": golden["is_abstention"],
        "question": golden["question"],
        "question_date": golden.get("question_date"),
        "gold_answer": golden["gold_answer"],
        "abstained": len(items) == 0,
        "granularity": "section",
        "k": k,
        "evidence": items,
    }


EVIDENCE_PACKER_CONFIG = {
    "name": "rank_order_whitespace_prefix_v1",
    "tokenizer": "unicode_non_whitespace_runs",
    "truncation": "truncate_first_overflow_then_stop",
}
EVIDENCE_PACKER_CONFIG["sha256"] = json_fingerprint(EVIDENCE_PACKER_CONFIG)


def pack_evidence(
    evidence_bodies: list[str], *, k: int, budget_tokens: int
) -> tuple[list[str], dict]:
    if type(k) is not int or k <= 0 or type(budget_tokens) is not int or budget_tokens <= 0:
        raise ValueError("evidence pack k and budget_tokens must be positive integers")
    packed: list[str] = []
    used = 0
    truncated = 0
    considered = evidence_bodies[:k]
    for body in considered:
        spans = list(re.finditer(r"\S+", body))
        remaining = budget_tokens - used
        if remaining <= 0:
            break
        if len(spans) <= remaining:
            packed.append(body)
            used += len(spans)
            continue
        packed.append(body[: spans[remaining - 1].end()])
        used += remaining
        truncated = 1
        break
    return packed, {
        "evidence_packer_sha256": EVIDENCE_PACKER_CONFIG["sha256"],
        "evidence_budget_tokens": budget_tokens,
        "evidence_packed_tokens": used,
        "evidence_truncated_items": truncated,
        "evidence_dropped_items": len(considered) - len(packed),
    }


def write_jsonl(path: Path, rows: list[dict]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    lines = [json.dumps(row, ensure_ascii=False) for row in rows]
    path.write_text("\n".join(lines) + ("\n" if lines else ""))

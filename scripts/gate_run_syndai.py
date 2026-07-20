#!/usr/bin/env python3
"""Syndai (incumbent) engine runner for the Syndai replacement gate (W10).

Ingests the pinned Syndai docs corpus through Syndai's REAL knowledge pipeline
(``prepare_content_chunks`` -> ``embed_and_store_chunks``: the production
sectionizer + 500tok/75 chunker + text-embedding-3-small@1536 + HNSW/GIN index)
into a LOCAL dev Postgres, then runs the production hybrid search
(``search_knowledge_detached``: HNSW + BM25 + RRF K=60, optional Jina rerank)
top-k=10 per golden question and emits the SAME two artifacts the MemPhant
runner does:
- an evidence JSONL in ``scripts/run_reader.py`` shape (so the same reader/judge
  scores both engines);
- a provenance report with per-question hit@5/hit@10 + R@5/R@10, graded by
  ``gate_common.provenance_hit`` (identical grading for both engines).

MUST be run under the Syndai backend interpreter with Doppler (for
OPENAI_API_KEY / JINA_API_KEY and the rest of ``get_settings()``) and the
DATABASE_URL overridden to the LOCAL dev DB — NEVER the Supabase project:

    cd /Users/sidsharma/Syndai/backend && \
    doppler run --project syndai --config dev -- \
      env DATABASE_URL="postgresql://syndai:syndai@127.0.0.1:55432/syndai_local" \
      uv run python /Users/sidsharma/Memphant/scripts/gate_run_syndai.py \
        --out-evidence ... --out-provenance ...

Ingest granularity: the common corpus contract emits every pinned markdown leaf
section. Syndai feeds each section through its real production sectionizer and
chunker; MemPhant retains the same section body as a resource. Both therefore
index identical source content and are graded by the same span-containment
metric, while preserving their distinct internal chunking policies.

The Syndai repo is used strictly as-is — nothing is written to it. Only the
local dev DB is populated (a throwaway); no Supabase, no schema changes.
"""

from __future__ import annotations

import argparse
import asyncio
import json
import os
import statistics
import sys
import time
from datetime import UTC, datetime
from pathlib import Path
from unittest.mock import patch
from uuid import uuid4

MEMPHANT_SCRIPTS = "/Users/sidsharma/Memphant/scripts"
sys.path.insert(0, MEMPHANT_SCRIPTS)
import gate_common as gc  # noqa: E402

# Syndai backend imports (available under its .venv / uv run).
from sqlalchemy.exc import IntegrityError  # noqa: E402
from sqlalchemy.ext.asyncio import AsyncSession, create_async_engine  # noqa: E402

import src.model_registry  # noqa: E402,F401  (register every ORM model)
from sqlalchemy.orm import configure_mappers  # noqa: E402

configure_mappers()  # resolve all column types before the factories introspect

from src.config import get_settings  # noqa: E402
from src.features.agents.models import Agent  # noqa: E402
from src.features.knowledge.models import (  # noqa: E402
    AgentKnowledgeSource,
    KnowledgeSource,
    KnowledgeSourceVersion,
)
from src.features.knowledge.processing_chunks import (  # noqa: E402
    embed_and_store_chunks,
    prepare_content_chunks,
)
from src.features.knowledge.search_detached import search_knowledge_detached  # noqa: E402
from src.features.knowledge.trace_context import KnowledgeSearchTraceContext  # noqa: E402
from src.infrastructure.db import (  # noqa: E402
    psycopg_async_database_url,
    setup_checkout_hook,
)
from tests.fixtures.factories import ProjectFactory, UserFactory  # noqa: E402

DEFAULT_SYNDAI_ROOT = Path("/Users/sidsharma/Syndai")
GOLDEN_PATH = gc.MEMPHANT_ROOT / "benchmarks" / "data" / "syndai_docs_golden.jsonl"
RECALL_E2E_P95_CEILING_MS = 1500
SYNDAI_SUPPORTED_NEGATIVE_KINDS = {
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
SYNDAI_SCOPE_ADAPTER_MAPPING = {
    "wrong_tenant": "user_id adapter mapping (Syndai user is the tenant boundary)",
    "wrong_user": "user_id",
    "wrong_project": "project_id",
    "wrong_agent": "agent_id",
    "post_snapshot": "snapshot_at",
    "stale_superseded_only": "natural-language dated query (no valid_at parameter)",
}


class RerankObserver:
    def __init__(self) -> None:
        self.events: list[tuple[str, str, dict]] = []

    def info(self, event: str, **facts) -> None:
        self.events.append(("info", event, facts))

    def warning(self, event: str, **facts) -> None:
        self.events.append(("warning", event, facts))

    def events_for(self, trace_id: str) -> list[tuple[str, str, dict]]:
        return [event for event in self.events if event[2].get("trace_id") == trace_id]


async def _ingest_one(
    session,
    agent,
    user,
    rel: str,
    text: str,
    *,
    with_sections: bool = True,
    project_id=None,
) -> int:
    """Ingest one file as a KnowledgeSource via the real pipeline; returns its
    chunk count. ``with_sections=False`` is the steelman fallback for a latent
    Syndai bug: their ``_add_sections_and_edges`` inserts KnowledgeSectionEdge
    rows referencing ``section.id`` BEFORE the sections are flushed, so any
    document whose sectionizer emits cross-reference edges dies with
    NotNullViolation (11/108 real product docs; their eval fixture sidesteps it
    with explicit uuid4 ids). The fallback stores chunks WITHOUT the section
    tree — chunks keep heading_hierarchy/context prefixes and stay fully
    searchable; only structure expansion is lost for those files."""
    source = KnowledgeSource(
        id=uuid4(),
        user_id=user.id,
        name=rel,
        source_type="text",
        raw_content=text,
        char_count=len(text),
        status="ready",
        processed_at=datetime.now(UTC),
        project_id=project_id,
    )
    session.add(source)
    await session.flush()
    version = KnowledgeSourceVersion(
        id=uuid4(),
        knowledge_source_id=source.id,
        status="ready",
        processed_at=datetime.now(UTC),
        chunk_count=0,
        refresh_kind="initial",
    )
    session.add(version)
    await session.flush()
    prepared = prepare_content_chunks(source.id, text)
    await embed_and_store_chunks(
        session,
        source,
        prepared.chunks,
        version=version,
        language="en",
        sectionized_document=prepared.sectionized_document if with_sections else None,
    )
    version.chunk_count = len(prepared.chunks)
    source.active_version_id = version.id
    session.add(
        AgentKnowledgeSource(
            id=uuid4(),
            agent_id=agent.id,
            knowledge_source_id=source.id,
            attached_at=datetime.now(UTC),
        )
    )
    await session.flush()
    return len(prepared.chunks)


async def ingest_corpus(
    engine, sections: list[gc.Section], negative_cases: list[dict] | None = None
) -> tuple[str, str, str, int, int, list[str], list[str]]:
    """Ingest each pinned section as a KnowledgeSource via the real pipeline.
    Returns (agent_id, user_id, source_count, chunk_count, fallback_sections,
    skipped_sections). A section that trips the upstream section-edge bug is
    retried chunks-only (see _ingest_one); any second failure is reported to
    the caller, which rejects the run before search."""
    total_chunks = 0
    sources = 0
    async with AsyncSession(engine, expire_on_commit=False) as session:
        user = UserFactory.build()
        session.add(user)
        await session.flush()
        project = ProjectFactory.build(user_id=user.id)
        session.add(project)
        await session.flush()
        agent = Agent(
            id=uuid4(),
            owner_id=user.id,
            project_id=project.id,
            name="Syndai Gate Agent",
            system_name="syndai_gate_agent",
            description="Agent for the engine-vs-engine docs gate",
            instructions="You answer questions from your knowledge base.",
            level=0,
        )
        session.add(agent)
        await session.flush()
        agent_id, user_id, project_id = str(agent.id), str(user.id), str(project.id)

        other_project = ProjectFactory.build(user_id=user.id)
        session.add(other_project)
        await session.flush()
        other_agent = Agent(
            id=uuid4(),
            owner_id=user.id,
            project_id=project.id,
            name="Syndai Gate Other Agent",
            system_name=f"syndai_gate_other_{uuid4().hex[:8]}",
            description="Foreign-agent negative gate fixture",
            instructions="Isolation fixture.",
            level=0,
        )
        session.add(other_agent)
        foreign_user = UserFactory.build()
        session.add(foreign_user)
        await session.flush()
        foreign_project = ProjectFactory.build(user_id=foreign_user.id)
        session.add(foreign_project)
        await session.flush()
        foreign_agent = Agent(
            id=uuid4(),
            owner_id=foreign_user.id,
            project_id=foreign_project.id,
            name="Syndai Gate Foreign User Agent",
            system_name=f"syndai_gate_foreign_{uuid4().hex[:8]}",
            description="Foreign-user negative gate fixture",
            instructions="Isolation fixture.",
            level=0,
        )
        session.add(foreign_agent)
        await session.flush()

        skipped: list[str] = []
        fallback: list[str] = []
        for i, section in enumerate(sections):
            rel = section.uri()
            text = section.body
            savepoint = await session.begin_nested()
            try:
                n_chunks = await _ingest_one(session, agent, user, rel, text)
                await savepoint.commit()
            except IntegrityError:
                await savepoint.rollback()
                retry_savepoint = await session.begin_nested()
                try:
                    n_chunks = await _ingest_one(
                        session, agent, user, rel, text, with_sections=False
                    )
                    await retry_savepoint.commit()
                    fallback.append(rel)
                    print(f"  FALLBACK (chunks-only) {rel}", file=sys.stderr)
                except Exception as exc:  # noqa: BLE001
                    await retry_savepoint.rollback()
                    skipped.append(f"{rel}: {type(exc).__name__}: {exc}")
                    print(f"  SKIP {rel}: {exc}", file=sys.stderr)
                    continue
            except Exception as exc:  # noqa: BLE001
                await savepoint.rollback()
                skipped.append(f"{rel}: {type(exc).__name__}: {exc}")
                print(f"  SKIP {rel}: {exc}", file=sys.stderr)
                continue
            total_chunks += n_chunks
            sources += 1
            if (i + 1) % 20 == 0:
                print(
                    f"  ingested {i + 1}/{len(sections)} sections, "
                    f"{total_chunks} chunks",
                    file=sys.stderr,
                )

        if fallback:
            print(f"  ingest used chunks-only fallback for {len(fallback)} files", file=sys.stderr)
        if skipped:
            print(f"  ingest skipped {len(skipped)} files", file=sys.stderr)
        for case in negative_cases or []:
            if case["case_kind"] not in SYNDAI_SUPPORTED_NEGATIVE_KINDS:
                continue
            for document in gc.negative_ingest_projection(case):
                target_agent, target_user, target_project = agent, user, None
                if document["scope"] == "other_agent":
                    target_agent = other_agent
                elif document["scope"] in ("other_user", "other_tenant"):
                    target_agent, target_user = foreign_agent, foreign_user
                elif document["scope"] == "other_project":
                    target_project = other_project.id
                await _ingest_one(
                    session,
                    target_agent,
                    target_user,
                    f"memphant://negative/{document['document_id']}",
                    document["body"],
                    project_id=target_project,
                )
        # Commit so the detached search (its own sessions) can see the rows.
        await session.commit()
    return agent_id, user_id, project_id, sources, total_chunks, fallback, skipped


async def run(args) -> int:
    import uuid as _uuid

    golden_path = Path(args.golden)
    goldens = gc.load_goldens(golden_path)
    # Lock path is derived FROM --golden (gc.golden_lock_path), not a
    # hardcoded v1-only constant, so a v2 (or future vN) --golden run
    # verifies against ITS OWN lock file instead of silently checking v1's.
    lock = json.loads(gc.golden_lock_path(golden_path).read_text())
    actual_sha = gc.sha256_hex(golden_path.read_bytes())
    if actual_sha != lock["sha256"]:
        raise RuntimeError(
            f"golden sha256 mismatch: file={actual_sha[:12]} lock={lock['sha256'][:12]}"
        )
    print(f"goldens={len(goldens)} sha256={actual_sha[:12]} (lock verified)", file=sys.stderr)
    if bool(args.negative_slice) != bool(args.out_negative_evidence):
        raise RuntimeError("--negative-slice and --out-negative-evidence must be supplied together")
    negative_cases = None
    if args.negative_slice:
        negative_path = Path(args.negative_slice)
        negative_cases = gc.load_negative_cases(
            negative_path,
            gc.golden_lock_path(negative_path),
            disjoint_question_ids={golden["question_id"] for golden in goldens},
        )

    root = Path(args.syndai_root)
    files, corpus_sections, corpus_manifest = gc.load_pinned_corpus(root)
    if args.limit_files:
        raise RuntimeError("--limit-files violates the full common-corpus contract")
    print(
        f"corpus files={len(files)} sections={len(corpus_sections)} "
        f"revision={corpus_manifest['section_revision'][:19]}",
        file=sys.stderr,
    )

    settings = get_settings()
    db_url = str(settings.database_url)
    scratch_name = os.environ.get(gc.DISPOSABLE_DATABASE_ENV)
    if scratch_name is None:
        scratch_name = f"syndai_gate_{uuid4().hex}"
        return gc.run_in_disposable_database(
            db_url,
            scratch_name,
            [sys.executable, str(Path(__file__).resolve()), *sys.argv[1:]],
        )
    gc.validate_disposable_database_child(db_url, scratch_name)
    if "supabase.com" in db_url:
        raise RuntimeError(
            "refusing to run against Supabase; use a local disposable database"
        )
    engine = create_async_engine(
        psycopg_async_database_url(db_url),
        connect_args={"prepare_threshold": None},
        pool_pre_ping=True,
    )
    setup_checkout_hook(engine, settings, application_name="syndai-gate")

    try:
        t0 = time.time()
        (
            agent_id,
            user_id,
            project_id,
            n_sources,
            n_chunks,
            fallback_sections,
            skipped_sections,
        ) = await ingest_corpus(engine, corpus_sections, negative_cases)
        print(
            f"ingest done: sources={n_sources} chunks={n_chunks} "
            f"fallback={len(fallback_sections)} skipped={len(skipped_sections)} "
            f"in {time.time() - t0:.1f}s",
            file=sys.stderr,
        )
        if skipped_sections or n_sources != len(corpus_sections):
            raise RuntimeError(
                f"Syndai did not index the full pinned corpus: "
                f"sources={n_sources}/{len(corpus_sections)} "
                f"skipped={len(skipped_sections)}"
            )

        reranking = bool(settings.jina_api_key)
        if not reranking:
            raise RuntimeError("Syndai gate requires its production Jina reranker configuration")
        evidence_rows = []
        provenance_rows = []
        negative_evidence = []
        negative_rows = []
        rerank_observer = RerankObserver()
        with patch(
            "src.features.knowledge.search_finalize.logger",
            rerank_observer,
        ):
            for i, golden in enumerate(goldens):
                search_trace_id = "memphant-gate-" + gc.sha256_hex(
                    golden["question_id"].encode()
                )[:24]
                recall_started = time.perf_counter()
                try:
                    results = await search_knowledge_detached(
                        engine=engine,
                        agent_id=_uuid.UUID(agent_id),
                        user_id=_uuid.UUID(user_id),
                        query=golden["question"],
                        search_credits=0,
                        top_k=args.k,
                        min_score=0.0,
                        trace_context=KnowledgeSearchTraceContext(trace_id=search_trace_id),
                    )
                    bodies = [r.content for r in results]
                    error = None
                except Exception as exc:  # noqa: BLE001
                    bodies = []
                    error = f"{type(exc).__name__}: {exc}"
                    print(f"  search error {golden['question_id']}: {error}", file=sys.stderr)
                rerank_events = rerank_observer.events_for(search_trace_id)
                complete_events = [event for event in rerank_events if event[1] == "knowledge_search_rerank_complete"]
                if error is None and len(complete_events) != 1:
                    names = [event[1] for event in rerank_events]
                    raise RuntimeError(
                        f"Syndai reranker success was not observed for {golden['question_id']}: {names}"
                    )
                recall_e2e_ms = int((time.perf_counter() - recall_started) * 1000)
                packed_bodies, pack_facts = gc.pack_evidence(
                    bodies, k=args.k, budget_tokens=args.budget_tokens
                )
                evidence_rows.append(gc.evidence_row(golden, packed_bodies, args.k))
                provenance_rows.append(
                    {
                        "question_id": golden["question_id"],
                        "question_type": golden["question_type"],
                        "multi_hop": golden["multi_hop"],
                        "returned_items": len(bodies),
                        "packed_items": len(packed_bodies),
                        "search_error": error,
                        "recall_e2e_ms": recall_e2e_ms,
                        "degraded": False,
                        "fallback": False,
                        "skipped": False,
                        "failure": "none" if error is None else "search_error",
                        "reranker_configured": reranking,
                        "reranker_observed": len(complete_events) == 1,
                        "hit_at_5": gc.provenance_hit(golden, packed_bodies, 5),
                        "hit_at_10": gc.provenance_hit(golden, packed_bodies, min(10, args.k)),
                    }
                    | pack_facts
                )
                if (i + 1) % 20 == 0:
                    print(f"  searched {i + 1}/{len(goldens)}", file=sys.stderr)
            for case in negative_cases or []:
                supported = case["case_kind"] in SYNDAI_SUPPORTED_NEGATIVE_KINDS
                raw_bodies = []
                if supported:
                    query = gc.negative_query_projection(case)
                    trace_id = "memphant-negative-" + gc.sha256_hex(
                        case["case_id"].encode()
                    )[:24]
                    results = await search_knowledge_detached(
                        engine=engine,
                        agent_id=_uuid.UUID(agent_id),
                        user_id=_uuid.UUID(user_id),
                        query=query["question"],
                        search_credits=0,
                        top_k=args.k,
                        min_score=0.0,
                        snapshot_at=(
                            datetime.fromisoformat(
                                query["transaction_as_of"].replace("Z", "+00:00")
                            )
                            if query["transaction_as_of"]
                            else None
                        ),
                        project_id=_uuid.UUID(project_id),
                        trace_context=KnowledgeSearchTraceContext(trace_id=trace_id),
                    )
                    raw_bodies = [result.content for result in results]
                    complete = [
                        event for event in rerank_observer.events_for(trace_id)
                        if event[1] == "knowledge_search_rerank_complete"
                    ]
                    if len(complete) != 1:
                        raise RuntimeError(
                            f"Syndai reranker success was not observed for {case['case_id']}"
                        )
                bodies, _ = gc.pack_evidence(
                    raw_bodies, k=args.k, budget_tokens=args.budget_tokens
                )
                negative_evidence.append(gc.negative_evidence_row(case, bodies, k=args.k))
                negative_rows.append(
                    gc.negative_result_row(
                        case,
                        raw_bodies[: args.k],
                        supported=supported,
                        unsupported_reason=(
                            None if supported else "valid_time_contract_absent"
                        ),
                    )
                )
    finally:
        await engine.dispose()

    gc.write_jsonl(Path(args.out_evidence), evidence_rows)
    negative_summary = None
    if negative_cases:
        negative_evidence_path = Path(args.out_negative_evidence)
        gc.write_jsonl(negative_evidence_path, negative_evidence)
        negative_summary = gc.negative_report(negative_rows) | {
            "negative_evidence_sha256": gc.file_sha256(negative_evidence_path),
            "scope_adapter_mapping": SYNDAI_SCOPE_ADAPTER_MAPPING,
        }
    n = len(provenance_rows)
    r5 = sum(r["hit_at_5"] for r in provenance_rows) / n if n else 0.0
    r10 = sum(r["hit_at_10"] for r in provenance_rows) / n if n else 0.0
    latencies = [row["recall_e2e_ms"] for row in provenance_rows]
    p50 = float(statistics.median(latencies))
    p95 = gc._percentile([float(value) for value in latencies], 95)
    golden_revision = "sha256:" + actual_sha
    run_identity = gc.generation_identity(
        root=root,
        files={
            "runner": Path(__file__),
            "gate_common": Path(gc.__file__),
            "processing_chunks": root / "backend/src/features/knowledge/processing_chunks.py",
            "search_detached": root / "backend/src/features/knowledge/search_detached.py",
        },
    )
    run_identity["database"] = gc.database_schema_identity(
        db_url,
        "select 'migration:' || version_num from alembic_version",
    )
    run_identity["template_database"] = os.environ[gc.DISPOSABLE_TEMPLATE_ENV]
    run_identity["migration_sources"] = gc.sql_sources_identity(
        root / "backend/migrations",
        root / "backend/evalrank_migrations",
    )
    run_identity["sha256"] = gc.json_fingerprint(
        {key: value for key, value in run_identity.items() if key != "sha256"}
    )
    runtime_config = {
        "runtime": "search_knowledge_detached (HNSW+BM25+RRF K=60)",
        "reranking_enabled": reranking,
        "k": args.k,
        "budget_tokens": args.budget_tokens,
        "evidence_packer": gc.EVIDENCE_PACKER_CONFIG,
        "min_score": 0.0,
        "ingest_granularity": "per-pinned-section (real sectionizer+chunker)",
        "haystack_sections": len(corpus_sections),
        "golden_revision": golden_revision,
        "corpus_revision": corpus_manifest["section_revision"],
        "generation_identity": run_identity,
    }
    report = {
        "engine": "syndai",
        "runtime": "search_knowledge_detached (HNSW+BM25+RRF K=60)",
        "reranking_enabled": reranking,
        "database": db_url.rsplit("/", 1)[-1] if isinstance(db_url, str) else "local",
        "k": args.k,
        "min_score": 0.0,
        "ingest_granularity": "per-pinned-section (real sectionizer+chunker)",
        "input_files": len(files),
        "source_sections": len(corpus_sections),
        "corpus_revision": corpus_manifest["section_revision"],
        "sources": n_sources,
        "chunks": n_chunks,
        "chunks_only_fallback_sections": fallback_sections,
        "skipped_sections": skipped_sections,
        "golden_sha256": actual_sha,
        "golden_revision": golden_revision,
        "golden_count": n,
        "expected_n": n,
        "runtime_config": runtime_config,
        "runtime_config_fingerprint": gc.json_fingerprint(runtime_config),
        "fallback_count": len(fallback_sections),
        "degraded_count": 0,
        "skipped_count": len(skipped_sections),
        "reranker_failure_count": sum(row["failure"] != "none" for row in provenance_rows),
        "recall_e2e_ms_p50": p50,
        "recall_e2e_ms_p95": p95,
        "recall_e2e_p95_ceiling_ms": RECALL_E2E_P95_CEILING_MS,
        "recall_e2e_p95_within_ceiling": p95 <= RECALL_E2E_P95_CEILING_MS,
        "recall_at_5": r5,
        "recall_at_10": r10,
        "per_question": provenance_rows,
    }
    if negative_summary is not None:
        report["negative"] = negative_summary
    gc.finalize_provenance_report(report, Path(args.out_evidence))
    Path(args.out_provenance).write_text(json.dumps(report, indent=2) + "\n")
    print(
        f"syndai done: R@5={r5:.3f} R@10={r10:.3f} n={n} rerank={reranking} "
        f"evidence={args.out_evidence} provenance={args.out_provenance}",
        file=sys.stderr,
    )
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--syndai-root", default=str(DEFAULT_SYNDAI_ROOT))
    parser.add_argument("--golden", default=str(GOLDEN_PATH))
    parser.add_argument("--out-evidence", required=True)
    parser.add_argument("--out-provenance", required=True)
    parser.add_argument("--negative-slice", help="hash-locked negative JSONL")
    parser.add_argument("--out-negative-evidence", help="separate abstention evidence JSONL")
    parser.add_argument("--k", type=int, default=10)
    parser.add_argument("--budget-tokens", type=int, default=8192)
    parser.add_argument(
        "--limit-files", type=int, default=0, help="smoke only: cap corpus files (0 = full)"
    )
    args = parser.parse_args()
    return asyncio.run(run(args))


if __name__ == "__main__":
    raise SystemExit(main())

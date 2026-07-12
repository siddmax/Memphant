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

Ingest granularity: Syndai ingests each markdown FILE and its real sectionizer +
chunker split it (this IS its production path). MemPhant, whose resource channel
does not auto-chunk, is fed per-section resources. Both index the full corpus
and are graded by the identical span-containment provenance metric.

The Syndai repo is used strictly as-is — nothing is written to it. Only the
local dev DB is populated (a throwaway); no Supabase, no schema changes.
"""

from __future__ import annotations

import argparse
import asyncio
import json
import sys
import time
from datetime import UTC, datetime
from pathlib import Path
from unittest.mock import AsyncMock, patch
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
from src.infrastructure.db import (  # noqa: E402
    psycopg_async_database_url,
    setup_checkout_hook,
)
from tests.fixtures.factories import ProjectFactory, UserFactory  # noqa: E402

DEFAULT_SYNDAI_ROOT = Path("/Users/sidsharma/Syndai")
GOLDEN_PATH = gc.MEMPHANT_ROOT / "benchmarks" / "data" / "syndai_docs_golden.jsonl"


async def _ingest_one(
    session, agent, user, rel: str, text: str, *, with_sections: bool = True
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
        project_id=None,
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
    engine, root: Path, files: list[str]
) -> tuple[str, str, int, int, list[str], list[str]]:
    """Ingest each corpus file as a KnowledgeSource via the real pipeline.
    Returns (agent_id, user_id, source_count, chunk_count, fallback_files,
    skipped_files). A file that trips the upstream section-edge bug is retried
    chunks-only (see _ingest_one); a file that still fails is rolled back to a
    savepoint and skipped so one bad file cannot abort the whole ingest."""
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
        agent_id, user_id = str(agent.id), str(user.id)

        skipped: list[str] = []
        fallback: list[str] = []
        for i, rel in enumerate(files):
            text = (root / rel).read_text(encoding="utf-8", errors="replace")
            if not text.strip():
                continue
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
                print(f"  ingested {i + 1}/{len(files)} files, {total_chunks} chunks", file=sys.stderr)

        if fallback:
            print(f"  ingest used chunks-only fallback for {len(fallback)} files", file=sys.stderr)
        if skipped:
            print(f"  ingest skipped {len(skipped)} files", file=sys.stderr)
        # Commit so the detached search (its own sessions) can see the rows.
        await session.commit()
    return agent_id, user_id, sources, total_chunks, fallback, skipped


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

    root = Path(args.syndai_root)
    files = gc.list_corpus_files(root)
    if args.limit_files:
        files = files[: args.limit_files]
        print(f"SMOKE: corpus limited to first {len(files)} files", file=sys.stderr)
    print(f"corpus files={len(files)}", file=sys.stderr)

    settings = get_settings()
    db_url = settings.database_url
    host = str(db_url)
    if "supabase.com" in host:
        raise RuntimeError(
            "refusing to run against Supabase; set DATABASE_URL to the local dev DB"
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
            n_sources,
            n_chunks,
            fallback_files,
            skipped_files,
        ) = await ingest_corpus(engine, root, files)
        print(
            f"ingest done: sources={n_sources} chunks={n_chunks} "
            f"fallback={len(fallback_files)} skipped={len(skipped_files)} "
            f"in {time.time() - t0:.1f}s",
            file=sys.stderr,
        )

        reranking = bool(settings.jina_api_key)
        evidence_rows = []
        provenance_rows = []
        # Patch the adaptive-query LLM retry off (as the Syndai eval does): it is
        # a separate concern and would add nondeterministic LLM calls.
        with patch(
            "src.features.knowledge.search_detached.build_adaptive_query",
            new_callable=AsyncMock,
            return_value=None,
        ):
            for i, golden in enumerate(goldens):
                try:
                    results = await search_knowledge_detached(
                        engine=engine,
                        agent_id=_uuid.UUID(agent_id),
                        user_id=_uuid.UUID(user_id),
                        query=golden["question"],
                        search_credits=0,
                        top_k=args.k,
                        min_score=0.0,
                    )
                    bodies = [r.content for r in results]
                    error = None
                except Exception as exc:  # noqa: BLE001
                    bodies = []
                    error = f"{type(exc).__name__}: {exc}"
                    print(f"  search error {golden['question_id']}: {error}", file=sys.stderr)
                evidence_rows.append(gc.evidence_row(golden, bodies, args.k))
                provenance_rows.append(
                    {
                        "question_id": golden["question_id"],
                        "question_type": golden["question_type"],
                        "multi_hop": golden["multi_hop"],
                        "returned_items": len(bodies),
                        "search_error": error,
                        "hit_at_5": gc.provenance_hit(golden, bodies, 5),
                        "hit_at_10": gc.provenance_hit(golden, bodies, min(10, args.k)),
                    }
                )
                if (i + 1) % 20 == 0:
                    print(f"  searched {i + 1}/{len(goldens)}", file=sys.stderr)
    finally:
        await engine.dispose()

    gc.write_jsonl(Path(args.out_evidence), evidence_rows)
    n = len(provenance_rows)
    r5 = sum(r["hit_at_5"] for r in provenance_rows) / n if n else 0.0
    r10 = sum(r["hit_at_10"] for r in provenance_rows) / n if n else 0.0
    report = {
        "engine": "syndai",
        "runtime": "search_knowledge_detached (HNSW+BM25+RRF K=60)",
        "reranking_enabled": reranking,
        "database": db_url.rsplit("/", 1)[-1] if isinstance(db_url, str) else "local",
        "k": args.k,
        "min_score": 0.0,
        "ingest_granularity": "per-file (real sectionizer+chunker)",
        "sources": n_sources,
        "chunks": n_chunks,
        "chunks_only_fallback_files": fallback_files,
        "skipped_files": skipped_files,
        "golden_sha256": actual_sha,
        "golden_count": n,
        "recall_at_5": r5,
        "recall_at_10": r10,
        "per_question": provenance_rows,
    }
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
    parser.add_argument("--k", type=int, default=10)
    parser.add_argument(
        "--limit-files", type=int, default=0, help="smoke only: cap corpus files (0 = full)"
    )
    args = parser.parse_args()
    return asyncio.run(run(args))


if __name__ == "__main__":
    raise SystemExit(main())

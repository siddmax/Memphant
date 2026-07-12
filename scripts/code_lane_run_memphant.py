#!/usr/bin/env python3
"""MemPhant engine runner for the R0 code-lane sub-bakeoff (R0-T6).

Ingests the pinned coding-events corpus into MemPhant as raw EPISODES (real
runtime path: packaged ``memphant-server`` + ``memphant-worker`` +
``memphant-cli`` against Postgres), then calls ``/v1/recall`` (k=10,
mode=exhaustive, budget_tokens=8192) per golden question and emits an
evidence JSONL in the ``run_reader.py``-consumable shape plus a provenance
report (span-level hit@5/hit@10 via ``gate_common.provenance_hit`` — the
SAME grading function the docs-lane runner uses).

Ingest mapping (episode, not resource — the brief's explicit choice for this
lane; documented here since the REST API has no literal "turns" field):
``POST /v1/episodes`` takes a single ``body: Option<String>`` (see
``RetainEpisodeHttpRequest`` in ``memphant-types``) — there is no turn-array
wire shape. One episode is retained per sampled attempt; its body is that
attempt's content events concatenated as ``role: text`` lines in sequence
order, ONE role-prefixed line per event (the exact convention
``memphant-eval``'s ``bench_lme::session_body`` already uses for LongMemEval
turns, and the format ``memphant-core::service::segment_episode_body``
recognizes as "turn-structured" for its citation-window segmentation —
`parse_turn` there matches a `role: content` PREFIX per physical line, so a
multi-line event's continuation lines don't themselves parse as turns; this
is an accepted characteristic of the existing convention, not new here).
``source_kind="agent"`` (episode content is the coding agent's own
transcript, not tool/user/web input per the source_kind taxonomy in spec
`04`), ``source_trust="trusted_system"`` (an advisory hint, capped at the
provisioned key's ceiling exactly like the docs runner).

Isolation: each run re-execs itself through ``scripts/with_scratch_db.sh``
(``gate_runtime.reexec_through_scratch_db``) onto a fresh, migrated, per-run
scratch DB minted from ``--database-url`` (the campaign *base* server) and
dropped on exit — even if killed — with a freshly-minted tenant inside it.
No shared named DB, so the worker's global oldest-first job-claim can never
touch or be starved by foreign ``job_state`` debris. Same isolation contract
as ``gate_run_memphant.py``, the e2e probe, and the pg contract tests.

Smoke mode (``--limit-attempts``): caps the number of ingested attempts for
a tiny pass, but ALWAYS keeps every attempt referenced by the golden set's
provenance (never silently drops gold coverage) — same "coverage assertion,
never drop the gold" contract as the docs runner's ``--limit-haystack``.
"""

from __future__ import annotations

import json
import os
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import gate_common as gc  # noqa: E402
import gate_runtime as gr  # noqa: E402

# Base campaign *server* url to mint the per-run scratch DB from (see
# gate_runtime.reexec_through_scratch_db); the named DB in it is never touched.
DEFAULT_BASE_DATABASE_URL = "postgres://memphant:memphant@localhost:5432/memphant"
CORPUS_PATH = gc.MEMPHANT_ROOT / "benchmarks" / "data" / "coding_events_corpus.jsonl"
GOLDEN_PATH = gc.MEMPHANT_ROOT / "benchmarks" / "data" / "coding_events_golden.jsonl"
SCOPE_ID = "7c000000-0000-4000-8000-0000000000b1"
ACTOR_ID = "7c000000-0000-4000-8000-0000000000b2"

def golden_lock_path(golden_path: Path) -> Path:
    return golden_path.with_name(golden_path.stem + ".lock.json")


# --- pure functions (unit-tested in tests/test_code_lane_run_memphant.py) ---


def build_episode_body(events: list[dict]) -> str:
    """One ``role: text`` line per content event, sequence order — the
    conversation-episode convention documented at module level."""
    return "\n".join(f"{event['role']}: {event['text']}" for event in events)


def select_ingest_attempts(
    corpus_rows: list[dict], goldens: list[dict], limit_attempts: int
) -> list[dict]:
    """Attempts to ingest for this run. ``limit_attempts <= 0`` means the
    full corpus. Otherwise: every attempt referenced by ANY golden's
    provenance is kept unconditionally (gold coverage is never dropped —
    same contract as the docs runner's ``--limit-haystack``), then filled up
    to ``limit_attempts`` with the remaining attempts in sorted attempt_id
    order for determinism."""
    if limit_attempts <= 0:
        return corpus_rows
    gold_attempt_ids = {
        entry["attempt_id"] for golden in goldens for entry in golden["provenance"]
    }
    by_id = {row["attempt_id"]: row for row in corpus_rows}
    kept = [by_id[aid] for aid in sorted(gold_attempt_ids) if aid in by_id]
    kept_ids = {row["attempt_id"] for row in kept}
    others = sorted(
        (row for row in corpus_rows if row["attempt_id"] not in kept_ids),
        key=lambda row: row["attempt_id"],
    )
    fill = max(0, limit_attempts - len(kept))
    return kept + others[:fill]


def assert_gold_coverage(ingested_rows: list[dict], goldens: list[dict]) -> None:
    ingested_ids = {row["attempt_id"] for row in ingested_rows}
    missing = sorted(
        {
            entry["attempt_id"]
            for golden in goldens
            for entry in golden["provenance"]
            if entry["attempt_id"] not in ingested_ids
        }
    )
    if missing:
        raise RuntimeError(f"gold attempt_id(s) not in ingest set: {missing}")


# --- ingest ------------------------------------------------------------------


def ingest_attempt(client: gr.ApiClient, row: dict) -> str:
    body = build_episode_body(row["events"])
    payload = {
        "tenant_id": client.tenant_id,
        "scope_id": SCOPE_ID,
        "actor_id": ACTOR_ID,
        "source_kind": "agent",
        "source_trust": "trusted_system",
        "subject_hint": f"coding-attempt:{row['attempt_id']}",
        "body": body,
    }
    response = client.post("/v1/episodes", payload)
    return response.get("episode_id") or ""


def main() -> int:
    import argparse

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--database-url", default=DEFAULT_BASE_DATABASE_URL,
        help="base campaign SERVER url to mint the per-run scratch DB from; the "
             "run uses a fresh ephemeral DB dropped on exit, never this one",
    )
    parser.add_argument("--corpus", default=str(CORPUS_PATH))
    parser.add_argument("--golden", default=str(GOLDEN_PATH))
    parser.add_argument("--out-evidence", required=True)
    parser.add_argument("--out-provenance", required=True)
    parser.add_argument(
        "--embed-model",
        default=None,
        help="MEMPHANT_EMBEDDINGS id passed into BOTH the server and worker subprocess env",
    )
    parser.add_argument("--label", default=None)
    parser.add_argument("--port", type=int, default=39413)
    parser.add_argument("--k", type=int, default=10)
    parser.add_argument("--budget-tokens", type=int, default=8192)
    parser.add_argument("--mode", default="exhaustive", choices=("fast", "balanced", "exhaustive"))
    parser.add_argument(
        "--limit-attempts", type=int, default=0,
        help="0 = full corpus; otherwise a smoke cap that always keeps every gold-referenced attempt",
    )
    parser.add_argument("--server-bin", default=str(gc.MEMPHANT_ROOT / "target/release/memphant-server"))
    parser.add_argument("--worker-bin", default=str(gc.MEMPHANT_ROOT / "target/release/memphant-worker"))
    parser.add_argument("--cli-bin", default=str(gc.MEMPHANT_ROOT / "target/release/memphant-cli"))
    args = parser.parse_args()

    # Re-exec through with_scratch_db.sh (unless already inside one): mints a
    # fresh migrated DB, drops it on exit. args.database_url then points at that
    # scratch DB for every downstream call (provision/server/worker/report).
    gr.reexec_through_scratch_db(args.database_url)
    args.database_url = os.environ["DATABASE_URL"]

    gr.check_embed_model_key(args.embed_model)
    label_prefix = f"[{args.label}] " if args.label else ""

    golden_path = Path(args.golden)
    goldens = gc.load_goldens(golden_path)
    lock = json.loads(golden_lock_path(golden_path).read_text())
    golden_sha = gc.sha256_hex(golden_path.read_bytes())
    if golden_sha != lock["sha256"]:
        raise RuntimeError(
            f"golden sha256 mismatch: file={golden_sha[:12]} lock={lock['sha256'][:12]}"
        )
    print(
        f"{label_prefix}goldens={len(goldens)} path={golden_path.name} "
        f"sha256={golden_sha[:12]} (lock verified)",
        file=sys.stderr,
    )

    corpus_rows = gc.load_goldens(Path(args.corpus))
    ingest_rows = select_ingest_attempts(corpus_rows, goldens, args.limit_attempts)
    assert_gold_coverage(ingest_rows, goldens)
    print(
        f"{label_prefix}corpus attempts={len(corpus_rows)} ingesting={len(ingest_rows)} "
        f"(limit_attempts={args.limit_attempts or 'full'})",
        file=sys.stderr,
    )

    tenant_id, api_key = gr.provision_tenant(args.cli_bin, args.database_url, name_prefix="code-lane-gate")
    print(f"{label_prefix}tenant={tenant_id}", file=sys.stderr)

    log_name = f"server-{args.label}.log" if args.label else "server.log"
    server_log_path = Path(args.out_provenance).resolve().parent / log_name
    server = gr.Server(
        args.server_bin, args.database_url, args.port, args.embed_model,
        log_path=server_log_path,
    )
    # Symmetric cleanup: start() and the ingest/recall body are both inside
    # this try so the server child is always killed on any exception path,
    # not just after a successful start (a failed start() already
    # self-terminates before raising; stop() here is then a safe no-op).
    try:
        server.start()
        client = gr.ApiClient(args.port, api_key, tenant_id)
        t0 = time.time()
        for i, row in enumerate(ingest_rows):
            ingest_attempt(client, row)
            if (i + 1) % 25 == 0:
                print(f"{label_prefix}  ingested {i + 1}/{len(ingest_rows)}", file=sys.stderr)
        print(
            f"{label_prefix}ingest done in {time.time() - t0:.1f}s; draining worker...",
            file=sys.stderr,
        )
        compiled = gr.drain_worker(args.worker_bin, args.database_url, args.embed_model)
        print(f"{label_prefix}worker drained: compiled={compiled} jobs", file=sys.stderr)

        evidence_rows = []
        provenance_rows = []
        for i, golden in enumerate(goldens):
            bodies, degraded = gr.recall_query(
                client, SCOPE_ID, ACTOR_ID, golden["question"], args.k, args.budget_tokens, args.mode
            )
            evidence_rows.append(gc.evidence_row(golden, bodies, args.k))
            provenance_rows.append(
                {
                    "question_id": golden["question_id"],
                    "question_type": golden["question_type"],
                    "returned_items": len(bodies),
                    "degraded": degraded,
                    "hit_at_5": gc.provenance_hit(golden, bodies, 5),
                    "hit_at_10": gc.provenance_hit(golden, bodies, min(10, args.k)),
                }
            )
            if (i + 1) % 10 == 0:
                print(f"{label_prefix}  recalled {i + 1}/{len(goldens)}", file=sys.stderr)

        gc.write_jsonl(Path(args.out_evidence), evidence_rows)
        n = len(provenance_rows)
        r5 = sum(r["hit_at_5"] for r in provenance_rows) / n if n else 0.0
        r10 = sum(r["hit_at_10"] for r in provenance_rows) / n if n else 0.0
        report = {
            "engine": "memphant",
            "lane": "code",
            "runtime": "memphant-server episode ingest (role-prefixed turn body) + /v1/recall",
            "embed_model": args.embed_model,
            "label": args.label,
            "golden_path": str(golden_path),
            "database_url_db": args.database_url.rsplit("/", 1)[-1],
            "k": args.k,
            "recall_mode": args.mode,
            "budget_tokens": args.budget_tokens,
            "ingested_attempts": len(ingest_rows),
            "corpus_attempts": len(corpus_rows),
            "limit_attempts": args.limit_attempts,
            "golden_sha256": golden_sha,
            "golden_count": n,
            "recall_at_5": r5,
            "recall_at_10": r10,
            "per_question": provenance_rows,
        }
        Path(args.out_provenance).write_text(json.dumps(report, indent=2) + "\n")
        print(
            f"{label_prefix}done: R@5={r5:.3f} R@10={r10:.3f} n={n} "
            f"evidence={args.out_evidence} provenance={args.out_provenance}",
            file=sys.stderr,
        )
    finally:
        server.stop()

    return 0


if __name__ == "__main__":
    raise SystemExit(main())

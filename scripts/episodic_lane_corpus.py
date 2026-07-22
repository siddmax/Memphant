#!/usr/bin/env python3
"""Synthetic episodic-memory corpus for the C1 cutover backfill.

Prod's 252 ``syndai.episodic_memories`` rows live in an off-limits Supabase
schema (AGENTS.md §18) and the local dev DB is wiped (verified 2026-07-22, same
wall C3 hit), so C1 backfills a schema-faithful SYNTHETIC corpus — correctness-
only, the C3 posture. This module is the deterministic generator; the runner
(``episodic_lane_run_memphant.py``) retains each row as one MemPhant episode.

Schema fidelity: each row carries the ``episodic_memories`` columns the backfill
mapping and the Conversations-tab DTO read (``dtos.EpisodicMemoryRead``). Two
distinct ``user_id``s partition the corpus so the same rows also seed the RLS
two-tenant leakage fixture. Bodies are kept short (< ``MAX_BODY_CHARS``) so one
episode compiles to exactly one MemPhant unit (no contextual-chunk split), which
is what makes Bar 2's 1:1 count deterministic. A handful of rows are archived or
``source_kind == 'user_correction'`` so Bar 2 exercises recall's state/exclusion
filters (recall drops both; a plain listing would not).
"""

from __future__ import annotations

import argparse
import random
import sys
import uuid
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import gate_common as gc  # noqa: E402

# Syndai's source_kind -> importance_score map (verified recon
# 05-codebase-syndai.md:56: "user_correction 1.5 ... system_generated 0.3").
SOURCE_KIND_WEIGHTS = {
    "user_correction": 1.5,
    "user_message": 1.0,
    "assistant_message": 0.8,
    "dialog_turn": 0.8,
    "system_generated": 0.3,
}

# Bodies stay well under the ~1200-token contextual-chunk split window so each
# episode mints exactly one unit (Bar 2's 1:1 count depends on it).
MAX_BODY_CHARS = 400

# The recall-visible source kinds (everything except the audit kind).
_VISIBLE_KINDS = [k for k in SOURCE_KIND_WEIGHTS if k != "user_correction"]

# A small deterministic bank of distinct short conversation fragments; the row
# index is woven in so recall-visible bodies never collide.
_TOPICS = [
    "The deployment window is Tuesdays 09:00 UTC.",
    "The staging region moved to eu-west-2 last sprint.",
    "The on-call rotation handoff is every Monday.",
    "The invoice export uses the v3 CSV schema now.",
    "The mobile build signs with the release keystore.",
    "The search index rebuild runs nightly at 02:00.",
    "The rate limit for the public API is 600/min.",
    "The feature flag for dark mode ships next release.",
    "The backup retention policy is 30 days hot, 180 cold.",
    "The webhook retries use exponential backoff to 5 tries.",
]


def _uuid_for(rng: random.Random) -> str:
    """A deterministic UUID from the seeded RNG (never global randomness)."""
    return str(uuid.UUID(int=rng.getrandbits(128)))


def build_corpus(count: int, seed: int) -> list[dict]:
    """Deterministic list of ``count`` episodic rows for the given ``seed``.

    Same ``(count, seed)`` -> identical rows. Rows span exactly two ``user_id``s.
    ``importance_score`` is derived from ``source_kind`` via
    ``SOURCE_KIND_WEIGHTS`` (Syndai's own rule). ``created_at`` is strictly
    monotonic (oldest first) so recency ordering is testable. Some rows are
    archived and some are ``user_correction`` audit rows; the recall-visible
    remainder have distinct bodies.
    """
    rng = random.Random(seed)
    user_a = _uuid_for(rng)
    user_b = _uuid_for(rng)
    agent_a = _uuid_for(rng)
    agent_b = _uuid_for(rng)
    project = _uuid_for(rng)

    rows: list[dict] = []
    for index in range(count):
        is_user_a = index % 2 == 0
        user_id = user_a if is_user_a else user_b
        l0_agent_id = agent_a if is_user_a else agent_b

        # Deterministic disposition: every 17th row is an audit correction,
        # every 23rd (non-correction) row is archived. The rest are visible.
        if index % 17 == 0:
            source_kind = "user_correction"
        else:
            source_kind = _VISIBLE_KINDS[index % len(_VISIBLE_KINDS)]
        archived = source_kind != "user_correction" and index % 23 == 0

        # Distinct short body: a bank topic plus the row index keeps every
        # recall-visible body unique (Bar 2 matches by content).
        topic = _TOPICS[index % len(_TOPICS)]
        body = f"[{index:03d}] {topic}"
        assert len(body) <= MAX_BODY_CHARS

        # Monotonic timestamps: oldest at index 0.
        minute = index
        created_at = f"2026-01-01T{minute // 60:02d}:{minute % 60:02d}:00Z"

        rows.append(
            {
                "id": _uuid_for(rng),
                "user_id": user_id,
                "l0_agent_id": l0_agent_id,
                "project_id": project if index % 3 == 0 else None,
                "mission_id": _uuid_for(rng) if index % 5 == 0 else None,
                "content": body,
                "source_kind": source_kind,
                "importance_score": SOURCE_KIND_WEIGHTS[source_kind],
                "trust_level": 80,
                "tainted": False,
                "archived_at": created_at if archived else None,
                "created_at": created_at,
                "idempotency_key": f"episodic:{seed}:{index}",
            }
        )
    return rows


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--count", type=int, default=252)
    parser.add_argument("--seed", type=int, default=20260721)
    parser.add_argument("--out", required=True, help="corpus JSONL output path")
    args = parser.parse_args()
    rows = build_corpus(args.count, args.seed)
    gc.write_jsonl(Path(args.out), rows)
    print(f"wrote {len(rows)} episodic rows -> {args.out}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

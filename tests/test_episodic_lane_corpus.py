"""Contract tests for the synthetic episodic-memory corpus generator.

The C1 cutover backfills Syndai's ``episodic_memories`` into MemPhant. Prod's 252
rows live in an off-limits Supabase schema and the local dev DB is wiped, so C1
runs on a schema-faithful SYNTHETIC corpus (correctness-only, the C3 posture).
These tests pin the corpus shape the backfill runner and the three acceptance
bars depend on: deterministic 252 rows, two tenants (for the RLS leakage
fixture), short bodies (so one episode compiles to exactly one unit — no
contextual-chunk split), and the presence of archived + ``user_correction`` rows
(so Bar 2 exercises recall's state/exclusion filters).
"""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
import episodic_lane_corpus as corpus  # noqa: E402


def test_build_corpus_is_deterministic_and_sized():
    """Same seed -> byte-identical rows; default target is 252 (prod's count)."""
    a = corpus.build_corpus(count=252, seed=20260721)
    b = corpus.build_corpus(count=252, seed=20260721)
    assert len(a) == 252
    assert a == b, "corpus generation must be deterministic for a fixed seed"


def test_two_tenants_partition_the_corpus():
    """Both users own rows (the RLS two-tenant leakage fixture needs each to
    have episodes the other must never see), and every row names a user."""
    rows = corpus.build_corpus(count=252, seed=20260721)
    users = {row["user_id"] for row in rows}
    assert len(users) == 2, "corpus must span exactly two tenants/users"
    per_user = {u: sum(1 for r in rows if r["user_id"] == u) for u in users}
    assert all(n > 0 for n in per_user.values()), "each tenant must own >=1 row"


def test_rows_are_schema_faithful():
    """Every row carries the episodic_memories fields the backfill mapping and
    the Conversations-tab DTO read."""
    required = {
        "id",
        "user_id",
        "l0_agent_id",
        "project_id",
        "mission_id",
        "content",
        "source_kind",
        "importance_score",
        "trust_level",
        "tainted",
        "archived_at",
        "created_at",
        "idempotency_key",
    }
    for row in corpus.build_corpus(count=252, seed=20260721):
        assert required <= row.keys(), f"row missing fields: {required - row.keys()}"
        assert 0 <= row["trust_level"] <= 100
        assert isinstance(row["content"], str) and row["content"]


def test_bodies_are_short_enough_for_one_unit_per_episode():
    """Bar 2's 1:1 count needs bodies below the contextual-chunk split window.
    The MemPhant resource-chunk threshold is ~1200 tokens; keep bodies well
    under that (a few hundred chars) so each episode mints exactly one unit."""
    for row in corpus.build_corpus(count=252, seed=20260721):
        assert len(row["content"]) <= corpus.MAX_BODY_CHARS


def test_includes_archived_and_correction_rows():
    """Bar 2 asserts archived + user_correction rows are ABSENT from recall.
    The corpus must actually contain some, or the assertion is vacuous."""
    rows = corpus.build_corpus(count=252, seed=20260721)
    assert any(row["archived_at"] is not None for row in rows), "need archived rows"
    assert any(
        row["source_kind"] == "user_correction" for row in rows
    ), "need user_correction audit rows"
    # ...and some plain recall-visible rows, or equivalence has nothing to match.
    assert any(
        row["archived_at"] is None and row["source_kind"] != "user_correction"
        for row in rows
    ), "need recall-visible rows"


def test_source_kind_importance_weights_match_syndai():
    """Syndai derives importance_score from source_kind (user_correction 1.5 ...
    system_generated 0.3). The corpus must encode the same map so a downstream
    parity check is meaningful."""
    weights = corpus.SOURCE_KIND_WEIGHTS
    assert weights["user_correction"] == 1.5
    assert weights["system_generated"] == 0.3
    for row in corpus.build_corpus(count=252, seed=20260721):
        assert row["importance_score"] == weights[row["source_kind"]]


def test_recall_visible_rows_have_distinct_content():
    """Bar 2 matches episodes by content; recall-visible rows must be uniquely
    identifiable so an equivalence mismatch can't be masked by duplicates."""
    rows = corpus.build_corpus(count=252, seed=20260721)
    visible = [
        r["content"]
        for r in rows
        if r["archived_at"] is None and r["source_kind"] != "user_correction"
    ]
    assert len(visible) == len(set(visible)), "recall-visible bodies must be distinct"

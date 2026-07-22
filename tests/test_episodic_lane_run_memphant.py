"""Unit tests for the episodic backfill runner's pure functions.

The runner (``episodic_lane_run_memphant.py``) mirrors the C3 code-lane runner:
re-exec through a scratch DB, start the packaged server/worker, bind context per
tenant (C0 strict contract), retain one episode per corpus row, drain, recall.
These tests pin the pure pieces the live run and Bar 2 depend on, without a
server: the strict-contract retain payload, the recall-visible expected set (the
Conversations-tab surface), and the equivalence assertion.
"""

from __future__ import annotations

import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
import episodic_lane_corpus as corpus  # noqa: E402
import episodic_lane_run_memphant as runner  # noqa: E402


def _ctx():
    return {
        "subject_id": "s",
        "scope_id": "sc",
        "actor_id": "a",
        "agent_node_id": "ag",
        "subject_generation": 0,
    }


def test_source_kind_maps_syndai_to_memphant_enum():
    """MemPhant's episode source_kind is a fixed 6-value enum
    (user/agent/tool/web/resource/system). Syndai's episodic kinds must map onto
    it (spec-28 convention: user-origin -> user, agent turns -> agent, system
    -> system). An unmapped kind is a hard error, never silently passed."""
    m = runner.map_source_kind
    assert m("user_correction") == "user"
    assert m("user_message") == "user"
    assert m("assistant_message") == "agent"
    assert m("dialog_turn") == "agent"
    assert m("system_generated") == "system"
    with pytest.raises(KeyError):
        m("not_a_syndai_kind")


def test_retain_payload_is_strict_contract_and_maps_source_kind():
    """No banned tenant_id/allowed_scope_ids; body/source_ref/observed_at map
    straight from the row; source_kind is translated to MemPhant's enum."""
    row = corpus.build_corpus(count=252, seed=20260721)[0]
    payload = runner.retain_payload(_ctx(), row)
    assert "tenant_id" not in payload
    assert "allowed_scope_ids" not in payload
    assert payload["payload"]["episode"]["body"] == row["content"]
    assert payload["payload"]["episode"]["source_kind"] == runner.map_source_kind(
        row["source_kind"]
    )
    assert payload["payload"]["episode"]["source_kind"] in {
        "user", "agent", "tool", "web", "resource", "system"
    }
    assert payload["source_ref"] == f"episodic:{row['id']}"
    assert payload["observed_at"] == row["created_at"]
    # the bound identity is spread in verbatim
    assert payload["subject_id"] == "s"
    assert payload["subject_generation"] == 0


def test_backfill_disposition_matches_syndai_recall_semantics():
    """user_correction audit rows -> skip (Syndai excludes them from recall);
    archived rows -> forget (retain then soft-forget, the archive verb); the
    rest -> retain. This is what makes the backfilled store's recall surface
    equal the Conversations tab."""
    assert runner.backfill_disposition(
        {"source_kind": "user_correction", "archived_at": None}
    ) == "skip"
    assert runner.backfill_disposition(
        {"source_kind": "user_message", "archived_at": "2026-01-01T00:00:00Z"}
    ) == "forget"
    assert runner.backfill_disposition(
        {"source_kind": "dialog_turn", "archived_at": None}
    ) == "retain"


def test_backfill_dispositions_cover_the_whole_corpus():
    """Every row gets exactly one disposition; the retain+forget set equals the
    non-correction rows, and skip equals the correction rows."""
    rows = corpus.build_corpus(count=252, seed=20260721)
    dispositions = [runner.backfill_disposition(r) for r in rows]
    assert set(dispositions) == {"retain", "forget", "skip"}
    n_skip = sum(1 for d in dispositions if d == "skip")
    n_corrections = sum(1 for r in rows if r["source_kind"] == "user_correction")
    assert n_skip == n_corrections
    n_forget = sum(1 for d in dispositions if d == "forget")
    n_archived = sum(
        1 for r in rows
        if r["archived_at"] is not None and r["source_kind"] != "user_correction"
    )
    assert n_forget == n_archived


def test_expected_recall_set_excludes_archived_and_corrections_and_is_recency_ordered():
    """The Conversations tab shows recall-visible episodes newest-first; archived
    and user_correction audit rows are absent (recall's own state filter)."""
    rows = corpus.build_corpus(count=252, seed=20260721)
    expected = runner.expected_recall_set(rows)
    assert expected, "expected set must be non-empty"
    assert all(r["archived_at"] is None for r in expected)
    assert all(r["source_kind"] != "user_correction" for r in expected)
    created = [r["created_at"] for r in expected]
    assert created == sorted(created, reverse=True), "must be recency DESC"


def test_expected_recall_set_is_single_tenant_scoped():
    """Bar 2 is proven per tenant; the expected set filters to one user_id so a
    cross-tenant row can never inflate the match."""
    rows = corpus.build_corpus(count=252, seed=20260721)
    user_id = rows[0]["user_id"]
    expected = runner.expected_recall_set(rows, user_id=user_id)
    assert expected
    assert all(r["user_id"] == user_id for r in expected)


def _perfect_recall_fn(rows, user_id):
    """A fake recall that behaves like a correct MemPhant: a query for a body
    returns it IFF that episode is recall-visible for this tenant (state filter
    applied). Injected so equivalence is unit-testable without a server."""
    visible = {
        r["content"]
        for r in rows
        if r["user_id"] == user_id
        and r["archived_at"] is None
        and r["source_kind"] != "user_correction"
    }

    def recall_fn(query: str) -> list[str]:
        return [query] if query in visible else []

    return recall_fn


def test_probe_equivalence_passes_when_recall_matches_the_tab():
    rows = corpus.build_corpus(count=252, seed=20260721)
    user_id = rows[0]["user_id"]
    summary = runner.probe_conversations_equivalence(
        _perfect_recall_fn(rows, user_id), rows, user_id
    )
    assert summary["retrievable"] > 0
    assert summary["correctly_excluded"] > 0


def test_probe_equivalence_fails_when_a_visible_episode_is_not_retrievable():
    """(a) retrievability: a recall that drops a visible episode is a divergence."""
    rows = corpus.build_corpus(count=252, seed=20260721)
    user_id = rows[0]["user_id"]
    base = _perfect_recall_fn(rows, user_id)
    visible_body = runner.expected_recall_set(rows, user_id=user_id)[0]["content"]

    def dropping_recall(query: str) -> list[str]:
        return [] if query == visible_body else base(query)

    with pytest.raises(RuntimeError):
        runner.probe_conversations_equivalence(dropping_recall, rows, user_id)


def test_probe_equivalence_fails_when_archived_episode_leaks_into_recall():
    """(b) state-filter correctness: an archived/correction episode surfacing in
    recall is the load-bearing failure the tab's archived filter must prevent."""
    rows = corpus.build_corpus(count=252, seed=20260721)
    user_id = rows[0]["user_id"]
    base = _perfect_recall_fn(rows, user_id)
    filtered_body = next(
        r["content"]
        for r in rows
        if r["user_id"] == user_id
        and (r["archived_at"] is not None or r["source_kind"] == "user_correction")
    )

    def leaking_recall(query: str) -> list[str]:
        return [query] if query == filtered_body else base(query)

    with pytest.raises(RuntimeError):
        runner.probe_conversations_equivalence(leaking_recall, rows, user_id)

"""Unit tests for the R0-T6 code-lane runner's pure functions
(``scripts/code_lane_run_memphant.py``): the episode-body turn-formatting
convention, the gold-coverage-preserving attempt selection for
``--limit-attempts`` smoke runs, and the coverage assertion. No DB, no
server process — these run under plain ``pytest tests/``.
"""

from __future__ import annotations

import importlib.util
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[1]


def _load(name: str, rel: str):
    spec = importlib.util.spec_from_file_location(name, ROOT / rel)
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(module)
    return module


@pytest.fixture(scope="module")
def clr():
    return _load("code_lane_run_memphant", "scripts/code_lane_run_memphant.py")


# --- build_episode_body -------------------------------------------------


def test_build_episode_body_role_prefixes_each_event(clr):
    events = [
        {"sequence": 0, "role": "user", "text": "please fix the bug"},
        {"sequence": 1, "role": "assistant", "text": "looking into it"},
    ]
    body = clr.build_episode_body(events)
    assert body == "user: please fix the bug\nassistant: looking into it"


def test_build_episode_body_preserves_sequence_order_as_given(clr):
    """The runner is expected to pass already sequence-sorted events (the
    corpus already stores them sorted); this function itself does not
    re-sort, it just formats in the given order."""
    events = [
        {"sequence": 2, "role": "toolResult", "text": "b"},
        {"sequence": 0, "role": "user", "text": "a"},
    ]
    body = clr.build_episode_body(events)
    assert body == "toolResult: b\nuser: a"


def test_build_episode_body_empty_events_is_empty_string(clr):
    assert clr.build_episode_body([]) == ""


# --- select_ingest_attempts / assert_gold_coverage --------------------------


def _row(attempt_id: str) -> dict:
    return {"attempt_id": attempt_id, "run_id": "r", "started_at": "t", "events": []}


def _golden(attempt_id: str, question_id: str = "q1") -> dict:
    return {
        "question_id": question_id,
        "provenance": [{"role": "answer", "attempt_id": attempt_id, "span": "x"}],
    }


def test_select_ingest_attempts_full_corpus_when_no_limit(clr):
    corpus = [_row("a1"), _row("a2"), _row("a3")]
    out = clr.select_ingest_attempts(corpus, [_golden("a1")], limit_attempts=0)
    assert out == corpus


def test_select_ingest_attempts_keeps_gold_attempts_under_limit(clr):
    corpus = [_row("a1"), _row("a2"), _row("a3"), _row("a4")]
    goldens = [_golden("a3", "q1")]
    out = clr.select_ingest_attempts(corpus, goldens, limit_attempts=2)
    ids = {row["attempt_id"] for row in out}
    assert "a3" in ids
    assert len(out) == 2


def test_select_ingest_attempts_fills_deterministically(clr):
    corpus = [_row("a1"), _row("a2"), _row("a3"), _row("a4")]
    goldens = [_golden("a3", "q1")]
    out1 = clr.select_ingest_attempts(corpus, goldens, limit_attempts=2)
    out2 = clr.select_ingest_attempts(corpus, goldens, limit_attempts=2)
    assert [r["attempt_id"] for r in out1] == [r["attempt_id"] for r in out2]


def test_select_ingest_attempts_never_drops_gold_even_if_limit_smaller(clr):
    corpus = [_row("a1"), _row("a2"), _row("a3")]
    goldens = [_golden("a1", "q1"), _golden("a2", "q2"), _golden("a3", "q3")]
    out = clr.select_ingest_attempts(corpus, goldens, limit_attempts=1)
    ids = {row["attempt_id"] for row in out}
    assert ids == {"a1", "a2", "a3"}


def test_assert_gold_coverage_passes_when_all_present(clr):
    ingested = [_row("a1"), _row("a2")]
    goldens = [_golden("a1")]
    clr.assert_gold_coverage(ingested, goldens)  # must not raise


def test_assert_gold_coverage_raises_when_missing(clr):
    ingested = [_row("a1")]
    goldens = [_golden("a2")]
    with pytest.raises(RuntimeError, match="a2"):
        clr.assert_gold_coverage(ingested, goldens)

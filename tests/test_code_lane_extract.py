"""Unit tests for the R0-T6 coding-lane corpus extractor's pure functions
(``scripts/code_lane_extract.py``): text extraction from raw event payloads,
truncation, the event-gap exclusion rule, eligibility, and the deterministic
attempt sample. No DB, no network — these are pure functions over fixture
rows (TDD per the brief), so they run under plain ``pytest tests/``.
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
def cle():
    return _load("code_lane_extract", "scripts/code_lane_extract.py")


# --- extract_event_text -------------------------------------------------


def test_extract_event_text_message_end_assistant(cle):
    payload = {"message": {"role": "assistant", "content": [{"text": "hello world", "type": "text"}]}}
    role, text = cle.extract_event_text("message_end", payload)
    assert role == "assistant"
    assert text == "hello world"


def test_extract_event_text_message_end_user(cle):
    payload = {"message": {"role": "user", "content": [{"text": "do the thing"}]}}
    role, text = cle.extract_event_text("message_end", payload)
    assert role == "user"
    assert text == "do the thing"


def test_extract_event_text_message_end_toolresult_role(cle):
    payload = {"message": {"role": "toolResult", "content": [{"text": "build ok"}]}}
    role, text = cle.extract_event_text("message_end", payload)
    assert role == "toolResult"
    assert text == "build ok"


def test_extract_event_text_message_end_contentless_is_skipped(cle):
    """Usage-metadata-only message_end (empty content array, e.g. a
    tool-use-only turn) has no text — brief requirement to skip it."""
    payload = {"message": {"role": "assistant", "content": [], "usage": {"input": 10}}}
    role, text = cle.extract_event_text("message_end", payload)
    assert (role, text) == (None, None)


def test_extract_event_text_message_end_missing_content_key(cle):
    payload = {"message": {"role": "assistant"}}
    assert cle.extract_event_text("message_end", payload) == (None, None)


def test_extract_event_text_message_end_filters_non_text_content_items(cle):
    """Content items without a usable 'text' string (e.g. tool_use blocks)
    are dropped; only real text items are joined."""
    payload = {
        "message": {
            "role": "assistant",
            "content": [
                {"type": "tool_use", "name": "bash"},
                {"text": "running the build"},
            ],
        }
    }
    role, text = cle.extract_event_text("message_end", payload)
    assert role == "assistant"
    assert text == "running the build"


def test_extract_event_text_message_end_all_items_non_text_is_skipped(cle):
    payload = {"message": {"role": "assistant", "content": [{"type": "tool_use", "name": "bash"}]}}
    assert cle.extract_event_text("message_end", payload) == (None, None)


def test_extract_event_text_message_end_joins_multiple_text_blocks(cle):
    payload = {
        "message": {
            "role": "assistant",
            "content": [{"text": "first block"}, {"text": "second block"}],
        }
    }
    role, text = cle.extract_event_text("message_end", payload)
    assert role == "assistant"
    assert text == "first block\n\nsecond block"


def test_extract_event_text_tool_execution_end(cle):
    payload = {"result": {"content": [{"text": "npm test failed: 2 tests"}]}}
    role, text = cle.extract_event_text("tool_execution_end", payload)
    assert role == "toolResult"
    assert text == "npm test failed: 2 tests"


def test_extract_event_text_tool_execution_end_contentless(cle):
    payload = {"result": {"content": []}}
    assert cle.extract_event_text("tool_execution_end", payload) == (None, None)


def test_extract_event_text_unknown_event_type_is_skipped(cle):
    assert cle.extract_event_text("tool_execution_start", {"anything": True}) == (None, None)


# --- truncate_text --------------------------------------------------------


def test_truncate_text_under_limit_unchanged(cle):
    text, truncated = cle.truncate_text("short", 4000)
    assert (text, truncated) == ("short", False)


def test_truncate_text_exactly_at_limit_unchanged(cle):
    body = "x" * 4000
    text, truncated = cle.truncate_text(body, 4000)
    assert (text, truncated) == (body, False)


def test_truncate_text_over_limit_is_truncated(cle):
    body = "x" * 4001
    text, truncated = cle.truncate_text(body, 4000)
    assert text == "x" * 4000
    assert truncated is True


# --- build_content_events --------------------------------------------------


def test_build_content_events_orders_by_sequence(cle):
    raw = [
        {"sequence": 5, "event_type": "message_end", "event_id": "e5",
         "payload": {"message": {"role": "assistant", "content": [{"text": "second"}]}}},
        {"sequence": 1, "event_type": "message_end", "event_id": "e1",
         "payload": {"message": {"role": "user", "content": [{"text": "first"}]}}},
    ]
    out = cle.build_content_events(raw, truncate_chars=4000)
    assert [e["sequence"] for e in out] == [1, 5]
    assert [e["text"] for e in out] == ["first", "second"]


def test_build_content_events_skips_contentless_rows(cle):
    raw = [
        {"sequence": 1, "event_type": "message_end", "event_id": "e1",
         "payload": {"message": {"role": "assistant", "content": []}}},
        {"sequence": 2, "event_type": "message_end", "event_id": "e2",
         "payload": {"message": {"role": "assistant", "content": [{"text": "kept"}]}}},
    ]
    out = cle.build_content_events(raw, truncate_chars=4000)
    assert len(out) == 1
    assert out[0]["sequence"] == 2


def test_build_content_events_marks_truncation(cle):
    raw = [
        {"sequence": 1, "event_type": "tool_execution_end", "event_id": "e1",
         "payload": {"result": {"content": [{"text": "x" * 5000}]}}},
    ]
    out = cle.build_content_events(raw, truncate_chars=4000)
    assert len(out[0]["text"]) == 4000
    assert out[0]["truncated"] is True
    assert out[0]["event_id"] == "e1"
    assert out[0]["role"] == "toolResult"


# --- has_event_gap ----------------------------------------------------------


def test_has_event_gap_false_when_dense(cle):
    assert cle.has_event_gap(n_events_total=10, max_sequence=9) is False


def test_has_event_gap_true_when_missing_events(cle):
    assert cle.has_event_gap(n_events_total=8, max_sequence=9) is True


def test_has_event_gap_true_when_more_events_than_max_plus_one(cle):
    # Should not happen given the unique (attempt_id, sequence) constraint,
    # but the rule is a strict equality check either direction.
    assert cle.has_event_gap(n_events_total=11, max_sequence=9) is True


# --- is_eligible -------------------------------------------------------------


def _events(n: int, chars_each: int) -> list[dict]:
    return [
        {"sequence": i, "role": "assistant", "text": "x" * chars_each, "event_id": f"e{i}", "truncated": False}
        for i in range(n)
    ]


def test_is_eligible_true_at_exact_boundary(cle):
    events = _events(6, 334)  # 6 * 334 = 2004 >= 2000
    assert cle.is_eligible(events, min_events=6, min_chars=2000) is True


def test_is_eligible_false_below_event_count(cle):
    events = _events(5, 1000)  # plenty of chars, too few events
    assert cle.is_eligible(events, min_events=6, min_chars=2000) is False


def test_is_eligible_false_below_char_count(cle):
    events = _events(10, 10)  # plenty of events, too few chars
    assert cle.is_eligible(events, min_events=6, min_chars=2000) is False


def test_is_eligible_true_when_both_bars_cleared(cle):
    events = _events(10, 500)
    assert cle.is_eligible(events, min_events=6, min_chars=2000) is True


# --- sample_attempts ---------------------------------------------------------


def test_sample_attempts_is_deterministic_for_same_seed(cle):
    counts = [(f"attempt-{i}", 50) for i in range(30)]
    chosen1, cum1 = cle.sample_attempts(counts, seed=20260713, haystack_min=600, haystack_max=1200)
    chosen2, cum2 = cle.sample_attempts(counts, seed=20260713, haystack_min=600, haystack_max=1200)
    assert chosen1 == chosen2
    assert cum1 == cum2


def test_sample_attempts_different_seed_can_differ(cle):
    counts = [(f"attempt-{i}", 50) for i in range(30)]
    chosen_a, _ = cle.sample_attempts(counts, seed=1, haystack_min=600, haystack_max=1200)
    chosen_b, _ = cle.sample_attempts(counts, seed=2, haystack_min=600, haystack_max=1200)
    assert chosen_a != chosen_b


def test_sample_attempts_stops_once_min_reached(cle):
    counts = [("a", 300), ("b", 300), ("c", 300), ("d", 300)]
    chosen, cumulative = cle.sample_attempts(counts, seed=20260713, haystack_min=600, haystack_max=1200)
    assert cumulative >= 600
    # Never adds an attempt beyond the one that first crosses haystack_min.
    assert cumulative - 600 < 300 or len(chosen) <= 2


def test_sample_attempts_never_drops_below_min_if_pool_is_large_enough(cle):
    counts = [(f"attempt-{i}", 40) for i in range(50)]
    chosen, cumulative = cle.sample_attempts(counts, seed=20260713, haystack_min=600, haystack_max=1200)
    assert cumulative >= 600
    assert len(chosen) == len(set(chosen))


def test_sample_attempts_uses_every_eligible_attempt_if_pool_too_small(cle):
    counts = [("a", 10), ("b", 10)]
    chosen, cumulative = cle.sample_attempts(counts, seed=20260713, haystack_min=600, haystack_max=1200)
    assert set(chosen) == {"a", "b"}
    assert cumulative == 20

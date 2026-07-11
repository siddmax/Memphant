"""Contract tests for the R0-T6 code-lane golden set: the committed lock
(``coding_events_golden.lock.json``) against the gitignored golden JSONL and
corpus JSONL. Per the brief: span-verbatim-in-corpus, no duplicate
question_ids, strata/count match lock — every case skips (rather than fails)
when the gitignored content files are absent, since CI never has them (they
are the user's private coding content and are never committed; only the
lock file is).
"""

from __future__ import annotations

import importlib.util
import json
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[1]
GOLDEN = ROOT / "benchmarks" / "data" / "coding_events_golden.jsonl"
GOLDEN_LOCK = ROOT / "benchmarks" / "data" / "coding_events_golden.lock.json"
CORPUS = ROOT / "benchmarks" / "data" / "coding_events_corpus.jsonl"

REQUIRED_GOLDEN_KEYS = {
    "question_id",
    "question_type",
    "is_abstention",
    "question",
    "question_date",
    "gold_answer",
    "multi_hop",
    "provenance",
    "source_event_key",
}
REQUIRED_EVIDENCE_KEYS = {
    "question_id",
    "question_type",
    "is_abstention",
    "question",
    "question_date",
    "gold_answer",
    "evidence",
}


def _load(name: str, rel: str):
    spec = importlib.util.spec_from_file_location(name, ROOT / rel)
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(module)
    return module


def _gc():
    return _load("gate_common", "scripts/gate_common.py")


def _clm():
    return _load("code_lane_mine", "scripts/code_lane_mine.py")


def _skip_if_absent(path: Path) -> None:
    if not path.exists():
        pytest.skip(f"{path} not present (gitignored, private coding content)")


def _rows(path: Path) -> list[dict]:
    return [json.loads(line) for line in path.read_text().split("\n") if line.strip()]


def test_lock_file_is_committed_and_well_formed() -> None:
    """The one artifact this gate DOES commit."""
    assert GOLDEN_LOCK.exists(), "coding_events_golden.lock.json must be committed"
    lock = json.loads(GOLDEN_LOCK.read_text())
    for key in ("golden_path", "sha256", "bytes", "count", "strata", "sample_seed", "generator_model"):
        assert key in lock, f"lock missing key: {key}"
    assert len(lock["sha256"]) == 64


def test_golden_lock_sha256_and_count_match() -> None:
    _skip_if_absent(GOLDEN)
    lock = json.loads(GOLDEN_LOCK.read_text())
    raw = GOLDEN.read_bytes()
    import hashlib

    assert hashlib.sha256(raw).hexdigest() == lock["sha256"], "golden JSONL drifted from its lock"
    goldens = _rows(GOLDEN)
    assert len(goldens) == lock["count"]


def test_strata_counts_match_lock() -> None:
    _skip_if_absent(GOLDEN)
    lock = json.loads(GOLDEN_LOCK.read_text())
    goldens = _rows(GOLDEN)
    strata: dict[str, int] = {}
    for row in goldens:
        strata[row["question_type"]] = strata.get(row["question_type"], 0) + 1
    assert strata == lock["strata"]


def test_no_duplicate_question_ids() -> None:
    _skip_if_absent(GOLDEN)
    goldens = _rows(GOLDEN)
    ids = [g["question_id"] for g in goldens]
    assert len(ids) == len(set(ids)), "duplicate question_id in golden set"


def test_golden_rows_are_well_formed_and_single_hop_only() -> None:
    _skip_if_absent(GOLDEN)
    goldens = _rows(GOLDEN)
    assert goldens, "golden set is empty"
    for g in goldens:
        assert REQUIRED_GOLDEN_KEYS <= set(g), f"missing keys: {REQUIRED_GOLDEN_KEYS - set(g)}"
        assert g["is_abstention"] is False
        assert g["multi_hop"] is False, "code lane is single-hop only per the brief"
        assert isinstance(g["question"], str) and g["question"].strip()
        assert isinstance(g["gold_answer"], str) and g["gold_answer"].strip()
        prov = g["provenance"]
        assert isinstance(prov, list) and len(prov) == 1
        entry = prov[0]
        assert entry["role"] == "answer"
        assert {"attempt_id", "event_sequence", "event_role", "span", "char_start", "char_end"} <= set(entry)
        assert entry["char_end"] > entry["char_start"]
        assert 3 <= len(entry["span"]) <= 200, "span must be 3-200 chars per the brief"


def test_evidence_row_shape_is_consumable_by_run_reader() -> None:
    _skip_if_absent(GOLDEN)
    gc = _gc()
    reader = _load("run_reader", "scripts/run_reader.py")
    goldens = _rows(GOLDEN)
    golden = goldens[0]
    row = gc.evidence_row(golden, ["body one text", "body two text"], k=10)
    assert REQUIRED_EVIDENCE_KEYS <= set(row)
    assert [item["rank"] for item in row["evidence"]] == [1, 2]
    prompt = reader.build_reader_prompt(row)
    assert golden["question"] in prompt


def test_answer_spans_are_verbatim_in_the_pinned_corpus() -> None:
    """The strongest pin: every recorded span is present at its char offsets
    in the corresponding corpus event's text."""
    _skip_if_absent(GOLDEN)
    _skip_if_absent(CORPUS)
    goldens = _rows(GOLDEN)
    corpus_rows = _rows(CORPUS)
    events_by_key: dict[tuple[str, int], dict] = {}
    for row in corpus_rows:
        for event in row["events"]:
            events_by_key[(row["attempt_id"], event["sequence"])] = event

    for g in goldens:
        entry = g["provenance"][0]
        key = (entry["attempt_id"], entry["event_sequence"])
        assert key in events_by_key, f"{g['question_id']}: source event {key} not in corpus"
        event = events_by_key[key]
        excerpt = event["text"][entry["char_start"] : entry["char_end"]]
        assert excerpt == entry["span"], f"{g['question_id']}: span not verbatim at recorded offsets"


def test_no_span_appears_in_more_than_three_distinct_attempts() -> None:
    _skip_if_absent(GOLDEN)
    _skip_if_absent(CORPUS)
    clm = _clm()
    goldens = _rows(GOLDEN)
    corpus_rows = _rows(CORPUS)
    corpus_index = {
        row["attempt_id"]: "\n\n".join(e["text"] for e in row["events"]) for row in corpus_rows
    }
    for g in goldens:
        span = g["provenance"][0]["span"]
        assert not clm.too_generic(span, corpus_index, threshold=3), (
            f"{g['question_id']}: span appears in more than 3 distinct attempts"
        )

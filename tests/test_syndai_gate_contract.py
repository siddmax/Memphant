"""Contract tests for the W10 Syndai replacement gate: the golden-set lock and
the evidence-row shape the two engine runners hand to run_reader.py.

These run under ``pytest tests/`` with no network or DB. The verbatim-span pin
(each answer span is present at its recorded char offsets in the real corpus)
is gated on the Syndai corpus being present on disk.
"""

from __future__ import annotations

import importlib.util
import json
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[1]
GOLDEN = ROOT / "benchmarks" / "data" / "syndai_docs_golden.jsonl"
GOLDEN_LOCK = ROOT / "benchmarks" / "data" / "syndai_docs_golden.lock.json"
MANIFEST = ROOT / "benchmarks" / "manifests" / "syndai_docs_gate.lock.json"

# v2 (R0-T3): a second, disjoint sample of the SAME pinned corpus (mined with
# --exclude-golden against v1). Tests below are parameterized over both golden
# files; a v2 case skips if the file is absent, same pattern as the existing
# Syndai-root skip in test_answer_spans_are_verbatim_in_the_pinned_corpus.
GOLDEN_V2 = ROOT / "benchmarks" / "data" / "syndai_docs_golden_v2.jsonl"
GOLDEN_V2_LOCK = ROOT / "benchmarks" / "data" / "syndai_docs_golden_v2.lock.json"

GOLDEN_SETS = [
    pytest.param(GOLDEN, GOLDEN_LOCK, id="v1"),
    pytest.param(GOLDEN_V2, GOLDEN_V2_LOCK, id="v2"),
]


def _skip_if_absent(path: Path) -> None:
    if not path.exists():
        pytest.skip(f"{path} not present")


def _rows(path: Path) -> list[dict]:
    return [json.loads(line) for line in path.read_text().split("\n") if line.strip()]


REQUIRED_GOLDEN_KEYS = {
    "question_id",
    "question_type",
    "is_abstention",
    "question",
    "question_date",
    "gold_answer",
    "multi_hop",
    "provenance",
}
# The subset run_reader.py actually consumes from an evidence row.
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


def _goldens() -> list[dict]:
    return [json.loads(line) for line in GOLDEN.read_text().split("\n") if line.strip()]


@pytest.mark.parametrize("golden_path,lock_path", GOLDEN_SETS)
def test_golden_lock_sha256_and_counts_match(golden_path: Path, lock_path: Path) -> None:
    _skip_if_absent(golden_path)
    lock = json.loads(lock_path.read_text())
    raw = golden_path.read_bytes()
    import hashlib

    assert hashlib.sha256(raw).hexdigest() == lock["sha256"], "golden JSONL drifted from its lock"
    goldens = _rows(golden_path)
    assert len(goldens) == lock["count"]
    assert sum(1 for g in goldens if g["multi_hop"]) == lock["multi_hop_count"]


@pytest.mark.parametrize("golden_path,lock_path", GOLDEN_SETS)
def test_golden_rows_are_well_formed(golden_path: Path, lock_path: Path) -> None:
    _skip_if_absent(golden_path)
    goldens = _rows(golden_path)
    assert goldens, "golden set is empty"
    ids = set()
    for g in goldens:
        assert REQUIRED_GOLDEN_KEYS <= set(g), f"missing keys: {REQUIRED_GOLDEN_KEYS - set(g)}"
        assert g["question_id"] not in ids, "duplicate question_id"
        ids.add(g["question_id"])
        assert g["is_abstention"] is False
        assert isinstance(g["question"], str) and g["question"].strip()
        assert isinstance(g["gold_answer"], str) and g["gold_answer"].strip()
        prov = g["provenance"]
        assert isinstance(prov, list) and prov
        for entry in prov:
            assert {"role", "file", "heading_path", "span", "char_start", "char_end"} <= set(entry)
            assert entry["char_end"] > entry["char_start"]
            assert entry["span"].strip()
        if g["multi_hop"]:
            assert len(prov) == 2
            assert {e["role"] for e in prov} == {"bridge", "answer"}
        else:
            assert len(prov) == 1 and prov[0]["role"] == "answer"


def test_evidence_row_shape_is_consumable_by_run_reader() -> None:
    gc = _gc()
    reader = _load("run_reader", "scripts/run_reader.py")
    goldens = _goldens()
    golden = goldens[0]
    row = gc.evidence_row(golden, ["body one text", "body two text"], k=10)
    assert REQUIRED_EVIDENCE_KEYS <= set(row)
    assert [item["rank"] for item in row["evidence"]] == [1, 2]
    assert all({"rank", "session_id", "body"} <= set(item) for item in row["evidence"])
    # run_reader must be able to build a prompt from it without error.
    prompt = reader.build_reader_prompt(row)
    assert golden["question"] in prompt


def test_provenance_hit_is_span_containment_and_multi_hop_needs_both() -> None:
    gc = _gc()
    single = {
        "multi_hop": False,
        "provenance": [{"role": "answer", "span": "Stripe and Payoneer"}],
    }
    assert gc.provenance_hit(single, ["...uses Stripe and Payoneer for payouts"], 10)
    assert not gc.provenance_hit(single, ["unrelated body", "another"], 10)
    # Rank cutoff is respected: a hit at rank 3 does not count at k=2.
    assert not gc.provenance_hit(single, ["x", "y", "Stripe and Payoneer"], 2)

    multi = {
        "multi_hop": True,
        "provenance": [
            {"role": "bridge", "span": "region Taipei"},
            {"role": "answer", "span": "value fifty"},
        ],
    }
    assert gc.provenance_hit(multi, ["region Taipei here", "the value fifty"], 10)
    # Only one of the two required spans present -> not a hit.
    assert not gc.provenance_hit(multi, ["region Taipei here", "no answer"], 10)


def test_corpus_manifest_is_well_formed() -> None:
    manifest = json.loads(MANIFEST.read_text())
    assert manifest["file_count"] == len(manifest["files"])
    assert manifest["git_commit"]
    assert manifest["excluded_prefixes"] == ["docs/superpowers/"]
    for entry in manifest["files"].values():
        assert len(entry["sha256"]) == 64
        assert entry["bytes"] >= 0


@pytest.mark.parametrize("golden_path,lock_path", GOLDEN_SETS)
def test_answer_spans_are_verbatim_in_the_pinned_corpus(golden_path: Path, lock_path: Path) -> None:
    """The strongest pin: every recorded span is present at its char offsets in
    the real corpus file (skipped when the golden file or the Syndai corpus is
    not on disk). v1 and v2 share the identical pinned corpus (MANIFEST)."""
    _skip_if_absent(golden_path)
    manifest = json.loads(MANIFEST.read_text())
    root = Path(manifest["syndai_root"])
    if not root.exists():
        pytest.skip(f"Syndai corpus not present at {root}")
    for g in _rows(golden_path):
        for entry in g["provenance"]:
            text = (root / entry["file"]).read_text(encoding="utf-8", errors="replace")
            excerpt = text[entry["char_start"] : entry["char_end"]]
            assert excerpt == entry["span"], (
                f"{g['question_id']} {entry['role']} span not verbatim at offsets in {entry['file']}"
            )


def test_v2_has_no_question_id_or_section_key_overlap_with_v1() -> None:
    """The R0-T3 mining sanity check, pinned permanently as a contract test:
    v2 was mined with --exclude-golden against v1, so the two sets must share
    zero question_ids and zero source_section_key values (skips if v2 absent,
    same pattern as the other v2-parameterized cases above)."""
    _skip_if_absent(GOLDEN_V2)
    v1_rows = _rows(GOLDEN)
    v2_rows = _rows(GOLDEN_V2)

    v1_ids = {g["question_id"] for g in v1_rows}
    v2_ids = {g["question_id"] for g in v2_rows}
    assert not (v1_ids & v2_ids), "v2 question_id collides with v1"

    def section_keys(rows: list[dict]) -> set[str]:
        keys: set[str] = set()
        for g in rows:
            keys.update(g["source_section_key"].split("||"))
        return keys

    v1_keys = section_keys(v1_rows)
    v2_keys = section_keys(v2_rows)
    assert not (v1_keys & v2_keys), "v2 source_section_key overlaps with v1"

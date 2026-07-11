"""Unit-level tests for the R0-T3 golden-set-miner additions:
``--exclude-golden`` candidate filtering, the question_id/section-key overlap
sanity check, and the corpus-pin drift check. All pure functions, no network,
no DB, no Syndai corpus dependency — run under ``pytest tests/``.
"""

from __future__ import annotations

import importlib.util
import json
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
def gc():
    return _load("gate_common", "scripts/gate_common.py")


@pytest.fixture(scope="module")
def miner():
    return _load("gate_mine_goldens", "scripts/gate_mine_goldens.py")


def make_section(gc, rel_path: str, heading: str, char_start: int, body: str):
    return gc.Section(rel_path, "root", [heading], char_start, char_start + len(body), body)


# --- load_excluded_keys ------------------------------------------------------


def test_load_excluded_keys_single_hop(tmp_path, miner):
    golden_path = tmp_path / "golden.jsonl"
    golden_path.write_text(
        json.dumps({"question_id": "q1", "source_section_key": "docs/a.md::H::0"}) + "\n"
    )
    assert miner.load_excluded_keys(golden_path) == {"docs/a.md::H::0"}


def test_load_excluded_keys_splits_multi_hop_on_double_pipe(tmp_path, miner):
    golden_path = tmp_path / "golden.jsonl"
    golden_path.write_text(
        json.dumps(
            {"question_id": "m1", "source_section_key": "docs/a.md::H1::0||docs/b.md::H2::10"}
        )
        + "\n"
    )
    assert miner.load_excluded_keys(golden_path) == {
        "docs/a.md::H1::0",
        "docs/b.md::H2::10",
    }


def test_load_excluded_keys_unions_across_rows(tmp_path, miner):
    golden_path = tmp_path / "golden.jsonl"
    rows = [
        {"question_id": "q1", "source_section_key": "docs/a.md::H::0"},
        {"question_id": "m1", "source_section_key": "docs/b.md::H::0||docs/c.md::H::5"},
    ]
    golden_path.write_text("\n".join(json.dumps(r) for r in rows) + "\n")
    assert miner.load_excluded_keys(golden_path) == {
        "docs/a.md::H::0",
        "docs/b.md::H::0",
        "docs/c.md::H::5",
    }


# --- filter_excluded ---------------------------------------------------------


def test_filter_excluded_is_noop_when_no_excluded_keys(gc, miner):
    s1 = make_section(gc, "docs/a.md", "H", 0, "one two three four five six seven")
    per_file = {"docs/a.md": [s1]}
    all_candidates = [s1]
    out_per_file, out_candidates = miner.filter_excluded(per_file, all_candidates, set())
    assert out_per_file == per_file
    assert out_candidates == all_candidates


def test_filter_excluded_drops_matching_section_from_both_structures(gc, miner):
    s1 = make_section(gc, "docs/a.md", "H1", 0, "kept section body text here")
    s2 = make_section(gc, "docs/a.md", "H2", 100, "excluded section body text here")
    per_file = {"docs/a.md": [s1, s2]}
    all_candidates = [s1, s2]

    out_per_file, out_candidates = miner.filter_excluded(per_file, all_candidates, {s2.key()})

    assert out_per_file == {"docs/a.md": [s1]}
    assert out_candidates == [s1]


def test_filter_excluded_leaves_non_matching_sections_untouched(gc, miner):
    s1 = make_section(gc, "docs/a.md", "H1", 0, "body one")
    s2 = make_section(gc, "docs/b.md", "H2", 0, "body two")
    per_file = {"docs/a.md": [s1], "docs/b.md": [s2]}
    all_candidates = [s1, s2]

    out_per_file, out_candidates = miner.filter_excluded(
        per_file, all_candidates, {"docs/nonexistent.md::X::0"}
    )

    assert out_per_file == per_file
    assert out_candidates == all_candidates


# --- assert_no_overlap -------------------------------------------------------


def test_assert_no_overlap_passes_on_disjoint_sets(miner):
    excluded = [{"question_id": "syndai_docs_s001_root", "source_section_key": "docs/a.md::H::0"}]
    new = [{"question_id": "syndai_docs_v2_s001_root", "source_section_key": "docs/z.md::H::0"}]
    miner.assert_no_overlap(new, excluded)  # must not raise


def test_assert_no_overlap_raises_on_question_id_collision(miner):
    excluded = [{"question_id": "syndai_docs_s001_root", "source_section_key": "docs/a.md::H::0"}]
    new = [{"question_id": "syndai_docs_s001_root", "source_section_key": "docs/z.md::H::0"}]
    with pytest.raises(RuntimeError, match="question_id collision"):
        miner.assert_no_overlap(new, excluded)


def test_assert_no_overlap_raises_on_section_key_overlap(miner):
    excluded = [{"question_id": "syndai_docs_s001_root", "source_section_key": "docs/a.md::H::0"}]
    new = [{"question_id": "syndai_docs_v2_s001_root", "source_section_key": "docs/a.md::H::0"}]
    with pytest.raises(RuntimeError, match="source_section_key overlap"):
        miner.assert_no_overlap(new, excluded)


def test_assert_no_overlap_checks_multi_hop_split_keys_too(miner):
    excluded = [
        {"question_id": "m1", "source_section_key": "docs/a.md::H::0||docs/b.md::H::5"},
    ]
    new = [
        {"question_id": "m2", "source_section_key": "docs/b.md::H::5||docs/c.md::H::9"},
    ]
    with pytest.raises(RuntimeError, match="source_section_key overlap"):
        miner.assert_no_overlap(new, excluded)


# --- verify_corpus_pin --------------------------------------------------------


def test_verify_corpus_pin_reports_no_drift_when_unchanged(tmp_path, miner):
    root = tmp_path / "corpus"
    root.mkdir()
    (root / "docs").mkdir()
    (root / "docs" / "a.md").write_text("hello world")
    import hashlib

    sha = hashlib.sha256(b"hello world").hexdigest()
    manifest_path = tmp_path / "manifest.lock.json"
    manifest_path.write_text(
        json.dumps({"files": {"docs/a.md": {"sha256": sha, "bytes": 11}}})
    )

    drift = miner.verify_corpus_pin(manifest_path, root, ["docs/a.md"])
    assert drift == []


def test_verify_corpus_pin_reports_sha256_mismatch(tmp_path, miner):
    root = tmp_path / "corpus"
    root.mkdir()
    (root / "docs").mkdir()
    (root / "docs" / "a.md").write_text("changed content")
    manifest_path = tmp_path / "manifest.lock.json"
    manifest_path.write_text(
        json.dumps({"files": {"docs/a.md": {"sha256": "0" * 64, "bytes": 11}}})
    )

    drift = miner.verify_corpus_pin(manifest_path, root, ["docs/a.md"])
    assert len(drift) == 1
    assert "sha256 mismatch" in drift[0]
    assert "docs/a.md" in drift[0]


def test_verify_corpus_pin_reports_missing_file(tmp_path, miner):
    root = tmp_path / "corpus"
    root.mkdir()
    manifest_path = tmp_path / "manifest.lock.json"
    manifest_path.write_text(
        json.dumps({"files": {"docs/gone.md": {"sha256": "0" * 64, "bytes": 0}}})
    )

    drift = miner.verify_corpus_pin(manifest_path, root, [])
    assert len(drift) == 1
    assert "MISSING" in drift[0]


def test_verify_corpus_pin_reports_new_file(tmp_path, miner):
    root = tmp_path / "corpus"
    root.mkdir()
    (root / "docs").mkdir()
    (root / "docs" / "new.md").write_text("new")
    manifest_path = tmp_path / "manifest.lock.json"
    manifest_path.write_text(json.dumps({"files": {}}))

    drift = miner.verify_corpus_pin(manifest_path, root, ["docs/new.md"])
    assert len(drift) == 1
    assert "NEW file" in drift[0]


# --- rel_to_root --------------------------------------------------------------


def test_rel_to_root_returns_posix_relative_path(miner):
    abs_path = str(ROOT / "benchmarks" / "data" / "syndai_docs_golden_v2.jsonl")
    assert miner.rel_to_root(abs_path) == "benchmarks/data/syndai_docs_golden_v2.jsonl"


def test_rel_to_root_passes_through_unrelated_absolute_path(miner):
    assert miner.rel_to_root("/etc/hosts") == "/etc/hosts"

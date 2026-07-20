from __future__ import annotations

import hashlib
import importlib.util
import json
from pathlib import Path

import pytest


ROOT = Path(__file__).resolve().parents[1]
MANIFEST = ROOT / "benchmarks/manifests/swe_explore.lock.json"


def _module():
    spec = importlib.util.spec_from_file_location(
        "run_swe_explore", ROOT / "scripts/run_swe_explore.py"
    )
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(module)
    return module


def _write_jsonl(path: Path, rows: list[dict]) -> bytes:
    raw = b"".join(
        json.dumps(row, sort_keys=True, separators=(",", ":")).encode() + b"\n"
        for row in rows
    )
    path.write_bytes(raw)
    return raw


def test_manifest_pins_official_code_dataset_and_protocol() -> None:
    lock = json.loads(MANIFEST.read_text())
    assert lock["code"]["commit"] == "3c12dc5a551937038afcbdb6eb6bbf19f3ddd8c1"
    assert lock["dataset"]["revision"] == "bdb0ae45d7c337d9e1dc3ebfe2a0af6bc7c1fbd9"
    assert lock["dataset"]["file"]["sha256"] == (
        "dc4f114ececd0bfb987361c26ae5e2440456e2cccb36adfccb09ea5385aec202"
    )
    assert lock["dataset"]["file"]["rows"] == 848
    assert lock["protocol"]["top_k"] == 5
    assert lock["protocol"]["primary_metrics"] == [
        "precision",
        "ndcg_at_500",
        "hit_file_rate",
        "context_efficiency",
    ]
    assert lock["public_execution_ready"] is False


def test_dataset_verification_and_gap_audit_fail_closed(tmp_path: Path) -> None:
    module = _module()
    dataset = tmp_path / "bench.jsonl"
    raw = _write_jsonl(
        dataset,
        [
            {"instance_id": "a__b-1", "ground_truth": {}},
            {"instance_id": "c__d-2", "ground_truth": {}},
        ],
    )
    expected = {
        "sha256": hashlib.sha256(raw).hexdigest(),
        "bytes": len(raw),
        "rows": 2,
    }

    audit = module.verify_dataset(dataset, expected)
    assert audit == {
        "rows": 2,
        "problem_statement_rows": 0,
        "base_commit_rows": 0,
    }
    with pytest.raises(RuntimeError, match="not publicly executable"):
        module.require_execution_inputs(audit)


def test_dataset_verification_rejects_drift(tmp_path: Path) -> None:
    module = _module()
    dataset = tmp_path / "bench.jsonl"
    raw = _write_jsonl(
        dataset,
        [{"instance_id": "a__b-1", "problem_statement": "bug", "base_commit": "abc"}],
    )
    expected = {
        "sha256": "0" * 64,
        "bytes": len(raw),
        "rows": 1,
    }
    with pytest.raises(RuntimeError, match="sha256 mismatch"):
        module.verify_dataset(dataset, expected)


def test_official_urls_are_revision_pinned() -> None:
    module = _module()
    lock = json.loads(MANIFEST.read_text())
    urls = module.release_urls(lock)
    assert lock["code"]["commit"] in urls["code_archive"]
    assert lock["dataset"]["revision"] in urls["dataset"]
    assert "/resolve/main/" not in urls["dataset"]

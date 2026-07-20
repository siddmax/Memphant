from __future__ import annotations

import importlib.util
import json
import statistics
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[1]


def _load_module():
    path = ROOT / "scripts" / "compare_retrieval_screen.py"
    spec = importlib.util.spec_from_file_location("compare_retrieval_screen", path)
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(module)
    return module


def _golden(tmp_path: Path) -> Path:
    path = tmp_path / "golden.jsonl"
    rows = [
        {"question_id": "q1", "provenance": [{"file": "a.md"}]},
        {"question_id": "q2", "provenance": [{"file": "a.md"}]},
        {"question_id": "q3", "provenance": [{"file": "b.md"}]},
        {
            "question_id": "q4",
            "provenance": [{"file": "b.md"}, {"file": "c.md"}],
        },
    ]
    path.write_text("".join(json.dumps(row) + "\n" for row in rows))
    return path


def _report(module, golden: Path, *, model: str, hits, latencies=(10, 20, 30, 40)):
    golden_revision = "sha256:" + module.sha256_file(golden)
    rows = []
    for index, (hit5, hit10) in enumerate(hits, start=1):
        rows.append(
            {
                "question_id": f"q{index}",
                "hit_at_5": bool(hit5),
                "hit_at_10": bool(hit10),
                "recall_e2e_ms": latencies[index - 1],
                "degraded": False,
                "fallback": False,
                "skipped": False,
                "failure": "none",
                "packed_items": 1,
                "evidence_packer_sha256": module.gc.EVIDENCE_PACKER_CONFIG["sha256"],
                "evidence_budget_tokens": 8192,
                "evidence_packed_tokens": 10,
                "evidence_truncated_items": 0,
                "evidence_dropped_items": 0,
                # Candidate-provided labels must never control clustering.
                "source_document": "candidate-controlled.md",
            }
        )
    runtime = {
        "runtime": "memphant-server resource ingest + /v1/recall",
        "embed_model": model,
        "k": 10,
        "recall_mode": "deep",
        "budget_tokens": 8192,
        "haystack_sections": 100,
        "golden_revision": golden_revision,
        "corpus_revision": "sha256:" + "c" * 64,
        "generation_identity": {
            "source_tree": {
                "git_commit": "d" * 40,
                "tracked_tree_sha256": "e" * 64,
            },
            "files": {"runner": "f" * 64},
            "database": {
                "schema_sha256": "1" * 64,
                "extensions_and_migrations_sha256": "2" * 64,
            },
            "migration_sources": {
                "files": {"001.sql": "3" * 64},
            },
        },
        "evidence_packer": module.gc.EVIDENCE_PACKER_CONFIG,
    }
    runtime["generation_identity"]["source_tree"]["sha256"] = module.fingerprint(
        runtime["generation_identity"]["source_tree"]
    )
    runtime["generation_identity"]["database"]["sha256"] = module.fingerprint(
        runtime["generation_identity"]["database"]
    )
    runtime["generation_identity"]["migration_sources"]["sha256"] = module.fingerprint(
        runtime["generation_identity"]["migration_sources"]["files"]
    )
    runtime["generation_identity"]["sha256"] = module.fingerprint(
        runtime["generation_identity"]
    )
    report = {
        "engine": "memphant",
        "expected_n": len(rows),
        "golden_count": len(rows),
        "golden_revision": golden_revision,
        "golden_sha256": module.sha256_file(golden),
        "corpus_revision": "sha256:" + "c" * 64,
        "runtime_config": runtime,
        "runtime_config_fingerprint": module.fingerprint(runtime),
        "fallback_count": 0,
        "degraded_count": 0,
        "skipped_count": 0,
        "reranker_failure_count": 0,
        "evidence_sha256": "a" * 64,
        "recall_at_5": sum(row[0] for row in hits) / len(rows),
        "recall_at_10": sum(row[1] for row in hits) / len(rows),
        "recall_e2e_ms_p50": statistics.median(latencies),
        "recall_e2e_ms_p95": statistics.quantiles(
            latencies, n=100, method="inclusive"
        )[94],
        "recall_e2e_p95_ceiling_ms": 1500,
        "recall_e2e_p95_within_ceiling": True,
        "per_question": rows,
    }
    report["provenance_sha256"] = module.gc.provenance_fingerprint(report)
    return report


def test_compare_pairs_exactly_and_bootstraps_source_document_clusters(tmp_path: Path):
    module = _load_module()
    golden = _golden(tmp_path)
    baseline = _report(
        module, golden, model="small", hits=[(0, 0), (0, 1), (1, 1), (0, 0)]
    )
    candidate = _report(
        module,
        golden,
        model="modernbert",
        hits=[(1, 1), (0, 1), (1, 1), (1, 1)],
        latencies=(100, 200, 300, 400),
    )

    result = module.compare_reports(
        [(baseline, candidate, golden)],
        variant_keys={"embed_model"},
        resamples=1000,
        seed=7,
    )

    assert result["n_paired"] == 4
    assert result["n_source_document_clusters"] == 3
    assert result["cluster_source"] == "pinned_golden.provenance[].file"
    assert result["baseline"]["recall_at_5"] == pytest.approx(0.25)
    assert result["candidate"]["recall_at_5"] == pytest.approx(0.75)
    assert result["delta_recall_at_5"]["mean"] == pytest.approx(0.5)
    assert result["delta_recall_at_10"]["mean"] == pytest.approx(0.5)
    assert result["candidate"]["recall_e2e_ms_p50"] == 250.0
    assert result["candidate"]["recall_e2e_ms_p95"] == 385.0
    assert result["hard_ceiling_ms"] == 1500
    assert result["candidate"]["p95_within_hard_ceiling"] is True
    assert result["sets"][0]["baseline"]["recall_e2e_ms_p50"] == 25.0
    assert result["sets"][0]["candidate"]["recall_e2e_ms_p95"] == 385.0


@pytest.mark.parametrize(
    "mutate,match",
    [
        (lambda report: report.update(corpus_revision="sha256:other"), "corpus"),
        (lambda report: report["per_question"].pop(), "expected_n"),
        (lambda report: report["per_question"][0].update(degraded=True), "degraded"),
        (lambda report: report["per_question"][0].update(fallback=True), "fallback"),
        (lambda report: report["per_question"][0].update(skipped=True), "skipped"),
        (lambda report: report["per_question"][0].update(failure="timeout"), "retrieval"),
        (lambda report: report["per_question"][0].pop("recall_e2e_ms"), "latency"),
    ],
)
def test_compare_rejects_incomplete_unhealthy_or_mixed_reports(
    tmp_path: Path, mutate, match: str
):
    module = _load_module()
    golden = _golden(tmp_path)
    baseline = _report(module, golden, model="small", hits=[(0, 0)] * 4)
    candidate = _report(module, golden, model="modernbert", hits=[(1, 1)] * 4)
    mutate(candidate)
    candidate["provenance_sha256"] = module.gc.provenance_fingerprint(candidate)

    with pytest.raises(ValueError, match=match):
        module.compare_reports(
            [(baseline, candidate, golden)],
            variant_keys={"embed_model"},
            resamples=100,
            seed=7,
        )


def test_compare_rejects_runtime_drift_outside_declared_arm_dimension(tmp_path: Path):
    module = _load_module()
    golden = _golden(tmp_path)
    baseline = _report(module, golden, model="small", hits=[(0, 0)] * 4)
    candidate = _report(module, golden, model="modernbert", hits=[(1, 1)] * 4)
    candidate["runtime_config"]["budget_tokens"] = 4096
    for row in candidate["per_question"]:
        row["evidence_budget_tokens"] = 4096
    candidate["runtime_config_fingerprint"] = module.fingerprint(
        candidate["runtime_config"]
    )
    candidate["provenance_sha256"] = module.gc.provenance_fingerprint(candidate)

    with pytest.raises(ValueError, match="runtime invariant"):
        module.compare_reports(
            [(baseline, candidate, golden)],
            variant_keys={"embed_model"},
            resamples=100,
            seed=7,
        )


def test_compare_accepts_explicit_candidate_selection_arm(tmp_path: Path):
    module = _load_module()
    golden = _golden(tmp_path)
    baseline = _report(module, golden, model="modernbert", hits=[(0, 0)] * 4)
    candidate = _report(module, golden, model="modernbert", hits=[(1, 1)] * 4)
    baseline["runtime_config"]["cross_rerank_candidates"] = "fused-head"
    candidate["runtime_config"]["cross_rerank_candidates"] = "vector-lexical-balanced"
    baseline["runtime_config_fingerprint"] = module.fingerprint(
        baseline["runtime_config"]
    )
    candidate["runtime_config_fingerprint"] = module.fingerprint(
        candidate["runtime_config"]
    )
    baseline["provenance_sha256"] = module.gc.provenance_fingerprint(baseline)
    candidate["provenance_sha256"] = module.gc.provenance_fingerprint(candidate)

    result = module.compare_reports(
        [(baseline, candidate, golden)],
        variant_keys={"cross_rerank_candidates"},
        resamples=1000,
        seed=7,
    )

    assert result["valid"] is True
    assert result["variant_keys"] == ["cross_rerank_candidates"]


def test_compare_rejects_arm_configuration_drift_between_golden_sets(tmp_path: Path):
    module = _load_module()
    golden = _golden(tmp_path)
    baseline = _report(module, golden, model="small", hits=[(0, 0)] * 4)
    candidate = _report(module, golden, model="modernbert", hits=[(1, 1)] * 4)
    changed_candidate = _report(module, golden, model="small", hits=[(1, 1)] * 4)

    with pytest.raises(ValueError, match="candidate arm configuration drift"):
        module.compare_reports(
            [
                (baseline, candidate, golden),
                (baseline, changed_candidate, golden),
            ],
            variant_keys={"embed_model"},
            resamples=100,
            seed=7,
        )


def test_compare_rejects_unpinned_golden_or_duplicate_questions(tmp_path: Path):
    module = _load_module()
    golden = _golden(tmp_path)
    baseline = _report(module, golden, model="small", hits=[(0, 0)] * 4)
    candidate = _report(module, golden, model="modernbert", hits=[(1, 1)] * 4)
    candidate["per_question"][3]["question_id"] = "q3"
    candidate["provenance_sha256"] = module.gc.provenance_fingerprint(candidate)

    with pytest.raises(ValueError, match="duplicate"):
        module.compare_reports(
            [(baseline, candidate, golden)],
            variant_keys={"embed_model"},
            resamples=100,
            seed=7,
        )

    candidate = _report(module, golden, model="modernbert", hits=[(1, 1)] * 4)
    golden.write_text(
        golden.read_text()
        + json.dumps({"question_id": "q5", "provenance": [{"file": "d.md"}]})
        + "\n"
    )
    with pytest.raises(ValueError, match="golden revision"):
        module.compare_reports(
            [(baseline, candidate, golden)],
            variant_keys={"embed_model"},
            resamples=100,
            seed=7,
        )

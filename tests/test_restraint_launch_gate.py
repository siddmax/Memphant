from __future__ import annotations

import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SCORECARD = ROOT / "docs" / "launch" / "restraint-launch-scorecard.json"


def load_scorecard() -> dict:
    return json.loads(SCORECARD.read_text(encoding="utf-8"))


def status_marks_restraint_complete() -> bool:
    status = (ROOT / "docs/superpowers/specs/memphant/STATUS.md").read_text(encoding="utf-8")
    return "- [x] **Restraint launch gate**" in status


def test_restraint_scorecard_enforces_op_bench_launch_threshold() -> None:
    scorecard = load_scorecard()

    assert scorecard["status"] in {"pass", "candidate", "fail"}
    if status_marks_restraint_complete():
        assert scorecard["status"] == "pass"
    assert scorecard["metric"] == "relative_drop_vs_memory_free"
    assert scorecard["threshold_max_drop"] == 0.15
    assert scorecard["relevance_gate_mandatory_if_drop_exceeds_threshold"] is True
    assert scorecard["pinned_block_in_scope"] is True
    if scorecard["status"] == "pass":
        assert scorecard["benchmark"] in {"op-bench", "ps-bench"}
        assert scorecard["sample_count"] >= 50
        assert scorecard["ci"]["upper"] <= scorecard["threshold_max_drop"]
        assert scorecard["measured_drop"] <= scorecard["threshold_max_drop"]


def test_restraint_profile_axis_matches_scorecard_measurement() -> None:
    scorecard = load_scorecard()
    profile = json.loads((ROOT / scorecard["profile_path"]).read_text(encoding="utf-8"))
    restraint = profile["axes"]["restraint"]

    assert restraint["benchmark"] == scorecard["benchmark"]
    assert restraint["gate"] == scorecard["status"]
    if scorecard["status"] == "pass":
        assert restraint["source_status"] == "sampled_public"
        assert restraint["sample_count"] == scorecard["sample_count"]
    assert restraint["score"] == scorecard["memphant_score"]
    assert restraint["baseline_score"] == scorecard["memory_free_baseline_score"]
    assert abs(restraint["delta_vs_baseline"]) == scorecard["measured_drop"]
    assert restraint["trace_ref"] in scorecard["trace_refs"]


def test_restraint_trace_has_no_mismatches() -> None:
    scorecard = load_scorecard()
    for trace_ref in scorecard["trace_refs"]:
        trace = json.loads((ROOT / trace_ref).read_text(encoding="utf-8"))
        if scorecard["status"] == "pass":
            assert trace["metrics"]["total_cases"] >= 50
            assert trace["source_status"] == "sampled_public"
            assert trace["metrics"]["passed_cases"] == trace["metrics"]["total_cases"]
            for case in trace["case_results"]:
                assert case["passed"] is True
                assert case["dropped_mismatches"] == []
        else:
            assert trace["metrics"]["passed_cases"] < trace["metrics"]["total_cases"]


def test_pinned_block_content_is_in_scope_for_restraint_gate() -> None:
    scorecard = load_scorecard()
    owner_text = "\n".join((ROOT / path).read_text(encoding="utf-8") for path in scorecard["owner_refs"])

    assert "pinned-block content" in owner_text or "pinned block" in owner_text
    assert "OP-Bench-gated" in owner_text
    assert "must not drop >15%" in owner_text

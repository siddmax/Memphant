from __future__ import annotations

import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SCORECARD = ROOT / "docs" / "launch" / "standing-quality-bars.json"


def load_scorecard() -> dict:
    return json.loads(SCORECARD.read_text(encoding="utf-8"))


def test_standing_quality_bars_all_pass() -> None:
    scorecard = load_scorecard()

    assert scorecard["status"] == "pass"
    assert set(scorecard["bars"]) == {
        "hot_path_slo",
        "memory_utility_trend",
        "landscape_completeness",
    }
    for name, bar in scorecard["bars"].items():
        assert bar["status"] == "pass", name


def test_hot_path_slo_has_executable_threshold_guard_and_profile_proof() -> None:
    bar = load_scorecard()["bars"]["hot_path_slo"]
    test_path = ROOT / "crates/memphant-core/tests/hot_path_slo.rs"
    test_source = test_path.read_text(encoding="utf-8")
    profile = json.loads((ROOT / bar["proofs"][1]).read_text(encoding="utf-8"))

    assert bar["mode"] == "fast"
    assert bar["thresholds_ms"] == {"p50_lt": 200, "p95_lt": 500}
    assert "FAST_P50_LIMIT" in test_source
    assert "FAST_P95_LIMIT" in test_source
    assert "RecallMode::Fast" in test_source
    assert profile["id"] == bar["profile_decision_source"]
    measured_p95s = [
        decision["p95_ms"]
        for decision in profile["rung_decisions"] + profile["activation_decisions"]
        if decision.get("p95_ms", 0) > 0
    ]
    assert measured_p95s
    assert max(measured_p95s) < bar["thresholds_ms"]["p95_lt"]


def test_memory_utility_trend_is_wired_to_public_mark_contract() -> None:
    bar = load_scorecard()["bars"]["memory_utility_trend"]
    trace_compare = json.loads((ROOT / bar["proofs"][0]).read_text(encoding="utf-8"))
    core_test = (ROOT / "crates/memphant-core/tests/surface_mutations.rs").read_text(
        encoding="utf-8"
    )
    rest_test = (ROOT / "crates/memphant-server/tests/rest_contract.rs").read_text(
        encoding="utf-8"
    )
    mcp_source = (ROOT / "crates/memphant-mcp/src/lib.rs").read_text(encoding="utf-8")

    assert bar["lane"] == trace_compare["case_id"]
    assert trace_compare["answer_bearing_recall"] == 1.0
    assert bar["current_success_rate"] >= bar["baseline_success_rate"]
    assert bar["declined_vs_baseline"] is False
    assert set(bar["outcomes_counted"]) == {"success", "failure", "corrected", "ignored"}
    assert bar["mark_contract"]["required_fields"] == ["trace_id", "used_ids", "outcome"]
    assert "mark_records_outcome_feedback_for_trace" in core_test
    assert '"/v1/mark"' in rest_test
    assert "MarkRequest" in mcp_source
    assert "record_mark" in mcp_source


def test_landscape_completeness_lists_every_verified_threshold_repo() -> None:
    bar = load_scorecard()["bars"]["landscape_completeness"]
    prior_art = (
        ROOT / "docs/superpowers/specs/memphant/13-prior-art-and-competitive-spec.md"
    ).read_text(encoding="utf-8")

    assert bar["threshold_stars_gte"] == 50000
    assert bar["entries"]
    for entry in bar["entries"]:
        assert entry["stars"] >= 50000, entry["repo"]
        assert entry["listed_or_excluded"] in {"listed", "excluded"}
        if entry["listed_or_excluded"] == "listed":
            assert entry["repo"] in prior_art, entry["repo"]

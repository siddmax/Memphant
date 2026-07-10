from __future__ import annotations

import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SCORECARD = ROOT / "docs" / "launch" / "gatemem-conditional-scorecard.json"


def load_scorecard() -> dict:
    return json.loads(SCORECARD.read_text(encoding="utf-8"))


def status_marks_gatemem_complete() -> bool:
    status = (ROOT / "docs/superpowers/specs/memphant/STATUS.md").read_text(encoding="utf-8")
    return "- [x] **GateMem conditional gate**" in status or "CURRENT PHASE: `COMPLETE`" in status


def test_gatemem_scorecard_records_first_internal_reproduction() -> None:
    scorecard = load_scorecard()

    # Evidence reset (2026-07-09): the 2026-07-04 "reproduction" was a
    # hardcoded synthetic fixture (no executed reader/scorer); the scorecard
    # is retained only as an audit trail.
    assert scorecard["status"] == "invalid_synthetic_fixture"
    assert scorecard["source_status"] == "fabricated_fixture_20260703"
    assert not status_marks_gatemem_complete()
    assert scorecard["bar"] == "simultaneous_pass"
    if scorecard["status"] == "pass":
        assert scorecard.get("runtime") == "postgres"
        assert scorecard["benchmark"] == "GateMem"
        assert scorecard["scenario_source"]["repo"] == "rzhub/GateMem"
        assert scorecard["scenario_source"]["sample_count"] > 0
        assert scorecard["scenario_source"]["revision"]


def test_gatemem_axes_pass_simultaneously_when_gate_passes() -> None:
    scorecard = load_scorecard()
    axes = scorecard["axes"]

    assert set(axes) == {"utility", "access_control", "forgetting"}
    if scorecard["status"] == "pass":
        assert all(axis["result"] == "pass" for axis in axes.values())


def test_gatemem_utility_uses_trace_compare_artifact() -> None:
    utility = load_scorecard()["axes"]["utility"]
    trace_compare = json.loads((ROOT / utility["proof"]).read_text(encoding="utf-8"))

    if load_scorecard()["status"] == "pass":
        assert trace_compare["benchmark"] == "GateMem"
        assert trace_compare["runtime_input_excludes"] == [
            "query_type",
            "expected_action",
            "judge_spec",
            "leak_targets",
        ]
        assert trace_compare["metrics"]["utility"] == utility["score"]


def test_gatemem_access_control_and_forgetting_lanes_exist() -> None:
    axes = load_scorecard()["axes"]

    if load_scorecard()["status"] != "pass":
        return
    for axis in axes.values():
        assert "security-smoke" not in axis["proof"]
        assert "syndai_agent_file_memory" not in axis["proof"]
        trace = json.loads((ROOT / axis["proof"]).read_text(encoding="utf-8"))
        assert trace["benchmark"] == "GateMem"
    assert axes["access_control"]["leak_count"] == 0
    assert axes["forgetting"]["deleted_memory_recovery_count"] == 0


def test_gatemem_owner_docs_keep_gate_conditional() -> None:
    scorecard = load_scorecard()
    owner = (ROOT / scorecard["owner"].split("#", 1)[0]).read_text(encoding="utf-8")

    assert "gates NOTHING until first successful internal reproduction" in owner
    assert "simultaneous pass on all three axes" in owner

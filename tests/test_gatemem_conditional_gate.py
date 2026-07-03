from __future__ import annotations

import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SCORECARD = ROOT / "docs" / "launch" / "gatemem-conditional-scorecard.json"


def load_scorecard() -> dict:
    return json.loads(SCORECARD.read_text(encoding="utf-8"))


def test_gatemem_scorecard_records_first_internal_reproduction() -> None:
    scorecard = load_scorecard()

    assert scorecard["benchmark"] == "gatemem-internal-reproduction"
    assert scorecard["first_internal_reproduction"] is True
    assert scorecard["bar"] == "simultaneous_pass"
    assert scorecard["status"] == "pass"


def test_gatemem_axes_pass_simultaneously() -> None:
    axes = load_scorecard()["axes"]

    assert set(axes) == {"utility", "access_control", "forgetting"}
    assert all(axis["result"] == "pass" for axis in axes.values())
    assert axes["utility"]["score"] == 1.0


def test_gatemem_utility_uses_trace_compare_artifact() -> None:
    utility = load_scorecard()["axes"]["utility"]
    trace_compare = json.loads((ROOT / utility["proof"]).read_text(encoding="utf-8"))

    assert trace_compare["answer_bearing_recall"] == 1.0
    assert trace_compare["forbidden_returned"] == []
    assert trace_compare["missing_answer_bearing"] == []


def test_gatemem_access_control_and_forgetting_lanes_exist() -> None:
    security = (ROOT / "examples/evals/security-smoke.yaml").read_text(encoding="utf-8")
    axes = load_scorecard()["axes"]

    assert f"kind: {axes['access_control']['security_lane']}" in security
    assert f"kind: {axes['forgetting']['security_lane']}" in security
    assert "expect_rejected: true" in security
    assert "invalidated_units:" in security


def test_gatemem_owner_docs_keep_gate_conditional() -> None:
    scorecard = load_scorecard()
    owner = (ROOT / scorecard["owner"].split("#", 1)[0]).read_text(encoding="utf-8")

    assert "gates NOTHING until first successful internal reproduction" in owner
    assert "simultaneous pass on all three axes" in owner

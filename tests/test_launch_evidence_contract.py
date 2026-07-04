from __future__ import annotations

import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
STATUS = ROOT / "docs/superpowers/specs/memphant/STATUS.md"
PUBLIC_SCORECARD = ROOT / "docs/launch/public-launch-scorecard.json"
RESTRAINT_SCORECARD = ROOT / "docs/launch/restraint-launch-scorecard.json"
PUBLIC_LOG = ROOT / "docs/build-log/2026-07-03-public-launch-gate.md"
RESTRAINT_LOG = ROOT / "docs/build-log/2026-07-03-restraint-launch-gate.md"


def read_json(path: Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8"))


def checked(label: str) -> bool:
    return f"- [x] **{label}**" in STATUS.read_text(encoding="utf-8")


def test_launch_scorecards_derive_status_from_checked_in_traces() -> None:
    public = read_json(PUBLIC_SCORECARD)
    restraint = read_json(RESTRAINT_SCORECARD)
    profile = read_json(ROOT / public["profile"]["path"])
    lme_trace = read_json(ROOT / profile["axes"]["long_horizon"]["trace_ref"])
    restraint_trace = read_json(ROOT / restraint["trace_refs"][0])

    lme_pass = lme_trace["metrics"]["passed_cases"] == lme_trace["metrics"]["total_cases"]
    restraint_score = (
        restraint_trace["metrics"]["passed_cases"] / restraint_trace["metrics"]["total_cases"]
    )
    restraint_drop = 1.0 - restraint_score

    assert restraint["memphant_score"] == restraint_score
    assert restraint["measured_drop"] == restraint_drop
    assert restraint["status"] == (
        "pass" if restraint_drop <= restraint["threshold_max_drop"] else "fail"
    )
    assert public["status"] == (
        "pass"
        if lme_pass
        and restraint["status"] == "pass"
        and lme_trace["metrics"]["recall_p95_ms"] is not None
        else "candidate_pass"
    )

    if checked("Public launch gate"):
        assert public["status"] == "pass"
    if checked("Restraint launch gate"):
        assert restraint["status"] == "pass"


def test_launch_build_logs_match_scorecards() -> None:
    public = read_json(PUBLIC_SCORECARD)
    restraint = read_json(RESTRAINT_SCORECARD)
    public_log = PUBLIC_LOG.read_text(encoding="utf-8")
    restraint_log = RESTRAINT_LOG.read_text(encoding="utf-8")

    assert public["profile"]["path"] in public_log
    assert public["profile"]["sample_manifest"] in public_log
    for trace_ref in public["profile"]["public_sampled_trace_refs"]:
        assert trace_ref in public_log
    assert f"Status: `{public['status']}`" in public_log

    assert restraint["profile_path"] in restraint_log
    assert restraint["trace_refs"][0] in restraint_log
    assert f"Status: `{restraint['status']}`" in restraint_log
    assert f"Measured drop: `{restraint['measured_drop']}`" in restraint_log
    assert "rung15-inferred-belief" not in public_log
    assert "rung15-inferred-belief" not in restraint_log
    if public["status"] == "pass" and restraint["status"] == "pass":
        assert "candidate" not in public_log.lower()
        assert "next unchecked" not in restraint_log.lower()


def test_done_definition_explains_dormant_activation_gates() -> None:
    status = STATUS.read_text(encoding="utf-8")

    assert "DORMANT with unmet activation gate" in status
    assert "terminal for §5" in status
    assert "CURRENT PHASE: `COMPLETE`" not in status or (
        checked("Public launch gate") and checked("Restraint launch gate")
    )

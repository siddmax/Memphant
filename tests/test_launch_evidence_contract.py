from __future__ import annotations

import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
STATUS = ROOT / "docs/superpowers/specs/memphant/STATUS.md"
PUBLIC_SCORECARD = ROOT / "docs/launch/public-launch-scorecard.json"
RESTRAINT_SCORECARD = ROOT / "docs/launch/restraint-launch-scorecard.json"
GATEMEM_SCORECARD = ROOT / "docs/launch/gatemem-conditional-scorecard.json"
GATE_SCORECARDS = [PUBLIC_SCORECARD, RESTRAINT_SCORECARD, GATEMEM_SCORECARD]
PUBLIC_LOG = ROOT / "docs/build-log/2026-07-03-public-launch-gate.md"
RESTRAINT_LOG = ROOT / "docs/build-log/2026-07-03-restraint-launch-gate.md"

REOPENED_GATE_LABELS = [
    "Public launch gate",
    "Restraint launch gate",
    "GateMem conditional gate",
    "Dogfood gate",
]


def read_json(path: Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8"))


def checked(label: str) -> bool:
    return f"- [x] **{label}**" in STATUS.read_text(encoding="utf-8")


def test_launch_scorecards_are_invalidated_as_synthetic_fixtures() -> None:
    for path in GATE_SCORECARDS:
        scorecard = read_json(path)
        assert scorecard["status"] == "invalid_synthetic_fixture", path.name
        assert scorecard["source_status"] == "fabricated_fixture_20260703", path.name


def test_no_gate_scorecard_passes_without_postgres_runtime() -> None:
    # A launch/gate scorecard may only claim "pass" when its evidence was
    # produced by the packaged Postgres-backed runtime (promotion-provenance rule).
    for path in GATE_SCORECARDS:
        scorecard = read_json(path)
        if scorecard["status"] == "pass":
            assert scorecard.get("runtime") == "postgres", path.name


def test_status_ledger_reopened_synthetic_promotions() -> None:
    status = STATUS.read_text(encoding="utf-8")

    assert "**Promotion-provenance rule (2026-07-09):**" in status
    assert "Synthetic fixtures gate regressions, never promotions." in status
    assert "CURRENT PHASE: `RUNTIME INCOMPLETE`" in status

    for label in REOPENED_GATE_LABELS:
        assert not checked(label), label
    assert "- [x] **WS-F**" not in status
    assert "- [x] **WS-G**" not in status
    for rung in range(4, 16):
        assert f"[x] {rung} " not in status, f"rung {rung} must stay reopened"
    # Rungs 0-3 remain built (built locally, not promoted from synthetic evidence).
    for rung in range(0, 4):
        assert f"[x] {rung} " in status, f"rung {rung} stays checked"
    assert "reopened 2026-07-09: promotion evidence was synthetic fixtures" in status


def test_launch_build_logs_remain_as_audit_trail() -> None:
    public = read_json(PUBLIC_SCORECARD)
    restraint = read_json(RESTRAINT_SCORECARD)
    public_log = PUBLIC_LOG.read_text(encoding="utf-8")
    restraint_log = RESTRAINT_LOG.read_text(encoding="utf-8")

    # The build logs stay as audit trail for the invalidated 2026-07-03/04 run:
    # their referenced artifacts must still resolve, but their recorded statuses
    # no longer govern (the scorecards are invalid_synthetic_fixture).
    assert public["profile"]["path"] in public_log
    assert public["profile"]["sample_manifest"] in public_log
    for trace_ref in public["profile"]["public_sampled_trace_refs"]:
        assert trace_ref in public_log
    assert restraint["profile_path"] in restraint_log
    assert restraint["trace_refs"][0] in restraint_log


def test_done_definition_explains_dormant_activation_gates() -> None:
    status = STATUS.read_text(encoding="utf-8")

    assert "DORMANT with unmet activation gate" in status
    assert "terminal for §5" in status
    assert "CURRENT PHASE: `COMPLETE`" not in status or (
        checked("Public launch gate") and checked("Restraint launch gate")
    )

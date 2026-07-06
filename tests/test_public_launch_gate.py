from __future__ import annotations

import json
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SCORECARD = ROOT / "docs" / "launch" / "public-launch-scorecard.json"


def load_scorecard() -> dict:
    return json.loads(SCORECARD.read_text(encoding="utf-8"))


def status_marks_public_launch_complete() -> bool:
    status = (ROOT / "docs/superpowers/specs/memphant/STATUS.md").read_text(encoding="utf-8")
    return "- [x] **Public launch gate**" in status


def test_public_launch_scorecard_covers_every_gate_criterion() -> None:
    scorecard = load_scorecard()
    criteria = {entry["name"]: entry for entry in scorecard["criteria"]}

    if status_marks_public_launch_complete():
        assert scorecard["status"] == "pass"
    else:
        assert scorecard["status"] in {"pass", "candidate_pass", "fail"}
    assert set(criteria) == {
        "public_api_sdk_mcp_cli_docs_examples",
        "self_host_docker_compose",
        "security_policy_and_release_process",
        "golden_security_sampled_deletion_gates",
        "reproduced_public_benchmark_profile",
        "hosted_db_exposure",
        "no_hidden_syndai_only_behavior",
        "public_sota_claim_policy",
    }
    for entry in criteria.values():
        assert entry["proofs"], entry["name"]
        for proof in entry["proofs"]:
            assert (ROOT / proof).exists(), proof


def test_release_process_and_ci_run_public_launch_gates() -> None:
    release_process = (ROOT / "docs/release-process.md").read_text(encoding="utf-8")
    workflow = (ROOT / ".github/workflows/ci.yml").read_text(encoding="utf-8")

    required_commands = [
        "cargo fmt --check",
        "cargo clippy --all-targets --all-features -- -D warnings",
        "cargo test --all-targets --all-features",
        "cargo test --doc",
        "python -m pytest tests -q",
        "cargo run -p memphant-eval -- verify-golden examples/evals/golden.yaml",
        "cargo run -p memphant-eval -- run benchmarks/nightly-sampled.yaml",
        "cargo run -p memphant-eval -- security examples/evals/security-smoke.yaml",
        "cargo run -p memphant-cli -- db bootstrap-check --provider supabase",
        "npm test",
    ]

    for command in required_commands:
        assert command in release_process
        assert command in workflow
    assert "python3 scripts/check_spec_drift.py" in release_process
    assert "scripts/check_spec_drift.py" not in workflow


def test_public_benchmark_profile_is_real_sampled_public_evidence() -> None:
    scorecard = load_scorecard()
    profile_path = ROOT / scorecard["profile"]["path"]
    profile = json.loads(profile_path.read_text(encoding="utf-8"))
    manifest_path = ROOT / scorecard["profile"]["sample_manifest"]
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))

    assert profile["harness_pin"]["answer_model"]
    assert profile["harness_pin"]["embedding_profile"]
    assert manifest["tier"] == "sampled-public"
    assert manifest["root_seed"]
    assert manifest["benchmarks"]
    for benchmark in manifest["benchmarks"]:
        assert benchmark["sample_count"] >= 50
        assert benchmark["license"]
        assert benchmark["source_revision"]
        assert benchmark["sample_ids"]

    sampled_axes = [
        axis
        for axis in profile["axes"].values()
        if axis["source_status"] == "sampled_public"
    ]
    assert sampled_axes
    for axis in sampled_axes:
        trace_ref = ROOT / axis["trace_ref"]
        assert trace_ref.exists(), axis["trace_ref"]
        assert axis["score"] is not None
        assert axis["ci"] is not None
        assert axis["sample_count"] >= 50
        assert axis["sample_manifest"] == scorecard["profile"]["sample_manifest"]

    for axis in profile["axes"].values():
        assert axis["source_status"] not in {"sampled_public_style", "internal_mimic"}
        if axis["source_status"] == "not_run":
            assert axis["benchmark"]

    decisions = profile["rung_decisions"] + profile["activation_decisions"]
    measured = [
        decision
        for decision in decisions
        if decision.get("p95_ms", 0) > 0
        and decision.get("cost_per_1k_recalls_usd") is not None
        and decision.get("security_result") == "pass"
        and decision.get("deletion_result") == "pass"
    ]
    if status_marks_public_launch_complete():
        assert measured
    for decision in measured:
        after_trace = decision.get("after_trace_ref")
        if after_trace:
            assert (ROOT / after_trace).exists(), after_trace


def test_hosted_db_exposure_gate_is_fail_closed_for_supabase() -> None:
    scorecard = load_scorecard()
    hosted = next(entry for entry in scorecard["criteria"] if entry["name"] == "hosted_db_exposure")
    assert hosted["critical_findings"] == []

    supabase_profile = (ROOT / "deploy/provider-profiles/supabase.env.example").read_text(
        encoding="utf-8"
    )
    assert "MEMPHANT_SUPABASE_EXPOSED_SCHEMAS=public" in supabase_profile
    assert "MEMPHANT_SUPABASE_ANON_HAS_MEMPHANT_ACCESS=false" in supabase_profile
    assert "MEMPHANT_SUPABASE_AUTHENTICATED_HAS_MEMPHANT_ACCESS=false" in supabase_profile
    assert "MEMPHANT_SUPABASE_ADVISORS_REQUIRED=true" in supabase_profile
    assert "--fail-on warning" in supabase_profile


def test_public_surfaces_have_no_hidden_syndai_only_fields() -> None:
    scorecard = load_scorecard()
    public_surface = next(
        entry for entry in scorecard["criteria"] if entry["name"] == "no_hidden_syndai_only_behavior"
    )

    for proof in public_surface["proofs"]:
        text = (ROOT / proof).read_text(encoding="utf-8").lower()
        assert "syndai" not in text, proof
        assert "dogfood" not in text, proof


def test_public_sota_claim_policy_is_explicit_and_bare_claims_are_guarded() -> None:
    scorecard = load_scorecard()
    claim = scorecard["sota_claim"]
    assert claim["claim_made"] is False
    assert claim["axis"] is None

    result = subprocess.run(
        ["npm", "test"],
        cwd=ROOT / "web",
        text=True,
        capture_output=True,
        check=False,
    )
    assert result.returncode == 0, result.stdout + result.stderr

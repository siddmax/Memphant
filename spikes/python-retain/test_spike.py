from __future__ import annotations

from pathlib import Path

from memphant_spike import load_golden, load_policy, run_golden


ROOT = Path(__file__).resolve().parents[2]


def test_python_retain_runner_uses_external_policy() -> None:
    policy = load_policy(ROOT / "examples" / "spike" / "policies" / "extraction-policy-v1.json")
    golden = load_golden(ROOT / "examples" / "spike" / "golden.jsonl")

    result = run_golden(policy, golden)

    assert result == {
        "case_checkout_token": ["callback token version:v2"],
        "case_release_channel": ["release channel:#release"],
    }

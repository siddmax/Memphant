from __future__ import annotations

import json
import statistics
import subprocess
import sys
import tempfile
import time
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
POLICY = ROOT / "examples" / "spike" / "policies" / "extraction-policy-v1.json"
GOLDEN = ROOT / "examples" / "spike" / "golden.jsonl"
ARTIFACT = ROOT / "docs" / "build-log" / "artifacts" / "ws0-two-language-spike.json"
CARGO = Path.home() / ".cargo" / "bin" / "cargo"
RUSTC = Path.home() / ".cargo" / "bin" / "rustc"
RUST_MANIFEST = ROOT / "spikes" / "rust-retain" / "Cargo.toml"
RUST_BINARY = ROOT / "spikes" / "rust-retain" / "target" / "debug" / "memphant-rust-retain-spike"
SAMPLES = 5


def run_command(command: list[str], *, stdout: int | None = None) -> float:
    start = time.perf_counter()
    subprocess.run(command, cwd=ROOT, check=True, stdout=stdout)
    return time.perf_counter() - start


def decision_for_ratio(ratio: float) -> str:
    if ratio < 1.5:
        return "rust_proceeds"
    if ratio >= 3:
        return "reopen_decision_2"
    return "manual_review"


def mutate_policy(source: Path, target: Path) -> None:
    policy = json.loads(source.read_text(encoding="utf-8"))
    for rule in policy["rules"]:
        if rule["subject"] == "release channel":
            rule["value"] = "#launch"
            rule["contains"] = "#launch"
    target.write_text(json.dumps(policy, indent=2) + "\n", encoding="utf-8")


def mutate_golden(source: Path, target: Path) -> None:
    lines = []
    for line in source.read_text(encoding="utf-8").splitlines():
        row = json.loads(line)
        if row["id"] == "case_release_channel":
            row["episodes"][0]["body"] = "Prod deploy requires manual approval in #launch before merge."
            row["expected"] = ["release channel:#launch"]
        lines.append(json.dumps(row, separators=(",", ":")))
    target.write_text("\n".join(lines) + "\n", encoding="utf-8")


def ensure_rust_ready() -> None:
    if not CARGO.exists() or not RUSTC.exists():
        raise RuntimeError("cargo_or_rustc_missing")
    subprocess.run([str(RUSTC), "--version"], cwd=ROOT, check=True)


def python_command(policy: Path, golden: Path) -> list[str]:
    return [
        "python3",
        "-c",
        (
            "from pathlib import Path; "
            "import sys; "
            "sys.path.insert(0, 'spikes/python-retain'); "
            "from memphant_spike import load_policy, load_golden, run_golden; "
            f"run_golden(load_policy(Path({str(policy)!r})), "
            f"load_golden(Path({str(golden)!r})))"
        ),
    ]


def sample(command: list[str]) -> dict[str, float | list[float]]:
    timings = [
        run_command(command, stdout=subprocess.DEVNULL)
        for _ in range(SAMPLES)
    ]
    return {
        "seconds": statistics.median(timings),
        "samples_seconds": timings,
    }


def main() -> int:
    with tempfile.TemporaryDirectory() as tmpdir_raw:
        tmpdir = Path(tmpdir_raw)
        changed_policy = tmpdir / "policy.json"
        changed_golden = tmpdir / "golden.jsonl"
        mutate_policy(POLICY, changed_policy)
        mutate_golden(GOLDEN, changed_golden)

        subprocess.run(["python3", "-m", "pytest", "spikes/python-retain/test_spike.py", "-q"], cwd=ROOT, check=True)

        ensure_rust_ready()
        rust_build = run_command(
            [
                str(CARGO),
                "build",
                "--manifest-path",
                str(RUST_MANIFEST),
            ]
        )
        rust_command = [str(RUST_BINARY)]

        # Warm both runners once so the R83 measurement reflects policy iteration, not first process load.
        run_command(python_command(changed_policy, changed_golden), stdout=subprocess.DEVNULL)
        run_command(rust_command + [str(changed_policy), str(changed_golden)], stdout=subprocess.DEVNULL)

        python_baseline = sample(python_command(POLICY, GOLDEN))
        python_changed = sample(python_command(changed_policy, changed_golden))
        rust_baseline = sample(rust_command + [str(POLICY), str(GOLDEN)])
        rust_changed = sample(rust_command + [str(changed_policy), str(changed_golden)])

    ratio = rust_changed["seconds"] / python_changed["seconds"] if python_changed["seconds"] else float("inf")
    decision = decision_for_ratio(ratio)
    ARTIFACT.parent.mkdir(parents=True, exist_ok=True)
    ARTIFACT.write_text(
        json.dumps(
            {
                "decision": decision,
                "measurement_mode": (
                    "median of warm no-recompile policy-runner invocations; "
                    "Rust cargo build recorded separately and excluded from policy-change ratio"
                ),
                "sample_count": SAMPLES,
                "python_baseline_seconds": python_baseline["seconds"],
                "python_baseline_samples_seconds": python_baseline["samples_seconds"],
                "python_policy_change_seconds": python_changed["seconds"],
                "python_policy_change_samples_seconds": python_changed["samples_seconds"],
                "rust_baseline_seconds": rust_baseline["seconds"],
                "rust_baseline_samples_seconds": rust_baseline["samples_seconds"],
                "rust_build_seconds": rust_build,
                "rust_policy_change_seconds": rust_changed["seconds"],
                "rust_policy_change_samples_seconds": rust_changed["samples_seconds"],
                "rust_to_python_policy_change_ratio": ratio,
            },
            indent=2,
            sort_keys=True,
        )
        + "\n",
        encoding="utf-8",
    )
    print(f"artifact={ARTIFACT}")
    print(f"ratio={ratio:.3f}")
    print(f"decision={decision}")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as error:
        print(f"spike_error={error}", file=sys.stderr)
        raise SystemExit(1)

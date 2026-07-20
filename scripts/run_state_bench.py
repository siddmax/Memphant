#!/usr/bin/env python3
"""Acquire, audit, and natively aggregate the pinned STATE-Bench release."""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_MANIFEST = ROOT / "benchmarks" / "manifests" / "state_bench.lock.json"


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def verify_official_repo(
    repo: Path, manifest: dict, *, verify_revision: bool = True
) -> None:
    if verify_revision:
        revision = subprocess.run(
            ["git", "-C", str(repo), "rev-parse", "HEAD"],
            check=True,
            capture_output=True,
            text=True,
        ).stdout.strip()
        expected = manifest["code"]["revision"]
        if revision != expected:
            raise ValueError(f"official STATE-Bench revision mismatch: {revision} != {expected}")

    for relative, expected in manifest["native_scorer"]["files"].items():
        path = repo / relative
        actual = sha256_file(path) if path.is_file() else "missing"
        if actual != expected:
            raise ValueError(f"official STATE-Bench source hash mismatch for {relative}")


def _json_ids(directory: Path) -> set[str]:
    return {path.stem for path in directory.glob("*.json") if path.is_file()}


def verify_domain_inventory(repo: Path, domain: str, expected: dict) -> set[str]:
    domain_root = repo / "state_bench" / "domains" / domain
    task_ids = _json_ids(domain_root / "tasks")
    env_ids = _json_ids(domain_root / "task_envs")
    if task_ids != env_ids:
        raise ValueError(f"STATE-Bench {domain} task/environment IDs differ")

    split_path = domain_root / "splits" / "train_test.json"
    split_data = json.loads(split_path.read_text(encoding="utf-8"))["splits"]
    train_ids = [str(value) for value in split_data["train"]]
    test_ids = [str(value) for value in split_data["test"]]
    if len(train_ids) != len(set(train_ids)) or len(test_ids) != len(set(test_ids)):
        raise ValueError(f"STATE-Bench {domain} split contains duplicate IDs")
    if set(train_ids) & set(test_ids) or set(train_ids) | set(test_ids) != task_ids:
        raise ValueError(f"STATE-Bench {domain} train/test split does not exactly partition tasks")

    trajectory_ids = _json_ids(repo / "datasets" / "train_task_trajectories" / domain)
    if trajectory_ids != set(train_ids):
        raise ValueError(f"STATE-Bench {domain} train trajectory IDs differ from train split")

    actual = {
        "task_count": len(task_ids),
        "task_env_count": len(env_ids),
        "train_count": len(train_ids),
        "test_count": len(test_ids),
        "train_trajectory_count": len(trajectory_ids),
    }
    if actual != expected:
        raise ValueError(f"STATE-Bench {domain} inventory mismatch: {actual} != {expected}")

    for directory in (
        domain_root / "tasks",
        domain_root / "task_envs",
        repo / "datasets" / "train_task_trajectories" / domain,
    ):
        for path in directory.glob("*.json"):
            json.loads(path.read_text(encoding="utf-8"))
    return set(test_ids)


def inventory_digest(repo: Path, domains: list[str]) -> str:
    paths: list[Path] = []
    for domain in domains:
        domain_root = repo / "state_bench" / "domains" / domain
        paths.extend((domain_root / "tasks").glob("*.json"))
        paths.extend((domain_root / "task_envs").glob("*.json"))
        paths.append(domain_root / "splits" / "train_test.json")
        paths.extend((repo / "datasets" / "train_task_trajectories" / domain).glob("*.json"))
    digest = hashlib.sha256()
    for path in sorted(paths, key=lambda value: value.relative_to(repo).as_posix()):
        digest.update(path.relative_to(repo).as_posix().encode())
        digest.update(b"\0")
        digest.update(path.read_bytes())
        digest.update(b"\0")
    return digest.hexdigest()


def audit_checkout(repo: Path, manifest: dict) -> dict[str, set[str]]:
    verify_official_repo(repo, manifest)
    domains = manifest["protocol"]["domains"]
    expected_digest = manifest["inventory"]["aggregate_sha256"]
    if inventory_digest(repo, domains) != expected_digest:
        raise ValueError("official STATE-Bench task/dataset inventory hash mismatch")
    return {
        domain: verify_domain_inventory(
            repo, domain, manifest["inventory"]["domains"][domain]
        )
        for domain in domains
    }


def acquire_checkout(cache_dir: Path, manifest: dict) -> Path:
    revision = manifest["code"]["revision"]
    repo = cache_dir / revision
    if not repo.exists():
        repo.parent.mkdir(parents=True, exist_ok=True)
        subprocess.run(
            ["git", "clone", "--no-checkout", manifest["code"]["repo"], str(repo)],
            check=True,
        )
        subprocess.run(["git", "-C", str(repo), "checkout", "--detach", revision], check=True)
    return repo


def verify_results(
    results_dir: Path,
    expected_ids: set[str],
    *,
    num_runs: int,
    protocol_id: str | None = None,
) -> None:
    for run_index in range(1, num_runs + 1):
        run_dir = results_dir / f"run{run_index}"
        if not run_dir.is_dir():
            raise ValueError(f"missing run directory: {run_dir}")
        files = sorted(run_dir.glob("*.json"))
        if {path.stem for path in files} != expected_ids:
            raise ValueError(f"{run_dir} trajectory IDs must exactly match the official test split")
        for path in files:
            trajectory = json.loads(path.read_text(encoding="utf-8"))
            if trajectory.get("task_id") != path.stem:
                raise ValueError(f"{path} task_id does not match filename")
            if trajectory.get("error"):
                raise RuntimeError(f"{path} records an agent error")
            if trajectory.get("task_completion_pass") not in (0, 1):
                raise ValueError(f"{path} has unscored task completion")
            ux_score = trajectory.get("ux_score")
            if not isinstance(ux_score, int | float) or not 1 <= ux_score <= 5:
                raise ValueError(f"{path} has missing or invalid UX score")
            if protocol_id is not None:
                for field in ("evaluation_protocol_id", "scoring_protocol_id"):
                    if trajectory.get(field) != protocol_id:
                        raise ValueError(f"{path} has invalid {field}")


def verify_native_metrics(
    output_dir: Path,
    *,
    expected_ids: set[str],
    benchmark_version: str,
    protocol_id: str,
    num_runs: int,
) -> None:
    metrics_path = output_dir / "metrics.json"
    metrics = json.loads(metrics_path.read_text(encoding="utf-8"))
    expected_header = {
        "benchmark_version": benchmark_version,
        "evaluation_protocol_id": protocol_id,
        "num_runs": num_runs,
    }
    for field, expected in expected_header.items():
        if metrics.get(field) != expected:
            raise ValueError(f"native STATE-Bench metrics {field} mismatch")
    required = {
        "task_completion_pass@1",
        f"task_completion_pass^{num_runs}",
        "mean_ux_score",
        "mean_cost_usd",
    }
    if not isinstance(metrics.get("metrics"), dict) or not required <= set(metrics["metrics"]):
        raise ValueError("native STATE-Bench metrics are incomplete")
    per_task_ids = _json_ids(output_dir / "per_task_metrics")
    if per_task_ids != expected_ids:
        raise ValueError("native STATE-Bench per-task metrics are incomplete")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Audit STATE-Bench v0.8.0 or invoke its native metric aggregator."
    )
    parser.add_argument("--official-repo", type=Path)
    parser.add_argument(
        "--cache-dir",
        type=Path,
        default=Path.home() / ".cache" / "memphant-bench" / "state-bench",
    )
    parser.add_argument("--manifest", type=Path, default=DEFAULT_MANIFEST)
    parser.add_argument("--results-root", type=Path)
    parser.add_argument("--output-root", type=Path)
    parser.add_argument("--dry-run", action="store_true")
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    manifest = json.loads(args.manifest.read_text(encoding="utf-8"))
    repo = args.official_repo or acquire_checkout(args.cache_dir, manifest)
    expected_by_domain = audit_checkout(repo, manifest)
    if args.dry_run:
        print(
            json.dumps(
                {
                    "benchmark": manifest["benchmark"],
                    "revision": manifest["code"]["revision"],
                    "protocol_id": manifest["protocol"]["id"],
                    "domains": {
                        domain: len(ids) for domain, ids in expected_by_domain.items()
                    },
                    "status": "audit-ok-no-model-calls",
                },
                sort_keys=True,
            )
        )
        return
    if args.results_root is None:
        raise SystemExit("--results-root is required unless --dry-run is used")
    output_root = args.output_root or args.results_root
    protocol = manifest["protocol"]
    for domain in protocol["domains"]:
        results_dir = args.results_root / domain
        output_dir = output_root / domain
        output_dir.mkdir(parents=True, exist_ok=True)
        verify_results(
            results_dir,
            expected_by_domain[domain],
            num_runs=protocol["num_runs"],
            protocol_id=protocol["id"],
        )
        command = [
            sys.executable,
            "-m",
            manifest["native_scorer"]["entrypoint"],
            "--domain",
            domain,
            "--results-dir",
            str(results_dir.resolve()),
            "--output-dir",
            str(output_dir.resolve()),
            "--split",
            protocol["split"],
            "--num-runs",
            str(protocol["num_runs"]),
        ]
        subprocess.run(command, cwd=repo, check=True)
        verify_native_metrics(
            output_dir,
            expected_ids=expected_by_domain[domain],
            benchmark_version=manifest["code"]["tag"].removeprefix("v"),
            protocol_id=protocol["id"],
            num_runs=protocol["num_runs"],
        )
    proof = {
        "benchmark": manifest["benchmark"],
        "manifest_sha256": sha256_file(args.manifest),
        "official_revision": manifest["code"]["revision"],
        "protocol_id": protocol["id"],
        "domains": protocol["domains"],
        "num_runs": protocol["num_runs"],
        "split": protocol["split"],
    }
    output_root.mkdir(parents=True, exist_ok=True)
    (output_root / "state_bench.proof.json").write_text(
        json.dumps(proof, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )


if __name__ == "__main__":
    main()

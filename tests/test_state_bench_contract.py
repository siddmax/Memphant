from __future__ import annotations

import importlib.util
import json
from pathlib import Path

import pytest


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "run_state_bench.py"
MANIFEST = ROOT / "benchmarks" / "manifests" / "state_bench.lock.json"


def load_script():
    spec = importlib.util.spec_from_file_location("run_state_bench", SCRIPT)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def test_state_bench_lock_pins_official_release_and_native_metrics() -> None:
    lock = json.loads(MANIFEST.read_text(encoding="utf-8"))

    assert lock["benchmark"] == "STATE-Bench"
    assert lock["code"] == {
        "repo": "https://github.com/microsoft/STATE-Bench.git",
        "revision": "e2c8d7af51ef48fbbea51bb2ce1fb859af36b423",
        "tag": "v0.8.0",
    }
    assert lock["license"] == "MIT"
    assert lock["track"] == "agent-learning"
    assert lock["protocol"] == {
        "domains": ["travel", "customer_support", "shopping_assistant"],
        "id": "state_bench_v0.8.0_gpt54",
        "num_runs": 5,
        "retrieval_top_k": 3,
        "split": "test",
    }
    assert lock["native_scorer"]["entrypoint"] == "state_bench.scripts.compute_metrics"
    assert set(lock["native_scorer"]["files"]) == {
        "state_bench/scripts/compute_metrics.py",
        "state_bench/scoring.py",
        "state_bench/protocol.py",
        "state_bench/configs/eval_protocols/gpt54.json",
    }


def test_state_bench_audit_rejects_source_drift(tmp_path: Path) -> None:
    runner = load_script()
    source = tmp_path / "compute_metrics.py"
    source.write_text("changed", encoding="utf-8")
    manifest = {
        "code": {"revision": "ignored"},
        "native_scorer": {"files": {source.name: "0" * 64}},
    }

    with pytest.raises(ValueError, match="source hash mismatch"):
        runner.verify_official_repo(tmp_path, manifest, verify_revision=False)


def test_state_bench_inventory_requires_exact_pairing_and_split(tmp_path: Path) -> None:
    runner = load_script()
    domain_root = tmp_path / "state_bench" / "domains" / "travel"
    tasks = domain_root / "tasks"
    envs = domain_root / "task_envs"
    split = domain_root / "splits" / "train_test.json"
    train_trajectories = tmp_path / "datasets" / "train_task_trajectories" / "travel"
    for directory in (tasks, envs, split.parent, train_trajectories):
        directory.mkdir(parents=True, exist_ok=True)
    for task_id in ("1-train", "2-test"):
        (tasks / f"{task_id}.json").write_text("{}", encoding="utf-8")
        (envs / f"{task_id}.json").write_text("{}", encoding="utf-8")
    (train_trajectories / "1-train.json").write_text("{}", encoding="utf-8")
    split.write_text(
        json.dumps({"splits": {"train": ["1-train"], "test": ["2-test"]}}),
        encoding="utf-8",
    )
    inventory = {
        "task_count": 2,
        "task_env_count": 2,
        "train_count": 1,
        "test_count": 1,
        "train_trajectory_count": 1,
    }

    runner.verify_domain_inventory(tmp_path, "travel", inventory)
    (envs / "2-test.json").unlink()
    with pytest.raises(ValueError, match="task/environment IDs differ"):
        runner.verify_domain_inventory(tmp_path, "travel", inventory)


def test_state_bench_results_require_all_five_complete_scored_runs(tmp_path: Path) -> None:
    runner = load_script()
    expected = {"2-test"}
    for run_index in range(1, 6):
        run_dir = tmp_path / f"run{run_index}"
        run_dir.mkdir()
        (run_dir / "2-test.json").write_text(
            json.dumps(
                {
                    "task_id": "2-test",
                    "task_completion_pass": 1,
                    "ux_score": 4,
                }
            ),
            encoding="utf-8",
        )

    runner.verify_results(tmp_path, expected, num_runs=5)
    (tmp_path / "run5" / "2-test.json").write_text(
        json.dumps({"task_id": "2-test", "task_completion_pass": None, "ux_score": 4}),
        encoding="utf-8",
    )
    with pytest.raises(ValueError, match="unscored task completion"):
        runner.verify_results(tmp_path, expected, num_runs=5)
    (tmp_path / "run5").rename(tmp_path / "run6")
    with pytest.raises(ValueError, match="missing run directory"):
        runner.verify_results(tmp_path, expected, num_runs=5)


def test_state_bench_native_metrics_must_be_complete_and_protocol_stamped(
    tmp_path: Path,
) -> None:
    runner = load_script()
    metrics = tmp_path / "metrics.json"
    metrics.write_text(
        json.dumps(
            {
                "benchmark_version": "0.8.0",
                "evaluation_protocol_id": "state_bench_v0.8.0_gpt54",
                "num_runs": 5,
                "metrics": {
                    "task_completion_pass@1": 0.5,
                    "task_completion_pass^5": 0.25,
                    "mean_ux_score": 4.0,
                    "mean_cost_usd": 1.0,
                },
            }
        ),
        encoding="utf-8",
    )
    per_task = tmp_path / "per_task_metrics"
    per_task.mkdir()
    (per_task / "2-test.json").write_text("{}", encoding="utf-8")

    runner.verify_native_metrics(
        tmp_path,
        expected_ids={"2-test"},
        benchmark_version="0.8.0",
        protocol_id="state_bench_v0.8.0_gpt54",
        num_runs=5,
    )
    value = json.loads(metrics.read_text(encoding="utf-8"))
    value["num_runs"] = 4
    metrics.write_text(json.dumps(value), encoding="utf-8")
    with pytest.raises(ValueError, match="num_runs"):
        runner.verify_native_metrics(
            tmp_path,
            expected_ids={"2-test"},
            benchmark_version="0.8.0",
            protocol_id="state_bench_v0.8.0_gpt54",
            num_runs=5,
        )

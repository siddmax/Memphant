from __future__ import annotations

import json
import importlib.util
import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def test_memphant_lock_has_required_schema_keys() -> None:
    lock = json.loads((ROOT / "memphant.lock").read_text())

    assert sorted(lock) == [
        "compiler_version",
        "engine_version",
        "export_schema_version",
        "methodology_version",
        "schema_compat_revision",
        "trace_schema_version",
    ]


def test_linked_repos_manifest_points_to_current_syndai_main() -> None:
    manifest = json.loads((ROOT / ".codex" / "linked-repos.json").read_text())
    private_path = Path(manifest["private_repo"]["path"])

    assert private_path.exists()
    assert manifest["private_repo"]["branch"] == "main"
    assert (
        private_path / "backend" / "src" / "features" / "memory" / "memphant_dogfood_adapter.py"
    ).exists()
    assert manifest["source_docs"]["path"] == "docs/superpowers/specs/memphant"

    head = subprocess.check_output(
        ["git", "rev-parse", "HEAD"],
        cwd=private_path,
        text=True,
    ).strip()
    branch = subprocess.check_output(
        ["git", "rev-parse", "--abbrev-ref", "HEAD"],
        cwd=private_path,
        text=True,
    ).strip()

    assert branch == manifest["private_repo"]["branch"]
    assert head == manifest["source_docs"]["commit"]


def test_required_memphant_specs_are_present() -> None:
    required = [
        "STATUS.md",
        "29-implementation-plan.md",
        "26-decision-register.md",
        "00-relations-graph.md",
        "03-engineering-spec.md",
        "04-memory-model-spec.md",
        "05-retrieval-and-eval-spec.md",
        "02-architecture-spec.md",
        "08-api-sdk-mcp-spec.md",
        "06-trust-security-spec.md",
        "25-db-provider-byoc-and-app-surface-spec.md",
        "14-ingestion-seeding-and-ops-spec.md",
        "27-sota-ladder-and-validation.md",
        "07-syndai-integration-spec.md",
        "28-syndai-code-contract.md",
    ]
    spec_dir = ROOT / "docs" / "superpowers" / "specs" / "memphant"

    missing = [name for name in required if not (spec_dir / name).exists()]

    assert missing == []


def test_spec_drift_check_passes_against_linked_syndai_docs() -> None:
    result = subprocess.run(
        ["python3", "scripts/check_spec_drift.py"],
        cwd=ROOT,
        text=True,
        capture_output=True,
        check=False,
    )

    assert result.returncode == 0, result.stdout + result.stderr
    assert "spec_drift=clean" in result.stdout


def test_governance_docs_pin_public_private_boundary() -> None:
    required = ["LICENSE", "SECURITY.md", "CONTRIBUTING.md", "README.md"]

    missing = [name for name in required if not (ROOT / name).exists()]
    readme = (ROOT / "README.md").read_text(encoding="utf-8")
    contributing = (ROOT / "CONTRIBUTING.md").read_text(encoding="utf-8")

    assert missing == []
    assert "Apache-2.0" in readme
    assert "Syndai adapter" in readme
    assert "DCO" in contributing


def test_repo_hygiene_files_keep_generated_and_private_state_out() -> None:
    required = [
        "Cargo.lock",
        ".editorconfig",
        ".env.example",
        ".gitattributes",
        ".gitignore",
        "rust-toolchain.toml",
        "rustfmt.toml",
    ]

    missing = [name for name in required if not (ROOT / name).exists()]
    gitignore = (ROOT / ".gitignore").read_text(encoding="utf-8")
    toolchain = (ROOT / "rust-toolchain.toml").read_text(encoding="utf-8")

    assert missing == []
    assert "/target/" in gitignore
    assert "__pycache__/" in gitignore
    assert ".pytest_cache/" in gitignore
    assert ".env.*" in gitignore
    assert "!.env.example" in gitignore
    assert ".codex/feature-flow/" in gitignore
    assert 'channel = "1.96.1"' in toolchain

    ignored = subprocess.run(
        ["git", "check-ignore", "-q", ".codex/feature-flow/example.json"],
        cwd=ROOT,
        check=False,
    )
    repo_link = subprocess.run(
        ["git", "check-ignore", "-q", ".codex/linked-repos.json"],
        cwd=ROOT,
        check=False,
    )

    assert ignored.returncode == 0
    assert repo_link.returncode == 1


def test_spike_decision_thresholds_match_r83() -> None:
    spec = importlib.util.spec_from_file_location("run_spike", ROOT / "scripts" / "run_spike.py")
    assert spec is not None
    assert spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)

    assert module.decision_for_ratio(1.49) == "rust_proceeds"
    assert module.decision_for_ratio(1.5) == "manual_review"
    assert module.decision_for_ratio(2.99) == "manual_review"
    assert module.decision_for_ratio(3.0) == "reopen_decision_2"

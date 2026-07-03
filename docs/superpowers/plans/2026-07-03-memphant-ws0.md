# MemPhant WS-0 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Complete WS-0 enough to prove the public repo skeleton, `memphant.lock` schema snapshot, spec-drift checklist, and Rust-vs-Python two-language spike.

**Architecture:** Keep WS-0 deliberately thin: one Rust workspace with minimal crates matching `03-engineering-spec.md`, one Python spike package under `spikes/python`, shared policy YAML under `examples/spike/policies`, and test/proof scripts that exercise the public repo without private Syndai imports. The spike proves the iteration-loop rule by changing extraction policy data, not compiled code.

**Tech Stack:** Rust/Cargo workspace, Python stdlib test harness, pytest-compatible repo contract tests, YAML/JSON fixtures, and the linked Syndai worktree at `/Users/sidsharma/Syndai/.wt/codex-memphant-cross-repo`.

---

## File Structure

- Create `Cargo.toml`: virtual workspace with resolver `3`, shared package metadata, workspace dependencies, and crate members.
- Create `crates/memphant-{types,core,store-postgres,server,mcp,cli,eval,worker}/`: minimal Rust crates required by `03` §1 and `29` WS-0.
- Create `memphant.lock`: lock schema snapshot with engine/compiler/trace/schema/methodology/export versions.
- Create `scripts/check_spec_drift.py`: verifies required MemPhant spec files exist in public repo and match the linked Syndai worktree.
- Create `scripts/run_spike.py`: runs Rust and Python spike commands, measures policy-change wall-clock, writes a JSON proof artifact.
- Create `spikes/rust-retain/` and `spikes/python-retain/`: minimal retain + golden-runner implementations for the R83 measurement.
- Create `examples/spike/policies/extraction-policy-v1.json` and `examples/spike/golden.jsonl`: shared spike data/config.
- Create `tests/test_repo_contract.py`: red/green tests for repo skeleton, lock schema, linked-repo manifest, and spec-drift script behavior.
- Modify `docs/superpowers/specs/memphant/STATUS.md`: only after proof exists, attach proof path to WS-0 checkbox.
- Create `docs/build-log/YYYY-MM-DD-ws0-repo-freeze.md`: commands, artifacts, and what remains unbuilt.

## Task 1: Repo Contract Tests

**Files:**
- Create: `tests/test_repo_contract.py`
- Create: `scripts/check_spec_drift.py`

- [ ] **Step 1: Write failing repo contract tests**

```python
from __future__ import annotations

import json
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


def test_linked_repos_manifest_points_to_clean_syndai_worktree() -> None:
    manifest = json.loads((ROOT / ".codex" / "linked-repos.json").read_text())
    private_path = Path(manifest["private_repo"]["path"])
    assert private_path.exists()
    assert manifest["private_repo"]["branch"] == "codex/memphant-cross-repo"
    assert manifest["source_docs"]["path"] == "docs/superpowers/specs/memphant"


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
```

- [ ] **Step 2: Run the tests to verify RED**

Run: `python3 -m pytest tests/test_repo_contract.py -q`

Expected: FAIL because `memphant.lock`, `scripts/check_spec_drift.py`, and the Rust/Python skeleton do not exist yet.

- [ ] **Step 3: Implement `scripts/check_spec_drift.py` minimally**

```python
from __future__ import annotations

import filecmp
import json
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
MANIFEST = ROOT / ".codex" / "linked-repos.json"


def main() -> int:
    manifest = json.loads(MANIFEST.read_text())
    public_dir = ROOT / manifest["source_docs"]["path"]
    private_dir = Path(manifest["private_repo"]["path"]) / manifest["source_docs"]["path"]
    if not private_dir.exists():
        print(f"private_specs_missing={private_dir}", file=sys.stderr)
        return 2
    comparison = filecmp.dircmp(public_dir, private_dir)
    diffs = list(comparison.left_only) + list(comparison.right_only) + list(comparison.diff_files)
    if diffs:
        print("spec_drift=dirty")
        for item in sorted(diffs):
            print(item)
        return 1
    print(f"spec_drift=clean public={public_dir} private={private_dir}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
```

- [ ] **Step 4: Add `memphant.lock` snapshot**

```json
{
  "engine_version": "0.1.0-ws0",
  "compiler_version": "compiler-0.1.0-ws0",
  "trace_schema_version": "trace-0.1.0-ws0",
  "schema_compat_revision": "schema-compat-0",
  "methodology_version": "memphant-methodology-2026-07-03",
  "export_schema_version": "export-0.1.0-ws0"
}
```

- [ ] **Step 5: Run the repo contract tests to verify GREEN**

Run: `python3 -m pytest tests/test_repo_contract.py -q`

Expected: PASS.

## Task 2: Rust Workspace Skeleton

**Files:**
- Create: `Cargo.toml`
- Create: `crates/*/Cargo.toml`
- Create: `crates/*/src/lib.rs`
- Create: `crates/memphant-cli/src/main.rs`
- Create: `crates/memphant-server/src/main.rs`
- Create: `crates/memphant-mcp/src/main.rs`
- Create: `crates/memphant-worker/src/main.rs`

- [ ] **Step 1: Verify RED with Cargo**

Run: `cargo metadata --format-version 1 --no-deps`

Expected: FAIL because no Cargo workspace exists.

- [ ] **Step 2: Add the virtual workspace**

Use resolver `3`, workspace package metadata, and `crates/*` members. Use `license = "Apache-2.0"` and Rust edition `2024`.

- [ ] **Step 3: Add minimal member crates**

Every crate inherits workspace package metadata and compiles. `memphant-types` owns shared structs; `memphant-core` depends on it; binary crates expose a minimal `main()`.

- [ ] **Step 4: Verify Cargo metadata**

Run: `cargo metadata --format-version 1 --no-deps`

Expected: PASS and includes `memphant-core`, `memphant-types`, `memphant-store-postgres`, `memphant-server`, `memphant-mcp`, `memphant-cli`, `memphant-eval`, and `memphant-worker`.

- [ ] **Step 5: Verify Rust build gate**

Run: `cargo fmt --check && cargo clippy --all-targets --all-features -- -D warnings && cargo test --doc`

Expected: PASS for the skeleton.

## Task 3: Two-Language Spike

**Files:**
- Create: `examples/spike/policies/extraction-policy-v1.json`
- Create: `examples/spike/golden.jsonl`
- Create: `spikes/python-retain/memphant_spike.py`
- Create: `spikes/python-retain/test_spike.py`
- Create: `spikes/rust-retain/Cargo.toml`
- Create: `spikes/rust-retain/src/main.rs`
- Create: `scripts/run_spike.py`

- [ ] **Step 1: Write failing Python spike test**

The Python test imports `retain_episode`, loads the shared JSON policy, retains two events, and asserts the golden runner returns the expected extracted memory values.

- [ ] **Step 2: Run Python spike RED**

Run: `python3 -m pytest spikes/python-retain/test_spike.py -q`

Expected: FAIL because the spike module does not exist.

- [ ] **Step 3: Implement Python retain + golden runner**

Use only stdlib JSON and dataclasses. Extraction policy is loaded from JSON, so changing a pattern/value never edits Python code.

- [ ] **Step 4: Run Python spike GREEN**

Run: `python3 -m pytest spikes/python-retain/test_spike.py -q`

Expected: PASS.

- [ ] **Step 5: Add Rust spike command**

The Rust spike reads the same policy and golden files, runs retain + extraction deterministically, and exits nonzero on a mismatch.

- [ ] **Step 6: Verify Rust spike**

Run: `cargo run --manifest-path spikes/rust-retain/Cargo.toml -- examples/spike/policies/extraction-policy-v1.json examples/spike/golden.jsonl`

Expected: PASS.

- [ ] **Step 7: Add wall-clock measurement script**

`scripts/run_spike.py` copies the policy to a temp dir, changes one extraction replacement value, runs both implementations before and after the policy change, measures wall-clock seconds, writes `docs/build-log/artifacts/ws0-two-language-spike.json`, and prints `decision=rust_proceeds` when Rust/Python policy-change time ratio is `< 1.5`.

- [ ] **Step 8: Run spike proof**

Run: `python3 scripts/run_spike.py`

Expected: PASS, artifact created, ratio printed, decision printed.

## Task 4: Governance Skeleton and Status Proof

**Files:**
- Create: `LICENSE`
- Create: `SECURITY.md`
- Create: `CONTRIBUTING.md`
- Create: `README.md`
- Create: `docs/build-log/2026-07-03-ws0-repo-freeze.md`
- Modify: `docs/superpowers/specs/memphant/STATUS.md`

- [ ] **Step 1: Add governance docs**

Add Apache-2.0 license, security disclosure policy, DCO contribution note, and README that states the public/private split.

- [ ] **Step 2: Run complete WS-0 verification**

Run:

```bash
python3 -m pytest tests/test_repo_contract.py spikes/python-retain/test_spike.py -q
cargo metadata --format-version 1 --no-deps
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --doc
python3 scripts/check_spec_drift.py
python3 scripts/run_spike.py
```

Expected: every command exits 0.

- [ ] **Step 3: Write build log**

Record exact command outputs and the artifact path `docs/build-log/artifacts/ws0-two-language-spike.json`.

- [ ] **Step 4: Flip only the WS-0 checkbox**

In `STATUS.md`, change the WS-0 line to checked and append proof paths. Do not flip later workstreams.

## Self-Review

- Spec coverage: this plan covers `29` WS-0 exit packet, `03` repo layout, the `memphant.lock` shape from `08` §6.1/§7, linked Syndai drift proof, and R83 two-language spike. It deliberately does not implement WS-A schema/store work.
- Placeholder scan: no task uses deferred-marker language in production code; implementation steps name exact files and commands.
- Type consistency: the shared policy/golden files are used by both spike implementations, and the proof artifact is the single source for the STATUS/build-log update.

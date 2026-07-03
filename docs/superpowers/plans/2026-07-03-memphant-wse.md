# MemPhant WS-E Eval, Security, and Ops Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Complete WS-E's cheap-loop exit packet: deterministic YAML goldens, manifest/orphan checks, security fixtures, benchmark trace archives, deletion/ops checks, and read-only Markdown export with staleness verification.

**Architecture:** Keep evals fixture-driven and deterministic. `memphant-eval` owns YAML parsing, seed execution, oracle verification, security checks, ops checks, and trace archives. `memphant-cli` owns exported Markdown and stale export verification. Core retrieval remains the single behavior path.

**Tech Stack:** Rust 2024, serde/serde_json, `yaml_serde`, schemars JSON Schema snapshots, existing hand-parsed CLI.

---

## Scope Notes

- `29` WS-E requires YAML oracle, manifest/orphan guard, trace schema snapshot, security suite, sampled/release benchmark runners, blob/deletion/reindex checks, compiled Markdown export, and eval trace notebooks.
- `05` §4.0 makes `answer_bearing_ids` a load-bearing oracle label and requires `verify-golden` to mechanically fail when masked declared units still satisfy assertions.
- `06` §6.2 and `14` §4.1 make deletion completeness an adversarial lane across vector/lexical/cache/derived/blob/edge paths, plus blob GC and compaction SLA checks.
- WS-E does not claim live public benchmark wins. It creates the deterministic gate and archive format needed before those claims.

## File Structure

- Modify `Cargo.toml`: add workspace deps for `yaml_serde` and temporary-directory tests.
- Modify `crates/memphant-eval/Cargo.toml`, `src/lib.rs`; create `src/main.rs`.
- Add eval fixtures under `examples/evals/manifest.yaml`, `examples/evals/golden.yaml`, `examples/evals/golden/*.yaml`, `examples/evals/security-smoke.yaml`, `examples/evals/ops-smoke.yaml`, and `examples/evals/trace-schema.v1.json`.
- Add sampled/release fixtures under `benchmarks/`.
- Add CLI compile source fixture under `examples/evals/compiled-memory-source.json`.
- Modify `crates/memphant-cli/src/main.rs`; add compile/export tests.
- Add `docs/build-log/2026-07-03-wse-progress.md` when proof commands pass.

### Task 1: Eval Oracle and Manifest Guard

**Files:**
- Modify: `crates/memphant-eval/Cargo.toml`
- Modify: `crates/memphant-eval/src/lib.rs`
- Create: `crates/memphant-eval/src/main.rs`
- Test: `crates/memphant-eval/tests/eval_contract.rs`
- Fixtures: `examples/evals/manifest.yaml`, `examples/evals/golden.yaml`, `examples/evals/golden/*.yaml`

- [x] **Step 1: Write failing oracle tests**

Assert `run examples/evals/golden.yaml` passes, `verify-golden examples/evals/golden.yaml` passes, and a temporary manifest with an orphan/missing case fails.

- [x] **Step 2: Implement typed YAML fixtures and runner**

Parse fixtures with Serde, seed the existing in-memory store, call `memphant_core::recall`, and assert answer-bearing/top-k/citation/trace expectations. Implement manifest↔file checks with exact failure reasons.

- [x] **Step 3: Verify green**

Run:

```bash
cargo test -p memphant-eval --test eval_contract oracle
```

### Task 2: Security and Ops Lanes

**Files:**
- Modify: `crates/memphant-eval/src/lib.rs`
- Test: `crates/memphant-eval/tests/eval_contract.rs`
- Fixtures: `examples/evals/security-smoke.yaml`, `examples/evals/ops-smoke.yaml`

- [x] **Step 1: Write failing security/ops tests**

Assert security-smoke passes and reports all five required lanes: poisoning, query/filter injection, high-risk action suppression, tenant leakage, and deletion completeness. Assert ops smoke covers blob GC, deletion saga read-back, and reindex/compaction SLA.

- [x] **Step 2: Implement deterministic checks**

Use core recall/forget behavior for tenant and deletion lanes; use explicit fixture assertions for injection/high-risk filters and ops invariants. Treat missing required lanes as a hard failure.

- [x] **Step 3: Verify green**

Run:

```bash
cargo test -p memphant-eval --test eval_contract security
cargo run -p memphant-eval -- security examples/evals/security-smoke.yaml
```

### Task 3: Benchmark Trace Archives and Schema Snapshot

**Files:**
- Modify: `crates/memphant-eval/src/lib.rs`
- Test: `crates/memphant-eval/tests/eval_contract.rs`
- Fixtures: `examples/evals/trace-schema.v1.json`, `benchmarks/nightly-sampled.yaml`, `benchmarks/release.yaml`

- [x] **Step 1: Write failing archive/snapshot tests**

Assert `--archive-traces --archive-dir <tmp>` writes a JSON archive containing eval id, case ids, trace ids, metrics, and trace schema version. Assert the checked-in trace schema snapshot matches the generated `RetrievalTrace` schema.

- [x] **Step 2: Implement archive writer and schema command**

Make archives deterministic enough for CI by sorting case results and recording versions. Add `schema trace` for snapshot regeneration.

- [x] **Step 3: Verify green**

Run:

```bash
cargo test -p memphant-eval --test eval_contract archive
cargo run -p memphant-eval -- run benchmarks/nightly-sampled.yaml --archive-traces
```

### Task 4: Markdown Compile and Stale Verify

**Files:**
- Modify: `crates/memphant-cli/src/main.rs`
- Test: `crates/memphant-cli/tests/compile_contract.rs`
- Fixture: `examples/evals/compiled-memory-source.json`

- [x] **Step 1: Write failing compile tests**

Assert `memphant compile --scope ... --out <dir> --source ...` writes a read-only Markdown view plus export metadata. Assert `memphant verify --lock memphant.lock --export <dir>` passes, then fails after the source hash in metadata is drifted.

- [x] **Step 2: Implement export and staleness check**

Render scoped entries to Markdown, write `memphant-export.json` with lock metadata and source hash, and extend `verify` to compare export metadata against the current lock and source hash.

- [x] **Step 3: Verify green**

Run:

```bash
cargo test -p memphant-cli --test compile_contract
cargo run -p memphant-cli -- compile --scope project:checkout --out /tmp/memphant-wse-wiki --source examples/evals/compiled-memory-source.json
cargo run -p memphant-cli -- verify --lock memphant.lock --export /tmp/memphant-wse-wiki
```

### Task 5: Exit Packet and Ledger Sync

**Files:**
- Create: `docs/build-log/2026-07-03-wse-progress.md`
- Modify: `docs/superpowers/specs/memphant/STATUS.md`
- Mirror: `/Users/sidsharma/Syndai/.wt/codex-memphant-cross-repo/superpowers/memphant/files/docs/superpowers/specs/memphant/STATUS.md`

- [x] **Step 1: Run WS-E gates**

Run the full local proof set:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
cargo test --doc
python3 -m pytest tests
python3 scripts/check_spec_drift.py
cargo run -p memphant-eval -- run examples/evals/golden.yaml
cargo run -p memphant-eval -- verify-golden examples/evals/golden.yaml
cargo run -p memphant-eval -- security examples/evals/security-smoke.yaml
cargo run -p memphant-eval -- run benchmarks/nightly-sampled.yaml --archive-traces
cargo run -p memphant-cli -- db lint --provider plain-postgres
```

- [x] **Step 2: Record proof and flip WS-E**

Write the build log with command results and artifacts, then update the status banner to `WS-F READY — WS-E EXIT PACKET COMPLETE` and check WS-E.

- [x] **Step 3: Commit and push**

Commit Memphant changes, sync the mirrored status doc in Syndai, commit both repos, and push both branches.

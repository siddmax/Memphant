# Rung 10 Procedural Memory Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Promote Rung 10 by making validated procedural/failure-pattern memories retrievable only through the replay-safety gate, with controls, traces, fixtures, and archived profile proof.

**Architecture:** Keep procedures as `kind=procedural` units and use `state=validated` as the shipped validation state. Add a recall flag and trace facts for procedure IDs/validation states; do not add an executable rules DSL or exact tool-argument replay.

**Tech Stack:** Rust workspace, `memphant-core`, `memphant-types`, `memphant-eval`, YAML golden/profile fixtures, local full-gate scripts.

---

### Task 1: Red Tests

**Files:**
- Modify: `crates/memphant-core/tests/recall_trace_golden.rs`
- Modify: `crates/memphant-eval/tests/eval_contract.rs`
- Modify: `crates/memphant-eval/tests/profile_contract.rs`
- Create: `examples/evals/golden/procedural_memory_replay_validation.yaml`
- Create: `benchmarks/rung10-baseline-sampled.yaml`
- Create: `benchmarks/rung10-state-style-sampled.yaml`
- Create: `examples/evals/rung10-procedural-memory-profile.yaml`

- [ ] Add a core test that seeds validated safe, candidate, and high-risk procedural units; expects only the validated safe procedure to recall when `procedure_recall_enabled=true`, and expects no procedural answer when disabled.
- [ ] Add eval tests proving `benchmarks/rung10-state-style-sampled.yaml` passes with procedure recall and fails with `procedure_recall_enabled=false`.
- [ ] Add profile tests that accept a Rung 10 promoted profile only with procedural axes, replay sample refs, no-procedure control refs, and positive procedural/outcome deltas.
- [ ] Run:

```bash
cargo test -p memphant-core --test recall_trace_golden procedural_memory -- --nocapture
cargo test -p memphant-eval --test eval_contract rung10 -- --nocapture
cargo test -p memphant-eval --test profile_contract rung10 -- --nocapture
```

Expected before implementation: failing compile/test output for missing `procedure_recall_enabled` and missing procedure trace/profile validation.

### Task 2: Core Implementation

**Files:**
- Modify: `crates/memphant-types/src/lib.rs`
- Modify: `crates/memphant-core/src/lib.rs`
- Modify: `crates/memphant-server/src/lib.rs`
- Modify: `crates/memphant-mcp/src/lib.rs`
- Modify: request-construction tests under `crates/`

- [ ] Add `procedure_recall_enabled` to recall request surfaces and archive metrics.
- [ ] Add trace fields for recalled procedure IDs and validation states.
- [ ] Change `recallable` so procedural units require the procedure flag, `state=validated`, and safe strategy text.
- [ ] Emit dropped trace rows for disabled, candidate, or unsafe procedures.
- [ ] Add `procedure_recall` to stage facts and feature flags.

### Task 3: Eval/Profile Artifacts

**Files:**
- Modify: `examples/evals/golden.yaml`
- Modify: `examples/evals/manifest.yaml`
- Modify: `examples/evals/trace-schema.v1.json`
- Create generated artifacts in `docs/build-log/artifacts/`
- Create: `docs/build-log/2026-07-03-rung10-procedural-memory-profile.md`
- Modify: `docs/superpowers/specs/memphant/STATUS.md`

- [ ] Run golden, baseline-disabled, state-style-enabled, and profile commands with archives.
- [ ] Regenerate the trace schema snapshot.
- [ ] Update `STATUS.md` only after archived proof exists.
- [ ] Mirror `STATUS.md` to `/Users/sidsharma/Syndai/.wt/codex-memphant-cross-repo/docs/superpowers/specs/memphant/STATUS.md`.

### Task 4: Full Verification and Ship

**Commands:**

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
cargo test --doc
python3 -m pytest tests -q
cargo run -p memphant-eval -- verify-golden examples/evals/golden.yaml
cargo run -p memphant-eval -- security examples/evals/security-smoke.yaml
cargo run -p memphant-cli -- db lint --provider plain-postgres
cargo run -p memphant-cli -- db lint --provider supabase
cargo run -p memphant-cli -- db lint --provider neon
python3 scripts/apply_memphant_migrations.py --database-url postgres://memphant.invalid/memphant --dry-run
cargo run -p memphant-cli -- db bootstrap-check --provider plain-postgres
cargo run -p memphant-cli -- db bootstrap-check --provider supabase
cargo run -p memphant-cli -- db bootstrap-check --provider neon
python3 scripts/check_spec_drift.py
```

Expected: every command exits 0, except the deliberate disabled-control eval which must fail while writing its archive.

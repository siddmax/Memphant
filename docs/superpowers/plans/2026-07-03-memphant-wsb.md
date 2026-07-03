# MemPhant WS-B Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build WS-B's write path and first memory compiler so retain/reflect/dedup/contradiction/corroboration fixtures pass with traceable, idempotent behavior.

**Architecture:** Keep capture cheap and durable: `retain` validates input, computes a deterministic content dedup key, stores or collapses the raw episode, and enqueues a reflect job in the same transaction. `reflect` is a deterministic record-replay compiler for WS-B fixtures: extractor output is fixture/config data, admission control lives in core, and the store fake exposes enough state to prove idempotency, contradiction edges, belief observations, semantic promotion, freshness fields, and trace facts before a Postgres adapter implementation.

**Tech Stack:** Rust 1.96.1, `memphant-core`, `memphant-types`, in-memory `MemoryStore`, JSON golden fixtures, Cargo tests, Python repo-contract checks.

---

### Task 1: Retain Transaction + Reflect Enqueue

**Files:**
- Modify: `crates/memphant-types/src/lib.rs`
- Modify: `crates/memphant-core/src/lib.rs`
- Modify: `crates/memphant-core/tests/store_contract.rs`

- [x] **Step 1: Write the failing test**
  - Add `retain_pipeline_stores_episode_and_reflect_job_atomically`.
  - It constructs a `RetainRequest` with tenant, scope, actor, source kind/trust, subject hint, and body.
  - It calls `retain_episode(&store, request).await`.
  - It asserts the committed store has exactly one episode and exactly one queued `ReflectJob`.
  - It asserts the job references the retained episode and compiler version.

- [x] **Step 2: Run the test to verify RED**
  - Run: `cargo test -p memphant-core --test store_contract retain_pipeline_stores_episode_and_reflect_job_atomically`
  - Expected: compile failure for missing `RetainRequest`, `retain_episode`, and reflect job queue accessors.

- [x] **Step 3: Implement the minimal green path**
  - Add `RetainRequest`, `ReflectJob`, `ReflectJobKind`, and `QueuedReflectJob` to `memphant-types`.
  - Extend `MemoryStore` with `enqueue_reflect`.
  - Extend `InMemoryTxn` with staged jobs and commit them atomically.
  - Implement `retain_episode` in core as one transaction: stage episode, enqueue reflect, commit.

- [x] **Step 4: Verify GREEN**
  - Run: `cargo test -p memphant-core --test store_contract retain_pipeline_stores_episode_and_reflect_job_atomically`
  - Expected: pass.

### Task 2: Exact Dedup Collapse

**Files:**
- Modify: `crates/memphant-core/src/lib.rs`
- Modify: `crates/memphant-core/tests/store_contract.rs`

- [x] **Step 1: Write the failing test**
  - Add `retain_pipeline_collapses_duplicate_episode_by_dedup_key`.
  - It retains the same request twice.
  - It asserts one stored episode, `observation_count == 2`, and two queued reflect jobs both pointing at the same episode id.

- [x] **Step 2: Run the test to verify RED**
  - Run: `cargo test -p memphant-core --test store_contract retain_pipeline_collapses_duplicate_episode_by_dedup_key`
  - Expected: fail because existing `stage_episode` always inserts a new episode.

- [x] **Step 3: Implement minimal dedup**
  - Compute dedup keys from `scope_id`, `source_kind`, optional subject hint, and normalized body.
  - Make `InMemoryStore::stage_episode` search committed and staged episodes by `(tenant_id, scope_id, dedup_key)`.
  - On match, increment `observation_count` on the canonical episode and return `DedupOutcome { matched: true, observation_count }`.
  - Preserve existing raw episode body; do not delete or overwrite prior memory.

- [x] **Step 4: Verify GREEN**
  - Run: `cargo test -p memphant-core --test store_contract retain_pipeline_collapses_duplicate_episode_by_dedup_key`
  - Expected: pass.

### Task 3: Reflect Admission Golden Harness

**Files:**
- Create: `examples/evals/wsb-write-goldens.json`
- Create: `crates/memphant-core/tests/write_compiler_golden.rs`
- Modify: `crates/memphant-types/src/lib.rs`
- Modify: `crates/memphant-core/src/lib.rs`

- [x] **Step 1: Write failing golden tests**
  - Add fixtures for noisy-write rejection, duplicate collapse, contradiction detection, corroboration-farming resistance, and stale fact handling.
  - Add a Rust test that loads the JSON fixture, runs retain + reflect against `InMemoryStore`, and asserts expected `AdmissionAction`, unit states, edge kinds, and trace stage facts.

- [x] **Step 2: Run the tests to verify RED**
  - Run: `cargo test -p memphant-core --test write_compiler_golden`
  - Expected: compile failure for missing reflect compiler API and admission types.

- [x] **Step 3: Implement deterministic reflect compiler**
  - Add `AdmissionAction` with `reject`, `append`, `merge`, `supersede`, `invalidate`, and `quarantine`.
  - Add deterministic subject-key canonicalization for fixture facts.
  - Add record-replay extraction input in fixture data; no live LLM calls.
  - Add contradiction detection by exact subject key and overlapping validity in the in-memory compiler.
  - Add belief observation independence checks by distinct actor and source kind.
  - Add active freshness fields for stale/volatile semantic facts.
  - Emit a `ReflectTrace` with stage names, action, cost estimate, and consumed job id.

- [x] **Step 4: Verify GREEN**
  - Run: `cargo test -p memphant-core --test write_compiler_golden`
  - Expected: pass.

### Task 4: WS-B Proof Packet

**Files:**
- Create: `docs/build-log/2026-07-03-wsb-progress.md`
- Modify only if the exit packet is fully proven: `docs/superpowers/specs/memphant/STATUS.md`

- [x] **Step 1: Run focused gates**
  - `cargo fmt --check`
  - `cargo test -p memphant-core --test store_contract`
  - `cargo test -p memphant-core --test write_compiler_golden`
  - `python3 scripts/check_spec_drift.py`

- [x] **Step 2: Run full local gates**
  - `python3 -m pytest tests`
  - `cargo clippy --all-targets --all-features -- -D warnings`
  - `cargo test --all-targets --all-features`
  - `cargo test --doc`

- [x] **Step 3: Update build log**
  - Record exact command outputs.
  - State whether WS-B is complete or which exit-packet proof remains.

- [x] **Step 4: Update ledger only with complete proof**
  - Flip WS-B in `STATUS.md` only if every golden fixture passes, reflect is idempotent under duplicate job delivery, and trace facts are proven.

### Task 5: Completion Audit Gap Closure

**Files:**
- Modify: `crates/memphant-types/src/lib.rs`
- Modify: `crates/memphant-core/src/lib.rs`
- Modify: `crates/memphant-core/tests/store_contract.rs`
- Modify: `crates/memphant-core/tests/write_compiler_golden.rs`
- Modify: `examples/evals/wsb-write-goldens.json`
- Modify: `memphant_migrations/versions/20260703_001_wsa_bootstrap.sql`
- Modify: `scripts/check_memphant_live_catalog.py`
- Modify: `tests/test_wsa_migration_contract.py`
- Modify: `crates/memphant-store-postgres/src/lib.rs`

- [x] **Step 1: Prove resource capture before extraction**
  - Add `retain_resource` coverage that stores a resource pointer with `registered` extractor state and enqueues a resource reflect job in the same transaction.

- [x] **Step 2: Prove full admission action coverage**
  - Extend the golden fixture schema and data with explicit `invalidate` and `quarantine` cases.
  - Keep quarantined units out of active/candidate belief lists while exposing them through a quarantine accessor.

- [x] **Step 3: Prove active freshness due-scan surface**
  - Assert volatile active semantic units surface through `freshness_due_units`.

- [x] **Step 4: Land the reserved consolidation outbox table shape**
  - Add `memphant.event_outbox` with tenant RLS, poll-cursor indexes, and provider/catalog lint coverage.
  - Keep delivery consumers dormant; the table shape satisfies the WS-B-write schema requirement while `GET /v1/events` remains post-v1.

- [x] **Step 5: Re-run focused and full gates**
  - Focused WS-B suites, schema lint, spec drift, Python tests, clippy, all Rust tests, and doctests pass.

## Self-Review

- Spec coverage: Tasks 1-2 cover raw episode capture before extraction, transactional reflect enqueue, and exact dedup. Task 3 covers the named WS-B exit fixtures: noisy rejection, duplicate collapse, contradiction, corroboration-farming resistance, stale fact handling, trace facts, and duplicate-job idempotency. Task 5 closes the completion-audit gaps for raw resource capture, explicit invalidate/quarantine admission, active freshness due-scan visibility, and the reserved consolidation outbox table.
- Placeholder scan: no TODO/TBD placeholders are present; every task names files, commands, and expected red/green outcomes.
- Type consistency: the plan uses `RetainRequest`, `ReflectJob`, `AdmissionAction`, and `ReflectTrace` consistently across tests and implementation steps.

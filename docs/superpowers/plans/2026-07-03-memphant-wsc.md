# MemPhant WS-C Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build WS-C's read path and trace spine so every recall, including denied recall, produces a complete retrieval trace and the oracle/security fixtures prove answer-bearing recall, isolation, citations, stale/deleted suppression, and small-tenant filtered vector visibility.

**Architecture:** Keep WS-C in `memphant-core` first, using the existing deterministic `InMemoryStore` as the proof surface. The read path runs Stage 0 gates, exact/lexical/vector/temporal candidate channels, weighted RRF fusion, budgeted packing, citation whitelist emission, and trace persistence; Postgres-specific FTS/vector mechanics stay adapter-owned and are represented by channel-shaped trace fields until WS-D/adapter work expands them.

**Tech Stack:** Rust 1.96.1, `memphant-core`, `memphant-types`, in-memory `MemoryStore`, JSON golden fixtures, Cargo tests, Python repo-contract checks.

---

### Task 1: Recall Trace Types and Denial Trace Contract

**Files:**
- Modify: `crates/memphant-types/src/lib.rs`
- Modify: `crates/memphant-core/src/lib.rs`
- Create: `crates/memphant-core/tests/recall_trace_golden.rs`
- Create: `examples/evals/wsc-recall-goldens.json`

- [x] **Step 1: Write the failing test**
  - Add `recall_writes_trace_for_scope_denial` in `crates/memphant-core/tests/recall_trace_golden.rs`.
  - Test code:

```rust
let store = InMemoryStore::default();
let tenant_id = TenantId::from_u128(70_000);
let allowed_scope = ScopeId::from_u128(70_001);
let denied_scope = ScopeId::from_u128(70_002);
let actor_id = ActorId::from_u128(70_003);

let error = recall(
    &store,
    RecallRequest {
        tenant_id,
        scope_id: denied_scope,
        actor_id,
        allowed_scope_ids: vec![allowed_scope],
        query: "Which callback version is current?".to_string(),
        k: 3,
        budget_tokens: 80,
        mode: RecallMode::Fast,
        include_beliefs: false,
        engine_version: "engine-wsc-test".to_string(),
    },
)
.await
.expect_err("denied recall returns a policy error");

assert!(matches!(error, CoreError::PolicyDenied(_)));
let traces = store.retrieval_traces(tenant_id);
assert_eq!(traces.len(), 1);
assert_eq!(traces[0].scope_id, denied_scope);
assert_eq!(traces[0].policy_filters[0].reason, RecallDropReason::Scope);
assert!(traces[0].context_items.is_empty());
assert!(traces[0].abstention_signal);
```

- [x] **Step 2: Run the test to verify RED**
  - Run: `cargo test -p memphant-core --test recall_trace_golden recall_writes_trace_for_scope_denial`
  - Expected: compile failure for missing `recall`, `RecallRequest`, `RecallMode`, `RecallDropReason`, `retrieval_traces`, and `CoreError::PolicyDenied`.

- [x] **Step 3: Implement minimal denial trace support**
  - Add recall request/response/trace structs and enums to `memphant-types`.
  - Add `CoreError::PolicyDenied(String)`.
  - Add `retrieval_traces` storage and accessor to `InMemoryStore`.
  - Implement `recall(&InMemoryStore, RecallRequest)` Stage 0 denial handling that persists a trace before returning `PolicyDenied`.

- [x] **Step 4: Run the test to verify GREEN**
  - Run: `cargo test -p memphant-core --test recall_trace_golden recall_writes_trace_for_scope_denial`
  - Expected: `1 passed`.

### Task 2: Candidate Channels, Weighted RRF, and Context Packing

**Files:**
- Modify: `crates/memphant-types/src/lib.rs`
- Modify: `crates/memphant-core/src/lib.rs`
- Modify: `crates/memphant-core/tests/recall_trace_golden.rs`
- Modify: `examples/evals/wsc-recall-goldens.json`

- [x] **Step 1: Write the failing golden test**
  - Add `recall_golden_fixtures_pass` that loads `examples/evals/wsc-recall-goldens.json`.
  - The first fixture seeds one active semantic memory with source episode evidence and expects:
    - returned `candidate_whitelist` contains the answer-bearing unit id.
    - returned citations only reference whitelisted unit ids.
    - persisted trace has `exact`, `lexical`, `vector`, `temporal`, `fusion`, `assemble`, and `trace` stage facts.
    - `weight_vector_id` is `default`.

- [x] **Step 2: Run the test to verify RED**
  - Run: `cargo test -p memphant-core --test recall_trace_golden recall_golden_fixtures_pass`
  - Expected: failure because the recall pipeline has no channel candidates, fusion, packing, or citation whitelist.

- [x] **Step 3: Implement the minimal green read path**
  - Add evidence fields to `StoredMemoryUnit`: `source_episode_id: Option<EpisodeId>` and `source_resource_id: Option<ResourceId>`.
  - Extend `NewMemoryUnit` with optional evidence and source metadata so tests can seed citeable units through the store seam.
  - Build in-memory channels:
    - exact: subject key or body phrase match.
    - lexical: token overlap.
    - vector: deterministic token-Jaccard similarity, traced as the vector channel proof surface.
    - temporal: active semantic units get a recency/current boost when the query contains `current`, `latest`, or `now`.
  - Fuse with weighted RRF using `k_rrf = 60`.
  - Pack to `budget_tokens` by approximate word count, preserving citations and recording budget drops.

- [x] **Step 4: Run the test to verify GREEN**
  - Run: `cargo test -p memphant-core --test recall_trace_golden recall_golden_fixtures_pass`
  - Expected: fixture passes.

### Task 3: WS-C Exit Fixture Families

**Files:**
- Modify: `crates/memphant-core/tests/recall_trace_golden.rs`
- Modify: `examples/evals/wsc-recall-goldens.json`
- Modify: `crates/memphant-core/src/lib.rs`

- [x] **Step 1: Add failing fixtures for exit-packet coverage**
  - Add fixtures for:
    - tenant isolation plus small-tenant filtered vector recall, expecting target-tenant answer and `filter_selectivity < 1.0`.
    - L1+ denied memory by allowed-scope list, expecting denied-scope unit in dropped items with `scope`.
    - citation whitelist, expecting every citation unit id is in `candidate_whitelist`.
    - stale/deleted suppression, expecting deleted and invalidated units to be dropped and current active unit returned.
    - budgeted packing with decoy collapse, expecting the answer unit survives tight budget while decoys are dropped as `duplicate` or `budget`.

- [x] **Step 2: Run fixtures to verify RED**
  - Run: `cargo test -p memphant-core --test recall_trace_golden recall_golden_fixtures_pass`
  - Expected: failures identify the missing policy, deletion, stale, filter-selectivity, or packing behavior.

- [x] **Step 3: Implement missing exit-packet behavior**
  - Apply tenant and scope gates before channel ranking.
  - Exclude `Deleted`, `Invalidated`, `Superseded`, and `Quarantined` from default context, recording controlled drop reasons.
  - Keep belief units out of default recall unless `include_beliefs` is true.
  - Compute `filter_selectivity = visible_tenant_units / all_units` for the vector channel trace.
  - Deduplicate same `subject_key` plus normalized body across channels before packing.

- [x] **Step 4: Run fixtures to verify GREEN**
  - Run: `cargo test -p memphant-core --test recall_trace_golden`
  - Expected: all recall trace golden tests pass.

### Task 4: WS-C Proof Packet and Ledger

**Files:**
- Create: `docs/build-log/2026-07-03-wsc-progress.md`
- Modify: `docs/superpowers/specs/memphant/STATUS.md`
- Modify: `/Users/sidsharma/Syndai/.wt/codex-memphant-cross-repo/docs/superpowers/specs/memphant/STATUS.md`

- [x] **Step 1: Run focused gates**
  - `cargo fmt --check`
  - `cargo test -p memphant-core --test recall_trace_golden`
  - `cargo test -p memphant-core --test store_contract`
  - `cargo test -p memphant-core --test write_compiler_golden`
  - `python3 scripts/check_spec_drift.py`

- [x] **Step 2: Run full local gates**
  - `python3 -m pytest tests`
  - `cargo clippy --all-targets --all-features -- -D warnings`
  - `cargo test --all-targets --all-features`
  - `cargo test --doc`

- [x] **Step 3: Update build log**
  - Record RED/GREEN evidence for Task 1-3.
  - Record exact gate outputs.
  - State whether WS-C is complete or which exit-packet proof remains.

- [x] **Step 4: Update ledger only with complete proof**
  - Flip WS-C in `STATUS.md` only if every recall writes a trace, denied recall writes a trace, and the oracle/security fixture families all pass.
  - Sync the mirrored Syndai `STATUS.md`.
  - Run `python3 scripts/check_spec_drift.py` and require `spec_drift=clean`.

## Self-Review

- Spec coverage: Task 1 covers Stage 0 denial traces. Task 2 covers exact/lexical/vector/temporal channels, RRF fusion, packing, citations, trace stage facts, and candidate whitelist. Task 3 covers the named WS-C exit families: answer-bearing oracle, tenant isolation, L1+ denied scope, citation whitelist, small-tenant filtered vector recall, stale/deleted suppression, deletion-generation state filters, and budget packing.
- Placeholder scan: no TODO/TBD placeholders are present; every task names files, commands, and expected red/green outcomes.
- Type consistency: the plan consistently uses `RecallRequest`, `RecallMode`, `RecallDropReason`, `RetrievalTrace`, `RecallResponse`, `StoredMemoryUnit.source_episode_id`, and `StoredMemoryUnit.source_resource_id`.

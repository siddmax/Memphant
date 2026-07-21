# P1-T6 Build-Once Paired Gate Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development and superpowers:test-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the protocol-superseded 48-construction model screen with one pre-registered Sonnet Deep treatment paired against Fast on 12 cases, constructing each pinned case exactly once and cloning its immutable pre-query PostgreSQL state into distinct arm databases.

**Architecture:** Each case gets one content-addressed, data-only PostgreSQL construction bank. MemPhant retains and compiles the case once with Deep disabled, excludes credentials/transient tables, freezes a logical state identity, and atomically archives the bank using the existing Memora dump/restore contract. A fresh migrated source database restores that verified bank, mints one ephemeral key, then PostgreSQL clones the source for Fast and Deep after every source connection stops. The official adapter validates the complete trajectory digest in query-only mode and performs no retain or worker work. The archive survives interruption and its large dump is deleted only after both immutable arm rows are complete.

**Tech Stack:** Python 3.14 stdlib, PostgreSQL 17/18 `pg_dump`/`pg_restore` plus native `createdb --template`/`dropdb --force`, existing packaged MemPhant server/worker/CLI, pytest, pinned LongMemEval-V2 official harness.

## Global Constraints

- Fast remains the automatic product default; Deep remains explicit, bounded, cancellable, and never auto-selected.
- The T6 feasibility gate is exactly 12 Fast/Sonnet pairs: 12 constructions, 24 answer rows, and at most 12 Deep dispatches.
- Sonnet is selected before any Deep treatment output because the existing plan names it the accuracy/Pareto candidate; Luna and Sol are not run unless a later answer-blind amendment proves Sonnet infeasible and authorizes a new root.
- The stopped `run-408363c9` root remains diagnostic-only and immutable; never replay its completed Fast row.
- Every pair uses the exact same pinned case in both arms. Across the 12 cases this is 7,934 resources total (613-713 per case) over 5,979 trajectories (479 for `b05cf470`, 500 for every other case). Gold, answer, evaluator, and prior-output fields never enter construction or recall.
- A construction source has zero active connections before `createdb --template`; Fast and Deep database names are distinct; every arm clone is force-dropped on success, failure, or interruption recovery.
- Pair identity uses the established bank exclusions for `memphant.api_key`, schema migrations, event outbox, job state, retrieval traces, and review-event tables. Those are separately proven as schema identity, completed construction, empty query audit state, or transport authentication. Every corpus, compiled-unit, policy, context, subject, tenant, trust, embedding, and relevant sequence row included by the bank must match the frozen logical identity exactly before recall.
- Query-only adapter execution validates all trajectory IDs and hashes but performs zero episode POSTs and zero worker drains.
- A completed billable row is immutable. Missing/tampered archive state after one completed row invalidates the root rather than replaying that row.
- Before each paid or large-compute step, archive an efficiency checkpoint proving necessity, reusable work, expected information gain, maximum rows, maximum construction count, worst-case liability, and stop predicate.
- Construction, retrieval, reader generation, judge generation, and Deep generation latency/cost are recorded separately.
- No push, ledger flip, integration, public claim, or larger confirmation follows from implementation tests or one completed pair.
- Preserve the unrelated `docs/handoff/NEXT-SESSION-PROMPT.md` edit.

---

### Task 1: Supersede the four-arm execution contract

**Files:**
- Modify: `benchmarks/manifests/longmemeval_v2.p1_t6.json`
- Modify: `scripts/run_lme_v2_p1_t6.py` (manifest verification and row expansion contract only)
- Modify: `.superpowers/sdd/briefs/p1-t6-task-6-exposed-n12-gate.md`
- Modify: `docs/superpowers/plans/2026-07-20-agentic-deep-recall.md`
- Create: `docs/build-log/artifacts/p1-t6/PRE-EXECUTION-AMENDMENT-11.md`
- Test: `tests/test_run_lme_v2_p1_t6.py`

**Interfaces:**
- Consumes: existing answer-blind 12-case selection and Sonnet dated route/config hash.
- Produces: `run_order.arm_order_per_case == ["fast", "sonnet"]`, 24-row expansion, and a cumulative hard-cap contract that includes all prior settled/unsettled liability.

- [ ] **Step 1: Write failing manifest tests**

```python
def test_campaign_is_single_candidate_paired_gate(campaign):
    manifest = campaign.load_campaign_manifest()
    assert campaign.verify_campaign_manifest(manifest) == {
        "cases": 12, "rows": 24, "arms": 2, "constructions": 12,
    }
    assert manifest["run_order"]["arm_order_per_case"] == ["fast", "sonnet"]
    assert manifest["protocol"]["selected_deep_arm"] == "sonnet"
```

- [ ] **Step 2: Run the focused test and verify the old 48-row assertion fails**

Run: `python3 -m pytest tests/test_run_lme_v2_p1_t6.py -q`

- [ ] **Step 3: Freeze the two-arm manifest and spend ceiling**

Set the fresh maximum to 12 Deep reservations plus 24 reader/judge reservations. Add the 3,018-micro-dollar settled reader cost from `run-408363c9` to prior liability. Preserve Luna/Sol metadata only as an inactive researched shortlist; manifest verification and row expansion must use `selected_deep_arm`. Aggregate selection moves in Task 4.

- [ ] **Step 4: Write Amendment 11 before any new treatment output**

The amendment must name Sonnet, 12/24/12 counts, the stopped-root invalidation hash, the new manifest hash, cumulative liability, efficiency checkpoint fields, and the rule that Luna/Sol require a fresh amendment/root.

- [ ] **Step 5: Run focused tests and commit**

Run: `python3 -m pytest tests/test_run_lme_v2_p1_t6.py -q`

Commit: `docs: preregister efficient P1-T6 paired gate`

### Task 2: Add construction and query-only adapter modes

**Files:**
- Modify: `benchmarks/longmemeval_v2/memphant_memory.py`
- Modify: `benchmarks/manifests/longmemeval_v2_memphant_adapter.lock.json`
- Test: `tests/test_public_benchmark_adapters.py`

**Interfaces:**
- Produces: `MemphantMemory.prepare() -> dict[str, object]`, a non-secret construction proof; query-only mode loaded from `MEMPHANT_LME_PREBUILT_PROOF`.
- Consumes: `MEMPHANT_LME_PREBUILT_PROOF` path and a fresh clone-local API key minted for the frozen tenant.

- [ ] **Step 1: Add failing construction/query-only tests**

Cover normal `insert -> prepare`, exact trajectory/order/hash validation in query-only `insert`, zero retain requests, zero worker invocation, malformed proof rejection, tenant/context reuse, and proof output that references the construction hash without duplicating retains.

- [ ] **Step 2: Extract shared trajectory validation**

Keep one `_validate_trajectory()` path used by both modes. Normal mode retains fragments exactly as today. Query-only mode recomputes the trajectory hash and checks it against the frozen construction proof, then records no new resource.

- [ ] **Step 3: Add `prepare()`**

`prepare()` drains exactly `resource_count` jobs once, freezes ordered trajectory IDs/hashes, retain proofs, worker proof, tenant/context, binary fingerprints, and adapter/config hashes, and forbids a prior query. It never calls a reader, judge, or Deep provider.

- [ ] **Step 4: Make query-only `query()` skip construction**

Require all frozen trajectories to have been validated, reuse `resource_count` and worker proof, perform only recall/trace/mutation proof, and emit `construction_proof_sha256` plus `query_only: true`.

- [ ] **Step 5: Run adapter tests and commit**

Run: `python3 -m pytest tests/test_public_benchmark_adapters.py -q`

Commit: `feat: reuse frozen LongMemEval construction`

### Task 3: Archive one crash-safe case bank and clone per arm

**Files:**
- Modify: `scripts/run_lme_v2_p1_t6.py`
- Test: `tests/test_run_lme_v2_p1_t6.py`

**Interfaces:**
- Produces: `_database_bank_identity(database_url)`, `_dump_case_bank(...)`, `_restore_case_bank(...)`, `_clone_case_source(...)`, `_drop_local_database(...)`, and `_run_case(...)`.
- Consumes: the adapter construction/query-only contract from Task 2.

- [ ] **Step 1: Add failing database-orchestration tests**

Assert one construction/dump per case, verified restore before execution, PostgreSQL client/server-major equality, server shutdown and zero source connections before clone, two distinct clones, identical logical hashes, source recheck before the second clone, unconditional force-drop, query-only environment, no secret artifact, interrupted archive reuse, completed-row non-replay, and fail-closed behavior when a completed row's archive is missing or changed.

- [ ] **Step 2: Reuse the existing content-addressed bank contract**

Extract/reuse the generic parts of Memora's `postgres_tool_identity`, logical identity, custom data-only dump, archive SHA filename, schema identity, single-transaction restore, and tamper checks. Do not import Memora-specific group/provider semantics. Exclude the same transient tables and add a LongMemEval construction manifest binding question/materialization, adapter, binaries, compiler, resource count, retain hashes, worker proof, schema, PostgreSQL major, archive, and logical identity. Delete the large `.dump` only after both row proofs complete; preserve the manifest/hash in the immutable artifact.

- [ ] **Step 3: Implement strict local source/clone lifecycle**

Use the unchanged `with_scratch_db.sh` once around `_run_case` so every initial or resumed case starts with a fresh migrated source. If no archive exists, build and dump once; otherwise restore and verify without construction. Accept only local PostgreSQL URLs and arm names matching `memphant_p1t6_[0-9a-f]{8}_[0-9a-f]{8}_(fast|sonnet)`. Never use the shared campaign database as a data template.

- [ ] **Step 4: Freeze and restore the state identity**

Require the build worker to complete every resource and leave zero queued/running/dead jobs before dump. The dump excludes all API keys and transient tables. On restore, verify schema/migration identity, PostgreSQL major, archive SHA, and exact logical identity; then mint exactly one trusted-system key for the frozen tenant. Archive no raw key or database credential.

- [ ] **Step 5: Clone and execute each row**

Stop the source server and verify zero connections, run `createdb --maintenance-db=<admin> --template=<source> <arm-db>`, compare the clone's logical identity with the frozen source, and invoke the existing official row path in query-only mode with the ephemeral key. Force-drop the arm clone in `finally` and redact all external and local credentials after child reaping.

- [ ] **Step 6: Make resume semantics immutable**

Reuse a verified archive after interruption and rebuild only the cheap migrated source. Recover and drop orphan arm clones before continuing. Never reconstruct a missing/changed archive after either arm completed; preserve/invalidate the root instead of replaying a billable row.

- [ ] **Step 7: Run focused tests and commit**

Run: `python3 -m pytest tests/test_run_lme_v2_p1_t6.py tests/test_public_benchmark_adapters.py -q`

Commit: `fix: build P1-T6 cases once per pair`

### Task 4: Bind aggregation and evidence to build-once pairing

**Files:**
- Modify: `scripts/run_lme_v2_p1_t6.py`
- Modify: `.superpowers/sdd/p1-t6-task-6-report.md`
- Modify: `.superpowers/sdd/progress.md`
- Test: `tests/test_run_lme_v2_p1_t6.py`

**Interfaces:**
- Produces: aggregate proof for exactly 12 Fast/Sonnet pairs with 12 unique construction proofs and 24 distinct scratch identities.

- [ ] **Step 1: Add failing aggregate tests**

Reject missing construction proofs, more/fewer than 12 construction hashes, pair state mismatch, same arm database, non-query-only rows, construction work inside an arm, inactive candidate rows, or latency/cost fields that mix construction with recall/generation.

- [ ] **Step 2: Aggregate only Fast versus selected Sonnet**

Keep the existing official binary score, wins/losses, latency, cost, truncation, security, settlement, and positive-delta predicates. Report construction latency separately and never include it in the explicit Deep query p50/p95.

- [ ] **Step 3: Update durable progress truthfully**

Record the stopped diagnostic root and build-once implementation. Keep T6 open until all 12 pairs and aggregate predicates are proven.

- [ ] **Step 4: Run focused tests and commit**

Run: `python3 -m pytest tests/test_run_lme_v2_p1_t6.py -q`

Commit: `test: prove P1-T6 build-once pairing`

### Task 5: Verify before paid execution

**Files:**
- Add: immutable no-model build/clone proof under `docs/build-log/artifacts/p1-t6/`

- [ ] **Step 1: Run one no-model exact-case integration**

Build 670 resources once, dump once, restore once, clone twice, run scripted/local recall only, and prove equal pre-query identities, zero arm retains/drains, distinct DBs, complete cleanup, and no external dispatch.

- [ ] **Step 2: Run the focused and full repository gates**

Run the complete `AGENTS.md` verification suite, including scratch PostgreSQL contracts, all provider lints, migration dry-run, and packaged e2e probe. Preserve exact outputs at the measured commit.

- [ ] **Step 3: Independent review**

Review Task 1-4 diffs for spec compliance and code quality. Fix every Critical/Important finding and rerun its covering tests.

- [ ] **Step 4: Commit the authorization proof**

Commit: `docs: authorize build-once P1-T6 execution`

### Task 6: Execute only the necessary evidence ladder

**Files:**
- Add: fresh immutable run root under `docs/build-log/artifacts/p1-t6/`
- Modify `STATUS.md` only with the named passing n=12 and independent confirmation proofs.

- [ ] **Step 1: Archive the pre-dispatch efficiency checkpoint**

Require: 12 constructions, 24 answer rows, 12 maximum Deep dispatches, exact worst-case liability under the amended ceiling, complete cleanup, and stop-after-Sonnet predicates. If any value drifts upward, stop before dispatch.

- [ ] **Step 2: Run the 12 Fast/Sonnet pairs**

Preserve every outcome and settlement. Never rerun a completed billable row. Aggregate only after all 12 immutable pairs exist.

- [ ] **Step 3: Apply the registered decision**

If any feasibility predicate fails, preserve the negative artifact and stop T6; do not try Luna/Sol automatically. If all pass, preregister one independent n≈100-300 paired confirmation with a fresh answer-blind selection, paired CI, fixed/adaptive stop rule, and separate spend ceiling.

- [ ] **Step 4: Continue the existing campaign order**

Only after T6 confirmation passes: update the ledger, complete P1-T1, then SWE-ContextBench Lite, LongMemEval-S full-500, and LongMemEval-V2. Before each expansion, repeat the efficiency checkpoint and use the smallest run that can answer the active gate.

## Research basis

- PostgreSQL 18 documents that `CREATE DATABASE ... TEMPLATE` copies an existing database and prevents new source connections during the copy; it fails if a session is already connected. This is the native same-cluster mechanism used here: <https://www.postgresql.org/docs/current/sql-createdatabase.html>.
- PostgreSQL 18 documents custom-format logical dumps and `pg_restore`; the implementation reuses the repo's already-tested single-transaction, content-addressed pattern for interruption-safe construction reuse: <https://www.postgresql.org/docs/current/app-pgdump.html> and <https://www.postgresql.org/docs/current/app-pgrestore.html>.
- The 2026 Agent Memory systems characterization separates construction, retrieval, and generation and recommends phase-aware scheduling/amortization; the harness therefore measures and reuses construction rather than charging it repeatedly to every arm: <https://arxiv.org/abs/2606.06448>.
- The 2026 efficient-evaluation work reports up to 5x effective-sample-size gains from information-aware query selection while preserving coverage. We do not introduce adaptive sampling into this already-frozen n=12 gate, but larger confirmations must preregister an efficiency-aware stopping/selection method instead of brute-force model grids: <https://arxiv.org/abs/2601.20251>.

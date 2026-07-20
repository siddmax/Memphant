# Benchmark Paid-Call Provenance Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make STALE and MemSyco fail closed unless every paid reader and judge call has a fresh, reconciled, hash-bound attempt proof.

**Architecture:** Extract the proven Memora attempt ledger into one benchmark-neutral helper, then make the OpenRouter reader and OpenAI-compatible official harnesses write the same canonical attempt rows. STALE and MemSyco keep their pinned upstream evaluators and public metrics unchanged; repo-owned wrappers bootstrap metering, reconcile authoritative generation statistics, and verify complete proof bundles before results are accepted.

**Tech Stack:** Python 3.10, pytest, stdlib JSON/JSONL and hashing, OpenAI Python SDK, OpenRouter generation statistics API, pinned STALE and MemSyco upstream harnesses, MemPhant scratch Postgres runtime.

## Global Constraints

- Do not make paid model calls during implementation or no-cost verification.
- Do not reset, clean, commit, push, or rebase the existing dirty worktree.
- Do not modify pinned upstream STALE or MemSyco source files.
- Disable opaque SDK retries; repo-owned retry rows must be explicit and independently verifiable.
- Persist no secrets, prompts, or raw model responses in attempt ledgers; use canonical request/result hashes.
- Preserve all public REST, MCP, SDK, and benchmark metric interfaces.
- STALE smoke eligibility requires exactly three reader attempts and one judge attempt, all fresh, uniquely identified, positively priced, and retry/error/parse free.
- MemSyco continues to report five separate official task metrics and never emits an invented aggregate scalar.
- Use Python 3.10 via `uv` for official harness dependencies, scratch databases only, and Doppler-provided credentials only after explicit paid-run authorization.

---

## File Map

- Create `scripts/provider_attempts.py`: canonical durable attempt ledger, normalization, validation, OpenRouter generation reconciliation, and sync/async OpenAI SDK metering.
- Modify `scripts/generate_memora_memphant_answers.py`: import the shared ledger API without changing Memora proof semantics.
- Modify `scripts/run_reader.py`: retain explicit response identity, requested/served model, provider, per-attempt timing, retry index, and usage.
- Modify `scripts/generate_stale_memphant_answers.py`: archive the three reader attempt slices and bind their shared ledger hash into the answer proof.
- Create `benchmarks/stale/harness_bootstrap.py`: execute the unchanged official STALE judge under async metering with SDK retries disabled.
- Modify `scripts/run_stale.py`: launch the bootstrap and fail closed on the complete four-call smoke contract.
- Modify `benchmarks/memsyco/harness_bootstrap.py`: replace the narrow success-only wrapper with the shared sync/async meter.
- Modify `scripts/run_restraint_bench.py`: add `acquire`, `verify`, `run`, and `verify-results` orchestration and preserve five separate metrics.
- Modify `tests/test_run_reader_contract.py`, `tests/test_temporal_benchmark_contract.py`, and `tests/test_restraint_benchmark_contract.py`: regression coverage for every malformed proof listed in the approved plan.

### Task 1: Shared Durable Attempt Ledger and Reader Metadata

**Files:**
- Create: `scripts/provider_attempts.py`
- Modify: `scripts/generate_memora_memphant_answers.py`
- Modify: `scripts/run_reader.py`
- Test: `tests/test_run_reader_contract.py`
- Test: `tests/test_temporal_benchmark_contract.py`

**Interfaces:**
- Produces: `ProviderAttemptLedger(path: Path, fingerprint: Mapping[str, object])`, `ledger.record_start(...)`, `ledger.record_result(...)`, `ledger.record_error(...)`, `ledger.snapshot()`, `validate_provider_attempt_ledger(...)`, and `fresh_paid_usage(...)`.
- Produces: reader call metadata with `response_id`, `requested_model`, `served_model`, `provider`, `usage`, `elapsed_seconds`, and `retry_index`.
- Consumes: the existing `ReaderCli.provider_attempt_hook(event, cache_key, payload)` boundary and the existing Memora ledger semantics.

- [ ] **Step 1: Write failing shared-ledger and reader contract tests**

  Add focused tests that assert interrupted `start` rows are incomplete, duplicate result response IDs are rejected, zero or absent pricing is rejected, and an altered resume-ledger hash is rejected. Update reader assertions to require the explicit metadata fields and prove retry index plus elapsed time are retained.

- [ ] **Step 2: Run the focused tests and confirm contract failures**

  Run: `python3 -m pytest tests/test_run_reader_contract.py tests/test_temporal_benchmark_contract.py -q`

  Expected: new tests fail because the shared helper and expanded metadata do not exist.

- [ ] **Step 3: Extract the canonical implementation and expand reader evidence**

  Move the durable Memora ledger implementation into `scripts/provider_attempts.py`, preserving canonical JSON hashing and append-before-call semantics. Import it back into Memora. In `ReaderCli._call_openrouter`, record monotonic duration around each explicit attempt and return the response ID, requested model, served model, resolved provider, detailed usage, duration, and retry index. Increment the cache response-contract version so pre-provenance cached responses cannot satisfy the new contract.

- [ ] **Step 4: Run the focused tests to green**

  Run: `python3 -m pytest tests/test_run_reader_contract.py tests/test_temporal_benchmark_contract.py -q`

  Expected: all focused tests pass with existing Memora behavior preserved.

### Task 2: STALE Reader Proof and Official Judge Bootstrap

**Files:**
- Modify: `scripts/generate_stale_memphant_answers.py`
- Create: `benchmarks/stale/harness_bootstrap.py`
- Modify: `scripts/run_stale.py`
- Test: `tests/test_temporal_benchmark_contract.py`

**Interfaces:**
- Consumes: `ProviderAttemptLedger`, `validate_provider_attempt_ledger`, and expanded `ReaderCli` metadata from Task 1.
- Produces: each answer dimension’s `attempts` slice and `parse_status`; answer proof fields `provider_attempt_ledger`, `provider_attempt_ledger_sha256`, and `provider_attempt_summary`.
- Produces: judge sidecar ledger selected by `MEMPHANT_PROVIDER_ATTEMPT_LEDGER` and bound to an invocation fingerprint.

- [ ] **Step 1: Write failing STALE provenance tests**

  Add regressions proving generation rejects missing response identity/model/provider/token/cost fields and that result verification rejects missing judge metadata, duplicate response IDs, hidden retries, parse/error rows, judge/result count mismatch, fewer or more than three reader calls, and a mismatched ledger hash.

- [ ] **Step 2: Run STALE contract tests and confirm fail-open behavior is exposed**

  Run: `python3 -m pytest tests/test_temporal_benchmark_contract.py -q`

  Expected: the new malformed-proof cases fail because current validators accept them.

- [ ] **Step 3: Persist and validate the three reader attempt slices**

  Attach the reader attempt hook to one run-scoped durable ledger, record every start/result/error before advancing, copy the exact per-dimension slice into each answer row, and set parse status only after schema parsing succeeds. Bind the ledger hash and summary into the proof and require three complete fresh paid calls per generated smoke record.

- [ ] **Step 4: Meter the unchanged async STALE judge**

  Implement `benchmarks/stale/harness_bootstrap.py` with `runpy.run_path`, patch both `openai.AsyncOpenAI` and `openai.OpenAI` before the official module imports them, force `max_retries=0`, and write the same canonical start/result/error rows. When immediate OpenRouter metadata lacks provider or cost, reconcile `/api/v1/generation?id=<response_id>` and hash the normalized result rather than persisting raw output.

- [ ] **Step 5: Make the STALE wrapper fail closed**

  Launch the bootstrap instead of the official script directly, pass secrets only through the child environment, and verify one smoke record has exactly three reader results plus one judge result, four unique response IDs, positive tokens and reconciled cost, three non-degraded trace facts, zero retry/error/parse-failure rows, exact result/detail ID parity, and matching canonical hashes.

- [ ] **Step 6: Run STALE tests and the fixture-only dry-run**

  Run: `python3 -m pytest tests/test_temporal_benchmark_contract.py -q`

  Run: `python3 scripts/generate_stale_memphant_answers.py --help >/dev/null`

  Expected: tests pass and CLI construction performs no network or paid call.

### Task 3: Shared Sync/Async Meter and MemSyco CLI Lifecycle

**Files:**
- Modify: `scripts/provider_attempts.py`
- Modify: `benchmarks/memsyco/harness_bootstrap.py`
- Modify: `scripts/run_restraint_bench.py`
- Test: `tests/test_restraint_benchmark_contract.py`

**Interfaces:**
- Produces: `install_openai_meter(openai_module, ledger, context, generation_lookup=None)` wrapping sync and async `chat.completions.create` with `max_retries=0`.
- Produces: `run_restraint_bench.py acquire|verify|run|verify-results` subcommands.
- Consumes: `scripts.gate_runtime.reexec_through_scratch_db`, `provision_tenant`, `provision_api_key`, and `Server`; the existing `official_command(...)`; MemPhant per-sample proof files.

- [ ] **Step 1: Write failing meter and CLI tests**

  Add fake sync and async clients covering successful reconciliation, exceptions after a durable start row, missing pricing, duplicate response IDs, and constructor retry suppression. Add CLI tests for safe acquire/verify, five-task command construction, one-sample-per-task run metadata, interrupted resume rejection, result-count mismatch, and separate metric preservation without an aggregate key.

- [ ] **Step 2: Run MemSyco contract tests and confirm failures**

  Run: `python3 -m pytest tests/test_restraint_benchmark_contract.py -q`

  Expected: new tests fail because the meter and subcommands are incomplete.

- [ ] **Step 3: Replace the local meter with the shared implementation**

  Register the MemPhant adapter first, install the shared wrappers before importing the official harness, and tag each attempt with task, arm, request hash, retry index, parse status, elapsed time, response ID, requested/served model, provider, tokens, and cost. Never persist base URL credentials, prompts, or completions.

- [ ] **Step 4: Implement the MemSyco command lifecycle**

  Add argparse subcommands. `acquire` downloads and verifies the pinned release; `verify` is read-only; `run` reexecutes through one ephemeral migrated database, builds the three packaged binaries, starts the real server/worker, provisions per-task tenants, and launches exactly one official sample per task for the authorized smoke; `verify-results` performs no calls and validates an existing proof bundle.

- [ ] **Step 5: Implement fail-closed five-task result verification**

  Require one official report and one MemPhant proof per task, exact task/sample identity parity, complete extractor coverage, empty `gold_fields_consumed`, complete answer/judge attempt ledgers, unique response IDs, positive tokens and cost, no start/error/retry/parse-failure residue, and matching proof hashes. Emit a dictionary keyed by the five official tasks only; reject `aggregate`, `overall`, or synthesized scalar fields.

- [ ] **Step 6: Run MemSyco tests to green**

  Run: `python3 -m pytest tests/test_restraint_benchmark_contract.py -q`

  Expected: all contract tests pass without external calls.

### Task 4: No-Cost Gate and Handoff Evidence

**Files:**
- Modify: `docs/handoff/NEXT-SESSION-PROMPT.md`
- Modify only if a named proof checkbox is satisfied: `docs/superpowers/specs/memphant/STATUS.md`

**Interfaces:**
- Consumes: the complete STALE and MemSyco no-cost contracts from Tasks 1-3.
- Produces: a restartable handoff that names remaining paid authorization gates and exact commands.

- [ ] **Step 1: Run the approved focused no-cost gate**

  Run:

  ```sh
  python3 -m pytest \
    tests/test_run_reader_contract.py \
    tests/test_temporal_benchmark_contract.py \
    tests/test_restraint_benchmark_contract.py -q

  python3 -m py_compile \
    scripts/provider_attempts.py \
    scripts/generate_stale_memphant_answers.py \
    scripts/run_stale.py \
    scripts/run_restraint_bench.py \
    benchmarks/stale/harness_bootstrap.py \
    benchmarks/memsyco/memphant_baseline.py \
    benchmarks/memsyco/harness_bootstrap.py
  ```

  Expected: focused tests and compilation succeed with no credentials and no paid calls.

- [ ] **Step 2: Run fixture and CLI dry checks**

  Run fixture-only STALE generation and `--help`/read-only verification paths for both wrappers. Confirm no child command contains a secret and no paid endpoint is contacted.

- [ ] **Step 3: Update the handoff truthfully**

  Record the no-cost proof artifacts and exact paid ladder. Do not move any benchmark STATUS checkbox: STALE prefix smoke, five-task MemSyco smoke, causal Memora split, full runs, sealed 319, and Syndai/CaaS cutover remain explicitly blocked pending authorization and their named artifacts.

- [ ] **Step 4: Inspect the final diff for scope and accidental generated/public changes**

  Run: `git diff --check`

  Run: `git status --short`

  Expected: no whitespace errors; only the planned benchmark provenance files plus pre-existing unrelated dirty work are present.

## Engineering Review

### What already exists

- `scripts/generate_memora_memphant_answers.py` already has the durable start/result ledger and canonical hash used in a real paid lane. The plan extracts and strengthens it instead of creating a second accounting format.
- `scripts/run_reader.py` already owns explicit OpenRouter retries and exposes an attempt hook. The plan enriches that boundary; it does not add another retry engine.
- `benchmarks/memsyco/harness_bootstrap.py` already intercepts the official OpenAI-compatible client, and `scripts/run_restraint_bench.py` already pins acquisition and command construction. The plan completes those seams.
- `scripts/gate_runtime.py` already owns scratch database re-exec, runtime processes, and tenant/key provisioning. MemSyco orchestration reuses it.
- `scripts/run_stale.py` already verifies pinned source/data hashes and exact answer IDs. The plan adds attempt proof verification around the unchanged scorer.

### Reviewed data flow

```text
repo wrapper
  |
  +-- canonical request hash + context
  |
  v
ProviderAttemptLedger.record_start() -----> atomic ledger write
  |                                               |
  |                                               +-- interruption leaves visible started row
  v
provider call (SDK retries disabled / repo retry explicit)
  |
  +-- transport or parse error -----------> record_error() -> fail closed
  |
  v
immediate response metadata
  |
  +-- missing provider or cost -----------> OpenRouter generation lookup
  |                                               |
  |                                               +-- unavailable/incomplete -> fail closed
  v
normalize identity + model + usage + timing
  |
  v
record_result() -> atomic ledger write -> canonical hash
  |
  +-- STALE: 3 reader slices + 1 judge row + 3 trace facts
  |
  +-- MemSyco: per-task answer/judge rows + adapter proof
  v
offline verifier -> accepted proof or precise contract error
```

The ledger is the only file that needs the state-machine diagram inline. The thin bootstraps should point to the shared helper rather than copy the diagram or rules.

### Test coverage map

```text
provider_attempts.py
  +-- start -> result --------------------- unit: complete paid attempt
  +-- start -> interruption --------------- unit: incomplete ledger rejected
  +-- start -> error ---------------------- unit: error row rejected
  +-- duplicate response ID --------------- unit: cross-attempt rejection
  +-- missing/zero cost ------------------- unit: pricing rejection
  +-- stats lookup success/failure -------- unit: reconcile or fail closed
  +-- sync/async SDK wrappers ------------- unit: both clients, retries disabled

run_reader.py
  +-- first success ----------------------- unit: explicit metadata retained
  +-- explicit retry ---------------------- unit: retry index/timing retained
  +-- stale cache ------------------------- unit: response-contract key changes

STALE
  +-- three parsed reader dimensions ------ contract: exact attempt slices
  +-- official async judge ---------------- contract: sidecar ledger consumed
  +-- malformed proof variants ------------ regression: all fail closed
  +-- fixture-only generation ------------- integration: no network/paid call

MemSyco
  +-- acquire/verify ---------------------- contract: pinned hashes/counts
  +-- five one-sample task commands ------- contract: exact task identity
  +-- proof/result reconciliation --------- contract: count/hash/ID parity
  +-- five official metrics --------------- regression: no aggregate scalar
```

### Failure modes

- Process death after request start: the atomic `started` row survives; resume and verification reject it until a new run fingerprint/ledger is used. Covered by an interrupted-attempt test and a clear verifier error.
- Provider succeeds but immediate metadata omits cost/provider: generation lookup reconciles by response ID; missing or conflicting statistics fail closed. Covered by lookup success, lookup failure, and disagreement tests.
- SDK performs an unobservable retry: constructors are forced to `max_retries=0`; a constructor/attempt-count test detects regression.
- Response content cannot parse after a paid result: the result row remains, parse status is failure, and promotion verification rejects the proof. Covered by STALE and MemSyco parse-failure tests.
- Ledger or proof is edited during resume: canonical hash and invocation fingerprint mismatch before further calls. Covered by resume-ledger hash tests.
- Judge emits fewer/more results than samples: exact result, detail, proof, and attempt counts reject the run. Covered in both harness contracts.
- Stats lookup times out: no result is silently accepted; the wrapper reports a reconciliation error and retains durable evidence. Covered with a deterministic fake timeout.

No silent failure path remains in the accepted-proof flow.

### Performance review

- Generation-stat reconciliation runs only when the immediate response lacks authoritative provider or cost data. It is bounded to one lookup per otherwise complete response and never sits inside a database transaction.
- Ledger writes are intentionally atomic after each state transition. Smoke volume is four STALE calls and bounded MemSyco samples, so durability is more important than batching.
- Official harness concurrency remains pinned (`STALE` smoke contract and MemSyco `--workers 1`) to keep attempt/result pairing deterministic.

### Parallelization

Sequential implementation, no safe worktree parallelization opportunity. The two harness lanes both depend on the shared ledger schema and both modify the same Python contract-test surface; merging independent versions would risk proof-schema drift.

### NOT in scope

- Paid STALE, MemSyco, or Memora calls: require explicit authorization after this no-cost gate.
- Full STALE, full MemSyco, 600-question Memora, and sealed 319: remain separate promotion gates with existing artifacts.
- Syndai/CaaS cutover: remains blocked on benchmark and sealed confirmation.
- Public REST/MCP/SDK changes: attempt provenance is benchmark-internal.
- A generic proxy service or database-backed billing subsystem: the canonical file ledger is sufficient for reproducible local harnesses and avoids new infrastructure.
- A new benchmark aggregate: MemSyco's five official metrics remain separate.

## Implementation Tasks

Synthesized from the review. The four tasks above are the actionable units:

- [ ] **T1 (P1, human: ~4h / Codex: ~35min)** — Shared provenance — Extract and strengthen the durable ledger and reader metadata. Verify with the focused reader and temporal contracts.
- [ ] **T2 (P1, human: ~4h / Codex: ~35min)** — STALE — Meter the unchanged judge and enforce the exact four-call proof. Verify with malformed-proof regressions and fixture dry-run.
- [ ] **T3 (P1, human: ~6h / Codex: ~50min)** — MemSyco — Share sync/async metering and complete the four CLI commands plus five-metric verifier. Verify with the restraint contract.
- [ ] **T4 (P1, human: ~2h / Codex: ~20min)** — Gate/handoff — Run the no-cost gate and update only truthful restart evidence.

## GSTACK REVIEW REPORT

| Review | Trigger | Why | Runs | Status | Findings |
|--------|---------|-----|------|--------|----------|
| CEO Review | `/plan-ceo-review` | Scope and strategy | 0 | Not run | User supplied the approved scope |
| Codex Review | `/codex review` | Independent second opinion | 0 | Not run | Deferred to final diff review |
| Eng Review | `/plan-eng-review` | Architecture and tests | 1 | Clear | Existing seams reused; state machine centralized; no critical gaps |
| Design Review | `/plan-design-review` | UI and UX gaps | 0 | Not applicable | No UI change |
| DX Review | `/plan-devex-review` | Developer experience gaps | 0 | Covered here | CLI lifecycle, failure messages, dry paths, and handoff commands reviewed |

**VERDICT:** ENG CLEARED — ready to implement the no-cost provenance layer.

NO UNRESOLVED DECISIONS

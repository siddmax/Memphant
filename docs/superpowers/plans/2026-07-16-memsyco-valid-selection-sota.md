# MemSyco Valid Memory Selection SOTA Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: use `superpowers:executing-plans` and `superpowers:test-driven-development`. Work inline in the existing dirty tree; do not create a worktree, commit, push, rebase, reset, clean, edit `STATUS.md`, or touch the paused cutover.

**Goal:** Repair the shared structured-state preference collision, earn 12/12 development and independent confirmation gates, then run the untouched 350-row Valid Memory Selection official track once against a paired RawDialogue control.

**Architecture:** Keep the existing structured-state decoder, MemSyco runner, provider ledgers, packet verifier, calibration generator, and official scorer. Normalize grounded preference-shaped state before duplicate arbitration; extend existing verification functions instead of creating a second harness.

**Tech stack:** Rust, Python 3.10, PostgreSQL 17, fastembed BGE-M3, pinned MemSyco-Bench, OpenRouter DeepSeek-V4-Flash.

## Global constraints

- Correctness gates: MemPhant and RawDialogue must each be 12/12 on development and sealed confirmation, with latest-use 1.0, contamination 0.0, and zero parse, judge, retry, packet, hash, or provenance failures.
- Official gates: full-350 accuracy at least 81.29% with bootstrap lower bound above 78.29%; contamination at most 13.34% with upper bound below 16.34%; paired accuracy lower bound above zero; paired contamination upper bound below zero.
- Clean-349 excluding only `v11_000321` is sensitivity analysis and cannot rescue a full-350 miss.
- Episode-only is diagnostic. Retired official tracks remain untouched.
- No new dependency, public API, database schema, adapter contract, judge protocol, or benchmark harness.
- Accuracy and restraint outrank cost. Measure tokens, cost, and latency; retain the synchronous recall p95 ceiling of 1.5 seconds.

## Task 1: Prove and repair the decoder collision

**Files:**
- Modify/test: `crates/memphant-runtime/src/structured_state_openrouter.rs`

- [ ] Add privacy-safe duplicate diagnostics without changing acceptance behavior. Preserve source channel; emit operation shape, identity booleans, role booleans, quote order, and failed predicate codes only.
- [ ] Run non-official case 10 in fresh `memphant-v7-offset9-diagnostic-pre-fix`. Require one reconciled extractor result, `ABORTED`, zero answer/judge calls, and the predicted unreserved-existing/reserved-incoming later-quote collision. Do not run `verify-results`.
- [ ] Add failing Rust regressions for neutral generic plus dedicated preference in both channel orders and for delete never gaining preference fields. Retain generic collision, exact duplicate, and earlier/later order coverage.
- [ ] Run the focused tests and observe the expected failures.
- [ ] Compute the grounded explicit preference once for Create/Replace, include it in preference classification, and reuse it for reserved fields and validation. Delete receives no synthesized fields.
- [ ] Run focused Rust tests green.
- [ ] Run fresh post-fix case 10 in `memphant-v8-offset9-repair`; require verified result and packet proof.

## Task 2: Complete calibration contracts

**Files:**
- Modify/test: `scripts/verify_memsyco_calibration_packets.py`
- Modify/test: `scripts/audit_memsyco_calibration_overlap.py`
- Modify/test: `scripts/generate_memsyco_valid_selection_calibration.py`
- Test: `tests/test_restraint_benchmark_contract.py`
- Create: `benchmarks/memsyco/valid_selection_calibration/confirmation_v3*.jsonl`

- [ ] Add failing packet-verifier tests for current active structured personalization, absent outdated active personalization, and allowed typed historical conversation evidence.
- [ ] Implement VMS output fields `cases`, `current_preference_role_matches`, `outdated_active_personalization_absent`, and `pass`.
- [ ] Add a failing overlap test where only normalized five-gram overlap is nonzero; require all three overlap counts to be zero.
- [ ] Add confirmation-v3 with six polarity twins: tea, event seating, study session, museum visit, running route, and notification delivery.
- [ ] Require development, confirmation, and confirmation-v2 hashes to remain byte-identical and all four split topics to be disjoint.
- [ ] Generate v3, run official/prior-split overlap audits, and freeze its case/oracle hashes before opening it.

## Task 3: Requalify and seal candidate v8

**Artifacts:** fresh directories below `.../valid-selection/development/` and `.../valid-selection/confirmation-v3/`.

- [ ] Run fresh full development `memphant-v8` and `episode-only-v2` sequentially. Reuse RawDialogue only on exact input/request/model/provider/judge/config/ledger identity; otherwise rerun fresh.
- [ ] Require MemPhant and RawDialogue 12/12, latest-use 1.0, contamination 0.0, complete provenance, and all 12 MemPhant packet proofs. Record episode-only diagnostically.
- [ ] Create `CANDIDATE-FREEZE-v8-confirmation-v3.json` binding source, dirty-tree manifest, prompt, adapter, runner, meter, fixtures/oracles, binaries, Python requirements, model/provider identities, judge schema/parser, BGE-M3, top-k, official lock, and portable `SHA256SUMS`.
- [ ] Open confirmation-v3 once: RawDialogue first; only after 12/12 run MemPhant and episode-only. Any fixture or candidate change retires the entire opened pack and increments the confirmation version.

## Task 4: Add official aggregation only after calibration passes

**Files:**
- Modify/test: `scripts/run_restraint_bench.py`
- Test: `tests/test_restraint_benchmark_contract.py`

- [ ] Add failing `score-official` tests for 350 unique paired UIDs, gaps/duplicates, offset/config/model drift, duplicate response IDs, product versus infrastructure recovery, completed-row recall, deterministic confidence intervals, threshold boundaries, and clean-349 exclusion drift.
- [ ] Add the existing-runner subcommand with `--official-dir`, `--gate`, `--candidate-freeze`, arm roots, and `--out`.
- [ ] Reuse `verify_results` for complete runs. Validate partial infrastructure artifacts and fresh suffix recovery without re-calling completed rows.
- [ ] Use 10,000 paired percentile-bootstrap samples with seed `20260716`; report full-350 first and clean-349 only as sensitivity.
- [ ] Bind source, gate, freeze, run/input/report/proof/ledger/binary hashes, response IDs, models, tokens, costs, and stage latency in `OFFICIAL-SCORECARD.json`.

## Task 5: Execute the untouched official track

- [ ] Immediately before spend, verify benchmark revision `c31e2c85ee8cc3c6f643587b8a6f4b5ad5eb3bf6`, schema 1.2, 350 UIDs, leaderboard snapshot `84125959cb7db8af442783d4b063f39bd4267229`, published points, candidate freeze, and all checksums. Stop on drift.
- [ ] Write `OFFICIAL-GATE.json` with expected UIDs, 14 base slices, thresholds, bootstrap seed/method, `v11_000321` sensitivity exclusion, and permitted claim wording.
- [ ] Run 14 sequential paired slices at offsets 0, 25, ..., 325: MemPhant then RawDialogue, fresh directories, no completion cache.
- [ ] Permit only machine-identifiable infrastructure suffix recovery. Any decode, grounding, model output, judge, parser, provenance, or score failure retires VMS without row-content inspection or retry.
- [ ] Score full-350 and clean-349. Claim only “exceeds published Valid Memory Selection baselines as of the pinned date” or “reproduced task-specific SOTA against the pinned official table” when every gate passes.

## Task 6: Verification and boundary proof

- [ ] Run focused Rust/Python tests after each TDD cycle, then the complete `AGENTS.md` gate before a completion or SOTA claim.
- [ ] Verify portable `SHA256SUMS`, all scratch databases dropped, PostgreSQL restored to its initial stopped state, and all pre-existing dirty/untracked paths outside the allowlist byte-identical.
- [ ] Report unrelated full-gate failures separately; do not repair them opportunistically.
- [ ] If VMS earns or retires, stop this feature. Personalized Memory Use is the next independent feature-flow cycle.

## NOT in scope

- Graph memory, learned admission gates, generic decay, MMR diversity, RL, procedural memory, and trajectory consolidation: no current VMS trace justifies them.
- Formal leaderboard submission or global/full-suite SOTA: this run can support only a pinned task-specific claim.
- Permanent research automation: create a research escalation artifact only when a predeclared stuck trigger fires.

## What already exists

- `decode_response_with_state` and `transform_state` already own normalization and duplicate arbitration; repair them once rather than guarding callers.
- `run_restraint_bench.py` already owns acquisition, execution, and strict verification; add aggregation there only when needed.
- Provider/extractor ledgers, scratch databases, packet proofs, overlap auditing, and VMS development/prior confirmation splits already exist and remain the source of truth.

## Test flow

```text
wire operations
  -> transform_state
       -> generic non-preference ----------------------> retain fail-closed behavior
       -> grounded Create/Replace preference ----------> reserve role fields
       -> Delete --------------------------------------> never synthesize fields
  -> duplicate arbitration
       -> exact duplicate -----------------------------> idempotent
       -> two grounded preferences --------------------> later quote wins
       -> other collision -----------------------------> reject + safe diagnostic
  -> packet verifier ----------------------------------> current active, outdated inactive
  -> development 12/12 -> sealed confirmation 12/12 -> official one-shot
```

Sequential implementation, no parallelization opportunity: every later paid gate depends on the exact bytes and evidence produced by the prior step.

## GSTACK REVIEW REPORT

| Review | Trigger | Why | Runs | Status | Findings |
|--------|---------|-----|------|--------|----------|
| CEO Review | `/plan-ceo-review` | Scope and strategy | 0 | Not needed | Existing task-specific strategy is already locked |
| Codex Review | `/codex review` | Independent second opinion | 1 | FOLDED | Delete guard, overlap gate, immutable packs, and recovery semantics incorporated |
| Eng Review | `/plan-eng-review` | Architecture and tests | 1 | CLEAR | 4 issues reviewed, 0 critical gaps, 0 unresolved |
| Design Review | `/plan-design-review` | UI/UX gaps | 0 | Not applicable | No UI scope |
| DX Review | `/plan-devex-review` | Developer experience gaps | 0 | Not needed | Existing runner and commands are reused |

**CROSS-MODEL:** Both reviews selected the shared decoder repair and existing runner; no architecture tension remains.

**VERDICT:** ENG CLEARED — ready to implement sequentially.

NO UNRESOLVED DECISIONS

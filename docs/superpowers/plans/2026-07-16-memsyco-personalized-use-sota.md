# MemSyco Personalized Memory Use SOTA Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Qualify the current MemPhant candidate on Personalized Memory Use (PMU), then run and score both complete 300-row public benchmark arms without slices.

**Architecture:** Reuse the existing MemSyco runner, provider ledgers, packet proofs, overlap auditor, candidate freeze, and official scorer. Add only a PMU calibration generator, PMU packet assertions, and a task-parameterized official scoring path. Runtime extraction, recall, prompts, models, and schemas remain frozen unless a development trace falsifies the current objective-outcome safeguard.

**Tech Stack:** Python 3.14, pytest, Rust MemPhant binaries, PostgreSQL, OpenRouter-compatible providers, official MemSyco evaluator, deterministic paired bootstrap.

## Global Constraints

- Preserve every pre-existing dirty or untracked path outside the exact campaign allowlist; do not reset, clean, rebase, commit, push, or touch `docs/superpowers/specs/memphant/STATUS.md` or the paused Syndai cutover.
- Run PMU as one fresh 300-row MemPhant base run followed by one fresh 300-row RawDialogue base run. No 25-row slices and no completion cache.
- The public UID `v11_001093` was previously exposed by a promotion-ineligible smoke. Full-300 remains primary; clean-299 excluding only this predeclared UID is sensitivity analysis and cannot rescue a full-300 miss.
- Pin MemSyco source commit `c31e2c85ee8cc3c6f643587b8a6f4b5ad5eb3bf6`, schema 1.2, exactly 300 PMU rows, and the current leaderboard snapshot before spend.
- Beat published PMU Full Dialog values of 60.34% answer accuracy and 79.33% correct preference use. Promotion requires point estimates at least 63.34% and 82.33%, bootstrap lower 95% bounds above 60.34% and 79.33%, and paired MemPhant-minus-RawDialogue lower bounds above zero for both metrics.
- Development and sealed confirmation require MemPhant 12/12 and RawDialogue 12/12 for answer accuracy, preference use, and memory-use pass, with zero parse, judge, packet, ledger, retry, provenance, or hidden failures. Episode-only is diagnostic.
- Accuracy and restraint outrank cost. Measure tokens, cost, and synchronous recall p95; retain the existing p95 <= 1.5 seconds product gate.
- Do not add a dependency, public interface, database schema, second benchmark harness, graph memory, learned gate, decay system, retrieval rewrite, local judge jury, or fallback judge.

## Processing and Evidence Flow

```text
synthetic PMU cases + oracle
          |
          v
overlap audit against all 300 official rows ----fail----> replace unopened family
          |
          v
RawDialogue + MemPhant + episode-only development
          |
          +--> official evaluator: accuracy / preference_use / memory_use_pass
          +--> packet proof: active preference present, rejected outcome absent
          |
          v
candidate freeze --> one-time confirmation --> OFFICIAL-GATE.json
          |
          v
MemPhant full 300 --> RawDialogue full 300 --> deterministic paired scorecard
          |
          +--> full-300 primary
          +--> clean-299 disclosed sensitivity (exclude v11_001093 only)
```

### Task 1: Campaign preflight and immutable PMU calibration bank

**Files:**
- Create: `scripts/generate_memsyco_personalized_use_calibration.py`
- Create: `benchmarks/memsyco/personalized_use_calibration/development.jsonl`
- Create: `benchmarks/memsyco/personalized_use_calibration/development.oracle.jsonl`
- Create: `benchmarks/memsyco/personalized_use_calibration/confirmation.jsonl`
- Create: `benchmarks/memsyco/personalized_use_calibration/confirmation.oracle.jsonl`
- Create: `benchmarks/memsyco/personalized_use_calibration/manifest.json`
- Modify: `tests/test_restraint_benchmark_contract.py`
- Create: `docs/build-log/artifacts/unified-sota-20260714/memsyco-evidence-sota-20260715T172416Z/personalized-use/future-v1/preflight/*`

**Interfaces:**
- Consumes: the official PMU JSONL and `scripts/audit_memsyco_calibration_overlap.py`.
- Produces: two immutable 12-case, six-family, polarity-twin splits plus their SHA-256 manifest.

- [ ] Write the preflight artifact with NUL-safe porcelain v2 status, recursive SHA-256/size/mode inventory, deleted-path inventory, exact allowlist, repository HEAD, and observed PostgreSQL state.
- [ ] Add failing tests requiring 12 cases per split, six polarity twins, both official PMU subtypes, pairwise topic disjointness, stable case/oracle hashes, and objective-outcome distractors.
- [ ] Run `python3 -m pytest tests/test_restraint_benchmark_contract.py -k personalized_use_calibration -q` and confirm the generator contract is absent.
- [ ] Implement the minimal generator. Each case must state one explicit active preference and one positively worded but objectively unsuccessful experience whose candidate value must not be promoted as personalization.
- [ ] Generate both splits, record immutable hashes, and rerun the generator to prove byte stability.
- [ ] Run the existing overlap auditor against all 300 official PMU rows. Any exact hash, suspicious row match, or normalized five-gram overlap replaces the entire unopened family before hashes are frozen.

### Task 2: PMU packet proof contract

**Files:**
- Modify: `scripts/verify_memsyco_calibration_packets.py`
- Modify: `tests/test_restraint_benchmark_contract.py`

**Interfaces:**
- Consumes: PMU oracle fields `current_preference_value` and `rejected_experience_value`, label-free input identity, and archived typed memories.
- Produces: `cases`, `current_preference_role_matches`, `rejected_experience_personalization_absent`, and `pass`.

- [ ] Add failing tests proving the current preference must be an active structured `personalization` item with `epistemic_use=not_factual_evidence`.
- [ ] Add failing tests proving the rejected experience value may remain in typed historical conversation evidence but never in active structured personalization.
- [ ] Add identity-mismatch and missing-proof regressions so no packet can be silently rebound.
- [ ] Run the focused tests and confirm each new assertion fails for the missing PMU branch.
- [ ] Implement the smallest oracle-dispatch extension, reusing label-free identity and structured value traversal.
- [ ] Run focused packet-verifier tests and the existing VMS/scope/evidence packet tests to prove no regression.

### Task 3: Development matrix and candidate diagnosis

**Files:**
- Create: `docs/build-log/artifacts/unified-sota-20260714/memsyco-evidence-sota-20260715T172416Z/personalized-use/future-v1/development/*`
- Create only if stuck: `.../personalized-use/future-v1/research/<UTC>/RESEARCH-ESCALATION.json`

**Interfaces:**
- Consumes: development cases, frozen provider/model configuration, runner, real PostgreSQL, and official PMU evaluator.
- Produces: three complete development reports and 12 packet proofs for MemPhant.

- [ ] Run RawDialogue development first, all 12 cases sequentially. If it misses any metric, retire the fixture pack and replace it rather than tuning MemPhant.
- [ ] Run MemPhant development in a fresh database and directory, all 12 cases, then `verify-results` and the PMU packet verifier.
- [ ] Run episode-only development in a fresh directory as a diagnostic.
- [ ] Require all blocking metrics and provenance gates. Diagnose any failure by trace stage before changing code.
- [ ] Trigger the rolling 90-day research loop only after two focused attempts fail against the same diagnosed product mechanism or a complete quality gate misses; infrastructure incidents do not trigger research.
- [ ] If traces show objective wording is promoted despite an unsuccessful outcome, add a focused red Rust regression and the smallest deterministic promotion guard. Otherwise leave runtime code unchanged.

### Task 4: Task-parameterized full-run official scorer

**Files:**
- Modify: `scripts/run_restraint_bench.py`
- Modify: `tests/test_restraint_benchmark_contract.py`

**Interfaces:**
- Consumes: `OFFICIAL-GATE.json`, candidate freeze, complete/partial arm directories, official source, run manifests, reports, provider ledgers, extractor ledgers, binary hashes, response IDs, and recovery bindings.
- Produces: a hash-bound PMU `OFFICIAL-SCORECARD.json` while retaining byte-compatible VMS behavior.

- [ ] Add failing scorer tests for PMU task/source selection, exactly one base run at offset 0 with sample count 300, UID gaps/duplicates, duplicate response IDs, configuration drift, unbound artifacts, product versus infrastructure recovery, completed-row recall, and clean-exclusion drift.
- [ ] Add deterministic 10,000-resample paired percentile-bootstrap tests with seed `20260716`, exact threshold-boundary tests, and two higher-is-better metrics: `answer_accuracy` and `preference_used`.
- [ ] Run the focused scorer test selection and confirm the current VMS-only scorer rejects PMU.
- [ ] Parameterize the existing loader/collector/scorer by a sealed task specification. Keep VMS constants and output unchanged; PMU accepts a single full base run per arm and reports full-300 plus clean-299.
- [ ] Make the scorer reject judge/parser/product failures, recalls of completed UIDs, partial runs without machine-identifiable infrastructure recovery, provider/model/BGE/top-k drift, and nonunique response IDs.
- [ ] Run all official scorer tests twice and compare scorecard SHA-256 for deterministic output.

### Task 5: Candidate freeze and one-time sealed confirmation

**Files:**
- Create: `.../personalized-use/future-v1/CANDIDATE-FREEZE-v1-confirmation.json`
- Create: `.../personalized-use/future-v1/confirmation/*`

**Interfaces:**
- Consumes: green development artifacts, immutable confirmation hashes, binaries, source and dirty-tree manifests, Python requirements, provider policy, judge contract, and BGE-M3 lock.
- Produces: one frozen candidate and a one-time confirmation disposition.

- [ ] Freeze source, dirty-tree manifest, generator, fixtures, oracles, overlap reports, runner, verifier, provider meter, server/worker/CLI/test binaries, Python version, resolved requirements, requested/served models, provider policy, judge prompt/schema/parser/raw-verdict/no-fallback contract, BGE identity/dimensions/top-k, and portable relative `SHA256SUMS`.
- [ ] Open confirmation exactly once: RawDialogue first; if it misses, retire the entire pack without running MemPhant.
- [ ] If RawDialogue passes, run MemPhant and packet verification, then episode-only, each sequentially and in fresh directories/databases.
- [ ] Any MemPhant code, prompt, adapter, model, or configuration change after opening retires the pack and requires a wholly independent next confirmation version.

### Task 6: Official gate and two complete 300-row arms

**Files:**
- Create: `.../personalized-use/future-v1/official/OFFICIAL-GATE.json`
- Create: `.../personalized-use/future-v1/official/memphant/*`
- Create: `.../personalized-use/future-v1/official/raw-dialogue/*`
- Create: `.../personalized-use/future-v1/official/OFFICIAL-SCORECARD.json`

**Interfaces:**
- Consumes: pinned benchmark and leaderboard, frozen candidate, expected 300 UIDs, thresholds, bootstrap seed/method, and predeclared smoke exclusion.
- Produces: one complete scored result per UID per arm, all paid-call provenance, and a truthful task-specific claim disposition.

- [ ] Immediately before spend, reverify upstream source revision, schema, row count, leaderboard snapshot/frontier values, candidate freeze, and every `SHA256SUMS` entry. Drift stops execution for replanning.
- [ ] Write and hash `OFFICIAL-GATE.json` with exact UIDs, thresholds, bootstrap contract, public smoke disclosure, clean exclusion, permitted recovery, and claim wording.
- [ ] Run MemPhant once with `--offset 0 --limit 300` in a fresh directory and database; verify results and packet/provenance completeness before starting RawDialogue.
- [ ] Run RawDialogue once with `--offset 0 --limit 300` and identical answer/judge/provider configuration; verify results.
- [ ] Permit recovery only after transport/408/429/5xx without parsed result or local process/database failure before row completion. Preserve the base, start a fresh suffix at the first incomplete row, never recall a completed UID, and bind with `RECOVERY.json`. Any refusal, invalid output, decode, grounding, judge/parser, provenance, or quality failure retires the track without retry.
- [ ] Run `score-official`. A clean no-recovery campaign expects 1,500 complete paid calls: 900 MemPhant and 600 RawDialogue; partial infrastructure calls remain in authoritative cost totals.
- [ ] Claim only `reproduced task-specific SOTA against the pinned public table` if every full-300 gate passes, while explicitly disclosing the prior smoke UID. Never claim official rank, accepted leaderboard placement, untouched holdout, global SOTA, or suite-wide SOTA.

### Task 7: Verification, review, and postflight proof

**Files:**
- Create: `.../personalized-use/future-v1/postflight/*`

**Interfaces:**
- Consumes: all campaign artifacts and the preflight inventory.
- Produces: focused/full verification logs and proof that unrelated dirty paths are byte-identical.

- [ ] Run Python compilation, generator determinism, overlap regressions, packet-verifier regressions, all restraint benchmark contract tests, and focused Rust tests only if runtime code changed.
- [ ] Run the complete repository `AGENTS.md` gate before any completion/SOTA claim. Record unrelated pre-existing dirty-worktree failures separately; never repair them opportunistically.
- [ ] Run engineering code review and the Ponytail deletion pass. Remove losing experiments, dead flags, one-use abstractions, and duplicate helpers while retaining provenance and fail-closed validation.
- [ ] Recompute every pre-existing path outside the allowlist and require byte-identical SHA-256, size, and mode. Verify portable relative `SHA256SUMS`, drop scratch databases, and restore PostgreSQL to its observed initial state.

## Failure Modes and Tests

| Code path | Realistic failure | Test | Handling | User-visible result |
|---|---|---|---|---|
| Calibration generator | split or hash drifts after opening | immutable hash/determinism test | generator aborts | explicit failure |
| Overlap audit | synthetic phrase leaks an official five-gram | five-gram-only regression | pack cannot freeze | explicit overlap report |
| Packet verifier | unsuccessful experience becomes personalization | rejected-value regression | verification fails closed | explicit failed count |
| Full arm collector | recovery replays a completed UID | completed-row recall regression | score rejected | explicit exception |
| PMU bootstrap | metric direction or seed changes | golden deterministic score test | gate rejected | explicit score mismatch |
| Paid provider | transport dies after partial usage | recovery contract test | preserved partial + suffix | explicit recovery provenance |
| Judge/parser | parsed result is absent or invalid | hidden/product failure tests | no retry; track retires | `ABORTED.json` |

No code path above has a silent failure without a test and fail-closed handling.

## NOT in Scope

- Graph/trajectory/procedural memory, RL, learned gating, decay, and retrieval changes: PMU traces have not shown a measured need.
- A new public API, database schema, dependency, adapter contract, or benchmark harness: the existing internal runner is the durable seam.
- Official leaderboard submission or global SOTA language: this campaign can only reproduce task-specific performance against a pinned public table.
- Re-running or repairing retired VMS official tracks: exposed rows remain exposed.

## What Already Exists

- `scripts/run_restraint_bench.py` already runs all four memory arms, preserves provider/extractor ledgers, verifies reports, and contains the VMS official scorer; this plan extends it rather than adding a second runner.
- `scripts/audit_memsyco_calibration_overlap.py` already enforces exact, suspicious-row, and five-gram disjointness; this plan reuses it unchanged.
- `scripts/verify_memsyco_calibration_packets.py` already binds label-free input identities and validates structured personalization for VMS; this plan adds a narrow PMU oracle branch.
- The official MemSyco evaluator already owns PMU metric semantics; this plan never reimplements judge policy.

## Implementation Strategy

Sequential implementation, no parallelization opportunity. Calibration, packet proof, development diagnosis, candidate freeze, confirmation, and official execution are intentionally ordered evidence gates and touch the same benchmark subsystem.

## GSTACK REVIEW REPORT

| Review | Trigger | Why | Runs | Status | Findings |
|--------|---------|-----|------|--------|----------|
| CEO Review | `/plan-ceo-review` | Scope & strategy | 0 | NOT RUN | Existing handoff already fixes the task boundary |
| Codex Review | `/codex review` | Independent 2nd opinion | 0 | SKIPPED | Subagent review not authorized for this task |
| Eng Review | `/plan-eng-review` | Architecture & tests (required) | 1 | CLEAR | Reuses the existing harness; fail-closed tests cover every new path |
| Design Review | `/plan-design-review` | UI/UX gaps | 0 | NOT APPLICABLE | No UI work |
| DX Review | `/plan-devex-review` | Developer experience gaps | 0 | NOT APPLICABLE | Internal benchmark-only interface |

**VERDICT:** ENG CLEARED — ready to implement

NO UNRESOLVED DECISIONS

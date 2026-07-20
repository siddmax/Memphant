# MemSyco PMU v2 Unscoped Qualification Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Generate and run independent v2 development and confirmation packs that stress unscoped explicit-preference arbitration.

**Architecture:** Reuse the current generator, overlap auditor, packet verifier, and benchmark runner. Add two immutable split definitions and a single `scoped` input to the existing case builder; do not add a second generator or harness.

**Tech Stack:** Python 3.10, Rust 2024, pytest, cargo, pinned MemSyco evaluator, PostgreSQL, OpenRouter.

## Global Constraints

- Preserve the retired v1 official tree byte-for-byte and never rerun it.
- Preserve existing calibration hashes exactly.
- Run every arm sequentially with one worker and no completion cache.
- Do not commit, reset, clean, rebase, push, touch `STATUS.md`, or touch the paused cutover.

---

### Task 1: Generate immutable v2 splits

**Files:**
- Modify: `scripts/generate_memsyco_personalized_use_calibration.py`
- Modify: `tests/test_restraint_benchmark_contract.py`
- Create: `benchmarks/memsyco/personalized_use_calibration/development_v2.jsonl`
- Create: `benchmarks/memsyco/personalized_use_calibration/development_v2.oracle.jsonl`
- Create: `benchmarks/memsyco/personalized_use_calibration/confirmation_v2.jsonl`
- Create: `benchmarks/memsyco/personalized_use_calibration/confirmation_v2.oracle.jsonl`

**Interfaces:**
- Consumes: existing `build_case` and immutable v1 hashes.
- Produces: two 12-case unscoped polarity-twin splits and manifest hashes.

- [ ] Add a failing contract test requiring all four splits to be pairwise topic-disjoint, v1 hashes unchanged, and every v2 first user turn to start with `I prefer `.
- [ ] Run the focused test and require failure because v2 files are absent.
- [ ] Add `development_v2` and `confirmation_v2` family tables and a boolean `scoped` argument to `build_case`; emit `I prefer <value>.` only for v2.
- [ ] Run the generator twice and require identical combined SHA-256 output.
- [ ] Freeze the new case and oracle hashes in `IMMUTABLE_SPLIT_HASHES`.
- [ ] Run the complete calibration contract tests.

### Task 2: Prove official non-overlap

**Files:**
- Create: `docs/build-log/artifacts/unified-sota-20260714/memsyco-evidence-sota-20260715T172416Z/personalized-use/future-v2/CALIBRATION-OVERLAP.json`

**Interfaces:**
- Consumes: official PMU JSONL and both v2 case files.
- Produces: a passing zero-count overlap report.

- [ ] Audit both v2 splits together against all 300 official rows.
- [ ] Require exact normalized hashes, suspicious matches, and normalized five-gram overlaps all equal zero.

### Task 3: Run complete v2 development

**Files:**
- Create: `.../personalized-use/future-v2/development/raw-dialogue/*`
- Create: `.../personalized-use/future-v2/development/memphant/*`
- Create: `.../personalized-use/future-v2/development/episode-only/*`
- Create: `.../personalized-use/future-v2/development/DEVELOPMENT-QUALIFICATION-v2.json`

**Interfaces:**
- Consumes: `development_v2.jsonl`, repaired binaries, frozen model/provider configuration.
- Produces: full 12-case reports and 12 MemPhant packet proofs.

- [ ] Run RawDialogue with offset 0 and limit 12; require 12/12 accuracy and preference use.
- [ ] Run MemPhant with offset 0 and limit 12; require 12/12, zero extractor rejection, `verify-results`, and 12/12 packet proof.
- [ ] Run episode-only with offset 0 and limit 12 as diagnostic.
- [ ] Hash-bind the three reports and ledgers in the qualification artifact.

### Task 4: Freeze and open confirmation once

**Files:**
- Create: `.../personalized-use/future-v2/CANDIDATE-FREEZE-v2-confirmation-v2.json`
- Create: `.../personalized-use/future-v2/confirmation-v2/*`
- Create: `.../personalized-use/future-v2/SHA256SUMS`

**Interfaces:**
- Consumes: green development, implementation/binary hashes, v2 fixture hashes, provider and judge contracts.
- Produces: immutable candidate and confirmation disposition.

- [ ] Freeze the candidate and portable hashes before opening confirmation.
- [ ] Run RawDialogue first; retire the pack on any miss.
- [ ] If RawDialogue passes, run MemPhant and require 12/12 plus all packet proofs.
- [ ] Run episode-only diagnostically and seal confirmation qualification.

### Task 5: Verify and stop at the truthful boundary

**Files:**
- Create: `.../personalized-use/future-v2/POSTFLIGHT.json`

**Interfaces:**
- Consumes: v2 campaign artifacts and original dirty-tree manifest.
- Produces: verified future candidate without an official claim.

- [ ] Run Python compilation, full benchmark contracts, Rust structured-state tests, and formatting.
- [ ] Verify all portable SHA-256 entries and all 2,237 protected dirty paths.
- [ ] Restore PostgreSQL to its observed running/healthy state.
- [ ] Do not run or claim official PMU SOTA until a new independent benchmark version or holdout exists.


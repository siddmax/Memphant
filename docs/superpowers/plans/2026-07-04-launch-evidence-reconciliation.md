# Launch Evidence Reconciliation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make MemPhant launch evidence, STATUS, and mirrored Syndai proof refs internally consistent and fail closed.

**Architecture:** Keep the restraint fix in the shared recall filter. Keep launch evidence drift checks in Python tests that read the checked-in scorecards, traces, build logs, and STATUS ledger.

**Tech Stack:** Rust core/eval crates, Python stdlib JSON/pathlib, pytest.

---

### Task 1: Restraint Root Cause

**Files:**
- Modify: `crates/memphant-core/src/lib.rs`
- Modify: `crates/memphant-core/tests/recall_trace_golden.rs`

- [ ] Replace the benchmark phrase inventory in `high_risk_action_query` with compact policy categories and token checks.
- [ ] Add/keep tests proving high-risk requests drop private profile context, including the GPS tracker recurrence, while benign profile recall still works.
- [ ] Run `cargo test -p memphant-core high_risk_action_query_drops_private_profile_context --test recall_trace_golden`.

### Task 2: Evidence Drift Contract

**Files:**
- Create: `tests/test_launch_evidence_contract.py`
- Modify: `docs/build-log/2026-07-03-public-launch-gate.md`
- Modify: `docs/build-log/2026-07-03-restraint-launch-gate.md`
- Modify: `docs/superpowers/specs/memphant/STATUS.md`

- [ ] Add a pytest contract that derives pass/fail from checked-in scorecards and trace metrics, then asserts STATUS and build-log wording cannot claim complete while scorecards fail.
- [ ] Update launch build-log narratives to point at `real-launch-evidence-20260704-v1` artifacts and current pass/fail status.
- [ ] Clarify the DONE definition: §5 DORMANT rows are terminal when their activation gate is unmet and recorded; activated rows still require built proof or retirement.
- [ ] Run `python3 -m pytest tests/test_launch_evidence_contract.py tests/test_public_launch_gate.py tests/test_restraint_launch_gate.py -q`.

### Task 3: Cross-Repo Finalization

**Files:**
- Modify: `.codex/linked-repos.json`
- Modify mirrored MemPhant specs under `/Users/sidsharma/Syndai/.wt/codex-memphant-cross-repo`

- [ ] Commit Memphant at a final SHA.
- [ ] Update all `Memphant@...` proof refs in both repos to that final SHA.
- [ ] Run `python3 scripts/check_spec_drift.py`.
- [ ] Run Syndai preflight/make check from the linked worktree and fix root causes.
- [ ] Push Syndai, then push Memphant.

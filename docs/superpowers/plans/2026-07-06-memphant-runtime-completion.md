# MemPhant Runtime Completion Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make MemPhant's checked-in status, public API snapshot, Python package metadata, and Syndai dogfood guardrails match the runtime that actually exists today.

**Architecture:** This pass deliberately does not invent a second storage stack or compatibility layer. It first locks the repo against false completion claims and ghost public surfaces, then leaves the Postgres runtime as the next real implementation target with failing-proof tests/docs that cannot be mistaken for done.

**Tech Stack:** Rust/Axum/OpenAPI JSON, Python stdlib SDK packaging, pytest repo-contract tests, Syndai backend pytest guardrails.

---

### Task 1: Runtime Honesty Contracts

**Files:**
- Modify: `tests/test_repo_contract.py`
- Modify: `docs/superpowers/specs/memphant/STATUS.md`
- Create: `docs/build-log/2026-07-06-runtime-completion-gap-audit.md`

- [x] Add a failing pytest that rejects `CURRENT PHASE: COMPLETE` while packaged binaries still use `InMemoryStore`, `memphant-worker` is a stub, or `memphant-store-postgres` is lint-only.
- [x] Update `STATUS.md` to `RUNTIME INCOMPLETE` and mark WS-D/WS-H/public launch/standing SLO as runtime-incomplete instead of checked complete.
- [x] Add the build-log audit with exact verified gaps and next proof required.
- [x] Run `python3 -m pytest tests/test_repo_contract.py -q`.

### Task 2: Public Surface Honesty

**Files:**
- Modify: `crates/memphant-server/src/lib.rs`
- Modify: `crates/memphant-server/tests/rest_contract.rs`
- Regenerate: `openapi/memphant.v1.json`

- [x] Add a failing Rust test that OpenAPI paths exactly match public contract operations and no GET operation has `requestBody`.
- [x] Remove unserved `/v1/memory`, `/v1/scopes/{id}/stats`, and `/v1/scopes/{id}/block` from the OpenAPI document.
- [x] Make GET path items emit only responses and parameter metadata, not JSON request bodies.
- [x] Run `cargo test -p memphant-server --test rest_contract`.

### Task 3: Python SDK Packaging Honesty

**Files:**
- Modify: `bindings/python/pyproject.toml`
- Modify: `tests/test_wsd_public_surfaces.py`
- Modify: `docs/superpowers/specs/memphant/03-engineering-spec.md`
- Modify: `docs/superpowers/specs/memphant/26-decision-register.md`

- [x] Add a failing pytest that the Python package is pure HTTP SDK metadata and does not advertise `memphant._native`.
- [x] Switch `pyproject.toml` to a pure Python build backend.
- [x] Update specs to say PyO3/maturin native bindings are deferred until a real embedded/local API exists.
- [x] Run `python3 -m pytest tests/test_wsd_public_surfaces.py -q`.

### Task 4: Syndai Dogfood Guardrails

**Files:**
- Fold MemPhant dogfood boundary bullets into `/Users/sidsharma/Syndai/backend/AGENTS.md`
- Modify: `/Users/sidsharma/Syndai/backend/TESTS.md`
- Modify: `/Users/sidsharma/Syndai/backend/tests/unit/features/memory/test_memphant_dogfood_adapter.py`
- Create: `/Users/sidsharma/Syndai/backend/tests/scripts/test_memphant_boundary.py`

- [x] Add an L0 negative test proving current L0 context loading does not call MemPhant active-read.
- [x] Add a static scan preventing web/mobile/app code from directly importing MemPhant DB/Supabase surfaces.
- [x] Add concise scoped guidance and one backend test command.
- [x] Run the focused Syndai tests for the adapter, config bounds, and boundary scan.

### Task 5: Drift And Focused Gates

**Files:**
- Existing files only.

- [x] Run `python3 scripts/check_spec_drift.py`.
- [x] Run the focused MemPhant pytest/Rust tests touched above.
- [x] Run the focused Syndai tests touched above.
- [x] Record any remaining runtime implementation gaps in the final response without claiming full runtime completion.

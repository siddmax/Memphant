# MemPhant WS-F Syndai Dogfood Cutover Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans. This plan is cross-repo: MemPhant owns public trace-compare behavior; Syndai owns the adapter and focused memory tests.

**Goal:** Complete the first low-risk WS-F dogfood slice by exporting Syndai agent-scoped file memory to MemPhant, trace-comparing it through the public eval runner, and proving the Syndai adapter can active-read/correct/forget through public MemPhant contracts without direct DB coupling.

**First surface:** L1+ agent-scoped file memory. It is low-risk because it is already behind `MemoryContextLoader`, excludes L0-only user facts/persona/episodic/behavioral memory, is query-ranked, and has existing focused tests.

---

## Scope Notes

- `29` WS-F stop-rule says the first low-risk surface may stall without destabilizing Syndai; mismatches become goldens instead of re-baselining.
- `07` §3 requires export, trace compare, one low-risk active-read surface, then correction/forget flows.
- `28` requires no Syndai-only MemPhant API fields, no direct MemPhant DB access, trace IDs recorded, and executable fixtures.
- This slice does not migrate L0 user facts, persona, episodic, behavioral, or project/resource-wide memory.

## Architecture

- MemPhant adds a public `memphant-eval syndai-trace-compare <fixture>` command for the file-memory surface. It maps exported file rows into neutral resource memory units and asserts answer-bearing recall, citation, budget, and forbidden-ID behavior.
- Syndai adds a small adapter module that:
  - exports `MemoryContext.file_memory` into the trace-compare fixture shape;
  - maps public MemPhant recall responses back into Syndai file-memory context rows;
  - active-reads L1+ agent file memory through public `/v1/recall` when `MEMPHANT_FILE_MEMORY_DOGFOOD_ENABLED=true`;
  - builds public `correct` and `forget` payloads for server-owned file-memory mutation flows;
  - carries `trace_id` without querying MemPhant tables.
- The adapter tests use synthetic contexts/fake public responses. No live MemPhant service or private SDK import is required for the local gate; deployment only changes config.

## Tasks

### Task 1: MemPhant Trace-Compare Runner

**Files:**
- Modify: `crates/memphant-eval/src/lib.rs`
- Modify: `crates/memphant-eval/src/main.rs`
- Test: `crates/memphant-eval/tests/syndai_trace_compare.rs`
- Fixture: `examples/syndai/file-memory-trace-compare.yaml`

- [x] Write failing tests for the Syndai file-memory trace-compare fixture.
- [x] Implement the runner and CLI command.
- [x] Verify `cargo test -p memphant-eval --test syndai_trace_compare` and `cargo run -p memphant-eval -- syndai-trace-compare examples/syndai/file-memory-trace-compare.yaml --archive-traces`.

### Task 2: Syndai Public Adapter Contract

**Files:**
- Create: `/Users/sidsharma/Syndai/.wt/codex-memphant-cross-repo/backend/src/features/memory/memphant_dogfood_adapter.py`
- Create: `/Users/sidsharma/Syndai/.wt/codex-memphant-cross-repo/backend/tests/unit/features/memory/test_memphant_dogfood_adapter.py`
- Modify: `/Users/sidsharma/Syndai/.wt/codex-memphant-cross-repo/backend/src/features/memory/context_loader.py`
- Modify: `/Users/sidsharma/Syndai/.wt/codex-memphant-cross-repo/backend/src/config.py`
- Modify: `/Users/sidsharma/Syndai/.wt/codex-memphant-cross-repo/backend/tests/features/config/test_config_bounds.py`

- [x] Write failing adapter tests for export shape, public recall mapping, trace ID capture, correction payload, and forget payload.
- [x] Implement typed `msgspec.Struct` payloads, public HTTP client helpers, config-gated L1+ active-read, and correction/forget payload functions.
- [x] Verify focused Syndai memory tests.

### Task 3: Proof and Status

**Files:**
- Create: `docs/build-log/2026-07-03-wsf-progress.md`
- Modify: `docs/superpowers/specs/memphant/STATUS.md`
- Mirror: `/Users/sidsharma/Syndai/.wt/codex-memphant-cross-repo/docs/superpowers/specs/memphant/STATUS.md`

- [x] Run MemPhant and Syndai gates.
- [x] Record trace-compare archive and focused test proof.
- [x] Mark WS-F complete only if the trace compare and adapter contract gates pass.

# MemPhant WS-D Public Surfaces Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Complete WS-D's public-surface exit packet: REST, MCP schema metadata, Python SDK examples, and `memphant verify` use one canonical JSON contract and round-trip through the current core.

**Architecture:** Add schema-bearing DTOs in `memphant-types`, keep behavior in `memphant-core`, and make REST/MCP/CLI/Python thin wrappers over those types. The first WS-D runtime uses the existing in-memory store for local round-trips; Postgres remains the WS-A store seam and is not bypassed by a second persistence model.

**Tech Stack:** Rust 2024, Axum 0.8, Schemars JSON Schema, serde/serde_json, Clap-free minimal CLI parser, Python stdlib `urllib.request`, pytest.

---

## Scope Notes

- `STATUS.md` names **REST/MCP/Python SDK round-trip; schemas validate; `memphant verify` works** as the WS-D proof line.
- `26-decision-register.md` says v1 build scope is **REST/MCP/Python SDK** while TypeScript SDK remains activation-gated in `STATUS.md` §5. WS-D will not implement TypeScript in this slice; if a later owner-doc cleanup is needed, the register already wins.
- `correct`, `forget`, and `mark` must mutate or record in-memory state. They may be minimal, but not fake success responses.

## File Structure

- Modify `Cargo.toml`: add workspace dependencies for `schemars`, `axum`, `tower`, `http-body-util`, `tokio` runtime features, and `serde_json` where needed.
- Modify `crates/memphant-types/src/lib.rs`: derive `JsonSchema` and add canonical DTOs/constants for REST, MCP, SDK, OpenAPI, and verify reports.
- Modify `crates/memphant-core/src/lib.rs`: add trace lookup, memory listing, correct, forget, and mark-recording helpers on `InMemoryStore`.
- Create `crates/memphant-server/src/lib.rs`: Axum router, shared `AppState`, handlers, error mapping, OpenAPI document builder.
- Modify `crates/memphant-server/src/main.rs`: start the Axum server when invoked normally.
- Create `crates/memphant-server/tests/rest_contract.rs`: REST round-trip and schema assertions.
- Modify `crates/memphant-mcp/src/main.rs`: expose schema metadata mode and future stdio/http entrypoint names.
- Create `crates/memphant-mcp/tests/mcp_schema_contract.rs`: assert every tool has input and output schema plus annotations.
- Modify `crates/memphant-cli/src/main.rs`: add `verify`, `lock`, `retain`, `recall`, and `trace` commands while preserving `db lint`.
- Create `crates/memphant-cli/tests/verify_contract.rs`: assert `memphant verify` succeeds on `memphant.lock` and fails on drift.
- Create `bindings/python/pyproject.toml`, `bindings/python/memphant/__init__.py`, and `bindings/python/examples/roundtrip.py`: stdlib HTTP SDK with one method per v1 verb.
- Create `tests/test_wsd_public_surfaces.py`: Python SDK smoke test against an in-process fake HTTP server and repo artifact checks.
- Create `openapi/memphant.v1.json` and `mcp/memphant.tools.v1.json`: generated/checked snapshots for the public schemas.
- Create `docs/build-log/2026-07-03-wsd-progress.md`: proof ledger entry.

### Task 1: Canonical DTOs and Store Mutations

**Files:**
- Modify: `Cargo.toml`
- Modify: `crates/memphant-types/Cargo.toml`
- Modify: `crates/memphant-types/src/lib.rs`
- Modify: `crates/memphant-core/src/lib.rs`
- Test: `crates/memphant-core/tests/surface_mutations.rs`

- [x] **Step 1: Write failing mutation tests**

Create tests that seed one active semantic unit, call `correct_memory`, assert the old unit is superseded and the new unit is active, call `forget_memory`, assert recall hides forgotten units, and call `record_mark`, assert the mark event is stored.

Run:

```bash
cargo test -p memphant-core --test surface_mutations
```

Expected: fail because the helper functions and DTOs do not exist.

- [x] **Step 2: Implement minimal DTOs and store helpers**

Add schema-capable request/response types for the seven verbs, stable version constants matching `memphant.lock`, `VerifyReport`, `OpenApiDocument`, and `McpToolSpec`. Implement store helpers that mutate the in-memory rows directly under the existing store mutex.

- [x] **Step 3: Verify green**

Run:

```bash
cargo test -p memphant-core --test surface_mutations
```

Expected: pass.

### Task 2: REST Public Surface

**Files:**
- Modify: `crates/memphant-server/Cargo.toml`
- Create: `crates/memphant-server/src/lib.rs`
- Modify: `crates/memphant-server/src/main.rs`
- Test: `crates/memphant-server/tests/rest_contract.rs`

- [x] **Step 1: Write failing REST contract tests**

Assert:
- `GET /v1/health` returns version fields.
- `POST /v1/episodes` then `POST /v1/reflect` then `POST /v1/recall` returns a cited item.
- `GET /v1/traces/{id}` returns the trace from recall.
- `POST /v1/correct`, `POST /v1/forget`, and `POST /v1/mark` return schema-stable JSON.
- `openapi_document()` includes all WS-D paths and component schemas.

Run:

```bash
cargo test -p memphant-server --test rest_contract
```

Expected: fail because the router does not exist.

- [x] **Step 2: Implement Axum router and handlers**

Use `State<Arc<AppState>>`, JSON extractors, and public error envelopes. Handlers call the existing core functions and store helpers; no handler constructs memory results independently.

- [x] **Step 3: Verify green**

Run:

```bash
cargo test -p memphant-server --test rest_contract
```

Expected: pass.

### Task 3: MCP Tool Schema Metadata

**Files:**
- Modify: `crates/memphant-mcp/Cargo.toml`
- Modify: `crates/memphant-mcp/src/main.rs`
- Test: `crates/memphant-mcp/tests/mcp_schema_contract.rs`
- Snapshot: `mcp/memphant.tools.v1.json`

- [x] **Step 1: Write failing MCP schema tests**

Assert the seven tools are present, each has `inputSchema`, `outputSchema`, `structuredContent`-compatible output type metadata, and annotations matching `08` §5.3.

Run:

```bash
cargo test -p memphant-mcp --test mcp_schema_contract
```

Expected: fail because the MCP metadata only prints the WS-0 placeholder.

- [x] **Step 2: Implement metadata and snapshot command**

Generate schemas from the same DTOs via Schemars. Keep stdio/Streamable HTTP behavior as named launch modes with schema metadata now; WS-D proof validates the schema contract before deeper hosted transport work.

- [x] **Step 3: Verify green**

Run:

```bash
cargo test -p memphant-mcp --test mcp_schema_contract
```

Expected: pass.

### Task 4: CLI Verify and Local Round-Trip

**Files:**
- Modify: `crates/memphant-cli/Cargo.toml`
- Modify: `crates/memphant-cli/src/main.rs`
- Test: `crates/memphant-cli/tests/verify_contract.rs`
- Snapshot: `openapi/memphant.v1.json`

- [x] **Step 1: Write failing CLI tests**

Assert `memphant verify --lock memphant.lock` exits 0, a drifted lock exits 1 and prints the mismatched key, and `memphant lock --out -` emits the current lock JSON.

Run:

```bash
cargo test -p memphant-cli --test verify_contract
```

Expected: fail because only `db lint` exists.

- [x] **Step 2: Implement verify/lock and preserve db lint**

Compare `engine_version`, `compiler_version`, `trace_schema_version`, `schema_compat_revision`, `methodology_version`, and `export_schema_version`. JSON output stays stable; human output may be terse.

- [x] **Step 3: Verify green**

Run:

```bash
cargo test -p memphant-cli --test verify_contract
```

Expected: pass.

### Task 5: Python SDK and Example

**Files:**
- Create: `bindings/python/pyproject.toml`
- Create: `bindings/python/memphant/__init__.py`
- Create: `bindings/python/examples/roundtrip.py`
- Test: `tests/test_wsd_public_surfaces.py`

- [x] **Step 1: Write failing Python SDK tests**

Use a stdlib `http.server` fake to assert `MemPhant.retain`, `reflect`, `recall`, `trace`, `correct`, `forget`, and `mark` send the expected method/path/body and convert error envelopes into typed exceptions.

Run:

```bash
python3 -m pytest tests/test_wsd_public_surfaces.py
```

Expected: fail because `bindings/python` does not exist.

- [x] **Step 2: Implement the thin HTTP client**

Use only stdlib `urllib.request` and one method per verb. Do not add client-side query builders, caches, or hidden native requirements.

- [x] **Step 3: Verify green**

Run:

```bash
python3 -m pytest tests/test_wsd_public_surfaces.py
```

Expected: pass.

### Task 6: Docs, STATUS, and Gates

**Files:**
- Modify: `docs/superpowers/specs/memphant/STATUS.md`
- Modify: `/Users/sidsharma/Syndai/.wt/codex-memphant-cross-repo/docs/superpowers/specs/memphant/STATUS.md`
- Create: `docs/build-log/2026-07-03-wsd-progress.md`
- Modify: `docs/superpowers/plans/2026-07-03-memphant-wsd.md`

- [x] **Step 1: Run local gates**

Run:

```bash
cargo fmt --check
cargo test -p memphant-core --test surface_mutations
cargo test -p memphant-server --test rest_contract
cargo test -p memphant-mcp --test mcp_schema_contract
cargo test -p memphant-cli --test verify_contract
python3 -m pytest tests/test_wsd_public_surfaces.py
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
cargo test --doc
python3 scripts/check_spec_drift.py
```

Expected: all pass.

- [x] **Step 2: Record proof and flip WS-D**

Add the build-log entry with exact command outputs, check WS-D in both STATUS ledgers, and leave current phase as WS-E ready.

- [x] **Step 3: Final verification**

Run:

```bash
git diff --check
python3 scripts/check_spec_drift.py
```

Expected: no whitespace errors and clean spec drift.

## Self-Review

- Spec coverage: tasks cover WS-D exit packet, `08` seven-verb REST/MCP/SDK/CLI contract, `memphant verify`, JSON schema snapshots, and the STATUS proof protocol.
- Placeholder scan: no implementation step asks for deferred behavior without naming the file, test, and command.
- Type consistency: `RetainEpisodeHttpRequest`, `RecallHttpRequest`, `CorrectRequest`, `ForgetRequest`, `MarkRequest`, `VerifyReport`, and `McpToolSpec` are the canonical names used across tasks.

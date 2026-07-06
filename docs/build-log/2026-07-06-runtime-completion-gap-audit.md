# 2026-07-06 Runtime Completion Gap Audit

Status: `runtime_incomplete`

This audit reopens the live ledger after checking the runnable implementation against the MemPhant specs.

## Verified Runtime Gaps

- `crates/memphant-server/src/main.rs` starts `AppState::new_in_memory()`, so the packaged REST server does not use Postgres even when Compose provides `DATABASE_URL`.
- `crates/memphant-mcp/src/main.rs` starts `McpRuntime::new_in_memory()`, so MCP tools do not share durable storage with REST.
- `crates/memphant-worker/src/main.rs` only prints `memphant-worker ws0`; it does not claim or run `memphant.job_state` work.
- `crates/memphant-store-postgres/` is provider/migration lint only and has no SQLx-backed runtime store.
- The prior OpenAPI document advertised unserved `/v1/memory`, `/v1/scopes/{id}/stats`, and `/v1/scopes/{id}/block` paths and described GET routes with JSON request bodies.
- `bindings/python/pyproject.toml` advertised a maturin/PyO3 native module even though the package is currently a pure HTTP SDK.

## Changes Made In This Audit

- `STATUS.md` now uses `CURRENT PHASE: RUNTIME INCOMPLETE`.
- WS-D, WS-H, public launch, and hot-path SLO ledger rows are unchecked until Postgres-backed REST/MCP/CLI/worker proof exists.
- OpenAPI was narrowed to the public contract operations and GET operations no longer publish request bodies.
- Python packaging metadata now describes the pure HTTP SDK only.
- Syndai dogfood guardrails remain backend-only and default-off for L1+ file memory.

## Proof Required To Recheck

- REST, MCP, CLI, and worker must all share a Postgres-backed MemPhant store.
- The worker must claim due `job_state` rows transactionally and persist reflect results/traces durably.
- A Compose smoke must retain, reflect, recall, restart the server, and recall the same memory again.
- Tenant isolation and forget read-back tests must pass against the Postgres-backed runtime, not only in-memory core tests.
- Hot-path SLO proof must measure the packaged API path or an explicitly equivalent runtime path.

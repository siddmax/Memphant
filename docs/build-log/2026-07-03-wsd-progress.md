# 2026-07-03 WS-D Public Surfaces Progress

## Scope

WS-D public-surface exit packet:

- REST round-trip over `retain`, `reflect`, `recall`, `trace`, `correct`, `forget`, and `mark`.
- MCP tool schemas and JSON-RPC tool round-trip over the same canonical DTOs.
- Python SDK round-trip and typed error mapping.
- `memphant verify` lock-drift gate.
- OpenAPI and MCP schema snapshots.

TypeScript remains activation-gated per `STATUS.md` §5 and `26-decision-register.md` v1 cut line; this WS-D proof covers the required v1 Python SDK surface.

## Artifacts

- OpenAPI snapshot: `openapi/memphant.v1.json`
- MCP tool snapshot: `mcp/memphant.tools.v1.json`
- REST contract tests: `crates/memphant-server/tests/rest_contract.rs`
- MCP schema/runtime tests: `crates/memphant-mcp/tests/mcp_schema_contract.rs`
- CLI verify tests: `crates/memphant-cli/tests/verify_contract.rs`
- Core mutation tests: `crates/memphant-core/tests/surface_mutations.rs`
- Python SDK/tests: `bindings/python/`, `tests/test_wsd_public_surfaces.py`

## Implementation Notes

- `memphant-types` owns JSON-schema-capable DTOs and lock/verify constants.
- `memphant-server` exposes an Axum `/v1` router and `--openapi-json`.
- `memphant-mcp` exposes schema metadata, stdio JSON-RPC, and Streamable HTTP `/mcp` JSON-RPC over the same in-memory tool runtime.
- `memphant-cli` preserves `db lint` and adds `lock --out` plus `verify --lock`.
- `correct`, `forget`, and `mark` mutate/record in-memory state instead of returning placeholder success.

## Verification

Focused gates:

```text
cargo fmt --check
PASS

cargo test -p memphant-core --test surface_mutations
PASS: 3 passed

cargo test -p memphant-server --test rest_contract
PASS: 2 passed

cargo test -p memphant-mcp --test mcp_schema_contract
PASS: 2 passed

cargo test -p memphant-cli --test verify_contract
PASS: 3 passed

python3 -m pytest tests/test_wsd_public_surfaces.py
PASS: 3 passed
```

Repo gates:

```text
python3 scripts/check_spec_drift.py
PASS: spec_drift=clean public=/Users/sidsharma/Memphant/docs/superpowers/specs/memphant private=/Users/sidsharma/Syndai/.wt/codex-memphant-cross-repo/docs/superpowers/specs/memphant

python3 -m pytest tests
PASS: 19 passed

cargo clippy --all-targets --all-features -- -D warnings
PASS

cargo test --all-targets --all-features
PASS

cargo test --doc
PASS

cargo run -q -p memphant-cli -- verify --lock memphant.lock
PASS: verify=clean path=memphant.lock

cargo run -q -p memphant-server -- --openapi-json > openapi/memphant.v1.json
python3 -m json.tool openapi/memphant.v1.json >/dev/null
PASS

cargo run -q -p memphant-mcp -- --list-tools-json > mcp/memphant.tools.v1.json
python3 -m json.tool mcp/memphant.tools.v1.json >/dev/null
PASS

printf '%s' '{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}' | cargo run -q -p memphant-mcp -- stdio
PASS: JSON-RPC response included the seven MemPhant tools.
```

## Status

WS-D exit packet is complete. Next workstream: WS-E eval, security, and ops.

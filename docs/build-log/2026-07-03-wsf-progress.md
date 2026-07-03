# 2026-07-03 WS-F Syndai Dogfood Cutover Progress

## Scope

WS-F first-surface exit packet:

- Export one low-risk Syndai memory surface: L1+ agent-scoped file memory.
- Trace-compare the exported surface through MemPhant's public eval runner.
- Add a Syndai adapter that uses only public MemPhant contracts and no MemPhant DB coupling.
- Add a config-gated active-read path for L1+ agent file memory through public `/v1/recall`.
- Cover correction and forget payloads through public `/v1/correct` and `/v1/forget` shapes.

## Artifacts

- WS-F plan: `docs/superpowers/plans/2026-07-03-memphant-wsf.md`
- Trace-compare fixture: `examples/syndai/file-memory-trace-compare.yaml`
- Trace archive: `docs/build-log/artifacts/syndai_agent_file_memory_001-trace-compare.json`
- MemPhant test: `crates/memphant-eval/tests/syndai_trace_compare.rs`
- MemPhant runner: `memphant-eval syndai-trace-compare`
- Syndai adapter: `/Users/sidsharma/Syndai/.wt/codex-memphant-cross-repo/backend/src/features/memory/memphant_dogfood_adapter.py`
- Syndai active-read switch: `/Users/sidsharma/Syndai/.wt/codex-memphant-cross-repo/backend/src/features/memory/context_loader.py`
- Syndai config aliases: `MEMPHANT_FILE_MEMORY_DOGFOOD_ENABLED`, `MEMPHANT_API_BASE_URL`, `MEMPHANT_API_KEY`, `MEMPHANT_REQUEST_TIMEOUT_SECONDS`
- Syndai tests: `/Users/sidsharma/Syndai/.wt/codex-memphant-cross-repo/backend/tests/unit/features/memory/test_memphant_dogfood_adapter.py`

## Implementation Notes

- The MemPhant trace-compare runner accepts a neutral Syndai fixture, seeds only `scope_kind: agent` file rows as `resource` memory, and asserts answer-bearing recall plus forbidden project-scope exclusion.
- The Syndai adapter builds public request payloads for `/v1/recall`, `/v1/correct`, and `/v1/forget`; it maps public recall responses back to `MemoryContext.file_memory` rows and preserves `memphant_trace_id`.
- `MemoryContextLoader` active-reads L1+ agent-scoped file memory from MemPhant only when `MEMPHANT_FILE_MEMORY_DOGFOOD_ENABLED=true` and `MEMPHANT_API_BASE_URL` is configured. The default path remains the existing local loader.
- No Syndai web/mobile client talks to MemPhant DB or MemPhant REST directly; this slice changes only the backend memory chokepoint and its tests.

## Verification

Focused MemPhant gates:

```text
cargo test -p memphant-eval --test syndai_trace_compare
PASS: 1 passed

cargo run -p memphant-eval -- syndai-trace-compare examples/syndai/file-memory-trace-compare.yaml --archive-traces
PASS: syndai_trace_compare=pass id=syndai_agent_file_memory_001 surface=agent_file_memory recall=1 archive=docs/build-log/artifacts/syndai_agent_file_memory_001-trace-compare.json
```

Focused Syndai gates:

```text
uv run pytest --no-cov tests/unit/features/memory/test_memphant_dogfood_adapter.py tests/features/config/test_config_bounds.py::TestConfigBounds::test_memphant_dogfood_env_values_parse tests/features/config/test_config_bounds.py::TestConfigBounds::test_memphant_timeout_bounds tests/unit/features/memory/test_memory_contracts.py tests/features/memory/test_context_loader.py tests/unit/features/memory/test_context_loader_observability.py -q
PASS: 60 passed

uv run ruff format --check src/config.py src/features/memory/context_loader.py src/features/memory/memphant_dogfood_adapter.py tests/features/config/test_config_bounds.py tests/unit/features/memory/test_memphant_dogfood_adapter.py
PASS

uv run ruff check src/config.py src/features/memory/context_loader.py src/features/memory/memphant_dogfood_adapter.py tests/features/config/test_config_bounds.py tests/unit/features/memory/test_memphant_dogfood_adapter.py
PASS
```

MemPhant repo gates:

```text
cargo fmt --check
PASS

cargo clippy --all-targets --all-features -- -D warnings
PASS

cargo test --all-targets --all-features
PASS

cargo test --doc
PASS

python3 -m pytest tests
PASS: 19 passed

python3 scripts/check_spec_drift.py
PASS: spec_drift=clean public=/Users/sidsharma/Memphant/docs/superpowers/specs/memphant private=/Users/sidsharma/Syndai/.wt/codex-memphant-cross-repo/docs/superpowers/specs/memphant
```

## Status

WS-F exit packet is complete for the first low-risk surface. Next workstream: WS-G Public UI, Docs, and Launch Surface.

# 2026-07-03 Dogfood Active-Read Gate

## Scope

`29` §6 dogfood gate for the first actively-read Syndai surface:
L1+ agent-scoped file memory.

This gate does not widen the cutover beyond the first low-risk surface. It
proves that Syndai's backend memory chokepoint can read that surface through
MemPhant's public contracts while keeping web/mobile clients out of MemPhant
storage and REST.

## Gate Mapping

| `29` §6 requirement | Proof |
|---|---|
| surface fixtures derived from `28-syndai-code-contract.md` are executable | `examples/syndai/file-memory-trace-compare.yaml`; `cargo test -p memphant-eval --test syndai_trace_compare` |
| trace compare passes or mismatches have accepted decisions | `memphant-eval syndai-trace-compare` passed with `recall=1` and no forbidden return |
| L1+ blocked-memory cases have zero failures | Syndai focused context-loader suite passed; it covers L0/L1+ separation and L1+ user-memory blocking |
| citations render in the target UX | `test_public_client_active_reads_agent_file_memory_with_trace_id` maps resource recall into `MemoryContext.file_memory` with `memphant_trace_id`, `memphant_unit_id`, and `memphant_citation_resource_id`; `test_file_memory_becomes_candidate` keeps file rows citeable through the existing candidate whitelist |
| correction candidate/selector flow meets adapter thresholds | `test_correct_and_forget_payloads_use_public_memphant_shapes` asserts the public `/v1/correct` selector payload for server-owned file updates |
| backend/UI forget semantics meet adapter thresholds | `test_correct_and_forget_payloads_use_public_memphant_shapes` asserts the public `/v1/forget` payload; scoped forget and route-operation regression tests passed |
| no web/mobile client talks to MemPhant DB directly | static scan of non-backend client/app directories found no web/mobile/app MemPhant usage; active-read references are confined to backend adapter/config/tests plus docs tooling |

## Verification

```text
cargo test -p memphant-eval --test syndai_trace_compare
PASS: 1 passed
```

```text
cargo run -p memphant-eval -- syndai-trace-compare examples/syndai/file-memory-trace-compare.yaml --archive-traces
PASS: syndai_trace_compare=pass id=syndai_agent_file_memory_001 surface=agent_file_memory recall=1 archive=docs/build-log/artifacts/syndai_agent_file_memory_001-trace-compare.json
```

```text
uv run pytest --no-cov tests/unit/features/memory/test_memphant_dogfood_adapter.py tests/features/config/test_config_bounds.py::TestConfigBounds::test_memphant_dogfood_env_values_parse tests/features/config/test_config_bounds.py::TestConfigBounds::test_memphant_timeout_bounds tests/unit/features/memory/test_memory_contracts.py tests/features/memory/test_context_loader.py tests/unit/features/memory/test_context_loader_observability.py -q
PASS: 60 passed
```

```text
uv run pytest --no-cov tests/unit/features/memory/test_scoped_forget.py tests/unit/features/memory/test_memory_experience_regressions.py tests/unit/features/memory/test_openapi_contracts.py -q
PASS: 17 passed
```

```text
rg -n "MEMPHANT|MemPhant|memphant|/v1/recall|/v1/correct|/v1/forget|memphant://" web mobile evalrank-web apps sdks services examples infrastructure ops .github scripts --glob '!**/docs/**'
PASS: no web/mobile/app MemPhant client usage; only scripts/validate_docs.py contained MemPhant spec-validation strings.
```

## Status

Dogfood gate is complete for the first low-risk surface. The next unchecked
launch gate is the public launch gate.

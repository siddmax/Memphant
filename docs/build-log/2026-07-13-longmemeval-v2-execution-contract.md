# 2026-07-13 LongMemEval-V2 execution contract

The pinned official harness defaults `--memory-context-max-tokens` to 200,000.
MemPhant's 32,768-token recall budget alone therefore did not freeze the
downstream reader context ceiling. Every native and MemPhant invocation now
passes `--memory-context-max-tokens 32768` explicitly.

The packaged adapter contract is schema v2. Every question proof fingerprints
the exact `memphant-server` and `memphant-cli` binaries, including resolved
path, byte count, and SHA-256. The adapter uses the current public recall
contract and no longer sends the removed client-controlled
`allowed_scope_ids` field.

Promotion evidence must contain exactly eight successful runs: MemPhant and
the official `no_retrieval` control for both `web` and `enterprise`, at both
`small` and `medium`. Within each domain/tier cell, both arms must have the
same question IDs, reader, judge, and 32,768-token memory-context ceiling.
Incomplete runs, duplicate cells, model drift, missing binary fingerprints,
or any recorded error fail closed through:

```text
python3 scripts/run_longmemeval_v2.py verify-matrix --matrix <matrix.json>
```

No paid model or full 7.12 GB benchmark run was performed in this contract
repair. The immutable adapter lock remains marked `paid_models_run: false`.

Verification:

```text
python3 -m pytest tests/test_public_benchmark_adapters.py \
  tests/test_temporal_benchmark_contract.py -q
```

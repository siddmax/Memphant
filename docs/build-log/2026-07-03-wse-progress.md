# 2026-07-03 WS-E Eval, Security, and Ops Progress

## Scope

WS-E cheap-loop exit packet:

- Executable YAML golden oracle and `verify-golden` load-bearing label check.
- Manifest/orphan guard for on-disk golden cases.
- Trace schema snapshot test.
- Security fixture suite covering poisoning, query/filter injection, high-risk action suppression, tenant leakage, and deletion completeness.
- Sampled public benchmark runner with trace archive output.
- Release benchmark runner fixture surface.
- Ops checks for blob GC, deletion saga read-back, and reindex/compaction SLA.
- `memphant compile --scope ... --out ...` read-only Markdown export and stale export verification through `memphant verify`.

## Artifacts

- WS-E plan: `docs/superpowers/plans/2026-07-03-memphant-wse.md`
- Golden suite: `examples/evals/golden.yaml`
- Manifest: `examples/evals/manifest.yaml`
- Golden cases: `examples/evals/golden/`
- Security suite: `examples/evals/security-smoke.yaml`
- Ops suite: `examples/evals/ops-smoke.yaml`
- Trace schema snapshot: `examples/evals/trace-schema.v1.json`
- Nightly sampled benchmark: `benchmarks/nightly-sampled.yaml`
- Release benchmark fixture: `benchmarks/release.yaml`
- Nightly trace archive: `docs/build-log/artifacts/nightly-sampled-traces.json`
- Markdown compile source fixture: `examples/evals/compiled-memory-source.json`
- Eval contract tests: `crates/memphant-eval/tests/eval_contract.rs`
- CLI compile/export tests: `crates/memphant-cli/tests/compile_contract.rs`

## Implementation Notes

- `memphant-eval` now parses typed YAML suites through Serde, seeds the existing in-memory core, runs `memphant_core::recall`, and evaluates deterministic oracle assertions without an LLM judge.
- `verify-golden` masks declared `answer_bearing_ids` and fails labels that are not load-bearing or lack second-author confirmation.
- The security runner requires all five WS-E lane kinds and makes deletion completeness call the core `forget` path before attacking recall.
- The ops runner keeps blob GC and compaction checks deterministic from fixture state, matching the cheap local gate before live provider jobs exist.
- `memphant compile` writes scoped Markdown plus `memphant-export.json`; `memphant verify --lock ... --export ...` compares both lock metadata and source hash to report stale exports.

## Verification

Focused gates:

```text
cargo test -p memphant-eval --test eval_contract
PASS: 7 passed

cargo test -p memphant-cli --test compile_contract
PASS: 1 passed

cargo run -p memphant-eval -- run examples/evals/golden.yaml
PASS: eval=pass id=pr-golden passed=2/2 archive=none

cargo run -p memphant-eval -- verify-golden examples/evals/golden.yaml
PASS: verify_golden=pass cases=2

cargo run -p memphant-eval -- verify-golden examples/evals/ --all
PASS: verify_golden=pass cases=2

cargo run -p memphant-eval -- security examples/evals/security-smoke.yaml
PASS: security=pass lanes=poisoning,query_filter_injection,high_risk_action_suppression,tenant_leakage,deletion_completeness deletion_completeness=pass

cargo run -p memphant-eval -- ops examples/evals/ops-smoke.yaml
PASS: ops=pass checks=blob_gc,deletion_saga_readback,reindex_compaction_sla

cargo run -p memphant-eval -- run benchmarks/nightly-sampled.yaml --archive-traces
PASS: eval=pass id=nightly-sampled passed=1/1 archive=docs/build-log/artifacts/nightly-sampled-traces.json

cargo run -p memphant-cli -- compile --scope project:checkout --out /tmp/memphant-wse-wiki --source examples/evals/compiled-memory-source.json
PASS: compile=written scope=project:checkout out=/tmp/memphant-wse-wiki entries=2

cargo run -p memphant-cli -- verify --lock memphant.lock --export /tmp/memphant-wse-wiki
PASS: verify=clean path=memphant.lock; export=clean path=/tmp/memphant-wse-wiki

cargo run -p memphant-cli -- db lint --provider plain-postgres
PASS: db_lint=clean provider=plain-postgres
```

Repo gates:

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

WS-E exit packet is complete. Next workstream: WS-F Syndai dogfood cutover.

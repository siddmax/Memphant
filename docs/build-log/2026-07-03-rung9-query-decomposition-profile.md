# Rung 9 Promotion - Query Decomposition

## Scope

- Added a default-on `query_decomposition_enabled` recall flag across core, REST, MCP, and eval surfaces.
- Added deterministic structural query decomposition for balanced/exhaustive recall only. Composite queries split into traceable subqueries; fast mode remains pass-through.
- Added trace fields for `subquery_ids`, `decomposition_reason`, and per-candidate `subquery_ids` so composite misses can identify which subquery failed.
- Added `memphant-eval run --disable-query-decomposition` so Rung 9 compares the enabled decomposer against a no-decomposition control with prior rungs still enabled.
- Added `examples/evals/golden/query_decomposition_deploy_release.yaml`, proving a composite deploy/release query recovers both answer-bearing units while the no-decomposition control misses one.
- Added a Rung 9 profile validator requiring a composite sample, no-decomposition control, state/LME sampled suite, and positive outcome/long-horizon/interactive axis deltas before promotion.
- Updated the trace schema snapshot and WSC golden expected stages to include `query_decomposition`.

## Verification

```bash
cargo test -p memphant-core --test recall_trace_golden query_decomposition -- --nocapture
# red before fix: RecallRequest/trace structs lacked query_decomposition fields; green after fix: 1 passed

cargo test -p memphant-core --test recall_trace_golden recall_golden_fixtures_pass -- --nocapture
# green after fixture update: 1 passed

cargo test -p memphant-eval --test eval_contract rung9 -- --nocapture
# red before fix: EvalRunOptions had no query_decomposition_enabled control; green after fix: 1 passed

cargo test -p memphant-eval --test profile_contract rung9 -- --nocapture
# red before fix: validator accepted a Rung 9 promotion without composite sample/control refs; green after fix: 2 passed

cargo test -p memphant-eval --test eval_contract -- --nocapture
# pass: 13 passed

cargo run -p memphant-eval -- run examples/evals/golden.yaml --archive-traces --archive-dir docs/build-log/artifacts
# eval=pass id=pr-golden passed=9/9 archive=docs/build-log/artifacts/pr-golden-traces.json

cargo run -p memphant-eval -- run benchmarks/rung9-baseline-sampled.yaml --disable-query-decomposition --archive-traces --archive-dir docs/build-log/artifacts
# expected control miss: eval=fail id=rung9-baseline-sampled passed=0/1

cargo run -p memphant-eval -- run benchmarks/rung9-state-lme-sampled.yaml --archive-traces --archive-dir docs/build-log/artifacts
# eval=pass id=rung9-state-lme-sampled passed=1/1 archive=docs/build-log/artifacts/rung9-state-lme-sampled-traces.json

cargo run -p memphant-eval -- profile examples/evals/rung9-query-decomposition-profile.yaml --compare-to rungs-0-8-baseline --archive docs/build-log/artifacts/rung9-query-decomposition-profile.json
# profile=pass id=rung9_query_decomposition_profile_001 compare_to=rungs-0-8-baseline activated=0 dormant=15 retired=0 archive=docs/build-log/artifacts/rung9-query-decomposition-profile.json

python3 scripts/check_spec_drift.py
# spec_drift=clean public=/Users/sidsharma/Memphant/docs/superpowers/specs/memphant private=/Users/sidsharma/Syndai/.wt/codex-memphant-cross-repo/docs/superpowers/specs/memphant

cargo fmt --check
# pass

cargo clippy --all-targets --all-features -- -D warnings
# pass

cargo test --all-targets --all-features
# pass

cargo test --doc
# pass

python3 -m pytest tests -q
# 25 passed

cargo run -p memphant-eval -- verify-golden examples/evals/golden.yaml
# verify_golden=pass cases=9

cargo run -p memphant-eval -- security examples/evals/security-smoke.yaml
# security=pass lanes=poisoning,query_filter_injection,high_risk_action_suppression,tenant_leakage,deletion_completeness deletion_completeness=pass

cargo run -p memphant-cli -- db lint --provider plain-postgres
# db_lint=clean provider=plain-postgres

cargo run -p memphant-cli -- db lint --provider supabase
# db_lint=clean provider=supabase

cargo run -p memphant-cli -- db lint --provider neon
# db_lint=clean provider=neon

python3 scripts/apply_memphant_migrations.py --database-url postgres://memphant.invalid/memphant --dry-run
# migration_plan=1
```

## Status

Rung 9 is promoted. The disabled no-decomposition control missed the release-approval half of the composite deploy query, while deterministic structural decomposition produced traceable subqueries and recovered both answer-bearing memories.

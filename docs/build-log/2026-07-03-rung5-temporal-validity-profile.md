# Rung 5 Promotion - Temporal Validity

## Scope

- Added optional bitemporal validity fields to memory-unit types: `valid_from`, `valid_to`, `transaction_from`, and `transaction_to`.
- Recall now suppresses expired semantic facts on current/latest/now queries and records `stale` drops.
- Corrections now close the superseded transaction generation and stamp correction validity windows onto the replacement generation.
- Added `memphant-eval run --disable-temporal-validity` so the same stale/current fixture can be run as a rungs-0-4 baseline.
- Added `examples/evals/golden/temporal_validity_current_office.yaml`, proving a stale fact is suppressed while current evidence remains citeable.
- Added a rung-5 profile validator requiring golden + STATE-style sampled axes before temporal validity can promote.

## Verification

```bash
cargo test -p memphant-core --test recall_trace_golden recall_drops_expired_validity_window_for_current_query
# 1 passed

cargo test -p memphant-core --test surface_mutations correct_supersedes_old_generation_and_recall_returns_new_value -- --nocapture
# red before fix: failed on missing transaction_to; green after fix: 1 passed

cargo run -p memphant-eval -- run examples/evals/golden.yaml
# eval=pass id=pr-golden passed=4/4 archive=none

cargo run -p memphant-eval -- verify-golden examples/evals/golden.yaml
# verify_golden=pass cases=4

cargo test -p memphant-eval --test eval_contract rung5_state_style_suite_proves_temporal_validity_delta
# 1 passed

cargo test -p memphant-eval --test profile_contract rung5
# 2 passed

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

python3 scripts/check_spec_drift.py
# spec_drift=clean public=/Users/sidsharma/Memphant/docs/superpowers/specs/memphant private=/Users/sidsharma/Syndai/.wt/codex-memphant-cross-repo/docs/superpowers/specs/memphant

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

cargo run -p memphant-eval -- run examples/evals/golden.yaml --archive-traces --archive-dir docs/build-log/artifacts
# eval=pass id=pr-golden passed=4/4 archive=docs/build-log/artifacts/pr-golden-traces.json

cargo run -p memphant-eval -- run benchmarks/rung5-baseline-sampled.yaml --disable-temporal-validity --archive-traces --archive-dir docs/build-log/artifacts || true
# eval=fail id=rung5-baseline-sampled passed=0/1

cargo run -p memphant-eval -- run benchmarks/rung5-state-style-sampled.yaml --archive-traces --archive-dir docs/build-log/artifacts
# eval=pass id=rung5-state-style-sampled passed=1/1 archive=docs/build-log/artifacts/rung5-state-style-sampled-traces.json

cargo run -p memphant-eval -- profile examples/evals/rung5-temporal-validity-profile.yaml --compare-to rungs-0-4-baseline --archive docs/build-log/artifacts/rung5-temporal-validity-profile.json
# profile=pass id=rung5_temporal_validity_profile_001 compare_to=rungs-0-4-baseline activated=0 dormant=15 retired=0 archive=docs/build-log/artifacts/rung5-temporal-validity-profile.json
```

## Status

Rung 5 is promoted. The paired baseline run returned the expired Seattle office fact when temporal validity was disabled; the enabled run suppressed it and retained the current Taipei evidence.

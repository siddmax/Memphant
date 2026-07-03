# Rung 7 Promotion - Packing + Abstention

## Scope

- Added an explicit `context_packing_abstention_enabled` recall flag across core, REST, MCP, and eval surfaces.
- Recall now uses deterministic fused-score-density packing when the flag is enabled, collapses duplicate subject-key evidence before budget fill, preserves contradiction-linked evidence, and abstains when the admitted pack carries unresolved contradiction labels.
- Added `memphant-eval run --disable-context-packing-abstention` so Rung 7 compares the enabled packer against a naive top-k control.
- Added `examples/evals/golden/packing_abstention_buried_deploy.yaml`, proving duplicate decoys cannot hide compact decisive evidence under a tight budget.
- Added `examples/evals/golden/packing_abstention_contradiction.yaml`, proving unresolved contradictory evidence returns both sides with an abstention signal.
- Added a Rung 7 profile validator requiring packing, abstention, and baseline-control sample refs before promotion.

## Verification

```bash
cargo test -p memphant-core --test recall_trace_golden packing_ -- --nocapture
# red before fix: RecallRequest had no context_packing_abstention_enabled field; green after fix: 2 passed

cargo test -p memphant-eval --test eval_contract rung7 -- --nocapture
# red before fix: EvalRunOptions had no context_packing_abstention_enabled field; green after fix: 1 passed

cargo test -p memphant-eval --test profile_contract rung7 -- --nocapture
# red before fix: validator accepted a Rung 7 promotion without packing/abstention sample refs; green after fix: 2 passed

cargo run -p memphant-eval -- run examples/evals/golden.yaml --archive-traces --archive-dir docs/build-log/artifacts
# eval=pass id=pr-golden passed=7/7 archive=docs/build-log/artifacts/pr-golden-traces.json

cargo run -p memphant-eval -- run benchmarks/rung7-baseline-sampled.yaml --disable-context-packing-abstention --archive-traces --archive-dir docs/build-log/artifacts
# expected control miss: eval=fail id=rung7-baseline-sampled passed=0/2

cargo run -p memphant-eval -- run benchmarks/rung7-state-style-sampled.yaml --archive-traces --archive-dir docs/build-log/artifacts
# eval=pass id=rung7-state-style-sampled passed=2/2 archive=docs/build-log/artifacts/rung7-state-style-sampled-traces.json

cargo run -p memphant-eval -- profile examples/evals/rung7-packing-abstention-profile.yaml --compare-to rungs-0-6-baseline --archive docs/build-log/artifacts/rung7-packing-abstention-profile.json
# profile=pass id=rung7_packing_abstention_profile_001 compare_to=rungs-0-6-baseline activated=0 dormant=15 retired=0 archive=docs/build-log/artifacts/rung7-packing-abstention-profile.json

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
# verify_golden=pass cases=7

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

Rung 7 is promoted. The disabled packing control missed the tight-budget deploy answer and failed to abstain on contradictory refund-window evidence, while the enabled packer recovered the compact answer and emitted the contradiction abstention signal.

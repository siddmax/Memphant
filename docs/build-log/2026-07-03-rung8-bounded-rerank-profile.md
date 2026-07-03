# Rung 8 Promotion - Bounded Rerank

## Scope

- Added a default-on `rerank_enabled` recall flag across core, REST, MCP, and eval surfaces.
- Added deterministic bounded rerank over the capped fused candidate set with trace fields for `reranker_id`, `rerank_input_count`, `rerank_overfetch_ratio`, per-candidate `rerank_rank`, and per-candidate `rerank_score`.
- Added `memphant-eval run --disable-rerank` so Rung 8 can compare the enabled reranker against a no-rerank control.
- Added `examples/evals/golden/bounded_rerank_incident_owner.yaml`, proving a lexically topical decoy can win the no-rerank fused top-1 while deterministic rerank recovers the answer-bearing owner memory.
- Added a Rung 8 profile validator requiring a bounded-rerank sample, no-rerank control, state-style sample, and positive outcome/interactive axis deltas before promotion.
- Updated the trace schema snapshot and WSC golden expected stages to include the explicit `rerank` stage.

## Verification

```bash
cargo test -p memphant-core --test recall_trace_golden bounded_rerank -- --nocapture
# red before fix: RecallRequest and retrieval trace structs lacked rerank flag/trace fields; green after fix: 1 passed

cargo test -p memphant-core --test recall_trace_golden recall_golden_fixtures_pass -- --nocapture
# green after fix: 1 passed

cargo test -p memphant-eval --test eval_contract rung8 -- --nocapture
# red before fix: EvalRunOptions had no rerank_enabled control; green after fix: 1 passed

cargo test -p memphant-eval --test profile_contract rung8 -- --nocapture
# red before fix: validator accepted a Rung 8 promotion without bounded-rerank sample/control refs; green after fix: 2 passed

cargo test -p memphant-eval --test eval_contract -- --nocapture
# pass: 12 passed

cargo run -p memphant-eval -- run examples/evals/golden.yaml --archive-traces --archive-dir docs/build-log/artifacts
# eval=pass id=pr-golden passed=8/8 archive=docs/build-log/artifacts/pr-golden-traces.json

cargo run -p memphant-eval -- run benchmarks/rung8-baseline-sampled.yaml --disable-rerank --archive-traces --archive-dir docs/build-log/artifacts
# expected control miss: eval=fail id=rung8-baseline-sampled passed=0/1

cargo run -p memphant-eval -- run benchmarks/rung8-state-style-sampled.yaml --archive-traces --archive-dir docs/build-log/artifacts
# eval=pass id=rung8-state-style-sampled passed=1/1 archive=docs/build-log/artifacts/rung8-state-style-sampled-traces.json

cargo run -p memphant-eval -- profile examples/evals/rung8-bounded-rerank-profile.yaml --compare-to rungs-0-7-baseline --archive docs/build-log/artifacts/rung8-bounded-rerank-profile.json
# profile=pass id=rung8_bounded_rerank_profile_001 compare_to=rungs-0-7-baseline activated=0 dormant=15 retired=0 archive=docs/build-log/artifacts/rung8-bounded-rerank-profile.json

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
# verify_golden=pass cases=8

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

Rung 8 is promoted. The disabled no-rerank control returned the topical decoy for the rank-sensitive owner query, while deterministic bounded rerank over the protected fused set recovered the answer-bearing memory and archived rerank trace evidence.

# Rung 6 Promotion - Edge Expansion

## Scope

- Added an explicit `edge_expansion_enabled` recall flag across core, REST, MCP, and eval surfaces.
- Recall now skips the edge channel when the flag is disabled and only emits `edge_expansion_enabled` in trace flags when the behavior is active.
- Added `memphant-eval run --disable-edge-expansion` and `--filesystem-control` so Rung 6 compares against both required controls.
- Added `examples/evals/golden/edge_expansion_runbook_lineage.yaml`, proving a one-hop `depends_on` edge recovers related answer-bearing evidence that direct retrieval misses.
- Added a Rung 6 profile validator requiring golden edge evidence plus no-edges and filesystem-control sample refs before promotion.

## Verification

```bash
cargo test -p memphant-core --test recall_trace_golden edge_expansion_can_be_disabled_and_traces_related_candidates -- --nocapture
# red before fix: RecallRequest had no edge_expansion_enabled field; green after fix: 1 passed

cargo test -p memphant-eval --test eval_contract rung6_state_lme_suite_proves_edge_expansion_delta -- --nocapture
# red before fix: EvalRunOptions had no edge_expansion_enabled field; green after fix: 1 passed

cargo test -p memphant-eval --test profile_contract rung6 -- --nocapture
# red before fix: validator accepted missing controls; green after fix: 2 passed

cargo run -p memphant-eval -- run examples/evals/golden.yaml --archive-traces --archive-dir docs/build-log/artifacts
# eval=pass id=pr-golden passed=5/5 archive=docs/build-log/artifacts/pr-golden-traces.json

cargo run -p memphant-eval -- run benchmarks/rung6-no-edges-sampled.yaml --disable-edge-expansion --archive-traces --archive-dir docs/build-log/artifacts
# expected control miss: eval=fail id=rung6-no-edges-sampled passed=0/1

cargo run -p memphant-eval -- run benchmarks/rung6-filesystem-control-sampled.yaml --filesystem-control --archive-traces --archive-dir docs/build-log/artifacts
# expected control miss: eval=fail id=rung6-filesystem-control-sampled passed=0/1

cargo run -p memphant-eval -- run benchmarks/rung6-state-lme-sampled.yaml --archive-traces --archive-dir docs/build-log/artifacts
# eval=pass id=rung6-state-lme-sampled passed=1/1 archive=docs/build-log/artifacts/rung6-state-lme-sampled-traces.json

cargo run -p memphant-eval -- profile examples/evals/rung6-edge-expansion-profile.yaml --compare-to rungs-0-5-baseline --archive docs/build-log/artifacts/rung6-edge-expansion-profile.json
# profile=pass id=rung6_edge_expansion_profile_001 compare_to=rungs-0-5-baseline activated=0 dormant=15 retired=0 archive=docs/build-log/artifacts/rung6-edge-expansion-profile.json

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
# verify_golden=pass cases=5

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

Rung 6 is promoted. The no-edges and filesystem-control sampled runs missed the edge-only lineage case, while the enabled run recovered the related answer-bearing unit through the one-hop edge channel.

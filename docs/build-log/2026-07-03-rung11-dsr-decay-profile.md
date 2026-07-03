# Rung 11 DSR Decay Profile

## Scope

- Promoted Rung 11 DSR decay from the live STATUS ladder.
- Added default-on `decay_enabled` recall/eval control plus `--disable-decay` for paired ablations.
- Added a fixed-prior DSR fold over deduped `review_event` rows produced by `mark`.
- Wrapped the current `fsrs` crate API (`MemoryState`, `FSRS::next_states`, `current_retrievability`) behind local deterministic scoring; learned parameter fitting remains dormant.
- Added trace evidence for `decay_model_id`, per-candidate `decay_retrievability`, DSR state, and reinforcement count.
- Aligned Rust `StoredMemoryUnit` with the migration's DSR columns: `difficulty`, `stability_days`, `last_reinforced_at`, `reinforcement_count`.
- Made `mark` idempotent per `(trace_id, caller_id)` so redelivered feedback cannot double-apply stability.

## Red Tests

```text
cargo test -p memphant-core --test recall_trace_golden dsr_decay_fold_promotes_reinforced_memory_over_ignored_stale_candidate -- --nocapture
assertion failed: stale candidate ranked before reinforced candidate
```

```text
cargo test -p memphant-eval --test eval_contract rung11_memorystress_style_suite_proves_dsr_decay_delta -- --nocapture
error[E0560]: struct `EvalRunOptions` has no field named `decay_enabled`
```

```text
cargo test -p memphant-eval --test profile_contract rung11_promotion_requires_memorystress_and_no_decay_control -- --nocapture
rung11 promotion without MemoryStress/no-decay proof should fail: SotaProfileReport { ... }
```

## Green Focused Tests

```text
cargo test -p memphant-core --test recall_trace_golden dsr_decay_fold_promotes_reinforced_memory_over_ignored_stale_candidate -- --nocapture
test dsr_decay_fold_promotes_reinforced_memory_over_ignored_stale_candidate ... ok
```

```text
cargo test -p memphant-core --test surface_mutations mark_is_idempotent_per_trace_and_caller -- --nocapture
test mark_is_idempotent_per_trace_and_caller ... ok
```

```text
cargo test -p memphant-eval --test eval_contract rung11_memorystress_style_suite_proves_dsr_decay_delta -- --nocapture
test rung11_memorystress_style_suite_proves_dsr_decay_delta ... ok
```

```text
cargo test -p memphant-eval --test profile_contract rung11_profile_archives_dsr_decay_promotion -- --nocapture
test rung11_profile_archives_dsr_decay_promotion ... ok
```

```text
cargo test -p memphant-eval --test profile_contract rung11_promotion_requires_memorystress_and_no_decay_control -- --nocapture
test rung11_promotion_requires_memorystress_and_no_decay_control ... ok
```

## Artifacts

```text
cargo run -p memphant-eval -- schema trace > examples/evals/trace-schema.v1.json
# pass; schema includes decay_model_id and per-candidate DSR fields
```

```text
cargo run -p memphant-eval -- run examples/evals/golden.yaml --archive-traces --archive-dir docs/build-log/artifacts
eval=pass id=pr-golden passed=11/11 archive=docs/build-log/artifacts/pr-golden-traces.json
```

```text
cargo run -p memphant-eval -- run benchmarks/rung11-baseline-sampled.yaml --disable-decay --archive-traces --archive-dir docs/build-log/artifacts
eval=fail id=rung11-baseline-sampled passed=0/1
case=dsr_decay_fold_review_event error=None
```

```text
cargo run -p memphant-eval -- run benchmarks/rung11-memorystress-sampled.yaml --archive-traces --archive-dir docs/build-log/artifacts
eval=pass id=rung11-memorystress-sampled passed=1/1 archive=docs/build-log/artifacts/rung11-memorystress-sampled-traces.json
```

```text
cargo run -p memphant-eval -- profile examples/evals/rung11-dsr-decay-profile.yaml --compare-to rungs-0-10-baseline --archive docs/build-log/artifacts/rung11-dsr-decay-profile.json
profile=pass id=rung11_dsr_decay_profile_001 compare_to=rungs-0-10-baseline activated=2 dormant=13 retired=0 archive=docs/build-log/artifacts/rung11-dsr-decay-profile.json
```

## Status

Rung 11 is promoted. The no-decay control returned the stale ignored runbook and missed the reinforced runbook; the fixed-prior DSR fold over `review_event` recovered the reinforced durable memory. Learned DSR/FSRS fitting remains dormant until enough MemPhant-native review traces exist.

## Full Gate

```text
cargo fmt --check
# pass
```

```text
cargo clippy --all-targets --all-features -- -D warnings
# Finished `dev` profile; no warnings
```

```text
cargo test --all-targets --all-features
# recall_trace_golden: 12 passed
# eval_contract: 15 passed
# profile_contract: 18 passed
# all workspace tests passed
```

```text
cargo test --doc
# all doc-test crates passed
```

```text
python3 -m pytest tests -q
25 passed in 1.29s
```

```text
cargo run -p memphant-eval -- verify-golden examples/evals/golden.yaml
verify_golden=pass cases=11
```

```text
cargo run -p memphant-eval -- security examples/evals/security-smoke.yaml
security=pass lanes=poisoning,query_filter_injection,high_risk_action_suppression,tenant_leakage,deletion_completeness deletion_completeness=pass
```

```text
cargo run -p memphant-cli -- db lint --provider plain-postgres
db_lint=clean provider=plain-postgres

cargo run -p memphant-cli -- db lint --provider supabase
db_lint=clean provider=supabase

cargo run -p memphant-cli -- db lint --provider neon
db_lint=clean provider=neon
```

```text
python3 scripts/apply_memphant_migrations.py --database-url postgres://memphant.invalid/memphant --dry-run
migration_plan=1
memphant_migrations/versions/20260703_001_wsa_bootstrap.sql
```

```text
cargo run -p memphant-cli -- db bootstrap-check --provider plain-postgres
bootstrap_check=clean provider=plain-postgres profile=deploy/provider-profiles/plain-postgres.env.example
migration_lint=clean provider=plain-postgres

cargo run -p memphant-cli -- db bootstrap-check --provider supabase
bootstrap_check=clean provider=supabase profile=deploy/provider-profiles/supabase.env.example
migration_lint=clean provider=supabase

cargo run -p memphant-cli -- db bootstrap-check --provider neon
bootstrap_check=clean provider=neon profile=deploy/provider-profiles/neon.env.example
migration_lint=clean provider=neon
```

```text
python3 scripts/check_spec_drift.py
spec_drift=clean public=/Users/sidsharma/Memphant/docs/superpowers/specs/memphant private=/Users/sidsharma/Syndai/.wt/codex-memphant-cross-repo/docs/superpowers/specs/memphant
```

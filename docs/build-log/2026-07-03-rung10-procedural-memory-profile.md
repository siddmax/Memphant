# Rung 10 Procedural Memory Profile

## Scope

- Promoted Rung 10 procedural memory from the live STATUS ladder.
- Built validated procedural recall as recommendation/warning evidence only:
  - `procedure_recall_enabled` request/eval control.
  - `kind=procedural` units must be `state=validated`.
  - destructive/high-risk procedure sketches are suppressed even when marked validated.
  - failure-pattern procedures are returned as avoid-warning evidence, not action scripts.
  - retrieval traces archive procedure IDs and validation/safety facts.
- Kept executable rules/auto-triggered memory out of scope per `26`.

## Red Tests

```text
cargo test -p memphant-core --test recall_trace_golden procedural_memory -- --nocapture
error[E0560]: struct `RecallRequest` has no field named `procedure_recall_enabled`
error[E0609]: no field `procedure_ids` on type `RetrievalTrace`
error[E0609]: no field `procedure_validation_states` on type `RetrievalTrace`
```

```text
cargo test -p memphant-eval --test eval_contract rung10 -- --nocapture
error[E0560]: struct `EvalRunOptions` has no field named `procedure_recall_enabled`
```

```text
cargo test -p memphant-eval --test profile_contract rung10 -- --nocapture
rung10_promotion_requires_replay_sample_and_no_procedure_control ... FAILED
```

## Green Focused Tests

```text
cargo test -p memphant-core --test recall_trace_golden procedural_memory -- --nocapture
test procedural_memory_replays_only_validated_safe_procedures_and_traces_gate ... ok
```

```text
cargo test -p memphant-eval --test eval_contract rung10 -- --nocapture
test rung10_state_style_suite_proves_procedural_memory_delta ... ok
```

```text
cargo test -p memphant-eval --test profile_contract rung10 -- --nocapture
test rung10_promotion_requires_replay_sample_and_no_procedure_control ... ok
test rung10_profile_archives_procedural_memory_promotion ... ok
```

## Artifacts

```text
cargo run -p memphant-eval -- run examples/evals/golden.yaml --archive-traces --archive-dir docs/build-log/artifacts
eval=pass id=pr-golden passed=10/10 archive=docs/build-log/artifacts/pr-golden-traces.json
```

```text
cargo run -p memphant-eval -- run benchmarks/rung10-baseline-sampled.yaml --disable-procedure-recall --archive-traces --archive-dir docs/build-log/artifacts
eval=fail id=rung10-baseline-sampled passed=0/1
case=procedural_memory_replay_validation error=None
```

```text
cargo run -p memphant-eval -- run benchmarks/rung10-state-style-sampled.yaml --archive-traces --archive-dir docs/build-log/artifacts
eval=pass id=rung10-state-style-sampled passed=1/1 archive=docs/build-log/artifacts/rung10-state-style-sampled-traces.json
```

```text
cargo run -p memphant-eval -- profile examples/evals/rung10-procedural-memory-profile.yaml --compare-to rungs-0-9-baseline --archive docs/build-log/artifacts/rung10-procedural-memory-profile.json
profile=pass id=rung10_procedural_memory_profile_001 compare_to=rungs-0-9-baseline activated=1 dormant=14 retired=0 archive=docs/build-log/artifacts/rung10-procedural-memory-profile.json
```

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
# recall_trace_golden: 11 passed
# eval_contract: 14 passed
# profile_contract: 16 passed
# all workspace tests passed
```

```text
cargo test --doc
# all doc-test crates passed
```

```text
python3 -m pytest tests -q
25 passed in 1.30s
```

```text
cargo run -p memphant-eval -- verify-golden examples/evals/golden.yaml
verify_golden=pass cases=10
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

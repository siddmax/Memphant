# Rung 4 Promotion - Contextual Chunks

## Scope

- Added a first-class `rung_decisions` section to SOTA profile archives.
- Added `memphant-eval run --disable-contextual-chunks` so a paired baseline can archive the same sampled cases with contextual chunks removed.
- Added pinned sampled-public LME-V2 and BEAM probes:
  - `examples/evals/public-sampled/lme_v2_static_state_contextual_chunk.yaml`
  - `examples/evals/public-sampled/beam_100k_contradiction_contextual_chunk.yaml`
- Fixed temporal recency matching so `ServiceNow` no longer triggers the `now` recency token.

## Public Sample Sources

- LongMemEval-V2: `hf:xiaowu0162/longmemeval-v2@2026-05-17/questions.jsonl`, sample question `057a2d4d`.
- BEAM 100K: `hf:Mohammadta/BEAM@2026-01-30/data/100K-00000-of-00001.parquet`, sample conversation `conversation_id=1`.

## Verification

```bash
cargo test -p memphant-core --test recall_trace_golden servicenow_query_does_not_trigger_temporal_recency_match
# 1 passed

cargo test -p memphant-eval --test eval_contract sampled_public_rung4_suite_proves_contextual_chunk_delta
# 1 passed

cargo test -p memphant-eval --test profile_contract rung4
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

cargo run -p memphant-eval -- verify-golden examples/evals/golden.yaml
# verify_golden=pass cases=3

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

cargo run -p memphant-eval -- run benchmarks/rung4-baseline-sampled.yaml --disable-contextual-chunks --archive-traces --archive-dir docs/build-log/artifacts || true
# eval=fail id=rung4-baseline-sampled passed=0/2

cargo run -p memphant-eval -- run benchmarks/rung4-lme-beam-sampled.yaml --archive-traces --archive-dir docs/build-log/artifacts
# eval=pass id=rung4-public-sampled passed=2/2 archive=docs/build-log/artifacts/rung4-public-sampled-traces.json

cargo run -p memphant-eval -- profile examples/evals/rung4-contextual-chunks-profile.yaml --compare-to rungs-0-3-baseline --archive docs/build-log/artifacts/rung4-contextual-chunks-profile.json
# profile=pass id=rung4_contextual_chunks_sampled_profile_001 compare_to=rungs-0-3-baseline activated=0 dormant=15 retired=0 archive=docs/build-log/artifacts/rung4-contextual-chunks-profile.json
```

## Status

Rung 4 is promoted. The paired sampled profile shows both LME-V2 and BEAM probes missed under the rungs-0-3 baseline with contextual chunks disabled, then passed with contextual chunks enabled. Next ladder rung is temporal validity.

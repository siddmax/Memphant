# Rung 4 Progress - Contextual Chunks

## Scope

- Added `ContextualChunk` metadata to memory units.
- Added `ReflectCandidate.contextual_chunks` so extraction output can attach chunk metadata to promoted memory units.
- Contextual chunk scoring now participates in the vector channel while returning the source memory unit and original episode/resource citation.
- Recall traces include `contextual_chunks_enabled`, and context items selected through chunk text use `inclusion_reason=contextual_chunk`.
- Added `examples/evals/golden/contextual_chunk_breaker.yaml` and included it in PR golden + nightly sampled suites.

## Verification

```bash
cargo test -p memphant-core --test write_compiler_golden reflect_candidate_contextual_chunks_are_stored_with_source_episode
# 1 passed

cargo test -p memphant-core --test recall_trace_golden contextual_chunk_recall_finds_source_unit_and_traces_flag
# 1 passed

cargo run -p memphant-eval -- verify-golden examples/evals/golden.yaml
# verify_golden=pass cases=3

cargo run -p memphant-eval -- run examples/evals/golden.yaml --archive-traces --archive-dir docs/build-log/artifacts
# eval=pass id=pr-golden passed=3/3 archive=docs/build-log/artifacts/pr-golden-traces.json

cargo run -p memphant-eval -- run benchmarks/nightly-sampled.yaml --archive-traces --archive-dir docs/build-log/artifacts
# eval=pass id=nightly-sampled passed=2/2 archive=docs/build-log/artifacts/nightly-sampled-traces.json

cargo fmt --check
# pass

cargo clippy --all-targets --all-features -- -D warnings
# pass

cargo test --all-targets --all-features
# pass

cargo test --doc
# pass

cargo run -p memphant-cli -- db lint --provider plain-postgres
# db_lint=clean provider=plain-postgres

cargo run -p memphant-cli -- db lint --provider supabase
# db_lint=clean provider=supabase

cargo run -p memphant-cli -- db lint --provider neon
# db_lint=clean provider=neon

python3 -m pytest tests -q
# 25 passed

python3 scripts/check_spec_drift.py
# spec_drift=clean public=/Users/sidsharma/Memphant/docs/superpowers/specs/memphant private=/Users/sidsharma/Syndai/.wt/codex-memphant-cross-repo/docs/superpowers/specs/memphant

python3 scripts/apply_memphant_migrations.py --database-url postgres://memphant.invalid/memphant --dry-run
# migration_plan=1
```

## Status

The local contextual-chunk implementation is green, but STATUS rung 4 remains unchecked. `27` requires top-k improvement on LME-V2/BEAM samples for rung promotion; this change does not include that external sampled profile.

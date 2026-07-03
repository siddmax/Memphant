# WS-B Progress

## Changed

- Added the WS-B execution plan: `docs/superpowers/plans/2026-07-03-memphant-wsb.md`.
- Added `RetainRequest`, `ReflectJob`, `ReflectJobKind`, and `QueuedReflectJob` to `memphant-types`.
- Extended the `MemoryStore` seam with transactional `enqueue_reflect`.
- Extended the deterministic `InMemoryStore` fake with a reflect-job queue that publishes only on commit.
- Added `retain_episode` in `memphant-core`: validate body, derive a deterministic dedup key, stage the raw episode, enqueue `reflect`, and commit as one unit.
- Added exact episode dedup collapse in the in-memory store: matching `(tenant_id, scope_id, dedup_key)` increments `observation_count` without deleting or overwriting the existing episode.
- Added store-contract tests for transactional retain/enqueue and duplicate collapse.
- Added resource retain support: `retain_resource` stores raw resource pointers in `registered` extractor state and enqueues `reflect_resource` jobs in the same transaction.
- Added record-replay WS-B golden fixtures in `examples/evals/wsb-write-goldens.json`.
- Added `write_compiler_golden` coverage for noisy-write rejection, duplicate collapse, contradiction detection, same-origin corroboration-farming resistance, independent-source belief promotion, explicit invalidate/quarantine admission, stale volatile fact handling, trace stage/cost facts, and duplicate-job idempotency.
- Added first deterministic `reflect_recorded` compiler path in `memphant-core` over recorded candidates; it writes memory units, contradiction/supersession/derived edges, quarantine states, freshness markers, and durable reflect traces in the in-memory store.
- Added the reserved `memphant.event_outbox` table shape with tenant RLS, cursor indexes, and provider/catalog lint coverage. Delivery consumers remain post-v1 as specified.

## Proof

- `cargo test -p memphant-core --test store_contract retain_pipeline_stores_episode_and_reflect_job_atomically`
  - RED result before implementation: compile failed because `retain_episode`, `RetainRequest`, and `reflect_jobs` did not exist.
  - GREEN result after implementation: `1 passed; 3 filtered out`.
- `cargo test -p memphant-core --test store_contract retain_pipeline_collapses_duplicate_episode_by_dedup_key`
  - RED result before implementation: failed with `left: 2, right: 1` because duplicate retain inserted two episodes.
  - GREEN result after implementation: `1 passed; 4 filtered out`.
- `cargo fmt --check`
  - Initial result: formatting diff in `crates/memphant-core/src/lib.rs`.
  - After `cargo fmt`: passed.
- `cargo test -p memphant-core --test store_contract`
  - Result: `5 passed`.
- `cargo test -p memphant-core --test write_compiler_golden write_compiler_golden_fixtures_pass`
  - RED result after adding independent-source promotion fixture: failed with `left: [Append, Merge]`, `right: [Append, Append]`.
  - GREEN result after implementation: `1 passed; 1 filtered out`.
- `cargo test -p memphant-core --test write_compiler_golden reflect_recorded_is_idempotent_for_duplicate_job_delivery`
  - RED result before job checkpointing: redelivery returned `actions: [Merge]` instead of the original `actions: [Append]`.
  - GREEN result after checkpointing by `(job_id, compiler_version)`: `1 passed; 1 filtered out`.
- `cargo test -p memphant-core --test write_compiler_golden`
  - Result: `2 passed`.
- `cargo test -p memphant-core --test store_contract retain_resource_stores_pointer_before_extraction_and_enqueues_reflect`
  - RED result before implementation: compile failed because `retain_resource`, `RetainResourceRequest`, `ResourceExtractorState`, `resources`, and `resource_id` did not exist.
  - GREEN result after implementation: `1 passed; 5 filtered out`.
- `cargo test -p memphant-core --test write_compiler_golden write_compiler_golden_fixtures_pass`
  - RED result after adding invalidate/quarantine/freshness expectations: compile failed because `ReflectCandidate.admission_hint`, `quarantined_units`, and `freshness_due_units` did not exist; then failed `quarantine_action` until quarantined beliefs were separated from visible belief units.
  - GREEN result after implementation: `1 passed; 1 filtered out`.
- `python3 -m pytest tests/test_wsa_migration_contract.py`
  - Result after adding `event_outbox`: `9 passed in 0.20s`.
- `cargo test -p memphant-store-postgres --test provider_lint`
  - Result after adding `event_outbox`: `3 passed`.
- `python3 scripts/check_spec_drift.py`
  - Result: `spec_drift=clean public=/Users/sidsharma/Memphant/docs/superpowers/specs/memphant private=/Users/sidsharma/Syndai/.wt/codex-memphant-cross-repo/docs/superpowers/specs/memphant`
- `cargo clippy --all-targets --all-features -- -D warnings`
  - Result: passed.
- `cargo test --all-targets --all-features`
  - Result: passed; includes `store_contract` (`6 passed`), `write_compiler_golden` (`2 passed`), and `provider_lint` (`3 passed`).
- `python3 -m pytest tests`
  - Result: `16 passed in 0.24s`.
- `cargo test --doc`
  - Result: passed doc tests for `memphant-core`, `memphant-eval`, `memphant-store-postgres`, and `memphant-types`.

## Status

WS-B is checked in `STATUS.md`. Current proof covers raw episode/resource capture before extraction, transactional reflect enqueue, exact dedup, the named write compiler golden families, explicit `invalidate`/`quarantine` admission actions, active freshness due-scan visibility, trace stage/cost facts, duplicate-job idempotency, and the reserved consolidation outbox table shape. Postgres write adapters, REST/MCP/SDK surfaces, and event consumers remain in later workstreams per `29`.

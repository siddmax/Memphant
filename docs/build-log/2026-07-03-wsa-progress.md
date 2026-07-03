# WS-A Progress

## Changed

- Added the WS-A execution plan: `docs/superpowers/plans/2026-07-03-memphant-wsa.md`.
- Added the MemPhant migration triad:
  - `memphant_migrations/versions/20260703_001_wsa_bootstrap.sql`
  - `scripts/apply_memphant_migrations.py`
  - `scripts/check_memphant_migration_boundary.py`
  - `scripts/check_memphant_migration_contract.py`
  - `scripts/check_memphant_migration_class.py`
- Added static migration contract coverage in `tests/test_wsa_migration_contract.py`.
- Added typed WS-A IDs and store input/output shapes in `memphant-types`.
- Added the `MemoryStore` transaction seam and deterministic `InMemoryStore` fake in `memphant-core`.
- Added provider linting in `memphant-store-postgres` and wired `memphant-cli db lint --provider <plain-postgres|supabase|neon>`.
- Added live catalog linting in `scripts/check_memphant_live_catalog.py` for the executable database contract: Postgres/pgvector floor, MemPhant-only schema placement, RLS/grants, tenant indexes, FK indexes, `search_path`, and the migration ledger.
- Added tenant-prefixed FK indexes found by the first live catalog run, so every composite FK has a matching leading-column index.

## Proof

- `python3 -m pytest tests/test_repo_contract.py tests/test_wsa_migration_contract.py spikes/python-retain/test_spike.py -q`
  - Result: `17 passed in 0.25s`
- `python3 scripts/check_spec_drift.py`
  - Result: `spec_drift=clean public=/Users/sidsharma/Documents/Memphant/docs/superpowers/specs/memphant private=/Users/sidsharma/Syndai/.wt/codex-memphant-cross-repo/docs/superpowers/specs/memphant`
- `~/.cargo/bin/cargo fmt --check`
  - Result: passed.
- `~/.cargo/bin/cargo clippy --all-targets --all-features -- -D warnings`
  - Result: passed.
- `~/.cargo/bin/cargo test --all-targets --all-features`
  - Result: passed; includes `store_contract` and `provider_lint`.
- `~/.cargo/bin/cargo test --doc`
  - Result: passed doc tests for `memphant-core`, `memphant-eval`, `memphant-store-postgres`, and `memphant-types`.
- `~/.cargo/bin/cargo run -p memphant-cli -- db lint --provider plain-postgres`
  - Result: `db_lint=clean provider=plain-postgres`
- `~/.cargo/bin/cargo run -p memphant-cli -- db lint --provider supabase`
  - Result: `db_lint=clean provider=supabase`
- `~/.cargo/bin/cargo run -p memphant-cli -- db lint --provider neon`
  - Result: `db_lint=clean provider=neon`
- `python3 scripts/apply_memphant_migrations.py --database-url postgres://memphant.invalid/memphant --dry-run`
  - Result: `migration_plan=1`, `memphant_migrations/versions/20260703_001_wsa_bootstrap.sql`

## DB Bootstrap Smoke

- Pulled and used `pgvector/pgvector:0.8.4-pg17` for the authoritative Postgres 17+ bootstrap proof.
- `python3 scripts/apply_memphant_migrations.py --database-url postgresql://postgres:memphant@127.0.0.1:55433/memphant`
  - Result: `migration_apply=complete`
- Immediate reapply of the same command:
  - Result: `migration_apply=complete`
- `python3 scripts/check_memphant_live_catalog.py --database-url postgresql://postgres:memphant@127.0.0.1:55433/memphant`
  - Result: `live_catalog=clean`
  - Observed `postgres_version_num=170010`, `vector_version=0.8.4`, `memphant_tables=22`, `rls_tables=21`.

## Status

WS-A is checked in `STATUS.md`.

Exit-packet coverage: all WS-A tables including `belief_observation`, `review_event`, and `scope_block`; typed tenant/scope IDs and store seam; static migration contract tests; provider lint for plain Postgres, Supabase, and Neon; fresh Postgres 17 + pgvector 0.8.4 bootstrap; immediate migration reapply; live catalog checks for tenant columns, RLS/grants, indexes, extensions, `search_path`, FK indexes, and `memphant.schema_migrations`.

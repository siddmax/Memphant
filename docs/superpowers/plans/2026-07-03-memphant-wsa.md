# MemPhant WS-A Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build WS-A's schema, core types, `MemoryStore` transaction seam, in-memory fake, and provider lint scaffold without entering WS-B write behavior.

**Architecture:** Keep the storage boundary explicit: `memphant-types` owns durable data shapes, `memphant-core` owns the trait and in-memory fake, `memphant-store-postgres` owns migration/lint logic, and Python scripts own repository-level SQL firewall checks. The first migration is the modulus-1 portable bootstrap; provider lint enforces the invariants that must hold before hosted partition/index expansion.

**Tech Stack:** Rust 1.96.1, Postgres SQL, pgvector DDL, Python pytest, Cargo tests.

---

### Task 1: Migration Contract Tests

**Files:**
- Create: `tests/test_wsa_migration_contract.py`
- Create: `memphant_migrations/versions/20260703_001_wsa_bootstrap.sql`
- Create: `scripts/check_memphant_migration_boundary.py`
- Create: `scripts/check_memphant_migration_contract.py`

- [ ] **Step 1: Write failing tests**
  - Assert the bootstrap migration exists.
  - Assert all WS-A tables are created in `memphant`.
  - Assert `schema_migrations` has `schema_compat_revision`.
  - Assert no migration references `public.` or `syndai.` and no migration contains `DROP TABLE`.
  - Assert tenant-scoped tables have `tenant_id`, RLS enabled, tenant indexes, and no browser-role grants.

- [ ] **Step 2: Run the tests**
  - Run: `python3 -m pytest tests/test_wsa_migration_contract.py -q`
  - Expected before implementation: fails because migration files/scripts do not exist.

- [ ] **Step 3: Add minimal SQL and scripts**
  - Add idempotent bootstrap SQL for the WS-A table set.
  - Add boundary and contract scripts that parse the migration text and fail closed on missing invariants.

- [ ] **Step 4: Verify**
  - Run: `python3 -m pytest tests/test_wsa_migration_contract.py -q`
  - Expected after implementation: pass.

### Task 2: Core Store Seam

**Files:**
- Modify: `crates/memphant-types/src/lib.rs`
- Modify: `crates/memphant-core/src/lib.rs`
- Create: `crates/memphant-core/tests/store_contract.rs`

- [ ] **Step 1: Write failing Rust tests**
  - Assert `InMemoryStore` stages episodes and units only inside a transaction.
  - Assert dropping a transaction without commit rolls back staged rows.
  - Assert tenant/scope ids are mandatory in `NewEpisode` and `NewMemoryUnit` construction.

- [ ] **Step 2: Run the tests**
  - Run: `cargo test -p memphant-core --test store_contract`
  - Expected before implementation: compile failure because the store seam does not exist.

- [ ] **Step 3: Implement minimal seam**
  - Add typed ids and stage structs in `memphant-types`.
  - Add `MemoryStore`, `InMemoryStore`, and transaction buffering in `memphant-core`.

- [ ] **Step 4: Verify**
  - Run: `cargo test -p memphant-core --test store_contract`
  - Expected after implementation: pass.

### Task 3: Provider Lint Command

**Files:**
- Modify: `crates/memphant-store-postgres/src/lib.rs`
- Modify: `crates/memphant-cli/src/main.rs`
- Create: `crates/memphant-store-postgres/tests/provider_lint.rs`

- [ ] **Step 1: Write failing tests**
  - Assert `lint_migrations("plain-postgres")`, `"supabase"`, and `"neon"` all pass on the bootstrap migration.
  - Assert the linter rejects missing RLS and browser-role grants in an injected bad migration snippet.

- [ ] **Step 2: Run the tests**
  - Run: `cargo test -p memphant-store-postgres --test provider_lint`
  - Expected before implementation: compile failure because lint API does not exist.

- [ ] **Step 3: Implement minimal lint**
  - Reuse the migration parser logic in Rust for CLI/provider checks.
  - Add `memphant-cli db lint --provider <plain-postgres|supabase|neon>`.

- [ ] **Step 4: Verify**
  - Run:
    - `cargo test -p memphant-store-postgres --test provider_lint`
    - `cargo run -p memphant-cli -- db lint --provider plain-postgres`
    - `cargo run -p memphant-cli -- db lint --provider supabase`
    - `cargo run -p memphant-cli -- db lint --provider neon`

### Task 4: WS-A Gate and Ledger

**Files:**
- Modify: `docs/build-log/2026-07-03-wsa-progress.md`
- Modify only if exit packet is fully proven: `docs/superpowers/specs/memphant/STATUS.md`

- [ ] **Step 1: Run local gates**
  - `python3 -m pytest tests/test_repo_contract.py tests/test_wsa_migration_contract.py spikes/python-retain/test_spike.py -q`
  - `python3 scripts/check_spec_drift.py`
  - `cargo fmt --check`
  - `cargo clippy --all-targets --all-features -- -D warnings`
  - `cargo test --all-targets --all-features`
  - `cargo test --doc`

- [ ] **Step 2: Attempt fresh DB bootstrap**
  - Prefer Postgres 17+ with pgvector >= 0.8.4.
  - If only an older local Postgres/pgvector image is available, record that as a missing WS-A proof and do not flip the checkbox.

- [ ] **Step 3: Update build log**
  - Paste exact command outputs and state whether WS-A is complete or still missing fresh DB bootstrap proof.

- [ ] **Step 4: Update ledger only with proof**
  - Flip WS-A only if fresh DB bootstrap, provider lint, and tenant/scope tests are all proven.

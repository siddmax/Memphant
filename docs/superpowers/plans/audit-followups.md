# Audit follow-ups (2026-07-11)

Execution plan for the 4 findings approved after the `complete_reflect_job`
PK-scoping fix. Pre-production, no back-compat, KISS/DRY, long-term best
practice only. Ordered low-risk first; each step verified before the next via
`cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`,
and the scratch-DB suite
(`bash scripts/with_scratch_db.sh postgres://memphant:memphant@localhost:5432/memphant MEMPHANT_TEST_DATABASE_URL cargo test -p memphant-store-postgres -p memphant-worker -- --ignored --test-threads=1`).

## 1. Dead-code + doc hygiene (trivial)
- [x] Delete `_resource_id_type_anchor` (crates/memphant-core/src/service.rs:1507)
      and drop `ResourceId` from the line-15 import (it feeds only that fn).
- [x] Strike the stale `include_trace` bullet in the handoff (already absent from code).
- [x] Record the `review_event` PK decision: it is the lone tenant table with an
      id-only PK (migration 002 drops+recreates it). Recommendation: restore
      composite `(tenant_id, id)` to match convention IF the migration-boundary
      lint allows editing 002 in-place (pre-production, DBs are re-minted);
      otherwise document `review_event` + `api_key` as the deliberate id-only
      exceptions. Verify against scripts/check_memphant_migration_*.py first.

## 2. `begin()` -> `Result<Self::Txn, StoreError>` (mechanical, wide)
- [x] Trait (crates/memphant-core/src/lib.rs:450): return `Result`.
- [x] PgStore (crates/memphant-store-postgres/src/store.rs:464): drop `.expect`,
      map_err(backend), return Result.
- [x] InMemoryStore (crates/memphant-core/src/lib.rs:871): return `Ok`.
- [x] Every `.begin().await` call site propagates with `?` (compiler-guided).
- [x] Result: pool exhaustion / DB restart degrades to `backend_unavailable`
      503 instead of panicking the connection.

## 3. Embedding consistency (the real gap)
Vector channel silently drifts from authoritative units. Two sub-fixes, shared
pattern: embeddings written in the SAME transaction as the units they describe.
- [x] Reflect: compute embeddings BEFORE the persist tx (network call stays out
      of the lock) and thread the rows into `persist_compiled_units`
      (crates/memphant-core/src/lib.rs:5136 + trait/both impls) so units +
      embeddings + trace marker commit atomically. Kills the retry short-circuit
      (lib.rs:4901) that leaves units permanently unembedded after an embed hiccup.
- [x] Correct: embed the replacement unit inside the `apply_correction` tx
      (crates/memphant-store-postgres/src/store.rs:1013 + in-memory lib.rs:1317)
      so corrected truth is vector-visible.
- [x] Test-first (BDD): (a) a failing embedder does NOT leave the reflect job
      marked done with unembedded units on retry; (b) a corrected unit is
      returned by vector recall.

## 4. Partial-PK regression guard
- [x] In crates/memphant-store-postgres/tests/provider_lint.rs (or sibling):
      assert every `update`/`delete ... memphant.<table>` statement text in
      store.rs contains `tenant_id`, with a bounded allowlist — id-only tables
      (`tenant`, `api_key`, `schema_migrations`) and the 2 intentional global
      `job_state` sweeps. No false positives. Catches a future partial-key write.

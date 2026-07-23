# B2 Task 2 implementation report: serializable file-sync batch

## Result

Implemented the server-side atomic file-sync batch foundation: strict ordered
correct/direct-retain/forget plans, exact plan and base fingerprint checks, one
serializable transaction, one mutation-ledger claim, and one commit. No CLI,
filesystem apply logic, P1 artifact, model/provider, deployment, or push work
was performed.

## Test-first evidence

Before adding the contract, the focused command
`cargo test -p memphant-core file_sync -- --nocapture` failed to compile with
the expected missing behavior: unresolved `FileSync*` types, no
`MemoryService::file_sync`, and no stable `SyncInvalid`/`SyncConflict` errors.
The implementation followed those failing in-memory contracts and then added
the ignored scratch-Postgres contract for the durable transaction semantics.

## Implementation

- Added strict, unknown-field-denying tagged request and response types. The
  service verifies exact context identity, a non-empty ordered plan, lowercase
  SHA-256 values, canonical UTC timestamps, valid confidence/validity bounds,
  immutable target metadata, and unique unit/fact-key use before writes.
- Precomputes correction embeddings and existing direct-unit compiled writes,
  then starts the required `MemoryStore::begin_serializable` seam. Postgres
  executes `SET TRANSACTION ISOLATION LEVEL SERIALIZABLE` before tenant binding,
  claims the entire batch, and reads both projection fingerprints through the
  same transaction.
- Reuses the existing correction, compiled direct-retain, and forget staging
  operations in plan order. Native contradiction/supersession edges are
  preserved. Any stale base, operation error, or serialization failure rolls
  back and returns the stable file-sync error contract; there is no automatic
  retry against newer truth.
- Commits the ordered receipt with the mutation claim. An identical
  idempotency-key/request-hash replay returns the exact committed response.
- Added authenticated `POST /v1/file-sync`, stable `sync_invalid` (422) and
  `sync_conflict` (409) mappings, and regenerated OpenAPI through the server
  binary.
- Added `file_sync` to the single squashed pre-production bootstrap migration's
  mutation-ledger verb constraint. The live scratch test exposed this as a
  required schema contract; without it, the new mutation claim was rejected.

## Verification

- `cargo test -p memphant-types file_sync -- --nocapture` - 1 passed.
- `cargo test -p memphant-core file_sync -- --nocapture` - 5 passed, covering
  mixed correct/retain/forget, exact replay, native contradiction edges,
  ordered distinct retains, stale zero-write rejection, late failure rollback,
  and duplicate plan targets/fact keys.
- `cargo test -p memphant-server --test rest_contract file_sync -- --nocapture`
  - 1 passed, including strict decode, stale conflict, and validation error
  codes.
- `cargo test -p memphant-server openapi -- --nocapture` - 8 passed, including
  generated snapshot equality.
- `cargo test -p memphant-store-postgres provider_lint -- --nocapture` - 5
  passed.
- `bash scripts/with_scratch_db.sh postgres://memphant:memphant@localhost:5432/memphant MEMPHANT_TEST_DATABASE_URL cargo test -p memphant-store-postgres --test pg_store_contract file_sync_is_atomic_rejects_stale_base_and_serializes_concurrent_batches -- --ignored --exact --test-threads=1 --nocapture`
  - 1 passed against an ephemeral migrated Postgres database. This proves
  operation-N rollback, stale-base zero writes, and exactly one winner for two
  concurrent different batches compiled from the same base; this was not a
  skipped live check.
- `python3 scripts/check_memphant_migration_contract.py` - clean.
- `python3 -m pytest tests/test_wsa_migration_contract.py -q` - 35 passed, 1
  skipped.
- `cargo clippy -p memphant-types -p memphant-core -p memphant-runtime -p memphant-store-postgres -p memphant-server --all-targets --all-features -- -D warnings`
  - passed.
- `cargo fmt --check` and `git diff --check` - passed.
- `python3 scripts/check_spec_drift.py` did not verify drift: it reported
  exactly `spec_drift=skipped reason=private_specs_missing private=/Users/sidsharma/.codex/worktrees/Memphant/Syndai/docs/superpowers/specs/memphant`.
  Therefore this task makes no private-spec drift-clean claim.

## Files

- `crates/memphant-types/src/lib.rs`
- `crates/memphant-core/src/lib.rs`
- `crates/memphant-core/src/service.rs`
- `crates/memphant-runtime/src/lib.rs`
- `crates/memphant-store-postgres/src/store.rs`
- `crates/memphant-store-postgres/tests/pg_store_contract.rs`
- `crates/memphant-server/src/lib.rs`
- `crates/memphant-server/tests/rest_contract.rs`
- `memphant_migrations/versions/20260703_001_wsa_bootstrap.sql`
- `openapi/memphant.v1.json` (generated)
- `.superpowers/sdd/task-2-report.md`

## Commit

This report is included in the local Task 2 commit; its final SHA is reported
in the implementer handoff because a commit cannot embed its own SHA.

## Self-review and concerns

The task is intentionally limited to the atomic server batch. The exact live
Postgres contract and all focused gates above are green. Private spec mirroring
was unavailable at the recorded path, so drift was skipped rather than passed.
The unrelated `.superpowers/sdd/progress.md` modification remains preserved and
unstaged.

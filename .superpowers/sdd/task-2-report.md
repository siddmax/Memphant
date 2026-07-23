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

## Review follow-up: replay ordering and protected admission snapshot

### Red proof

The review regressions were added before their seams existed. Running
`cargo test -p memphant-core file_sync -- --nocapture` failed to compile on the
three missing structural contracts: `prepare_compiled_write_from_snapshot`,
`MemoryStore::fetch_scope_open_units_in_tx`, and
`file_sync_plan_sha256`. The replay regression uses a one-shot embedder whose
second call fails, so the former prepare-before-claim ordering would also fail
an exact replay instead of returning its stored receipt.

### Root-cause fixes

- File sync now opens its serializable transaction and claims the whole batch
  before any compiler or embedding work. A committed replay returns immediately;
  an executing request checks the in-transaction base and immutable metadata
  before preparation. Stale bases likewise perform no provider work.
- Added a mandatory transaction-scoped full-open-scope read to every store
  implementation. The compiler now has one shared snapshot-driven admission
  helper: existing native paths fetch their current full scope then delegate,
  while file sync supplies the snapshot read from its serializable transaction.
  Sequential plan operations see preceding staged changes through that same
  transaction.
- Added a concurrency regression with a belief that is open and admission-
  relevant but intentionally absent from the canonical file projection. The
  protected transaction snapshot stays stable, while an unprotected live read
  demonstrably changes the native edge decision; the in-memory serializable
  commit detects the concurrent context change.
- Added public `file_sync_plan_sha256`, used by the service and all Task 2 test
  request builders. Its typed ordered JSON digest is pinned to
  `7c3fc04bc305ea5a0a54deb5c4f96fbd305d6001cb902c82dbff4a80ffda80d9`
  for the fixed short-retain fixture.
- Explicit keyed direct facts now accept any nonblank body, including `Hi.` and
  `Busy.`. The historical three-word noise floor remains only for unkeyed
  extraction candidates, preserving the write-compiler golden contract.
- Expanded the single scratch-Postgres contract to assert a committed mixed
  correct/short-retain/forget batch, exact replay bytes, native contradicts and
  supersedes edges, operation-N rollback, stale-base zero writes, and exactly
  one winner from concurrent same-base batches.

### Follow-up verification

- `cargo test -p memphant-types file_sync -- --nocapture` - 1 passed.
- `cargo test -p memphant-core` - the full package passed, including 102 library
  tests and every integration/doc test; no ignored/paid provider lane was run.
- `cargo test -p memphant-server --test rest_contract file_sync -- --nocapture`
  - 1 passed.
- `bash scripts/with_scratch_db.sh postgres://memphant:memphant@localhost:5432/memphant MEMPHANT_TEST_DATABASE_URL cargo test -p memphant-store-postgres --test pg_store_contract file_sync_is_atomic_rejects_stale_base_and_serializes_concurrent_batches -- --ignored --exact --test-threads=1 --nocapture`
  - 1 passed against a real ephemeral migrated Postgres database; this was not
  skipped.
- `python3 scripts/check_memphant_migration_contract.py` - clean.
- `python3 -m pytest tests/test_wsa_migration_contract.py -q` - 35 passed, 1
  skipped.
- `cargo test -p memphant-store-postgres provider_lint -- --nocapture` - 5
  provider-lint tests passed.
- Touched-package all-target/all-feature clippy with `-D warnings`,
  `cargo fmt --check`, and `git diff --check` passed.

The API schema did not change in this follow-up, so the generated OpenAPI file
was intentionally not regenerated. The prior exact private-spec drift skip and
non-claim remain unchanged. No paid or network provider calls, Task 3 work,
push, or deployment were performed.

## Second review follow-up: bounded two-phase execution

This follow-up supersedes the preceding review section's claim-before-provider
ordering. The final architecture matches the Task 2 brief: provider/compiler
preparation happens before the short serializable execution transaction, and
the transaction revalidates every mutable input before staging any operation.

### Red proof

The new regressions were added before their production seams. The focused
`cargo test -p memphant-core file_sync -- --nocapture` command failed on the
missing read-only `lookup_mutation_replay`, missing full admission-snapshot
digest, and missing active-claim observation contract. Once those structural
seams compiled, the nonprojected-drift test still failed against the old
transaction shape because the embedding provider ran while the mutation claim
was active instead of returning the required drift conflict.

### Root-cause architecture

- `MutationLedgerStore` now has a tenant/context-bound, read-only committed
  replay lookup. Exact receipts return before snapshot or provider work;
  mismatched request hashes return the static idempotency conflict without
  including the raw key. In-memory, Postgres, and runtime delegation implement
  the same contract.
- File sync fetches the complete open-scope admission unit snapshot outside the
  execution transaction and hashes the full typed rows, deterministically
  ordered by unit UUID. This includes nonprojected beliefs and every other unit
  field the compiler may inspect, rather than only the canonical file view.
- Correction embeddings and direct-retain compiler writes are prepared outside
  the database transaction in plan order against a caller-owned evolving
  snapshot. Correction replacement/remainder UUIDs are allocated during
  preparation and passed to both store implementations, so later prepared
  operations reference the same unit identities that persistence creates.
- The serializable transaction now only stages the whole mutation claim,
  re-reads both the canonical projection fingerprint and the full admission
  snapshot fingerprint, stages the already-prepared native writes, records the
  receipt, and commits. Any drift returns `sync_conflict` and rolls back. No
  embedding, compiler, or network/provider work runs under the claim.
- Every fallible path after `begin_serializable` explicitly rolls back,
  including canonical/admission fingerprint encoding and final receipt
  serialization. A same-request race may still return the exact committed
  replay from `stage_mutation_claim`; this path commits a read-only replay
  transaction and performs no prepared writes.
- The paired short-body contract is explicit: keyed direct `Hi.`/`Busy.` units
  remain admitted, while unkeyed one- or two-word extraction candidates remain
  rejected by the historical noise floor.

### Final verification

- `cargo test -p memphant-core file_sync -- --nocapture` - 12 passed. This
  includes exact replay and conflicting-hash preflight, stale-base provider
  bypass, no-active-claim provider execution, nonprojected admission drift with
  zero requested writes/receipt, full-row digest coverage, ordered mixed plans,
  operation rollback, native contradiction edges, and the paired short-body
  cases.
- `cargo test -p memphant-core --all-targets` - 107 library tests plus every
  core integration target passed.
- `cargo test -p memphant-server --test rest_contract file_sync -- --nocapture`
  - 1 passed; `cargo test -p memphant-server openapi -- --nocapture` - 8 passed.
- The ignored scratch-Postgres
  `file_sync_is_atomic_rejects_stale_base_and_serializes_concurrent_batches`
  contract passed against an ephemeral migrated database, retaining exact
  replay, mixed native edge semantics, operation-N rollback, stale zero writes,
  and exactly one same-base concurrent winner.
- The new ignored scratch-Postgres
  `file_sync_rejects_nonprojected_admission_drift_before_claiming` contract also
  passed. A concurrent belief inserted during provider preparation changed no
  canonical file record but changed the admission digest; file sync returned
  `sync_conflict`, persisted no requested unit, and left no replay receipt.
- `cargo test -p memphant-types file_sync -- --nocapture` - 1 passed;
  `cargo test -p memphant-store-postgres provider_lint -- --nocapture` - 5
  passed.
- `python3 scripts/check_memphant_migration_contract.py` - clean;
  `python3 -m pytest tests/test_wsa_migration_contract.py -q` - 35 passed, 1
  skipped.
- Touched-package all-target/all-feature clippy with `-D warnings` passed.
  Final format and whitespace checks are recorded immediately before commit.
- `cargo test --doc` passed. An additional workspace-wide diagnostic,
  `cargo test --all-targets --all-features`, is **not** claimed green: the
  unrelated existing
  `memphant-eval::trace_schema_snapshot_is_current` test fails because the
  generated `RetrievalTrace` schema contains the already-present
  `CrossRerankGranularity`/`docs_scored` fields while its checked snapshot does
  not. The exact test also fails under default features. Task 2 changes no
  `memphant-types` trace shape or eval snapshot, so this unrelated drift was
  preserved rather than folded into the file-sync change.

The API schema and migration did not change, so generated OpenAPI and migration
artifacts were intentionally not modified. Private spec drift again reported
exactly `spec_drift=skipped reason=private_specs_missing`; no drift-clean claim
is made. The unrelated `.superpowers/sdd/progress.md` modification remains
preserved and unstaged. No paid/network provider, Task 3, push, or deployment
work was performed.

# B2 Task 1 implementation report: canonical projection read

## Result

Implemented the authenticated `GET /v1/scopes/{id}/projection` foundation for
the B2 file plane. It is distinct from the historical, paginated
`/v1/scopes/{id}/memory` endpoint and returns one complete, unranked snapshot.
No CLI sync, P1 artifact, or model/provider work was touched.

## Test-first evidence

1. Added the REST contract before the route existed and ran:
   `cargo test -p memphant-server --test rest_contract canonical_projection_is_a_dedicated_unranked_visible_snapshot -- --exact`.
   It failed as expected with `404 Not Found` for the missing dedicated route.
2. Added the in-memory store contract for the permitted projection states,
   historical states, closed/deletion-marked rows, and wrong context rows.
3. Added the encoded-byte-ceiling contract, temporarily without the size guard,
   and ran it. It failed as expected with `200` where `413` was required.
4. Implemented the shared store/service/server path, then ran the focused
   contracts green.

## Implementation

- Added typed `CanonicalProjectionUnit` and `CanonicalProjectionResponse`.
- Added a separate `MemoryStore::canonical_projection_units` seam. In-memory
  reads one locked snapshot; Postgres executes one ordered statement in the
  tenant transaction.
- The store includes semantic `active|validated` and procedural `validated`
  rows only, with exact tenant/subject/generation/scope/agent/actor binding;
  it excludes closed, deletion-marked, historical, quarantined, wrong-kind,
  and wrong-context rows.
- The service builds the SHA-256 from canonical JSON for UUID-ordered unit
  records, publishes each body SHA-256, and rejects encoded responses above
  `1_048_576` bytes with a stable `413 projection_too_large` error.
- Regenerated `openapi/memphant.v1.json` through the server binary. The API
  description states the byte ceiling and no-truncation behavior.

## Verification

- Red REST route proof: failed with the expected `404 Not Found`.
- Red byte-limit proof: failed with the expected observed `200` versus required
  `413` before the guard was restored.
- `cargo test -p memphant-types` — 6 passed.
- `cargo test -p memphant-core --lib service::canonical_projection_store_tests -- --nocapture` — 2 passed.
- `cargo test -p memphant-server --test rest_contract` — 21 passed.
- `cargo test -p memphant-store-postgres` — local/unit/provider checks passed;
  67 live-Postgres checks were skipped because `MEMPHANT_TEST_DATABASE_URL` was
  not configured.
- `cargo test -p memphant-runtime` — 137 passed; 7 live/paid provider tests
  skipped, with no model calls made.
- `cargo fmt --check` — passed.
- `cargo clippy -p memphant-types -p memphant-core -p memphant-runtime -p memphant-store-postgres -p memphant-server --all-targets --all-features -- -D warnings` — passed.
- `cargo run -q -p memphant-server -- --openapi-json > openapi/memphant.v1.json` — regenerated successfully.
- `git diff --check` — passed.

## Files

- `crates/memphant-types/src/lib.rs`
- `crates/memphant-core/src/lib.rs`
- `crates/memphant-core/src/service.rs`
- `crates/memphant-runtime/src/lib.rs`
- `crates/memphant-store-postgres/src/store.rs`
- `crates/memphant-server/src/lib.rs`
- `crates/memphant-server/tests/rest_contract.rs`
- `openapi/memphant.v1.json` (generated)
- `.superpowers/sdd/task-1-report.md`

## Commit

This report is included in the local Task 1 commit; its SHA is reported in the
implementer handoff.

## Self-review and concerns

The historical paginated export remains unchanged, and no response is silently
truncated. The exact Postgres statement is covered by compile/lint and the
shared in-memory contract; the live-Postgres suite was skipped because this
worktree has no configured scratch database. The unrelated
`.superpowers/sdd/progress.md` modification was preserved unstaged.

## Review follow-up: bitemporal and trust visibility

### Test-first evidence

1. Extended the in-memory projection contract with an `active` unit at
   `TrustLevel::Quarantined`, a future-valid unit, an expired unit, and a
   future-transaction unit. Before the predicate fix,
   `canonical_projection_store_excludes_historical_and_disallowed_units`
   failed with seven projected records where three were required.
2. Extended the REST projection contract to require `valid_from` and
   `valid_to`. Before the response mapping change it failed with
   `valid_from: Null` instead of `"2026-07-01T00:00:00Z"`.
3. Changed the fixed ordered-record fingerprint fixture to reverse its UUID
   ordering and include validity bounds. Its old fingerprint failed as
   expected; the intentional canonical JSON fingerprint is now
   `4b2e0c7f4801952ddf18abfb6136d9c7cbf83a50180f49fba66774e1bd568cb8`.

### Implementation

- `MemoryStore::canonical_projection_units` now takes exactly one RFC3339
  `evaluated_at` instant. `MemoryService` reads its clock once, sends that
  value to the store, and returns it in `CanonicalProjectionResponse`.
- Both stores now exclude `TrustLevel::Quarantined` and enforce bitemporal
  currentness at that instant. The Postgres implementation remains one ordered
  tenant transaction statement, filtering future `transaction_from`, closed
  `transaction_to`, future `valid_from`, and elapsed `valid_to` intervals.
- Projection units publish `valid_from`/`valid_to`; those fields are included
  in the ordered-record fingerprint.
- Added a fixed-clock service regression and an HTTP regression that commits
  two staged records in reverse order, proving the route returns UUID order.
- Regenerated OpenAPI. The endpoint and response schema now document and
  expose `evaluated_at` plus per-unit validity bounds.

### Verification

- `cargo test -p memphant-types` — 6 passed.
- `cargo test -p memphant-core --lib service::canonical_projection_store_tests -- --nocapture` — 3 passed.
- `cargo test -p memphant-server --test rest_contract` — 22 passed.
- `cargo test -p memphant-store-postgres` — local/unit/provider checks passed;
  67 live-Postgres checks skipped because `MEMPHANT_TEST_DATABASE_URL` is not
  configured.
- `cargo test -p memphant-runtime` — 137 passed; 7 live/paid provider checks
  skipped, with no model calls made.
- `cargo fmt --check`, scoped all-features clippy with `-D warnings`, and
  `git diff --check` — passed.
- `cargo run -q -p memphant-server -- --openapi-json > openapi/memphant.v1.json` — regenerated successfully.

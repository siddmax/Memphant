# Task 4 Report: Sync plan and apply

## Red proof

The new real-CLI/Axum contract was run before sync dispatch existed:

- cargo test -p memphant-cli --test file_plane_contract --no-fail-fast
- Result: 0 passed, 3 failed.
- Every failure reached the packaged CLI and returned the old usage text, which
  did not recognize sync. The red cases were the four-edit-class round trip,
  local format/immutable rejection, and stale-base conflict.

Wave 2 review reproduced the canonical-byte gap before the fix:

- cargo test -p memphant-cli --test file_plane_contract
  sync_fails_closed_on_immutable_and_inbox_format_edits -- --nocapture
- Result: failed with `manifest_whitespace was accepted`.
- The same test passed after compile/sync/verify shared the canonical serializer;
  its expanded table also proves strict inbox separators and coordinated body +
  manifest hash tampering remain zero-POST and byte-preserving in both dry-run
  and apply modes.

## Implementation

- Added sync as a first-class CLI command. It uses the same explicit context
  flags as compile, is a deterministic JSON dry run by default, and short
  circuits a validated empty apply without posting an empty server batch.
- Added one edit-aware capability scan. The manifest and generated index remain
  exact: manifest parsing is followed by byte equality against the one shared
  canonical serializer used by compile, sync, and verify. Managed paths may
  only have an exact body-range edit or be missing.
  Every present managed and inbox file retains its no-follow identity and exact
  bytes for revalidation before GET, immediately before POST, after the
  committed receipt/final GET, and again at the namespace mutation.
- Canonical operation order is base-manifest UUID order for correct/forget,
  followed by safe inbox-path order for retain. The digest comes from the shared
  typed file_sync_plan_sha256 contract. A fact key cannot be corrected or
  forgotten and retained in one plan; unchanged same-key inbox facts retain
  through native contradiction admission.
- Inbox grammar is exact LF Markdown: one trimmed H1 fact key, exactly one
  separator blank line, a nonblank first semantic-body line, exactly one final
  LF, and no MemPhant footer.
  Additions are semantic only. Matching fact keys inherit predicate/confidence;
  absent values use states/1.0.
- Apply sends one typed batch after the last exact local preflight. One fresh
  plan-digest-plus-UUID idempotency key is paired with the exact request for
  that invocation; no cross-invocation replay claim is made. The client and
  route share one exact encoded-request ceiling, and the transaction checks the
  final encoded projection ceiling before commit. Typed conflicts and
  validation failures are distinct. Transport and untyped 5xx failures after
  dispatch are outcome_unknown.
- One configurable bounded HTTP agent serves projection GET and file-sync POST
  (`MEMPHANT_HTTP_TIMEOUT_MS`, default 30 seconds, range 1..300 seconds). GET
  transport/timeout/5xx failures are unavailable. Only an exact POST 200 can
  prove commit; any other 2xx, invalid 200 receipt, or receipt mismatch is
  outcome_unknown.
- The committed receipt is checked for base, digest, count, operation variants,
  target IDs, and fingerprint before local mutation. A fresh bound canonical
  GET is then compiled. The CLI reports committed and final snapshots
  separately so a legitimate later writer is not mistaken for corruption.
  Every post-commit failure includes the proven committed snapshot.
- Consumed inbox inodes are never directly unlinked. The manifest-last writer
  prevalidates each planned path, atomically moves it with no-replace semantics
  into durable recovery/inbox, then validates the detached identity and bytes.
  That child has retained handle/root-name identity checks matching
  recovery/units. Source-path substitution and late descriptor writes fail
  post-commit with original and unexpected data preserved.
- Planned missing managed files are accepted as forget inputs and must remain
  absent; a reappearing path fails rather than being deleted. Corrected,
  superseded, forgotten, and newly retained units are then reconciled through
  the existing prepared-file, durable-recovery, manifest-last, double-sweep
  compiler.

## Green proof

- cargo test -p memphant-cli --all-features --no-fail-fast:
  64 passed (29 binary unit, 4 bootstrap, 10 compile, 14 file-plane, 4 HTTP,
  3 verify).
- cargo test -p memphant-core file_sync -- --nocapture:
  16 passed, including deterministic digest, contradiction order, stale-base
  zero-write, late-operation rollback, exact replay, ordered cascade parity,
  and final projection-ceiling rollback.
- cargo test -p memphant-server --test rest_contract file_sync -- --nocapture:
  2 passed, proving the strict authenticated atomic route, stable error codes,
  and exact request-body ceiling.
- cargo clippy --all-targets --all-features -- -D warnings; cargo test
  --all-targets --all-features; cargo test --doc: passed after the final code
  change. Guarded network, paid-model, and live-Postgres cases remained skipped
  in the ordinary workspace run rather than being counted as passes.
- The ephemeral live-Postgres ignored-test harness passed every selected store
  and worker contract, including all three file-sync Postgres contracts. The
  real-binary Postgres e2e probe also ended `ALL CHECKS PASSED`; both created and
  dropped scratch databases without touching the shared P1 database.
- Provider lint passed for plain-postgres, Supabase, and Neon; migration dry-run,
  cargo fmt --check, and git diff --check passed.
- python3 scripts/check_spec_drift.py: skipped, not passed, because the private
  Syndai specs are absent from this worktree.
- The repository Python gate is not green: its configured spike path is absent,
  and `python3 -m pytest tests -q` reported 696 passed, 10 skipped, 24 failures
  in unrelated campaign/runtime fixture pins and missing Playwright tooling.
  Task 4 does not claim those failures as passed or modify their owners.

The unrelated .superpowers/sdd/progress.md modification remains unstaged.
No P1 campaign artifact, paid/model call, push, deployment, B3 watcher, CRDT,
legacy source mode, or compatibility shim was added.

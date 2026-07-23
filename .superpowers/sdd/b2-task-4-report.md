# Task 4 Report: Sync plan and apply

## Red proof

The new real-CLI/Axum contract was run before sync dispatch existed:

- cargo test -p memphant-cli --test file_plane_contract --no-fail-fast
- Result: 0 passed, 3 failed.
- Every failure reached the packaged CLI and returned the old usage text, which
  did not recognize sync. The red cases were the four-edit-class round trip,
  local format/immutable rejection, and stale-base conflict.

## Implementation

- Added sync as a first-class CLI command. It uses the same explicit context
  flags as compile, is a deterministic JSON dry run by default, and short
  circuits a validated empty apply without posting an empty server batch.
- Added one edit-aware capability scan. The manifest and generated index remain
  exact; managed paths may only have an exact body-range edit or be missing.
  Every present managed and inbox file retains its no-follow identity and exact
  bytes for revalidation before GET, immediately before POST, after the
  committed receipt/final GET, and again at the namespace mutation.
- Canonical operation order is base-manifest UUID order for correct/forget,
  followed by safe inbox-path order for retain. The digest comes from the shared
  typed file_sync_plan_sha256 contract. A fact key cannot be corrected or
  forgotten and retained in one plan; unchanged same-key inbox facts retain
  through native contradiction admission.
- Inbox grammar is exact LF Markdown: one trimmed H1 fact key, one blank line,
  a nonblank semantic body, exactly one final LF, and no MemPhant footer.
  Additions are semantic only. Matching fact keys inherit predicate/confidence;
  absent values use states/1.0.
- Apply sends one typed batch after the last exact local preflight. One fresh
  plan-digest-plus-UUID idempotency key is paired with the exact request for
  that invocation; no cross-invocation replay claim is made. Typed conflicts
  and validation failures are distinct. Transport and untyped 5xx failures
  after dispatch are outcome_unknown.
- The committed receipt is checked for base, digest, count, operation variants,
  target IDs, and fingerprint before local mutation. A fresh bound canonical
  GET is then compiled. The CLI reports committed and final snapshots
  separately so a legitimate later writer is not mistaken for corruption.
- Consumed inbox inodes are never directly unlinked. The manifest-last writer
  moves only the planned, identity/byte-matched paths with no-replace semantics
  into durable recovery/inbox. That child has retained handle/root-name
  identity checks matching recovery/units. Substitutions and late descriptor
  writes fail post-commit with the data retained in recovery.
- Planned missing managed files are accepted as forget inputs and must remain
  absent; a reappearing path fails rather than being deleted. Corrected,
  superseded, forgotten, and newly retained units are then reconciled through
  the existing prepared-file, durable-recovery, manifest-last, double-sweep
  compiler.

## Green proof

- cargo test -p memphant-cli --all-features --no-fail-fast:
  56 passed (27 binary unit, 4 bootstrap, 10 compile, 8 file-plane, 4 HTTP,
  3 verify).
- cargo test -p memphant-core file_sync -- --nocapture:
  15 passed, including deterministic digest, contradiction order, stale-base
  zero-write, late-operation rollback, exact replay, and ordered cascade parity.
- cargo test -p memphant-server --test rest_contract file_sync -- --nocapture:
  1 passed, proving the strict authenticated atomic route and stable error codes.
- cargo clippy -p memphant-cli --all-targets --all-features -- -D warnings:
  passed after the final code change.
- cargo fmt --all -- --check and git diff --check: passed.
- python3 scripts/check_spec_drift.py: skipped, not passed, because the private
  Syndai specs are absent from this worktree.

The unrelated .superpowers/sdd/progress.md modification remains unstaged.
No P1 campaign artifact, paid/model call, push, deployment, B3 watcher, CRDT,
legacy source mode, or compatibility shim was added.

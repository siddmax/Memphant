# Task 3 Report: Deterministic compiler and verifier

## Red proof

`cargo test -p memphant-cli --test compile_contract` built the replacement
server-backed contract, then failed 3/3 tests. Each failure reached the real
CLI binary and reported the old fixed-position usage because the only compiler
still required the deleted fixture-only `--source` path.

## Implementation

- Added `file_plane.rs` as the single compiler/validator seam. `compile` accepts
  the five explicit context-binding flags and `--out` in any order, fetches the
  dedicated authenticated canonical projection, validates its returned binding
  and SHA-256 fingerprint, renders every byte before writing, and never accepts
  `--source`.
- The exact output is `MEMORY.md`, `units/<lowercase canonical UUID>.md`, an
  empty `inbox/`, and `memphant-export.json`. Unit footers and the manifest are
  typed, deny unknown fields, reject duplicate JSON keys, and bind immutable
  metadata plus body/file/index SHA-256 digests. Volatile `evaluated_at` is not
  persisted, so identical canonical snapshots compile byte-identically.
- Compile validates an existing non-empty tree before the projection request
  or first replacement. Changed, missing, duplicate, traversal, unmanaged,
  malformed, immutable-footer, generation, and symlink states fail closed with
  every finding and the sync-or-restore instruction.
- All rendered managed files use same-directory `create_new` temporary files,
  `sync_all`, atomic rename, and parent-directory sync. Stale cleanup is limited
  to paths from the previously validated manifest, and the manifest is always
  replaced last.
- `verify --lock ... --export ...` now uses the same complete validator rather
  than the old source-file/FNV check. The obsolete source fixture, FNV helper,
  safe filename rewriting, and fixture exporter were deleted. No API schema
  changed, so no generated OpenAPI/MCP artifact was regenerated.

## Green proof

- `cargo test -p memphant-cli --test compile_contract`: 3 passed. The real CLI
  and in-memory Axum server prove exact tree/footers/manifest, historical-row
  exclusion, arbitrary flag order, deterministic repeated bytes, clean verify,
  dirty body/MEMORY/missing/unmanaged/duplicate-entry/duplicate-key/traversal/
  footer/generation refusal, managed symlink refusal, and removal of `--source`.
- `cargo test -p memphant-cli`: 14 integration tests passed across compile,
  verify/lock, HTTP memory verbs, and provider bootstrap checks.
- `cargo clippy -p memphant-cli --all-targets -- -D warnings`,
  `cargo fmt --all --check`, and `git diff --check` passed.

The unrelated `.superpowers/sdd/progress.md` modification remains unstaged.
No Task 4, P1 campaign, paid/model call, push, or deployment work was performed.

## Independent-review fix wave

The task review found that a self-consistent forged manifest could redefine the
offline base, pathname reads could follow a non-regular replacement after
recording an lstat finding, and compile did not preserve the exact validated
tree across its network request. New red contracts reproduced both coordinated
semantic forgery and a deterministic mid-request local edit overwrite before
the fix.

- Clean validation now rebuilds the exact `MEMORY.md`, reconstructs typed
  `CanonicalProjectionUnit` records from strict footers plus bodies, and uses
  the shared canonical fingerprint helper to bind body, validity, and immutable
  metadata to `snapshot_sha256`. UUIDs, semantic/procedural kinds, fact keys,
  predicates, confidence, UTC validity ranges, footer generation, and requested
  plus returned server context are validated explicitly.
- Validation retains root, units, and inbox directory descriptors and a digest
  of their identities, names, and exact managed bytes. Immediately after the
  projection response and render, it revalidates through those same handles;
  any concurrent edit or path-identity change fails before the first mutation.
- Managed reads use bounded `openat` with `O_NOFOLLOW|O_NONBLOCK`, then `fstat`
  before reading. Directory enumeration is capability-relative through
  `cap-std` over duplicated retained descriptors. FIFOs, symlinks, oversized
  files, and other non-regular objects therefore fail without following or
  blocking.
- New trees walk from a resolved existing parent with descriptor-relative
  `openat`/`mkdirat`. Existing and new writes use same-directory `create_new`
  descriptors, `renameat`, `unlinkat`, and directory fsync. Root/units/inbox
  identities are checked before mutation and again before success, so a swapped
  path cannot redirect output or stale cleanup.
- The negative CLI matrix now separately covers duplicate IDs, paths, and fact
  keys; duplicate footer JSON; coordinated snapshot/context/validity/body/index
  forgery; all managed root/directory/file symlink positions; FIFO nonblocking;
  concurrent edits; and delayed root/units redirection with outside sentinels
  unchanged. The shared inbox-name predicate rejects reserved export names and
  Windows device-style components. The fallback usage includes `compile`.

Fresh fix-wave proof: `cargo test -p memphant-cli` passed one unit test and 19
integration tests, including the 8 real-CLI compile contracts. CLI all-target
clippy with `-D warnings` and format passed after the last code change.

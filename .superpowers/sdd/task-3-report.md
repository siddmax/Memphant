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

## Second independent-review fix wave

The follow-up review identified three remaining release blockers: absent and
empty output trees were not retained as exact pre-request states; per-file
replacement/deletion did not recheck the validated file immediately before the
mutation or validate the complete rendered tree afterward; and the direct
Unix-only `rustix` backend prevented Windows compilation.

Two delayed real-CLI contracts were first run against pre-fix commit
`87253539`. Both failed behaviorally: an absent output that appeared during the
projection request was overwritten, and an empty output swapped during that
request was accepted and initialized. The same-inode writer contracts then
failed against the identity-only intermediate implementation, proving that an
in-place edit could still be overwritten or deleted without changing its inode.

- Compile now carries one explicit state across the network request:
  `Absent(retained parent capability + identity + missing components)`,
  `Empty(retained parent/root capabilities + identities + empty-name
  snapshot)`, or `Existing(ValidatedExport)`. It revalidates that exact state
  before creating a directory or writing a byte. Delayed tests prove appeared
  and swapped roots fail with parent sentinels and both original/replacement
  trees unchanged.
- The filesystem backend now uses portable `cap-std` and `cap-fs-ext`
  capability operations for no-follow directory/file opens, portable
  device/inode identities, nonblocking reads, relative rename/removal, and
  directory sync. Direct `rustix`, `OwnedFd`, pathname `openat`, and other
  Unix-only production code were removed. Every newly created component is
  synced through its retained parent before traversal continues.
- Existing validation retains the exact bytes and identity of every managed
  file. Immediately before each atomic replacement or stale deletion, the
  current no-follow regular file is reopened and both identity and bytes must
  still match (or the path must still be absent). Deterministic tests replace a
  future unit and a stale unit in place during the write sequence and prove the
  sentinel is preserved.
- The manifest remains the final replacement. After its directory sync, the
  validator rereads the exact names, types, bytes, hashes, metadata, and
  capability bindings through the retained handles and compares the resulting
  manifest with the rendered canonical manifest. A deterministic
  post-manifest tamper test proves this final gate fails closed.

Fresh proof after the final code change:

- `cargo test -p memphant-cli`: 4 unit and 21 integration tests passed,
  including 10 real-CLI compile contracts.
- `cargo clippy -p memphant-cli --all-targets --all-features -- -D warnings`,
  `cargo fmt --all --check`, and `git diff --check`: passed.
- `python3 scripts/check_spec_drift.py`: skipped, not passed, because the
  private Syndai specs are absent from this worktree.
- `rustup target add x86_64-pc-windows-msvc`: installed the Windows standard
  library. `cargo check -p memphant-cli --target x86_64-pc-windows-msvc` is
  externally blocked before MemPhant source compilation: transitive
  `ring 0.17.14` invokes host `cc` for MSVC and cannot find the Windows SDK
  `assert.h`; this host has no `VCINSTALLDIR` or Windows cross-C toolchain.
  `cargo tree -p memphant-cli --target x86_64-pc-windows-msvc -i ring` traces
  the blocker through existing `rustls` consumers (`ureq`, `sqlx`, and
  runtime dependencies), not the file-plane capability backend.

The unrelated `.superpowers/sdd/progress.md` modification remains unstaged.
No Task 4, P1 campaign, paid/model call, push, or deployment work was performed.

## Third independent-review fix wave

The final Task 3 review found three narrower compare/use gaps. An absent output
was still assembled through its final pathname before it was complete; managed
replacement and stale cleanup checked a name before a later namespace mutation;
and one exact validation pass could not establish that the completed tree had
stabilized. The same review also found that canonical procedural units may omit
predicate and confidence, and that `cap_std::Dir::rename` is replacing rather
than no-replace on Windows.

New deterministic hooks first reproduced all of these failures: a concurrent
root appeared at the absent install point, a managed target reappeared between
detach and prepared install, a stale target reappeared between detach and
deletion, and a managed unit changed between the two intended final sweeps. A
canonical procedural unit with absent optional predicate/confidence was also
rejected before the metadata correction.

- An absent output is now built as a unique sibling staging tree beneath the
  retained existing parent. Every missing descendant, `units/`, `inbox/`, and
  rendered byte is created and validated through retained capabilities before
  one atomic no-replace install of the first missing component. If the final
  name appears, it is left untouched and the fully validated staging tree is
  retained for recovery. Nested absent paths use the same single install.
- Managed replacement now syncs a unique prepared file, atomically detaches the
  validated name to a unique no-replace backup, validates the detached identity
  and exact bytes, then installs the prepared file with no-replace semantics.
  A name that reappears is never overwritten and the prior validated backup is
  retained. Stale cleanup uses the same detach-and-validate protocol and only
  removes the detached backup if the original name remains absent.
- Final validation performs two complete, exact sweeps of manifest, canonical
  snapshot, capability bindings, managed identities, and managed bytes. The
  two results must match, and no projection write occurs after the second
  sweep. Manifest validation now requires fact key, predicate, and confidence
  equality only when each canonical field is present.
- Audited no-replace backends are target-specific. Supported Unix targets use
  `rustix::fs::renameat_with(..., RenameFlags::NOREPLACE)`. Windows opens the
  source without following reparse points and with `DELETE` access relative to
  the retained capability, then calls `SetFileInformationByHandle` with
  `FileRenameInfo`, the retained directory handle as `RootDirectory`, and
  `ReplaceIfExists=false`. Both Windows target-exists codes normalize to
  `AlreadyExists`. This follows Microsoft's
  [`SetFileInformationByHandle`](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-setfileinformationbyhandle)
  and [`FILE_RENAME_INFO`](https://learn.microsoft.com/en-us/windows/win32/api/winbase/ns-winbase-file_rename_info)
  contracts. Windows syncs every prepared file but deliberately does not call
  unsupported `FlushFileBuffers` on cap-std's read-only directory handles.

Fresh proof after the final code change:

- `cargo test -p memphant-cli`: 10 unit and 21 integration tests passed. The
  added unit contracts cover absent-root collision preservation, atomic nested
  install, write and stale-delete compare/use collisions with recoverable
  backups, exact post-manifest validation, the stable second sweep, and
  optional procedural metadata.
- `cargo clippy -p memphant-cli --all-targets --all-features -- -D warnings`,
  `cargo fmt --all --check`, and `git diff --check`: passed.
- `python3 scripts/check_spec_drift.py`: skipped, not passed, because the
  private Syndai specs are absent from this worktree.
- `cargo check -p memphant-cli --target x86_64-pc-windows-msvc`: still stops
  before MemPhant source compilation because transitive `ring 0.17.14` invokes
  the host C compiler without the MSVC SDK (`assert.h` is missing and
  `VCINSTALLDIR=None`). To isolate the new code from that unrelated dependency,
  a temporary crate with the exact Windows function and the same
  `cap-std 4.0.2`, `cap-fs-ext 4.0.2`, and `windows-sys 0.61.2` dependencies was
  checked with
  `RUSTUP_TOOLCHAIN=1.96.1-aarch64-apple-darwin cargo check --target x86_64-pc-windows-msvc`;
  it passed. This proves Windows-target source compilation only. No Windows
  runtime rename test was executed on this macOS host, so runtime behavior is
  not claimed.

The unrelated `.superpowers/sdd/progress.md` modification remains unstaged.
No Task 4, P1 campaign, paid/model call, push, or deployment work was performed.

## Fourth independent-review fix wave

The final recovery audit found that a successful replacement or stale deletion
still unlinked its detached backup. An editor that had opened the old inode
before compile could write through that descriptor after validation, and those
bytes would disappear when the descriptor closed. The Windows absent-output
path also retained handles inside the populated staging subtree during its
directory rename, which violates Windows rename constraints, and every atomic
install error was incorrectly described as an output collision.

The new tests were red before this fix wave. Replacement and deletion through a
pre-opened writable descriptor had no durable recovery destination, a
byte-identical compile replaced the managed inode, a later stability-sweep
error omitted recovery provenance, a non-collision staging failure used the
collision message, and the real CLI did not report a recovery path after a
canonical change.

- Changed and deleted validated inodes now move directly, with atomic
  cross-directory no-replace semantics, into one lazy unique sibling
  `.memphant-recovery-<uuid>/`. The original layout is preserved as
  `MEMORY.md`, `memphant-export.json`, and `units/<uuid>.md`; no same-directory
  disposable backup remains. Source and recovery directories are synced where
  supported, `EXDEV` fails closed without copying, and B2 never prunes a
  recovery tree.
- Recovery is created only for the first changed or deleted managed file. Its
  path is canonical and absolute. Unix creates the root and `units/` with mode
  `0700`; Windows uses the output parent's inherited ACL and makes no stronger
  privacy claim. Successful compile and every subsequent error, including
  final validation failures, report `recovery=<absolute path>`. Operators may
  remove recovery only after processes holding old files open have closed.
- A byte-identical candidate is not rewritten. Compile freshly rereads its
  no-follow regular file at that skip point and requires both the previously
  validated identity and exact bytes to match. A no-op compile therefore keeps
  every managed inode and creates no recovery directory.
- The deterministic `write:<name>:recovered` and
  `delete:<name>:recovered` hooks run only after the moved inode passed identity
  and byte validation. Tests then rewrite through a descriptor opened before
  compile and prove successful compile retains the late bytes at the durable
  recovery path. Existing `:detached` hooks continue to prove target
  reappearance is not overwritten.
- `rename_noreplace` now accepts distinct retained source and target directory
  capabilities. Unix passes both descriptors to `renameat_with`; Windows opens
  the source relative to its retained directory and supplies the distinct
  target handle as `FILE_RENAME_INFO.RootDirectory`. This matches the
  [POSIX `renameat` descriptor contract](https://pubs.opengroup.org/onlinepubs/9799919799/functions/rename.html),
  [`rustix` no-replace API](https://docs.rs/rustix/latest/rustix/fs/fn.renameat_with.html),
  and Microsoft's
  [`FILE_RENAME_INFO`](https://learn.microsoft.com/en-us/windows/win32/api/winbase/ns-winbase-file_rename_info)
  contract.
- On Windows only, the fully rendered staging tree is validated and digested,
  then every handle into that subtree is dropped before the no-replace rename.
  The installed tree is reopened component-by-component from the retained
  external parent; its first-component identity and two exact validation
  sweeps must equal the staged snapshot. Unix continues retaining its stronger
  subtree handles across the rename. The reopen protocol has a cross-platform
  unit contract, plus Windows-only install and distinct-directory rename
  contracts. Only target compilation, not Windows runtime execution, is
  claimed on this macOS host.
- Only `AlreadyExists` maps to output-collision wording. Unsupported and other
  OS failures now state that the atomic install failed and name the retained
  validated staging tree.

Fresh proof after the final code change:

- `cargo test -p memphant-cli`: 14 unit and 21 integration tests passed,
  including 10 real-CLI compile contracts.
- `cargo clippy -p memphant-cli --all-targets --all-features -- -D warnings`,
  `cargo fmt --all --check`, and `git diff --check`: passed.
- `python3 scripts/check_spec_drift.py`: skipped, not passed, because the
  private Syndai specs are absent from this worktree.
- `cargo check -p memphant-cli --target x86_64-pc-windows-msvc`: still stops
  before MemPhant source compilation in transitive `ring 0.17.14` because this
  macOS host has no MSVC SDK (`assert.h` missing, `VCINSTALLDIR=None`). A
  temporary isolated crate containing the exact distinct-directory Windows
  backend plus the close-stage/reopen-parent protocol passed
  `RUSTUP_TOOLCHAIN=1.96.1-aarch64-apple-darwin cargo check --target x86_64-pc-windows-msvc`.
  This is Windows-target compilation evidence only; no Windows runtime result
  is claimed.

The approved B2 design now records the durable recovery, no-op, permissions,
operator-cleanup, and Windows handle lifecycle contracts. The implementation
plan did not need a scope or task-boundary change.

The unrelated `.superpowers/sdd/progress.md` modification remains unstaged.
No Task 4, P1 campaign, paid/model call, push, or deployment work was performed.

## Fifth independent-review fix wave

The final pathname audit found that durable recovery derived its absolute path
by canonicalizing `output_root.parent()` again after inspection, even though
all mutations continued through the retained original parent capability. If an
ancestor was renamed and replaced, compile could mutate the detached original
tree, return success, and label a path under the replacement parent as current
recovery.

Two deterministic contracts were red before this fix. A parent replacement at
the intended pre-mutation seam was never observed and compile succeeded; a
parent move after the first recovered inode also succeeded and returned a stale
`recovery=` path. A third race contract covers a component renamed after it was
opened during pathname traversal but before the reopen completed.

- Inspection now captures one canonical absolute existing output-parent anchor
  and its identity. `AbsentOutput`, `EmptyOutput`, and all staged, installed,
  and existing `TreeHandles` carry that same anchor and a retained anchor
  handle. Recovery derives its path only from this captured value; no mutation
  path canonicalizes the parent again.
- Before every namespace mutation and during both final validation sweeps,
  compile reopens the exact captured anchor from the filesystem/share root. It
  opens each normal component separately with `open_dir_nofollow`, retains each
  parent/name/identity binding, revalidates every binding after reaching the
  final directory, and compares that identity with the inspection-time retained
  parent. The deterministic during-walk rename contract proves traversal cannot
  continue through a detached component and falsely accept it.
- A parent change before the first mutation now fails with zero writes and no
  recovery. A parent change after recovery begins makes success impossible;
  cleanup also refuses further namespace mutation through the detached handles.
  Automatic restoration is removed entirely in the ninth review wave.
- `recovery=<absolute path>` is emitted only after both the captured anchor and
  recovery parent/name still identity-match the retained recovery handle. If
  that cannot be established, the error instead emits
  `recovery_last_known=<captured path>` plus `output parent changed; recovery
  was retained under that parent`. It never presents the stale pathname as
  current.
- The path splitter preserves Windows disk, verbatim-disk, UNC, and
  verbatim-UNC filesystem/share roots before no-follow component traversal.
  Current Windows identity evidence remains limited: `cap-fs-ext` supplies the
  volume serial plus 64-bit file index, while Microsoft's
  [`BY_HANDLE_FILE_INFORMATION`](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/ns-fileapi-by_handle_file_information)
  documentation says that identifier is not guaranteed unique on ReFS. Native
  Windows runtime proof and 128-bit `FileIdInfo` support for ReFS remain release
  predicates; no general Windows filesystem identity claim is made.
- The implementation follows `cap-std`'s retained-directory capability model,
  the documented POSIX trailing-component `O_NOFOLLOW` boundary, and Windows
  `FILE_FLAG_OPEN_REPARSE_POINT`; the component-by-component walk closes the
  earlier-component gap rather than relying on a multi-component no-follow
  open.

Fresh proof after the final code change:

- `cargo test -p memphant-cli`: 17 unit and 21 integration tests passed,
  including 10 real-CLI compile contracts.
- `cargo clippy -p memphant-cli --all-targets --all-features -- -D warnings`,
  `cargo fmt --all --check`, and `git diff --check`: passed.
- `python3 scripts/check_spec_drift.py`: skipped, not passed, because the
  private Syndai specs are absent from this worktree.
- `cargo check -p memphant-cli --target x86_64-pc-windows-msvc` remains blocked
  before MemPhant source compilation by transitive `ring 0.17.14` because this
  macOS host lacks the MSVC SDK (`assert.h` missing, `VCINSTALLDIR=None`). A
  temporary isolated crate containing the exact anchor splitter, per-component
  no-follow walker, retained binding revalidation, and Windows verbatim path
  inputs passed
  `RUSTUP_TOOLCHAIN=1.96.1-aarch64-apple-darwin cargo check --target x86_64-pc-windows-msvc`.
  This is source-compilation evidence only; no Windows runtime result is
  claimed.

The unrelated `.superpowers/sdd/progress.md` modification remains unstaged.
No Task 4, P1 campaign, paid/model call, push, or deployment work was performed.

## Sixth independent-review fix wave

The final diagnostic review found that `last_known_recovery` conflated three
states: an output-parent anchor change, an unconfirmed recovery path, and a
recovery tree that actually contains a moved managed inode. Consequently a
generic recovery setup or metadata failure falsely claimed both that the parent
changed and that recovery data had been retained, even when the recovery tree
was still empty.

Two focused contracts were red before this correction. An injected unchanged-
parent recovery fsync cause gained false parent-move and retained-data wording,
and displacing an empty recovery directory before the first managed move was
not observed because no deterministic pre-move confirmation seam existed.

- `RecoverySession` now records managed-data presence separately from pathname
  confidence. The bit flips immediately after the atomic move succeeds and
  before directory fsync, so even a later durability-barrier failure describes
  the actual inode location truthfully.
- The deterministic `recovery:created` seam runs after the recovery root and
  `units/` exist but before the first managed inode moves. Compile reconfirms
  the retained recovery name after that seam and refuses the move when the name
  was displaced.
- Unconfirmed recovery diagnostics preserve the actual fsync, open, metadata,
  handle, name, or path error and add only
  `recovery_last_known=<captured path>`. They do not invent `output parent
  changed`.
- Retained-data wording is conditional on the explicit moved-inode bit. The
  existing parent-move-after-first-recovery contract still reports
  `output parent changed; recovery was retained under that parent`; an empty
  recovery never makes that claim.

Fresh proof after the final code change:

- `cargo test -p memphant-cli`: 19 unit and 21 integration tests passed,
  including 10 real-CLI compile contracts.
- `cargo clippy -p memphant-cli --all-targets --all-features -- -D warnings`,
  `cargo fmt --all --check`, and `git diff --check`: passed.
- `python3 scripts/check_spec_drift.py`: skipped, not passed, because the
  private Syndai specs are absent from this worktree.

The unrelated `.superpowers/sdd/progress.md` modification remains unstaged.
No Task 4, P1 campaign, paid/model call, push, or deployment work was performed.

## Seventh independent-review fix wave

The final recovery-tree audit found two remaining precision gaps. Recovery
confirmation retained a `units` directory handle but validated only the root,
so a renamed-and-replaced `recovery/units` binding could route the first moved
inode into a detached directory. The retained-data boolean also stayed true
after a successful no-replace restoration emptied recovery, allowing later
diagnostics to claim data remained there.

Both deterministic contracts were red first. Replacing `recovery/units` at the
existing `recovery:created` seam was accepted and compile succeeded; corrupting
a recovered inode to force successful restoration, then displacing the empty
recovery at the new post-restore seam, produced a current `recovery=` result
instead of truthful last-known/empty state.

- `RecoveryArea` now retains the `units` identity. Every recovery confirmation
  verifies the units handle identity and the `root/units` name binding in
  addition to the anchor, recovery-root handle, and parent/name binding.
  Replacing `units/` therefore fails before the source inode moves, leaves both
  displaced and replacement units directories empty, and reports the actual
  units-binding cause without parent-move or retained-data wording.
- The boolean is replaced by an exact retained-managed-inode count. A successful
  atomic move increments it before either directory fsync. This wave initially
  decremented after a successful restoration; the ninth review wave removes
  restoration and makes the count monotonic for the compile operation.
- Deterministic `write:<name>:restored` and `delete:<name>:restored` seams fire
  only after the intermediate namespace restoration succeeded. Those seams and
  the restoration path are removed in the ninth review wave.

Fresh proof after the final code change:

- `cargo test -p memphant-cli`: 21 unit and 21 integration tests passed,
  including 10 real-CLI compile contracts.
- `cargo clippy -p memphant-cli --all-targets --all-features -- -D warnings`,
  `cargo fmt --all --check`, and `git diff --check`: passed.
- `python3 scripts/check_spec_drift.py`: skipped, not passed, because the
  private Syndai specs are absent from this worktree.

The unrelated `.superpowers/sdd/progress.md` modification remains unstaged.
No Task 4, P1 campaign, paid/model call, push, or deployment work was performed.

## Eighth independent-review fix wave

The final restoration audit found that successful validation failure cleanup
still resolved the recovery source only by name. If the recovered original was
renamed aside and an impostor was planted at its old name, cleanup moved the
impostor into output and decremented the count for the original inode that
remained in recovery.

The deterministic contract was red first. At the detached seam it renames the
original aside, plants a replacement at the recovery source name, and displaces
the recovery root. Before the fix, compile reported `validated name restored`,
installed the impostor into output, and omitted retained-data wording from the
last-known recovery diagnostic.

- Restoration now receives the recovered inode identity from the clean
  snapshot. It opens the recovery source without following links, retains that
  regular-file handle through the operation, and requires the handle identity
  to match the expected recovered inode.
- Immediately before the no-replace rename, restoration also requires the
  recovery source name to resolve to the same identity. A missing, renamed,
  non-regular, symlinked, or substituted source fails without a rename or count
  decrement.
- The regression contract proves the impostor remains in the displaced
  recovery tree, the original remains recoverable under its displaced name,
  output receives neither file, and the error reports only the last-known path
  plus truthful retained-managed-data wording.

This intermediate compare-then-rename mitigation is superseded by the ninth
review wave because it could not bind the path-based rename atomically to the
retained handle on every supported platform.

Fresh proof after the final code change:

- `cargo test -p memphant-cli`: 22 unit and 21 integration tests passed,
  including 10 real-CLI compile contracts.
- `cargo clippy -p memphant-cli --all-targets --all-features -- -D warnings`,
  `cargo fmt --all --check`, and `git diff --check`: passed.
- `python3 scripts/check_spec_drift.py`: skipped, not passed, because the
  private Syndai specs are absent from this worktree.

The unrelated `.superpowers/sdd/progress.md` modification remains unstaged.
No Task 4, P1 campaign, paid/model call, push, or deployment work was performed.

## Ninth independent-review fix wave

The final restoration review identified a compare/use window left by the eighth
wave: after the retained handle and recovery name identities matched, another
process could substitute the name before the path-based rename. The supported
cross-platform APIs do not provide a portable atomic no-replace move bound to
the already validated source handle.

The deterministic contract was red first. It corrupts the recovered inode to
force detached-file validation failure, expects a post-validation seam, then
renames the recovered original aside and plants an impostor. Before the fix,
the seam never ran and compile reported `validated name restored without
replacement`, proving the automatic restore path remained active.

- Automatic restoration on detached-file validation failure is removed. There
  is no compare-then-path-rename fallback and no recovery-source namespace
  mutation after validation fails.
- The retained-managed-inode count is monotonic for a compile operation. It
  increments immediately after each successful recovery move and is never
  decremented by validation-failure cleanup.
- Deterministic `write:<name>:validation_failed` and
  `delete:<name>:validation_failed` seams expose the former restore boundary.
  The regression contract plants an impostor there and proves output remains
  absent while both the displaced original and impostor stay in the confirmed
  durable recovery tree.
- At this wave, cleanup still removed the unrelated prepared pathname when the
  output anchor was current. The tenth review wave removes that unsafe
  path-based unlink. Recovery diagnostics remain confirmed-path or truthful
  last-known plus retained-data wording.

Fresh proof after the final code change:

- `cargo test -p memphant-cli`: 23 unit and 21 integration tests passed,
  including 10 real-CLI compile contracts.
- `cargo clippy -p memphant-cli --all-targets --all-features -- -D warnings`,
  `cargo fmt --all --check`, and `git diff --check`: passed.
- `python3 scripts/check_spec_drift.py`: skipped, not passed, because the
  private Syndai specs are absent from this worktree.

The unrelated `.superpowers/sdd/progress.md` modification remains unstaged.
No Task 4, P1 campaign, paid/model call, push, or deployment work was performed.

## Tenth independent-review fix wave

The final cleanup audit found the same compare/use defect in failure-path
prepared-file deletion. After a failure seam or check, compile called
`remove_file` by the prepared name; a concurrent process could rename the real
prepared inode aside, plant an unrelated file at that name, and have MemPhant
delete the unrelated file.

The deterministic contract was red first. At the post-validation-failure seam
it renames MemPhant's prepared inode aside, plants a byte-distinct sentinel at
the exact prepared name, and adds a second unrelated unknown file. Before the
fix, the sentinel disappeared with `NotFound`, proving pathname cleanup deleted
the replacement.

- All five failure-path `remove_file(&prepared)` calls are removed. No
  production `remove_file` call remains in the file-plane implementation.
- The audit also covered three outer post-prepare `?` exits and the prepared
  file's own write, sync, and metadata failures, all of which retained the
  temporary name without saying so. Every failure after successful creation and
  before the atomic install now reports
  `prepared_name_last_known=<relative-name>` plus the portable
  handle-bound-unlink cleanup-skipped reason.
- The diagnostic is deliberately last-known: it does not claim that the name
  still resolves to MemPhant's inode or that MemPhant owns whatever currently
  occupies it. Normal success still atomically consumes the prepared name with
  no-replace rename and retains its existing post-install identity check.
- The regression contract proves the planted prepared-name sentinel and a
  second unknown file survive byte-identically, MemPhant's prepared inode
  remains under its displaced name, the recovered source remains durable, and
  the output target remains absent.

Fresh proof after the final code change:

- `cargo test -p memphant-cli`: 24 unit and 21 integration tests passed,
  including 10 real-CLI compile contracts.
- `cargo clippy -p memphant-cli --all-targets --all-features -- -D warnings`,
  `cargo fmt --all --check`, and `git diff --check`: passed.
- `python3 scripts/check_spec_drift.py`: skipped, not passed, because the
  private Syndai specs are absent from this worktree.

The unrelated `.superpowers/sdd/progress.md` modification remains unstaged.
No Task 4, P1 campaign, paid/model call, push, or deployment work was performed.

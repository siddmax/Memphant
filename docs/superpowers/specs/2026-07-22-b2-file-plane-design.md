# B2 File Plane Design

**Status:** Approved for implementation on 2026-07-22
**Scope:** B2 only. B3 MCP resources and P1-T6 Deep promotion are out of scope.

## Decision

MemPhant remains the canonical memory store. The file plane is a deterministic,
editable projection of the current file-visible units in one bound scope. Local
edits never bypass the existing admission, correction, deletion, tenancy, or
bitemporal contracts.

The public surface is deliberately small:

```text
memory/
├── MEMORY.md                 # generated, read-only index
├── units/
│   └── <canonical UUID>.md   # one current unit per file
├── inbox/
│   └── <human slug>.md       # unmarked new semantic facts
└── memphant-export.json      # three-way base + integrity manifest
```

`memphant compile` reads one canonical database snapshot and writes the
projection. `memphant sync` validates the complete local tree and prints a
deterministic plan. `memphant sync --apply` submits that exact plan as one
serializable server mutation and recompiles only after the transaction commits.

## Why this shape

The projection model gives agents the file UX they already understand without
creating a second database. `MEMORY.md` matches the current Claude Code memory
entrypoint, while topic files keep the index compact and individually editable.
UUID-derived managed paths eliminate slug collisions and path-derived identity.

Two alternatives remain rejected:

1. A read-only export cannot round-trip human corrections, additions, or
   deletions.
2. A true bidirectional filesystem replica requires a merge archive, conflict
   state, and field ownership comparable to a synchronization system or CRDT.
   MemPhant already has the correct governed mutation layer, so reproducing that
   machinery in files would be a second source of truth.

## Canonical projection contract

The server exposes one authenticated, unranked, byte-bounded projection read.
It executes as one database statement and returns:

- the exact tenant, subject, actor, scope, agent-node, and subject-generation
  binding;
- current, non-deleted, non-quarantined semantic units in `active` or
  `validated` state;
- current procedural units only in `validated` state;
- no episodic, belief, resource, superseded, invalidated, expired, retired,
  candidate, wrong-generation, wrong-agent, or wrong-tenant rows;
- a SHA-256 fingerprint over the canonical ordered unit records.

The existing paginated scope-memory endpoint is not reused: it intentionally
returns historical states and cannot hold one snapshot across pages.

The projection read fails when its encoded payload would exceed the documented
byte ceiling. It never silently truncates memory.

## File format

Each managed file has an H1 display key, the exact semantic body, and one final
strict JSON footer:

```markdown
# importer wake-up decision

ADR-14 uses Postgres LISTEN/NOTIFY.

<!-- memphant {"unit_id":"…","body_sha256":"…","subject_generation":0,"kind":"semantic","fact_key":"…","predicate":"…","confidence":1.0} -->
```

The footer is drift evidence, not authentication. The manifest is the three-way
base and stores the complete context identity, schema/compiler version,
canonical snapshot SHA-256, immutable per-unit metadata, semantic-body SHA-256,
exact rendered-file SHA-256, relative UUID path, and `MEMORY.md` SHA-256. JSON
objects and entry maps use deterministic ordering; Markdown uses exact LF
rendering.

`MEMORY.md` is generated and never interpreted as a mutation. Existing unit
edits may change only the body. Kind, fact key, predicate, confidence, validity,
identity, generation, or path changes fail closed.

New facts use a footer-free `inbox/<slug>.md`:

```markdown
# importer wake-up decision

ADR-14 uses Postgres LISTEN/NOTIFY.
```

The H1 is the fact key. The default predicate is `states`, confidence is `1.0`,
and kind is `semantic`. When the H1 uniquely matches a projected fact key, sync
inherits its predicate and confidence; this is the explicit contradiction path
through normal admission. Procedural creation is excluded because the existing
direct-unit contract cannot claim procedure validation. Validated procedures
can still be compiled, corrected, or forgotten.

## Sync semantics

The manifest is the common ancestor:

| Local state | Native operation |
|---|---|
| managed file body changed | `correct(memory_unit_id)` |
| manifest-managed file missing | `forget(memory_unit_id)` |
| valid footer-free inbox file | trusted direct-unit `retain` |
| unchanged tree | no operation |

The existing direct-unit retain path is required for additions because it runs
admission synchronously. Episode/resource retain would enqueue reflection and
could not promise a compile-sync-compile fixed point.

Before printing or applying a plan, sync validates every path, footer, hash,
duplicate, symlink, immutable field, body, and context field. It also refetches
the canonical projection and requires its fingerprint to equal the manifest
base. Any failure performs zero remote writes.

The dry-run plan is ordered deterministically and includes a plan SHA-256.
`--apply` recomputes the plan locally and sends the plan digest, base snapshot,
context binding, observed time, and ordered operations in one request. The
server:

1. opens a serializable transaction;
2. claims the whole batch under one idempotency key;
3. recomputes and compares the base projection fingerprint in that transaction;
4. stages every correct, direct retain, and forget through the existing store
   operations;
5. commits once, or rolls back every operation.

A serialization failure or stale base is a conflict, never an automatic retry
against newer truth. Network retry is safe only with the identical
idempotency-key/request-hash pair. The CLI therefore uses a fresh
plan-digest-plus-UUID key for each apply invocation and retains that exact
request/key only within the invocation. It makes no cross-invocation replay
claim because the observed timestamp is not persisted in the deterministic
dry-run projection. A transport or untyped 5xx after POST is reported as
outcome unknown; the operator reruns preflight rather than constructing a blind
retry against possibly newer truth.

After a successful commit the CLI fetches the new projection, replaces managed
files with same-directory temporary files, syncs them, moves changed and stale
previous managed inodes into durable sibling recovery, removes only consumed
inbox files, and writes the manifest last. Compilation refuses to overwrite
dirty local projections. Generated paths are never accepted from arbitrary
footer input.

## Filesystem safety

- Inspection resolves one canonical absolute existing output-parent anchor,
  records its directory identity, and retains its handle. `AbsentOutput`,
  `EmptyOutput`, and every installed or staged `TreeHandles` instance carry
  that same anchor; compile never canonicalizes the output parent again.
- Before every namespace mutation and in both final validation sweeps, compile
  reopens the captured anchor from the filesystem or share root. It opens every
  normal component separately without following links, retains each parent /
  component / identity binding until it reaches the anchor, rechecks all of
  those bindings, and requires the final identity to equal the inspection-time
  retained handle. A renamed or replaced parent therefore fails before the
  next mutation; a change before the first mutation produces zero writes and
  no recovery tree.
- Managed paths are exactly `units/<lowercase canonical UUID>.md`.
- Inbox paths are one flat safe component under `inbox/`; nested, absolute,
  dot-segment, non-UTF-8, and reserved paths fail.
- Output roots and managed/inbox paths may not be symlinks.
- Duplicate IDs, paths, fact keys in one batch, footer fields, or JSON keys fail.
- All content is rendered and validated before the first replacement.
- Temporary files use `create_new` in the destination directory; files are
  `sync_all`ed before rename; the manifest is replaced last.
- A byte-identical managed file is freshly rechecked for the validated identity
  and exact bytes, then left on its existing inode. A no-op compile creates no
  recovery artifact.
- Before changing or deleting a managed file, compile atomically moves its
  current inode with no-replace semantics into one lazy unique sibling
  `.memphant-recovery-<uuid>/` outside the projection root. Recovery preserves
  the original `MEMORY.md`, `memphant-export.json`, and `units/<uuid>.md`
  layout. Source and recovery directories are synced where supported. A
  cross-filesystem (`EXDEV`) move fails closed; there is no copy fallback.
- The recovery pathname is derived only from the captured canonical anchor.
  After recovery creation and before reporting it as current, compile requires
  both the captured anchor identity and the retained parent/name identity to
  match the retained recovery-root handle. It also requires the retained
  `units/` handle identity and the `root/units` name binding to match. If the
  parent changes after recovery has received an inode, compile cannot succeed
  and retains that inode through the recovery handle/name under the moved
  parent.
- Recovery tracks the exact count of managed inodes it currently retains. The
  count increments immediately after a successful move and before directory
  fsync and is monotonic for the compile operation. Diagnostics therefore
  distinguish an empty recovery from one that owns managed data even when later
  validation or fsync fails.
- A detached-file validation failure never automatically restores by recovery
  pathname. The supported cross-platform rename APIs cannot atomically bind a
  no-replace move to a previously validated file handle, so a compare followed
  by a path-based rename would retain a substitution window. Compile instead
  leaves the recovered inode and retained-inode count untouched and fails with
  the confirmed recovery location for explicit operator recovery.
- Once a prepared file exists, no failure path unlinks its pathname. Portable
  APIs likewise cannot bind unlink atomically to the prepared file handle, so
  cleanup could delete a replacement planted after validation. Failures report
  `prepared_name_last_known=<relative-name>` and that cleanup was skipped; they
  do not claim the pathname still names MemPhant's prepared inode. Successful
  compilation still consumes the prepared name through the audited atomic
  no-replace install.
- Recovery directories use mode `0700` on Unix. Windows uses the inherited ACL
  of the retained output parent; MemPhant does not claim to narrow an already
  broader parent ACL without a platform ACL policy supplied by the operator.
- Windows closes every handle into a completed absent-output staging subtree
  before the atomic install, then reopens the installed tree from the retained
  external parent and requires the staged identity and two exact validation
  sweeps to match. Unix keeps its stronger retained-subtree handles throughout
  the install.
- The current Windows `FileIdentity` uses `cap-fs-ext`'s volume serial plus
  64-bit file index. The identity/race contract is therefore scoped to Windows
  filesystems that provide stable 64-bit IDs; native Windows runtime proof is
  still required, and ReFS support requires migration to and validation of
  128-bit `FileIdInfo` before release. No general ReFS identity claim is made.
- Cleanup is limited to paths named by the old manifest and inbox paths consumed
  by the committed plan. Unknown user files are never deleted.
- Consumed inbox files are not directly unlinked. After the committed receipt
  and a fresh exact local revalidation, each planned inode is moved with
  no-replace semantics to the recovery inbox under its original slug, then
  checked for the planned identity and bytes. The recovery inbox handle and
  root/name binding are retained and validated exactly like the recovery units
  directory; a substitution or late descriptor write fails post-commit with
  the data retained in recovery.

Manifest-last is crash-detectable, not a claim that many filesystem renames are
one transaction. A mixed tree never verifies clean and is repaired only from a
fresh canonical compile after the operator explicitly discards or syncs local
edits.

## UX and errors

- `compile` reports the scope, snapshot, output root, and entry count.
- When a compile changed or deleted managed files, success and later errors use
  absolute `recovery=<path>` only while the captured anchor and recovery name
  still identity-match their retained handles. If either pathname cannot be
  confirmed, failure preserves the actual setup or confirmation cause and
  reports `recovery_last_known=<captured-path>`; it never presents the stale
  path as current. Only an observed parent-anchor change after at least one
  managed inode moved adds `output parent changed; recovery was retained under
  that parent`. An empty or unconfirmed recovery directory is never described
  as containing retained managed data. B2 never prunes recovery trees; the
  operator may remove one only after all editors and other processes that could
  still hold the old files open have closed them.
- A dirty compile names every changed/missing/unexpected path and tells the user
  to run `sync` or restore it.
- `sync` defaults to a JSON dry-run plan. Forget operations are labelled
  destructive.
- `sync --apply` validates the committed receipt before local mutation, then
  fetches and compiles current canonical truth. It reports the receipt's
  committed snapshot and the final fetched snapshot separately because a later
  canonical writer may legitimately commit between them.
- Staleness is reported as `sync_conflict`; validation is `sync_invalid`; remote
  unavailability is distinct from either.
- Secrets and bearer tokens never enter the projection or error output.

## Acceptance gate

The deterministic n=12 gate uses three instances of each spec-28 coding
continuity family. Across the 12 bound scopes it applies exactly one of four
edit classes per scope: body mutation, new fact, deletion, or contradiction.
It proves the canonical correct/retain/forget/contradiction effects, an empty
post-apply plan, a byte-identical second compile, and clean verification.

Focused negative contracts cover historical-row exclusion, stale snapshot
rollback, failure on operation N rolling back operations 1..N-1, tampered
footers, duplicate IDs/paths, immutable metadata edits, traversal, symlinks,
dirty `MEMORY.md`, malformed inbox files, zero-write validation failure, parent
replacement before the first mutation, parent movement after recovery begins,
and a component rename during anchor reopening.
The fast gate uses the real CLI binary and in-memory Axum app. A live-Postgres
ignored contract proves serializable rollback, current-head visibility, and
concurrent stale-base conflict through the standard ephemeral scratch database.

## Non-goals

- No watcher, daemon, merge UI, CRDT, compatibility exporter, or legacy
  `--source` mode.
- No new source kind; additions use the existing `user`/trusted direct-unit
  provenance path.
- No procedural validation shortcut.
- No MCP resource exposure (B3).
- No paid model calls and no change to the P1-T6 campaign.

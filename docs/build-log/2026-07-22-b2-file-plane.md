# 2026-07-22 — B2 file-plane gate

## Decision and boundary

**B2 is built and its deterministic product gate passes.** Postgres remains the
only canonical store. `memphant compile` produces a byte-stable editable
projection, `memphant sync` defaults to an exact dry-run plan, and
`memphant sync --apply` commits the bound plan atomically before refreshing the
projection. B3/MCP distribution, P1-T6 Deep promotion, paid model calls, and
deployment are outside this result.

The proof target is the last code commit
`4672d364742280f1d066461b53bd9a6bd829bbfa`
(tree `9e7dcfa931208202c5b3385f0eb12a2b3ec141e7`). The fixture, gate, and
spec-28 source hashes are:

- `tests/fixtures/file-plane-n12.json`:
  `e53f01e7cde720988598594ec8788f269aabfd8f9090410260a4339dd3d7cf81`
- `crates/memphant-cli/tests/file_plane_n12.rs`:
  `89f778c5f2fe9b155c05ce4efc5e8c3b88015296135c6ba12e2add3763ac0a8f`
- `docs/superpowers/specs/memphant/28-syndai-code-contract.md`:
  `271f48b7337b3e9231dcab017bd51f1be37b74ca68a69df5d882b506c6f7d622`

## Deterministic n=12 result

The fixture binds 12 independent scopes: three file-edit scenarios for each of
four themes—architecture decision, compaction/rehydration,
cross-agent-transfer, and task-plus-semantic composite. Mutation, new-fact
append, deletion, and contradiction each occur exactly three times. The
cross-agent-themed deletion uses a validated procedural unit, while new inbox
facts remain semantic and do not bypass procedure validation.

This is deliberately not reported as three executions of each spec-28 recall
family. The file-plane gate does not exercise later-query/forbidden-text,
tight-context-budget, sibling-agent, or episodic-plus-semantic/subquery
predicates. The separate `syndai_trace_compare` suite executes those four
fixture families once each and passed 2/2 test functions.

All 12 cases passed with distinct runtime scope/unit identities. Each case
proved:

1. clean real-CLI compile and verify;
2. two byte-identical dry-runs with one expected native operation and the exact
   canonical plan digest;
3. successful apply through the real Axum route and in-memory governed store;
4. the expected canonical `correct`, `retain`, `forget`, or contradiction edge;
5. an empty inbox and empty post-apply plan;
6. clean post-apply verification; and
7. a byte-identical second compile and stable full-tree SHA-256.

The focused command was:

```sh
cargo test -p memphant-cli --test file_plane_n12 -- --nocapture
```

Result: **1/1 test passed, representing 12/12 fixture cases**.

## Review hardening

The final whole-branch review found and closed the remaining boundary and
upgrade bugs before this proof was refreshed:

- canonical projection and file-sync retain now enforce resolved memory-kind
  policy in both InMemory and Postgres;
- the REST route clamps direct-retain trust to the API-key ceiling, and retain
  fails closed below trusted direct provenance;
- file-sync admission snapshots and their edges are actor-bound in both stores;
- unit, episode, and resource forget recursion is actor- and scope-bound in
  both InMemory and Postgres, including an owned descendant behind a foreign
  actor bridge;
- projection metadata canonicalizes equivalent UTC timestamps before
  fingerprinting; and
- the immutable bootstrap migration was restored and `file_sync` is added by a
  new ordered forward migration, with a live old-schema-to-new-schema test;
  readiness compares the embedded migration head with both the database head
  and the authoritative recorded compatibility floor. Bootstrap-only and
  future-breaking/mislabeled-incompatible schemas fail closed under
  `memphant_app`, while a future additive head that retains the current floor
  remains ready.

REST contracts prove a low-trust key cannot mint trusted semantic memory and an
L1 context can neither project a deliberately seeded semantic row nor retain a
new one. CLI help and the README now include the complete lock → compile →
dry-run → apply → verify path.

## Why this architecture

The chosen design is a deterministic file projection over one canonical
Postgres snapshot plus one serializable mutation batch. PostgreSQL documents
that MVCC gives each statement a consistent snapshot and that Serializable
transactions either have an equivalent serial ordering or abort. Those are the
right primitives for a governed canonical store and stale-base rejection:
<https://www.postgresql.org/docs/current/mvcc-intro.html> and
<https://www.postgresql.org/docs/current/transaction-iso.html>.

The alternatives were rejected for concrete contract reasons:

- **Git index/object database:** valuable content-addressed history, but it
  would make repository state a second authority and still would not apply
  MemPhant admission, tenancy, correction lineage, or forgetting atomically.
  Git's data model is intentionally a content-addressed object graph:
  <https://git-scm.com/docs/gitdatamodel>.
- **SQLite sidecar/journal:** adds a second database and reconciliation path;
  WAL still has one writer at a time and requires same-host shared memory,
  neither of which replaces the canonical Postgres transaction:
  <https://sqlite.org/wal.html>.
- **Filesystem watcher/daemon:** watcher delivery is advisory and race-prone;
  Linux inotify explicitly identifies objects by watch descriptor and emits
  queue-overflow events, so correctness still requires a complete rescan and
  bound base snapshot: <https://www.man7.org/linux/man-pages/man7/inotify.7.html>.
- **CRDT/true bidirectional replica:** would require durable merge state and
  conflict semantics for admission, corrections, contradictions, deletion,
  tenancy, and bitemporality. That duplicates the governed mutation layer; the
  CRDT literature itself treats replicated conflict resolution as a distinct
  distributed-data-type problem: <https://arxiv.org/abs/1805.06358>.
- **Whole-tree replacement or hard links:** neither gives one portable atomic
  multi-file commit. The implementation instead uses validated, same-directory
  create-new files, durable recovery, atomic rename where supported, and a
  manifest-last detectable boundary. Rust documents that rename may replace an
  existing target and fails across mount points, which is why B2 uses stricter
  no-replace/identity checks around it:
  <https://doc.rust-lang.org/std/fs/fn.rename.html>.

## Verification

Every command below was rerun after the last code change unless explicitly
classified as an unmet or skipped repository predicate.

- `cargo test -p memphant-cli --test file_plane_n12 -- --nocapture` — PASS,
  1/1 test representing 12/12 fixture cases.
- `cargo test -p memphant-eval --test syndai_trace_compare -- --nocapture` —
  PASS, 2/2 tests; the coding-continuity test executes all four spec-28 recall
  fixtures.
- `cargo test -p memphant-cli --test help_contract -- --nocapture` — PASS, 3/3.
- `cargo test -p memphant-cli` — PASS, 69/69.
- `cargo test -p memphant-server --test auth_contract file_sync_ -- --nocapture`
  — PASS, 2/2 (low-trust ceiling and L1 semantic read/write denial).
- `cargo test -p memphant-core --lib canonical_projection_ -- --nocapture` —
  PASS, 5/5, including kind policy and canonical UTC metadata.
- `cargo test -p memphant-core --lib file_sync_transition_snapshot_is_actor_bound -- --nocapture`
  — PASS, 1/1.
- `cargo test -p memphant-core --lib file_sync_retain_requires_semantic_policy_and_trusted_direct_provenance -- --nocapture`
  — PASS, 1/1.
- `cargo test -p memphant-core --lib foreign_actor_bridge -- --nocapture` —
  PASS, 3/3 (memory-unit, episode, and resource forget).
- `bash scripts/with_scratch_db.sh postgres://memphant:memphant@localhost:5432/memphant MEMPHANT_TEST_DATABASE_URL cargo test -p memphant-store-postgres --test pg_store_contract file_sync_ -- --ignored --test-threads=1`
  — PASS, 4/4 live-Postgres file-sync tests.
- `bash scripts/with_scratch_db.sh postgres://memphant:memphant@localhost:5432/memphant MEMPHANT_TEST_DATABASE_URL cargo test -p memphant-store-postgres ping_rejects_bootstrap_only_schema_until_required_revision_is_applied -- --ignored --test-threads=1`
  — PASS, 1/1; the four-state handshake runs through a login inheriting
  `memphant_app`.
- `bash scripts/with_scratch_db.sh postgres://memphant:memphant@localhost:5432/memphant MEMPHANT_TEST_DATABASE_URL python3 -m pytest tests/test_wsa_migration_contract.py::test_live_forward_migration_upgrades_applied_bootstrap_atomically -q`
  — PASS, 1/1 live forward-upgrade test.
- `bash scripts/with_scratch_db.sh postgres://memphant:memphant@localhost:5432/memphant MEMPHANT_TEST_DATABASE_URL cargo test -p memphant-store-postgres -p memphant-worker -- --ignored --test-threads=1`
  — PASS, 77/77; the helper provisioned and dropped an ephemeral migrated DB.
- `DATABASE_URL=postgres://memphant:memphant@localhost:5432/memphant bash scripts/e2e_probe.sh`
  — PASS (`E2E PROBE: ALL CHECKS PASSED`).
- `cargo fmt --check` — PASS.
- `cargo clippy --all-targets --all-features -- -D warnings` — PASS.
- `cargo test --all-targets --all-features -q` — PASS, 678 passed / 92 ignored /
  0 failed across the workspace test harnesses.
- `cargo test --doc` — PASS, 0 failed.
- `cargo run -p memphant-cli -- db lint --provider plain-postgres` — PASS.
- `cargo run -p memphant-cli -- db lint --provider supabase` — PASS.
- `cargo run -p memphant-cli -- db lint --provider neon` — PASS.
- `python3 scripts/apply_memphant_migrations.py --database-url postgres://memphant.invalid/memphant --dry-run`
  — PASS, two ordered migrations planned: bootstrap then file-sync forward
  migration.
- `python3 scripts/check_spec_drift.py` — SKIPPED with exit 0 because the
  private mirror is absent (`private_specs_missing`); this is not a
  mirror-parity pass.
- `python3 -m pytest tests/ spikes/python-retain/test_spike.py -q` — UNMET,
  exit 4 before collection because the referenced spike path was removed.
- `python3 -m pytest tests/ -q` — UNMET: 697 passed / 24 failed / 11 skipped.
  The failures are 15 stale r15 runner contracts, six stale breadcrumb runner
  contracts, one stale embedder-arm pin, one missing Playwright installation,
  and one immutable campaign-lock assertion. That assertion compares the
  frozen OpenAPI hash
  `a5bac765d7c4c862a342d95b49049c27d3af57aea9f80af6d3a0a489ac055271`
  with B2's current generated hash
  `919b53aea39c3fb2b95129131de627b638e43f8531207e5d6dab589b218c1da9`.

## Non-claims

This is deterministic mechanism and round-trip evidence, not a production
deployment, native-Windows runtime proof, filesystem-crash atomicity claim,
cross-device rename guarantee, benchmark accuracy promotion, Deep promotion,
or SOTA/cutover claim. No model was called, no paid run was launched, and no
branch was pushed. The immutable `run-65981e4f` campaign root and the P1
worktree were never touched. During local review the benchmark adapter lock was
transiently edited in commit `9485d2fd`, restored by `13e8b795` 16 seconds
later, and both commits were rebased out; the final B2 tree is byte-identical to
base for that file. This disclosure supersedes the earlier absolute statement
that no campaign artifact was ever changed.

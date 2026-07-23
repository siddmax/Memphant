# 2026-07-22 — B2 file-plane gate

## Decision and boundary

**B2 is built and its deterministic product gate passes.** Postgres remains the
only canonical store. `memphant compile` produces a byte-stable editable
projection, `memphant sync` defaults to an exact dry-run plan, and
`memphant sync --apply` commits the bound plan atomically before refreshing the
projection. B3/MCP distribution, P1-T6 Deep promotion, paid model calls, and
deployment are outside this result.

The proof target is commit `816eeebdeee1e1daf9c44c35cb4cc5c69db42447`
(tree `f5906af494e14cc246bf83f218e4f318a7a2dc36`). The fixture, gate, and
spec-28 source hashes are:

- `tests/fixtures/file-plane-n12.json`:
  `eec33283f9acd2817471d284a79185eb132eb36072027f560ccbad7db33e9a59`
- `crates/memphant-cli/tests/file_plane_n12.rs`:
  `30a60173f9da4ff6874a489e1dcf9536a9adabe40d01cb89d2e9ef61c88b75b2`
- `docs/superpowers/specs/memphant/28-syndai-code-contract.md`:
  `271f48b7337b3e9231dcab017bd51f1be37b74ca68a69df5d882b506c6f7d622`

## Deterministic n=12 result

The fixture binds 12 independent scopes: three architecture-decision, three
compaction/rehydration, three cross-agent-transfer, and three task-plus-semantic
composite cases. Mutation, new-fact append, deletion, and contradiction each
occur exactly three times. The cross-agent deletion uses a validated procedural
unit, while new inbox facts remain semantic and do not bypass procedure
validation.

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

- n=12 real-CLI/Axum gate: PASS, 1 test / 12 cases.
- spec-28 trace compare:
  `cargo test -p memphant-eval --test syndai_trace_compare`: PASS, 2/2.
- complete CLI suite: `cargo test -p memphant-cli`: PASS, 66/66.
- focused live-Postgres file-sync contract: PASS, 3/3.
- full ignored scratch-Postgres store/worker gate: PASS, 73/73; the helper
  provisioned and dropped an ephemeral migrated database.
- real packaged server/worker/CLI Postgres probe: `scripts/e2e_probe.sh`: PASS.
- `cargo fmt --check`: PASS.
- `cargo clippy --all-targets --all-features -- -D warnings`: PASS.
- `cargo test --all-targets --all-features`: PASS, 666 passed / 88 ignored / 0
  failed.
- `cargo test --doc`: PASS, 0 failed.
- provider lint: PASS for `plain-postgres`, `supabase`, and `neon`.
- migration dry-run: PASS; one bootstrap migration planned.
- `python3 scripts/check_spec_drift.py`: SKIPPED with exit 0 because the private
  mirror is absent (`private_specs_missing`); this is not a mirror-parity pass.
- `python3 -m pytest tests/ -q`: 696 passed / 10 skipped / 24 failed. The 24
  failures are 15 stale r15 gate-runner signatures, six stale breadcrumb
  signatures, one stale embedder-arm pin, one missing Playwright installation,
  and one immutable benchmark-lock assertion. The last compares the frozen
  campaign OpenAPI hash
  `a5bac765d7c4c862a342d95b49049c27d3af57aea9f80af6d3a0a489ac055271`
  with B2's regenerated current OpenAPI hash
  `919b53aea39c3fb2b95129131de627b638e43f8531207e5d6dab589b218c1da9`.
  The frozen campaign lock was deliberately not changed. These are recorded as
  unmet repository predicates, not passing checks.

## Non-claims

This is deterministic mechanism and round-trip evidence, not a production
deployment, native-Windows runtime proof, filesystem-crash atomicity claim,
cross-device rename guarantee, benchmark accuracy promotion, Deep promotion,
or SOTA claim. No model was called, no paid run was launched, no P1 campaign
artifact changed, and no branch was pushed.

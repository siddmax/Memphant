# B2 File Plane Implementation Plan

> **For Codex:** Execute continuously with subagent-driven development. Every
> implementation task is test-first, locally committed, and task-reviewed before
> the next task. Never push. Stop when B2 is complete.

**Goal:** Deliver a deterministic, editable file projection whose additions,
corrections, deletions, and contradictions round-trip atomically through
MemPhant's canonical governed store.

**Architecture:** Add a single-snapshot file-projection read and a serializable
batch mutation at the existing service/store boundary. Replace the fixture-only
CLI exporter with `compile`, `sync`, and full manifest verification over
`MEMORY.md`, UUID unit files, and a footer-free semantic inbox.

**Tech stack:** Rust, Axum, serde/serde_json, sha2, ureq, SQLx/Postgres, existing
MemPhant store/service and testkit infrastructure.

**Design:** `docs/superpowers/specs/2026-07-22-b2-file-plane-design.md`

## Global constraints

- Canonical Postgres is the only source of truth; files are a projection.
- B2 only: no B3/MCP work, paid/model call, immutable `run-65981e4f`
  mutation, or P1-worktree mutation. A transient local adapter-lock edit was
  restored and rebased out; the final file must equal base.
- Remove the pre-production JSON `--source` exporter; add no compatibility shim.
- Project only current file-visible semantic heads and validated procedures.
- Reads are complete one-snapshot responses with an explicit byte ceiling; no
  pagination or silent truncation.
- Apply is one serializable, idempotent server transaction. Any stale base,
  invalid operation, operation failure, or serialization failure commits zero
  mutations.
- `sync` is dry-run by default; `--apply` recomputes and binds the exact plan
  digest and base snapshot.
- Managed paths are UUID-derived; new files live only in a flat `inbox/`.
- Compile never overwrites a dirty projection. Footer/manifest metadata is
  strict and immutable; all validation occurs before writes.
- Use same-directory create-new temporary files, `sync_all`, atomic rename, and
  manifest-last replacement. Delete only old-manifest or consumed-inbox paths.
- Every non-trivial logic branch has an executable regression test. Skipped live
  Postgres tests are reported as skipped, never passing.
- Preserve unrelated work and commit only explicit B2 paths with
  `Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>`.

## Task 1: Canonical projection read

**Files:**

- Modify: `crates/memphant-types/src/lib.rs`
- Modify: `crates/memphant-core/src/lib.rs`
- Modify: `crates/memphant-core/src/service.rs`
- Modify: `crates/memphant-runtime/src/lib.rs`
- Modify: `crates/memphant-store-postgres/src/store.rs`
- Modify: `crates/memphant-server/src/lib.rs`
- Modify: `crates/memphant-server/tests/rest_contract.rs`
- Modify generated: `openapi/memphant.v1.json`

1. Add failing REST/store tests proving one projection response excludes every
   historical/disallowed state and returns a stable ordered SHA-256 fingerprint.
   Add an over-byte-limit test that fails rather than truncates.
2. Run the focused tests and capture the expected missing-route/contract failure.
3. Add typed projection response structs, the current-visible store transaction
   query, the service fingerprint helper, and authenticated route.
4. Regenerate OpenAPI with the server binary; never hand-edit it.
5. Run focused types/core/server/store tests, `cargo fmt --check`, and clippy for
   touched packages. Commit the task.

## Task 2: Serializable file-sync batch

**Files:**

- Modify: `crates/memphant-types/src/lib.rs`
- Modify: `crates/memphant-core/src/lib.rs`
- Modify: `crates/memphant-core/src/service.rs`
- Modify: `crates/memphant-runtime/src/lib.rs`
- Modify: `crates/memphant-store-postgres/src/store.rs`
- Modify: `crates/memphant-server/src/lib.rs`
- Modify: `crates/memphant-server/tests/rest_contract.rs`
- Modify: `crates/memphant-store-postgres/tests/pg_store_contract.rs`
- Modify generated: `openapi/memphant.v1.json`

1. Add failing in-memory and ignored live-Postgres tests for correct/retain/forget
   batches, contradiction edges, stale-base zero-write conflicts, operation-N
   rollback, idempotent replay, and concurrent serializable conflict.
2. Run focused tests and capture the expected missing batch behavior.
3. Add strict tagged batch request/response types and `MutationVerb::FileSync`.
   Validate identity, exact plan digest, unique targets/fact keys, immutable
   metadata, timestamps, and non-empty operations before beginning writes.
4. Precompute embeddings/compiled writes, then begin a serializable transaction,
   claim the batch, compare its in-transaction projection fingerprint, stage
   existing correction/direct-retain/forget writes, and commit once.
5. Add the authenticated route and stable conflict/validation errors. Regenerate
   OpenAPI.
6. Run focused tests plus the ephemeral scratch-DB ignored contract. Run format
   and touched-package clippy. Commit the task.

## Task 3: Deterministic compiler and verifier

**Files:**

- Add: `crates/memphant-cli/src/file_plane.rs`
- Modify: `crates/memphant-cli/src/main.rs`
- Replace: `crates/memphant-cli/tests/compile_contract.rs`
- Delete: `examples/evals/compiled-memory-source.json`

1. Replace the old integration test with failing server-backed contracts for the
   exact tree, strict JSON footer/manifest, deterministic byte output, clean
   verify, dirty compile refusal, historical/path/symlink/duplicate/tamper
   failures, and removal of `--source`.
2. Run the CLI contract and capture its expected failure.
3. Implement flexible compile context parsing, projection GET, typed deterministic
   render/parse, SHA-256 hashing, complete tree validation, and safe manifest-last
   writes. Generalize/remove FNV helpers and `safe_file_stem`.
4. Replace export verification with complete manifest-to-filesystem validation.
5. Run the CLI contracts, format, and CLI clippy. Commit the task.

## Task 4: Sync plan and apply

**Files:**

- Modify: `crates/memphant-cli/src/file_plane.rs`
- Add: `crates/memphant-cli/tests/file_plane_contract.rs`

1. Add failing real-CLI/Axum contracts for dry-run determinism, body correction,
   inbox retain, delete/forget, same-fact contradiction, immutable edit refusal,
   stale base, malformed paths/footers, batch rollback, consumed inbox cleanup,
   empty post-apply plan, and compile-sync-compile byte identity.
2. Run the focused contract and capture the expected failure.
3. Implement the manifest three-way diff, strict inbox parser, deterministic
   operation ordering/plan digest, JSON plan output, `--apply` batch call, final
   canonical compile, and stable error taxonomy.
4. Run all CLI tests, format, and CLI clippy. Commit the task.

## Task 5: n=12 B2 gate and proof

**Files:**

- Add: `tests/fixtures/file-plane-n12.json`
- Add: `crates/memphant-cli/tests/file_plane_n12.rs`
- Add: `docs/build-log/2026-07-22-b2-file-plane.md`
- Add: `docs/build-log/artifacts/b2-file-plane/gate-summary.json`
- Modify: `docs/superpowers/specs/memphant/STATUS.md`

1. Add the 12-scope fixture: three deterministic file-edit scenarios themed
   after each spec-28 coding-continuity family, evenly assigned to mutation,
   append, delete, and contradiction edit classes. This gate proves file-plane
   behavior only; the spec-28 recall predicates stay in the separate
   `syndai_trace_compare` suite.
2. Add a failing gate that seeds canonical direct units, compiles every scope,
   applies its edit, asserts the expected correct/retain/forget/contradiction
   store effect, recompiles, checks an empty sync plan and byte-identical second
   compile, and verifies all exports.
3. Run the focused gate and capture the expected failure; then complete only the
   minimum implementation gaps it exposes.
4. Run the focused n=12 gate, spec-28 trace compare, CLI suite, live-Postgres
   file-sync contract, and real-binary scratch-DB probe. Record exact commands,
   pass/fail/skip counts, hashes, and non-claims in the build log and JSON proof.
5. Flip only the B2/roadmap ledger statement supported by the same proof change.
   Run spec drift, format, clippy, all-target/all-feature tests, doc tests,
   provider lint, migration dry-run, and the full repository verification gate.
6. Commit the proof task. Do not push.

## Final branch review

1. Generate one merge-base-to-HEAD review package.
2. Run an independent whole-branch specification and code-quality review. Fix
   every critical/important finding in one reviewed fix wave.
3. Re-run fresh full verification after the last code change.
4. Confirm the B2 worktree is clean, the immutable campaign root and P1
   worktree are untouched, the transient adapter-lock edit is disclosed and
   absent from the final tree, no paid/model call occurred, and no commits were
   pushed.
5. Mark B2 complete and stop. Do not begin B3.

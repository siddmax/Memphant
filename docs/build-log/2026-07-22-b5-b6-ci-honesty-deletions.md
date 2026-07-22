# B6 CI honesty legs + B5 deletions + flaky-claim race fix — 2026-07-22

Branch `codex/memphant-p1-deep-mode`, worktree `p1-deep-mode`. Commit `f637dc01`.
The free Week-1 spine tail (plan §4 items 5–6, §8). Owner priorities:
accuracy/SOTA > cost > speed, KISS/DRY, pre-production.

## Summary

- **B6** — CI now proves the **Postgres** store, not just InMemory. Every prior
  CI eval leg ran against `InMemoryStore` (the store-divergence anti-pattern,
  unguarded in CI); the 52 `#[ignore]`d live-PG contract tests + `e2e_probe.sh`
  never ran in CI at all.
- **B5** — retired dead WS-0 code and a false-confidence stub, marked schema-only
  tables dormant, corrected two mis-scoped audit items.
- **Prereq** — the known `pg_store_contract` concurrent-claim flake (flagged in
  the C1 STATUS entry) is fixed at the SQL root, so the new Postgres leg is green.

## B6 — CI honesty legs (`.github/workflows/ci.yml`)

All four legs were validated locally against the packaged runtime + a live
scratch Postgres before wiring.

### 1. `postgres-contracts` job (new) — the Postgres store leg

A `postgres:17` service (superuser `memphant`, so the migrations can
`create role memphant_app …` and the scratch-DB helper can create/drop DBs).
Steps:

- **Live-PG contract + worker suite:**
  `with_scratch_db.sh … cargo test -p memphant-store-postgres -p memphant-worker
  -- --ignored --test-threads=1`. Runs the full ignored suite (43/43 in
  `pg_store_contract`, incl. C1's `hot_path_slo_pg.rs` and
  `episodic_rls_leakage.rs`). Each test gets an ephemeral migrated DB that is
  dropped afterward — no `job_state`/tenant debris across tests (the AGENTS.md
  scratch-DB discipline).
- **`e2e_probe.sh`** — real `memphant-server`/`worker`/`cli` binaries against a
  scratch PG: retain → worker compile → recall → cross-tenant 404 → correct →
  forget-no-resurrection → mark → resource ingest → 401. Locally: `E2E PROBE:
  ALL CHECKS PASSED`.
- **LME-S n=5 retrieval chain smoke** — `fetch_longmemeval.py` (sha-pinned,
  cached via `actions/cache` on the manifest hash — a 277 MB download only on a
  pin change) → `bench-lme --sample 5 --embed-model small`. Locally:
  `bench_lme=done sample=5 recall_at_10=0.8`, real ingestion (44–47 sessions/q).
  Stops the full-500 chain from bit-rotting between paid runs. Placed last so an
  HF outage can't red the core contract legs.

### 2. `public-gates` additions (no PG, secret-free)

- **`ops` lane** — `cargo run -p memphant-eval -- ops examples/evals/ops-smoke.yaml`
  (`ops=pass checks=blob_gc,deletion_saga_readback,reindex_compaction_sla`).
  Was executable but never in CI.
- **fastembed-off leg** — `cargo test -p memphant-eval --no-default-features`
  compiles and runs the `#[cfg(not(feature="fastembed"))]` error-path arms that
  the `--all-features` build silently drops (63 tests pass), and
  `cargo build -p memphant-server --no-default-features` proves the
  default-features build the all-features run never exercises.

## Flaky-test prereq — root cause was SQL, not the test

`concurrent_workers_cannot_split_a_scope_lane_and_reclaim_reuses_preparation`
asserts `left.is_empty() ^ right.is_empty()` ("only one lane owner") on two
`tokio::join!`'d `claim_reflect_jobs` calls. It passed 3/3 in isolation but
failed ~1/60–1/120 under the loaded `--ignored` suite.

**Reproduced (evidence, not theory):** an instrumented run over 60 trials caught
`left_len=4 right_len=2`, and printing the IDs showed `left` = the canonical
4-job prefix (`== expected`), `right` = the disjoint tail jobs 5–6. The claims
never overlap and lane order is preserved — the code is correct; the *assertion*
was too strong.

**Root cause:** `memphant.claim_reflect_jobs`' lane lock is
`for update of agent skip locked` on the `agent_node` row, which sits *above the
Sort* in the plan. When two claimers' MVCC snapshots overlap on that row, one
`skip locked`s and returns empty (XOR holds). But when they do **not** overlap
(A commits before B reaches the lock), B re-reads, doesn't skip, and legitimately
claims the lane **tail** A left behind under `limit 4` (6 jobs, 4 per claim) — so
both come back non-empty on disjoint sets. The lock guarantees *at-most-once
claiming*, not mutual exclusion across non-overlapping time.

**Fix (at the source):** add `pg_try_advisory_xact_lock(hashtextextended(lane
key, 0))` to the `locked_lanes` CTE. It is atomic in shared memory with no
lock-above-Sort window — exactly one claimer keeps the lane, the loser's
`locked_lanes` is empty so it claims nothing. Held to transaction end, it covers
the job claim + prepared-state writes in the same statement. **0/500** under the
same hammer that failed 1/120 before; the full ignored suite is 43/43 green; the
strong XOR contract test is unchanged. (A sibling session had drafted then
reverted the same advisory-lock approach mid-session; it is re-applied here.)

`race_repro.rs` — a sibling's scratch repro harness — is deliberately **not
committed**; because CI checks out HEAD, the `--ignored` leg never runs its
200-trial hammer.

## B5 — deletions (KISS; each confirmed zero real callers by grep)

- **WS-0 `retain(RetainInput)` stub** (`core/src/lib.rs`) + `RetainInput` /
  `RetainResult` / `ScopeRef` types (`types/src/lib.rs`) — zero callers outside
  their own definitions; not in any generated schema. `CoreError::EmptyBody`
  kept (used by the real service path).
- **`memphant-eval compare` stub** — always printed `compare=pass` regardless of
  input (a false-confidence trap); deleted, and the `05-retrieval-and-eval-spec.md`
  release runbook repointed at the real `profile … --compare-to`.
- **Spike-dir cleanup** — a sibling C1 commit deleted the spike dirs
  (`spikes/python-retain`, `spikes/rust-retain`, `examples/spike`, `run_spike.py`)
  but left dangling references: the AGENTS.md gate still ran
  `spikes/python-retain/test_spike.py` (now a missing file → gate failure) and
  Cargo.toml still `exclude`d `spikes/rust-retain`. Both fixed here.
- **Dormant tables** — `trust_event` (no producer), `event_outbox` (no consumer),
  `scope_block` (no surface; the observation-block verb is plan item B1)
  annotated `DORMANT (2026-07-22)` via `comment on table` (catalog-queryable via
  `obj_description`) plus a source comment block.

### Findings that revise the §f audit

- **§f.8 "synthetic rung YAML fleet — archive the rest" is inverted.** Nearly
  every `benchmarks/rung*.yaml` and `examples/evals/rung*-profile.yaml` is a live
  regression gate referenced by `profile_contract.rs`/`eval_contract.rs`. Only
  `rung4-baseline-sampled.yaml` and `rung5-baseline-sampled.yaml` were genuine
  orphans (referenced only in immutable build-log docs); both were removed by the
  sibling C1 commit.
- **§f.7 `retention_tier` is not a schema-only table** — it is a live column on
  `episode` with a real partial index; what is dormant is the warm/cold tiering
  job, not the schema. Recorded in the dormant comment block.

### Deferred (with rationale)

- **Heuristic rerank stage + `RecallMode::Balanced` (§f.2/§f.3)** — the OLD
  heuristic reranker is production-dead (public path forces
  `rerank_enabled:false`), but full deletion ripples into 5 `RetrievalTrace`
  fields, ~15 tests, the `--disable-rerank`/`--disable-learned-rerank` eval
  flags, rung13 validators, and **three versioned external JSON schemas**
  (`trace-schema.v1.json`, `openapi/memphant.v1.json`, `mcp/memphant.tools.v1.json`);
  Balanced collapses to a Fast alias only once the reranker is gone. Owner
  redirected to first settle "which reranker actually helps." In-repo evidence is
  already decisive: heuristic rerank **HARMS** chat retrieval (ΔR@10 −0.074, CI
  excl 0, n=100, `2026-07-10-scaled-reader-campaign.md`) → delete it; the
  cross-encoder seam (`bge-reranker-base` / Voyage `rerank-2.5`) is the campaign's
  largest QA lever (**+0.158**, `2026-07-12-r15-rank-compression.md`) but
  latency-retired at 12.9–13.6 s/query (9× the 1.5 s ceiling) → **keep the seam,
  default OFF, adopt a faster OSS reranker or rank-compression** (the named
  4–8×-latency-cut path: truncated-input rerank / top-32 pool / smaller model /
  async second pass). A current-web-research pass on the 2026 reranker landscape
  is in flight to pick the model.
- **l4-naming shims (§f.5)** and the **`subject_hint` internal-type prune
  (§f.6)** — their own focused passes.

## Verification

- `cargo fmt --check` clean; `cargo clippy` on touched crates clean.
- `memphant-types` / `memphant-core` / `memphant-eval` lib + integration tests
  green; fastembed-off `memphant-eval` 63 tests green; `memphant-server`
  fastembed-off build green.
- Live-PG: 43/43 `pg_store_contract` (advisory-lock migration), `e2e_probe.sh`
  all checks passed, LME-S n=5 smoke `recall@10=0.8`.
- Migration applies cleanly on a fresh scratch DB; dormant comments land in the
  catalog; `apply_memphant_migrations.py --dry-run` and `check_spec_drift.py` OK.

## Coordination note

This work ran in a **shared worktree** with an active C1 session that commits
frequently and, during this session, twice reset uncommitted migration edits.
The advisory-lock + dormant-comment migration changes were re-applied and
committed promptly to protect them. The plan's B5/B6 rows carry the sibling's
C1 divergence-audit notes; the LANDED notes here were appended below them, never
overwriting.

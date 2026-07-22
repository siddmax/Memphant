# C1 — Episodic slice cutover: design (2026-07-21)

**Branch:** `codex/memphant-p1-deep-mode` · **Plan:** `docs/superpowers/plans/2026-07-21-tri-domain-sota-plan.md` §5 C1, §8 spine.
**Builds on:** C0 rails (strict-contract clients) + C3 backfill mechanism.
**Recon:** `docs/build-log/research/2026-07-21-tri-domain/05-codebase-syndai.md` + the 2026-07-21 Syndai episodic source map (this session).

## 1. Verdict / honest framing (read this first)

C1 is the first real-user-value cutover slice: Syndai's episodic conversation
memory. The three binding acceptance bars (hot-path SLO, identical Conversations
tab, two-user RLS leakage) are all **proven MemPhant-side**, because:

- **Production episodic data (252 rows) exists only in off-limits Supabase prod**
  (`syndai` schema, AGENTS.md §18). The local `syndai_local` dev DB (container
  `syndai-coding-local-db`, port 55432) has the episodic schema but **0 rows** —
  historical data wiped, the same wall C3 hit with coding events. Verified this
  session (read-only count = 0). A local/real extract is therefore **not
  runnable** without explicit prod authorization, which was not granted for a
  data copy. Per owner decision ("do the real extract if it runs / is worth it;
  else synthetic"), C1 sources its backfill from a **schema-faithful synthetic
  252-row corpus** — correctness-only, exactly the C3 posture.
- **No Syndai episodic oracle exists** (tests-audit `06-tests-audit.md`: "no
  episodic workload exists on either side; the gate is docs-RAG only"). So
  "identical Conversations tab" cannot be a live Syndai-vs-MemPhant head-to-head;
  it is proven as **MemPhant-side output-equivalence** on the backfilled corpus.

Like C0 and C3, C1 lands the **MemPhant-side cutover-ready proof + the 252-row
backfill mechanism + all three acceptance bars**, and **defers the live Syndai
`_load_episodic` / `episodic_controller` rewiring** to the same boundary C0/C3
deferred: Syndai has no `subject_generation`/context-binding concept yet, the
dogfood flag is default-off, and the SDK-based `MemphantMemoryAdapter` bridge is
the unbuilt "Task 6". Rewiring Syndai's hot transactional loader live against an
untestable path now would be speculative and violate the "no bypass/shim paths"
rule (AGENTS.md). What C1 proves is that the moment the adapter bridge + a real
Syndai context binding land, the episodic layer cuts over correctly.

## 2. What must be preserved (the cutover contract)

The user-visible **Conversations tab** is `GET /api/v1/memory/episodic` →
`EpisodicMemoryController.list_episodic` → `EpisodicMemoryRead` DTO
(`Syndai backend/src/features/memory/episodic_controller.py:80`, `dtos.py:93`).
It is **deterministic recency-only**: `WHERE user_id = :user AND archived_at IS
NULL [AND project/agent filters]`, `ORDER BY created_at DESC`, paginated. DTO
fields: `id, user_id, l0_agent_id, project_id, mission_id, content, summary,
source_kind, importance_score, trust_level, tainted, rolled_up, archived_at,
created_at`. (This is simpler than the hybrid-scored `_load_episodic` hot-context
path — the tab is the clean equivalence target.)

Current isolation (the risk the RLS bar addresses): production `episodic_memories`
has `relrowsecurity = false`; isolation is **app-level only** — every query threads
`user_id` (+ `l0_agent_id`/`project_id`) into the SQLAlchemy WHERE
(`episodic_service.py:_build_active_scope_filters`). MemPhant swaps this to
tenant-RLS + server-derived subject/scope predicates; the swap must be proven not
assumed, or a gap is a data-exposure incident.

## 3. Verb mapping (recall/correct/reinforce/archive/forget)

MemPhant's REST surface has no literal `reinforce`/`archive` verbs — and that is
correct, not a gap. The semantic bridge (a real C1 deliverable, the mapping Task 7
needs):

| Syndai episodic verb | MemPhant contract | Mechanism |
|---|---|---|
| store / retain (`EpisodicMemoryService.append`) | `POST /v1/episodes` `payload.episode{source_kind,body}` | backfill path |
| recall (`RecallMemoryTool`, loader) | `POST /v1/recall` Fast, budget 1200 | fusion + packing |
| correct (`correct_fact` replace/invalidate) | `POST /v1/correct` | bitemporal supersede |
| **reinforce** (`_reinforce_episodic`: trust +5, `last_accessed_at`) | `POST /v1/mark` outcome=success | DSR reinforcement (`dsr_reinforcement_count`) |
| **archive** (`_archive_episodic`: sets `archived_at`) | `POST /v1/forget` (soft) | removed from recall surface; retention *presentation* stays Syndai |
| forget (scoped project/mission, server-owned) | `POST /v1/forget` scope selector | matches Syndai's backend-owned erasure |

`e2e_probe.sh` already exercises retain/recall/correct/forget/mark end-to-end
against real Postgres, so the mapping is proof-backed, not asserted.

## 4. Components

### 4.1 Synthetic corpus generator — `scripts/episodic_lane_corpus.py`
Emits a deterministic 252-row episodic corpus schema-faithful to
`syndai.episodic_memories` (fields: `content`, `source_kind` with the real
importance weights — user_correction 1.5 … system_generated 0.3, `created_at`,
`trust_level`, `tainted`, `archived_at`, `user_id`, `l0_agent_id`,
`project_id`, `mission_id`, `idempotency_key`). Two `user_id`s (two tenants) so
the same corpus feeds the RLS leakage fixture. A handful of rows are
`archived_at != NULL` and `source_kind = 'user_correction'` (audit rows) so the
equivalence bar exercises the tab's `archived IS NULL` + recall-exclusion
filters. Deterministic: no `Date.now()`/random — seeded ids and timestamps.

### 4.2 Backfill runner — `scripts/episodic_lane_run_memphant.py`
Mirrors `code_lane_run_memphant.py`: re-exec through `with_scratch_db.sh`
(`gate_runtime.reexec_through_scratch_db`) → start packaged `memphant-server` +
`memphant-worker` → `ApiClient.bind_context()` per tenant (C0 handshake, no
`tenant_id`) → `retain(episode)` one per corpus row (`content`→body,
`source_kind`→source_kind, `created_at`→observed_at, `idempotency_key`→source_ref)
→ worker drain → recall + list.

**Row → episode mapping** is the same strict-contract `payload.episode` path C3
pinned. Runs on a run-owned ephemeral scratch DB; never a shared/Syndai DB.

### 4.3 The three acceptance bars

**Bar 1 — Hot-path SLO on the packaged runtime (measured at the HTTP boundary).**
The true user-facing hot path is `POST /v1/recall` over HTTP, which adds the
axum hop + a `resolve_memory_context` DB round-trip (`server lib.rs:433`) on top
of the pipeline. Syndai's 200 ms/500 ms budget is HTTP-observed, so the
**acceptance number is measured there**: an SLO leg in `scripts/e2e_probe.sh` (or
a small probe reusing `gate_runtime.Server`/`ApiClient`) issues N real
`POST /v1/recall` calls (Fast, budget 1200) against the packaged server + scratch
PG with the 252-row corpus loaded, and measures **client wall-clock p50/p95**;
assert p50 < 200 ms, p95 < 500 ms. In addition, a cheap CI component guard
`crates/memphant-store-postgres/tests/hot_path_slo_pg.rs` (`#[ignore]`d, run under
the AGENTS.md §37 scratch-DB leg) measures `MemoryService::recall` against
`PgStore` to catch pipeline regressions early — but the **acceptance claim rests
on the HTTP number**, not the service-layer test. This closes the exact STATUS §6
gap: the existing `hot_path_slo.rs` uses `InMemoryStore` in-process, which is not
the packaged runtime.

**Bar 2 — Conversations-tab equivalence (proven on recall, flags pinned, claim
scoped).** Equivalence is proven **on the recall output**, NOT on
`GET /v1/scopes/{id}/memory` (`scope_memory_page`), because that listing (a)
orders `by id` (UUIDv7 ascending) not `created_at DESC`, and (b) — verified,
`store.rs:3374-3389` — applies **no `state` filter**, so a forgotten/archived
episode still appears in it; only `recall` filters state
(`state in (active,validated)`, `deletion_generation is null`, `forgotten_source`
exclusion, `store.rs:1978-1990`). So Bar 2 asserts: after backfill, `recall`
reproduces the expected **episode SET** the tab would render — matching `content`
and `source_kind`, ordered by `observed_at` (the field carrying `created_at`),
with **archived/forgotten/`user_correction` rows absent** (recall's state filter
does this correctly). The claim is scoped to **episode-set + recall-visibility
equivalence**, NOT DTO byte-for-byte: `StoredMemoryUnit` carries none of the
tab's presentation fields (`tainted`, `importance_score`, `archived_at`,
`created_at`), which stay Syndai-side per recon line 66. **Flag posture is pinned**
so the 1-episode→1-unit count is deterministic: `fact_extraction_enabled=false`
and `resource_chunks_write_enabled=false` (both default-off, `service.rs:883,887`)
and corpus bodies kept short enough to avoid contextual-chunk splitting. The
equivalence assertion is **gated on a completed worker drain and `degraded=false`**
(per `e2e_probe.sh:127`) so an infra failure (partial drain) cannot masquerade as
a real divergence. Inline assertion in the runner + a pytest pinning the
row→episode field mapping.

**Bar 3 — Two-user RLS leakage proof (under the real least-privilege role).**
The acceptance proof is a new `#[ignore]`d live-PG test (memphant-store-postgres,
the `role_matrix.rs` pattern) that seeds episodic rows for tenant A and B, then —
under `set local role memphant_app` + `bind_tenant(B)` — asserts tenant B sees
**0 of A's episodes/memory_units at the DB layer** (FORCE RLS blocks it), and vice
versa. This is load-bearing because the packaged server currently connects as the
scratch-DB login `memphant`, which is **`rolsuper=t, rolbypassrls=t`** (verified
live) — RLS policies are `for all to memphant_app/worker/readonly`
(`bootstrap.sql:1011-1029`) and never fire for a superuser. So `e2e_probe.sh`'s
cross-tenant 404 proves **app-code + tenant-GUC** isolation, not RLS. The probe
gets an episodic cross-tenant leg too (defense-in-depth), **honestly labeled as
app+GUC isolation, not the RLS backstop**. The doc + build-log state explicitly
that **production must run the server under a non-superuser `memphant_app` login**
for RLS to be the real backstop (today it isn't — a standing note, not a C1
deliverable).

## 5. Testing & verification (TDD; AGENTS.md §37)

1. Corpus generator → unit test: 252 rows, deterministic, two tenants, short
   bodies, includes archived + `user_correction` rows; field shape matches the
   backfill mapping.
2. Backfill runner → recall-equivalence assertion inline (gated on drain-complete
   + `degraded=false`) + pytest pinning row→episode mapping.
3. SLO → HTTP-boundary p50/p95 leg (the acceptance number) + `hot_path_slo_pg.rs`
   service-layer CI guard (both assert the 200/500 ms thresholds; fail on breach).
4. RLS → `#[ignore]`d live-PG episodic two-tenant leakage test under
   `set local role memphant_app` (the acceptance proof) + an app+GUC cross-tenant
   leg in `e2e_probe.sh` (defense-in-depth, honestly labeled).
5. Full gate: `pytest tests/`, `cargo fmt --check`, `cargo clippy -D warnings`,
   `cargo test --all-targets --all-features`, `check_spec_drift.py`, the
   scratch-DB live-PG leg, `e2e_probe.sh`. Two-seed rule for any promotion;
   negative artifacts kept.

## 6. Deferred (honest, same boundary as C0/C3)

- Live Syndai `_load_episodic` / `episodic_controller` rewiring (needs the SDK
  adapter bridge "Task 6" + a real Syndai context binding; dogfood default-off ⇒
  nil blast radius).
- The 252 **real** prod rows (needs explicit per-op prod authorization; local DB
  is wiped). C1's backfill mechanism + goldens execute against real rows the
  moment they're authorized/available — the runner is corpus-source-agnostic.
- Re-embed of the 54 behavioral rows / other layers (facts, persona, timeline
  presentation) — out of the episodic slice.

## 7. What this explicitly is NOT — and what C1 does NOT prove

Not a Syndai-side hot-path rewrite; not a live Syndai-vs-MemPhant recall-quality
comparison (deferred to the C3-style golden when a volume corpus exists — its
runnable procedure already documented); not a new memory architecture (every
verb already exists end-to-end).

**Blunt residual-value statement (answers the "thin slice" critique).** The three
bars are deliberately close to existing tests — the value is precisely the
deltas, and nothing more:

- Bar 1 proves the SLO on the **packaged PG runtime at the HTTP boundary**, which
  `hot_path_slo.rs` (InMemory, in-process) never did — the exact STATUS §6 gap.
- Bar 3 proves **episodic two-tenant leakage under the real `memphant_app` RLS
  role**, and surfaces that the packaged server today runs as a superuser that
  bypasses RLS — a security finding, not a re-run.
- The backfill runner is the **reusable, corpus-source-agnostic cutover
  mechanism**; it runs against the real 252 prod rows the moment they are
  authorized, with zero code change.

C1 does **NOT** prove: recall QUALITY parity (no oracle; deferred to the golden);
that the live Syndai loader cuts over correctly (deferred — needs the Task-6
adapter bridge + a real context binding); that RLS holds on the *served* HTTP
path (the server isn't run under `memphant_app` yet — standing note); or anything
about the real prod corpus distribution (synthetic only). These are stated so the
honesty is in the doc, not implied by omission.

## GSTACK REVIEW REPORT

| Review | Trigger | Why | Runs | Status | Findings |
|--------|---------|-----|------|--------|----------|
| Eng Review | `/plan-eng-review` | Architecture & tests (required) | 1 | clean | 6 issues, 0 critical gaps, all folded |
| Outside Voice | Claude subagent | Independent 2nd opinion (Codex timed out at 2m → fell back) | 1 | issues_found | 8 findings; 4 material (Bar3-RLS, Bar2-state-filter, flag-posture, DTO-field-gap), all verified in code + folded |

**Completion summary**
- Step 0 Scope Challenge — scope accepted as-is (2 scripts + 1 Rust test + 1 probe leg + docs; under the 8-file/2-class smell threshold; all [Layer 1] reuse of `gate_runtime`/`role_matrix.rs`/`e2e_probe.sh`).
- Architecture Review — 2 issues (Bar 2 equivalence surface; Bar 1 SLO boundary), both folded.
- Code Quality Review — no issues (DRY: reuses `gate_runtime` classes; zero new abstractions).
- Test Review — coverage diagram produced; 0 critical gaps (every bar has a runnable check that exits non-zero on failure).
- Performance Review — no issues (backfill is 252 sequential retains + one drain; recall IS the measured hot path; listing is cursor-paginated).
- Outside voice — ran (Claude subagent); 4 material findings verified in code and folded (see below).
- NOT in scope / What already exists — written (§6 deferred boundary; §4/§7 reuse map).
- Failure modes — 0 critical gaps (each bar fails loud; Bar 2 gated on `degraded=false` so infra faults don't masquerade as divergence).
- Parallelization — Lane A: corpus generator → backfill runner (sequential, shared `scripts/`). Lane B: `hot_path_slo_pg.rs` + episodic RLS Rust test (independent, `crates/memphant-store-postgres/tests/`). Launch A + B in parallel; the probe legs depend on the runner (Lane A).

**CROSS-MODEL TENSION:** The outside voice argued C1's residual value is thin (all three bars ≈ re-parameterized existing tests) and proposed dropping the new harnesses in favor of `Episodic`-kind assertions on existing tests. Resolved toward keeping C1 as scoped — the deltas (packaged-runtime HTTP SLO, episodic role-bound RLS, reusable corpus-agnostic backfill mechanism) are the point of the spine slice — while folding the critique into an explicit "What C1 does NOT prove" section (§7) so the honesty is in the doc. All other outside-voice findings were accepted and folded, not contested.

**VERDICT:** ENG review complete, all 6 findings folded (2 primary + 4 material outside-voice). Design is internally consistent and now states its own limits. Ready to implement.

NO UNRESOLVED DECISIONS

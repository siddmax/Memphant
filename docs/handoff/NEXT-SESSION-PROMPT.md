# Next-Session Prompt (paste this to resume the MemPhant SOTA campaign)

Current STATUS mirror: RUNTIME COMPLETE — BENCHMARK EVIDENCE PENDING

> Update 2026-07-10 (later same day): the scaled n=100 campaign COMPLETED —
> see `docs/build-log/2026-07-10-scaled-reader-campaign.md`. Step 1 below is
> done (turns granularity promoted, rerank-off re-confirmed); resume at step 2.

> Update 2026-07-10 (evening, runtime-chunks campaign —
> `docs/build-log/2026-07-10-runtime-chunks-campaign.md`): **RUNG 4 CLOSED**
> (first real-evidence rung closure): reflect-stage contextual chunks +
> chunk-aware packing shipped default-on (`e669a3f`), ΔQA +0.110 excl 0 through
> the runtime; turns lane default superseded back to `session` (runtime ties
> client-side windowing). Falsified: w=8 windows, budget 16384, global reader
> prompt v2 (all ns). New harness: `scripts/run_reader.py --engine openrouter`
> (key via Doppler `syndai/dev`; reader `openai/gpt-5.6-terra`, judge
> `anthropic/claude-sonnet-5`, different-family) — built after the codex CLI
> quota outage; both baselines re-scored so all deltas are same-lattice paired.
> Step 2(a) is DONE; of step 2's remaining levers, next ranked work is:
> (1) multi-session composition (weakest stratum, 0.33 under promoted config —
> per-session diversity quota in packing), (2) query-date temporal filtering
> (+ stratum-targeted prompting; chunks+v2 hit temporal 0.78), then steps 3–5
> below unchanged.

You are resuming the MemPhant SOTA campaign in /Users/sidsharma/Memphant (sibling
repo /Users/sidsharma/Syndai; spec mirror must stay drift-free via
`python3 scripts/check_spec_drift.py` after any spec edit + rsync). Read, in
order: docs/handoff/2026-07-10-sota-campaign-handoff.md (umbrella state),
docs/build-log/2026-07-10-reader-campaign.md (n=30 verdicts),
docs/build-log/2026-07-10-scaled-reader-campaign.md (n=100, has
RESULTS_PLACEHOLDER if scoring didn't finish), docs/superpowers/specs/memphant/STATUS.md
(ledger + promotion-provenance rule) and 27-sota-ladder-and-validation.md §2
(rung gates). AGENTS.md holds the full local gate; run it before claiming done.

## Where things stand (2026-07-10 end of session)

- Runtime is COMPLETE and proven: `bash scripts/e2e_probe.sh` (needs the
  memphant-postgres-1 container on :5432) must stay green.
- Benchmark truth so far (LongMemEval-S pinned sha256 in
  benchmarks/manifests/longmemeval_s.lock.json; runtime=postgres; fastembed
  bge-small; seed 20260710): n=30 retrieval R@5 0.500/R@10 0.607; haiku-read QA
  0.433 baseline, 0.467 rerank-off; rerank HARMFUL at retrieval (+0.143 R@5
  when disabled, CI excl. 0) → rerank off by default (`7dad881`); 4-turn-window
  ingestion falsified at n=30 (retrieval up ns, QA −6.7). No rung 4–13/15
  promoted; 14 retired.
- Scaled n=100 campaign is MID-FLIGHT: all three retrieval runs + evidence
  JSONLs exist under docs/build-log/artifacts/real-retrieval-20260710/
  (scaled-lme-s-{session-rerank-off,session-rerank-on,turns-rerank-off}.json +
  reader-evidence-scaled-*.jsonl, gitignored); reader engine = Codex CLI,
  model gpt-5.6-terra (scripts/run_reader.py --engine codex, committed
  `85ac338`; sha256 reply cache means re-scoring is resumable for free).

## Do next, in order

1. **Finish the scaled scoring**: run scripts/run_reader.py for the three
   configs exactly as documented in the command block of
   docs/build-log/2026-07-10-scaled-reader-campaign.md; fill the
   RESULTS_PLACEHOLDER/DEVIATIONS_PLACEHOLDER sections with the real table
   (QA acc + paired bootstrap CIs vs session-rerank-off). Verdicts to apply:
   rerank default keep/revert per QA CI at n=100; turns falsification
   confirm/overturn; any positive-CI lever ships as default (code + tests +
   STATUS rung row with proof pointer — first real-evidence promotion).
   Commit `docs(memphant): scaled n=100 reader campaign + verdicts`; sync
   mirror; Syndai commit staged to docs/superpowers/specs/memphant only.
2. **Next accuracy levers (reader-scored, paired, same seed)** — weakest
   strata are knowledge-update and temporal-reasoning: (a) contextual chunks
   WITH session-context headers (rung 4 axis — window bodies got falsified,
   headers may fix it); (b) update-chain surfacing (supersedence-aware recall
   for "current X" questions); (c) evidence-pack size k sweep (10→15→20);
   (d) query-date-aware temporal filtering. Promote only on QA paired CI
   excluding zero at n=100.
3. **Rung evidence with the right corpora** (doc 27 §2 gates): rung 10 needs
   STATE-Bench-style tasks; 11 the longitudinal suite; 15 an OP-Bench
   restraint check; 13 archived-trace training floor (bench lane emits real
   traces now).
4. **STATE-Bench first-mover run** (spec's primary target; paired
   memory-on/off ablation to attribute the delta).
5. **Syndai RAG/KB replacement gate**: build a golden set mined from Syndai's
   own knowledge corpus (backend/src/features/knowledge/, tables
   knowledge_sources/sections/chunks); MemPhant must BEAT it before replacing
   (same gate later for episodic memory and the coding-continuity lane over
   the 62k coding_execution_attempt_events; design in
   docs/superpowers/plans/2026-07-09-sota-gap-closure.md Phase 2(b)). First
   wire the dogfood flag (`memphant_file_memory_dogfood_enabled` in Syndai
   backend/src/core/config.py) against a deployed MemPhant.

## Known small fixes queued

- RESOLVED 2026-07-12 (audit follow-up, same class as the direct-unit `unit_ids`
  fix — a write path misusing a bounded READ method): `reflect_recorded` loaded
  its dedup/supersede working set via `fetch_recall_candidates(.., usize::MAX)`,
  but PgStore's recall pool caps at the 100 most-recent units (InMemoryStore
  returns the whole scope — so every in-memory test passed while production was
  broken). In any scope >100 units, an update whose prior unit had aged past the
  window failed to supersede: high-trust **semantic** updates hard-failed on the
  `memphant_memory_unit_scope_subject_idx` unique index; beliefs/candidates
  silently duplicated. Fix: new write-seam store method `fetch_scope_open_units`
  (all `transaction_to is null` units in a scope, both stores identical);
  `reflect_recorded` uses it instead of the ranked recall pool. Recall keeps its
  bounded pool. Guard: the shared contract scenario
  `semantic_update_supersedes_unit_aged_past_recall_window` (verified RED → GREEN).
  Structural gap CLOSED 2026-07-12: the InMemoryStore and PgStore contract
  scenarios are now ONE generic-over-`MemoryStore` suite in the
  `memphant-store-testkit` dev crate, run against both backends (in-memory by
  default in `memphant-core`, PgStore `#[ignore]`+DB-gated in
  `pg_store_contract`) via a tiny `StoreHarness`. Extracting it immediately
  surfaced a second, benign divergence the mirrored files had hidden: a deduped
  retain leaves 2 pending reflect jobs on InMemoryStore vs 1 on PgStore (PgStore
  dedups the reflect job). Both guarantee `>= 1` (recompilation happens either
  way), so the shared assertion is the invariant, not the incidental count — no
  product change. Any future trait-method divergence now fails on at least one
  store.

- RESOLVED 2026-07-12 (shared-suite audit, same divergence class): InMemoryStore's
  `apply_forget` under-mirrored PgStore — it marked units `Deleted` WITHOUT setting
  `transaction_to`, so a forgotten unit leaked back through `fetch_scope_open_units`
  (the reflect write seam) on the in-memory store only; and it never deleted the
  unit's embedding, so a forgotten unit stayed vector-visible in-memory. Both are
  the exact InMemory/Pg divergence class. Fix: `ForgetWrite` gained a `now` field
  (threaded from the clock, like `CorrectionWrite.now`); InMemory `apply_forget` +
  `delete_composed_dependents` now close `transaction_to` and purge embeddings;
  PgStore binds `forget.now` for `transaction_to` instead of SQL `now()` (matches
  the correction path, deterministic under a test clock). Guard: shared scenario
  `forget_by_unit_closes_and_purges` (RED on pre-fix InMemory, GREEN on both).

- RESOLVED 2026-07-12 (recall-completeness divergence, MEDIUM): `RecallMode::
  Exhaustive` calls `fetch_episodes_for_scope(.., usize::MAX)` (lib.rs ~2233)
  intending the whole scope; PgStore silently capped at `limit.min(1_000)` while
  InMemoryStore honored `usize::MAX`. The consumer `l4_exhaustive_candidates`
  re-ranks the FULL episode set and looks up each unit's source episode in that
  slice — so on Postgres a relevant-but-not-recent unit (episode past the 1000
  recency cut) silently got no L4 score, and "Exhaustive" wasn't exhaustive. A
  prior note deemed the cap "acceptable"; that predated tracing the consumer. Fix
  (chosen: honor the limit): PgStore `fetch_episodes_for_scope` now binds
  `i64::try_from(limit).unwrap_or(i64::MAX)` — the caller's limit is authoritative,
  `usize::MAX` → effectively `LIMIT ALL` (avoids the `-1` wrap). Guard: shared
  scenario `fetch_episodes_honors_large_limit` (seeds 1001 episodes, asserts the
  whole scope returns; RED on pre-fix Pg's 1000 cap, GREEN on both).

- RESOLVED 2026-07-12 (DRY): the reflect dead-letter threshold `5` was a bare SQL
  literal in TWO PgStore sites (`claim_reflect_jobs` sweep `attempts >= 5` +
  eligibility `attempts < 5`) while core single-sources it as
  `JOB_DEAD_LETTER_ATTEMPTS`. Both now bind the const — one source of truth, no
  drift. Behavior-identical; existing `exhausted_jobs_dead_letter` /
  `dead_letter_sweep_stays_within_the_claim_filter` tests guard it.

- AUDIT CONCLUSION 2026-07-12 (store-divergence sweep, 3 rounds): the
  `MemoryStore` trait pair was the ONLY "two implementations that must agree"
  surface — a method-by-method sweep of all 33 trait methods plus a check for
  core-logic reimplementation found NO other divergence pairs (memphant-eval /
  -runtime / -mcp are thin adapters over the single core; they do not reimplement
  reflect / write-compiler / recall logic). The single-core / thin-adapter
  architecture holds. The shared contract suite (`memphant-store-testkit`, 12
  in-mem / 25 pg scenarios) is now the durable guard for this class. Remaining
  per-store differences are by-design (recall ranked-pool cap; `claim_reflect_jobs`
  queue fairness/visibility-timeout), unreachable (vector 1000-cap — callers pass
  32), or production-masked/misuse-only (`scope_memory_page` clamp,
  `complete_reflect_job` tenant laxity, edge dedup) — not worth converging.

- ~~bench-lme shares the runtime Postgres and leaves reflect-job debris that
  starves fresh jobs on the worker's global oldest-first tick (bit the e2e
  probe 3× over 2026-07-09..10).~~ RESOLVED 2026-07-11: every harness now mints
  its own ephemeral DB via `scripts/with_scratch_db.sh` (migrated, dropped on
  exit even if killed) — the probe + pg contract/worker tests re-exec through
  it in shell, and the two bench runners (`gate_run_memphant.py`,
  `code_lane_run_memphant.py`) now self-re-exec through it too
  (`gate_runtime.reexec_through_scratch_db`, guarded by `MEMPHANT_SCRATCH_ACTIVE`
  exactly like the probe). No shared named DB (`memphant_gate` / `memphant_code_r0`)
  remains; a killed bench cannot leave debris in any DB a live harness reads.
- ~~`job_state` writes-by-id seq-scan: `complete_reflect_job` / job-result update
  use `where id = $1`, which the composite PK `(tenant_id, id)` can't serve, so
  each is a seq scan.~~ RESOLVED 2026-07-11: `complete_reflect_job` now carries
  `tenant_id` (trait + PgStore + InMemoryStore + runtime delegate + both
  `service.rs` call sites via `job.job.tenant_id`) and keys on
  `where tenant_id = $1 and id = $2`. The job-result update was already scoped
  (`tenant_id + id + compiler_version`) — only the completion write remained.
  EXPLAIN on the live 165k-row table: bounded 2-column PK lookup (cost 8.44) vs
  the old id-only full-index scan (cost 7104 — not a heap seq scan; Postgres used
  `job_state_pkey` but couldn't bound the leading `tenant_id`).
- RESOLVED 2026-07-11 (post-fix audit follow-ups):
  - Embedding write-through is now atomic + idempotent. Reflect embeds BEFORE
    the persist tx and threads the rows into `persist_compiled_units` (via new
    `CompiledWrite.embedding_profile`/`embeddings`), so units + embeddings +
    idempotency marker commit together; an embed failure writes nothing and a
    retry recomputes instead of short-circuiting on a marker whose embeddings
    never landed. Corrections now embed the replacement unit in the
    `apply_correction` tx (via `CorrectionWrite.embedding`) so corrected truth
    is vector-visible immediately (was silently absent from the inner-joined
    vector channel). BDD tests in `tests/embedding_channel.rs`.
  - `MemoryStore::begin()` returns `Result<Self::Txn, StoreError>` (was
    infallible + `.expect`), so pool exhaustion / DB restart degrades to
    `backend_unavailable` instead of panicking the write path.
  - `review_event` PK restored to composite `(tenant_id, id)` to match the
    tenancy convention every other domain table follows (migration 002 rewrote
    it id-only); `review_event_unit` FK is now the tenant-composite form. Only
    `api_key`, `tenant`, `schema_migrations` remain deliberately id-only.
  - `tests/tenant_scoped_writes.rs` guards the whole partial-PK class: a new
    `update`/`delete` on a composite-PK table without `tenant_id` fails CI.
  - Dead `_resource_id_type_anchor` + its `ResourceId` import removed.
- RESOLVED 2026-07-12: retain of a DIRECT unit returned an unreliable `unit_ids`
  for scopes with >1000 units — `service.rs` recovered the created id via a
  `scope_memory_page` re-query (clamped to 1000, ordered by id) plus a `body`
  match, so a fresh unit past the clamp fell off the page. `reflect_recorded`
  now returns `(ReflectTrace, Vec<UnitId>)` and the direct-retain path uses those
  ids directly; the body-match re-query is gone. Regression guard:
  `pg_store_contract::direct_unit_retain_returns_unit_id_past_scope_page_clamp`.
- RESOLVED 2026-07-12 (reclaim resource double-insert): `persist_compiled_units`
  now takes a `for update` row lock on the job_state idempotency record, so a
  reclaimed re-compile serializes at the check instead of racing at READ
  COMMITTED. The second writer blocks until the first commits, then sees
  `result` set and no-ops — covering ALL compiled kinds, not just the semantic
  units the partial unique scope-subject index already protected. Regression
  guard: `reclaim_idempotency::reclaimed_resource_job_recompile_does_not_double_insert_units`
  hammers 64 truly-parallel compile races (red before the lock, green after).
- RESOLVED 2026-07-12 (correct an already-superseded generation): `apply_correction`
  in BOTH stores guarded the target with `state <> 'deleted'` only, so
  re-correcting the SAME unit id (double-submit / retry / concurrent) re-superseded
  an already-closed row and minted a SECOND live generation — same class as the
  reclaim double-insert. The partial unique scope-subject index masked it for
  `semantic` kinds (unique-violation error) but resource/belief/procedural
  silently double-inserted; in-memory (no index) duplicated any kind. Fix: the
  target must be the OPEN generation (`transaction_to is null`) — with the
  existing `for update` lock the second writer now re-reads the superseded row,
  fails the predicate, and returns NotFound. Guard:
  `surface_mutations::correct_rejects_an_already_superseded_generation`
  (deterministic; red before the predicate, green after).
- SURVEYED clean (2026-07-12, same audit): every other store write is already
  race-safe — `stage_episode`/`enqueue_reflect` are atomic `insert … on conflict
  do update` (unique-backed), `insert_edge`/`forgotten_source`/`review_event`/
  `retrieval_trace`/embeddings use `on conflict do nothing/update`, `apply_forget`
  mints a fresh `deletion_generation` per call (no counter race). The check-then-
  write-without-a-backing-constraint pattern existed only in `persist_compiled_units`
  and `apply_correction`. No static anti-pattern lint added: the safe cases are
  textually identical to the unsafe ones (all `select`+`insert`), so a scanner
  would be pure false positives — the two runtime guards above are the check.
- InMemoryStore has no reclaim window: a failed job stays `claimed` forever
  (attempts frozen at 1), diverging from Pg. Dev/test-only; align if the
  in-memory store is ever used to model worker retry semantics.
- RESOLVED 2026-07-12 (third audit round — MCP edge hardening; REST edge audited
  clean, and the lexical channel confirmed structurally immune to the embedding
  desync class because `body_tsv` is a Postgres generated-stored column):
  - MCP streamable-HTTP transport now enforces per-request auth: an axum layer on
    `/mcp` requires `Authorization: Bearer <MEMPHANT_API_KEY>` (constant-time
    compare) outside dev mode, so a widened `MEMPHANT_MCP_BIND` no longer serves
    the bound tenant unauthenticated. Decision logic in `mcp_http_authorized`.
  - MCP tool errors no longer leak raw backend detail: `mcp_error` mirrors the
    REST edge (`CoreError::Store(_)` -> generic "backend unavailable"; validation
    / not-found / policy messages still surface). Tests in `tests/edge_auth.rs`.
  - Recall `limit`/`budget_tokens` are clamped (defensive ceilings), matching the
    scope endpoint. No DoS existed (output is candidate-pool-bounded); symmetry only.
- DEFERRED (schema ahead of implementation — mark so it is not mistaken for
  wired): `event_outbox` is fully defined (table, RLS, indexes, 6-value event
  enum) but no code ever writes it — the transactional-outbox surface emits
  nothing. `citation`, `scope_block`, `belief_observation` are dead tables (schema
  + existence-check only, no reads/writes). Either wire or document as post-R0.
- WON'T DO (audit round 3): a crate-wide clippy `deny(unwrap_used/expect_used/
  panic/indexing_slicing)` panic-gate — the request path is already clean and the
  lint is a false-positive magnet on internal code (violates the no-false-positive
  bar). Latent footguns left as-is (not triggerable on the prod config): `enum_str`
  uses `.expect`; the public `reflect` verb lacks the worker's `catch_unwind`
  around `compile_job`.
- Global background claim/sweep still seq-scan `job_state` when the worker
  passes an empty filter (no tenant to lead the index). Fine while job_state is
  small; if it grows (no GC of `done` rows), add a partial index on
  non-terminal rows, e.g. `... (created_at) where state in ('queued','running')`.
  Also unaddressed: cross-tenant *fairness* — one tenant's old backlog can
  monopolize the global oldest-first worker. Needs real multi-tenant load
  before choosing a policy; deliberately not built now.
- Internal golden/security/ops eval subcommands are in-memory only — add
  --database-url paths so they also gate the Postgres runtime.
- RLS policies + non-owner runtime role (currently app-layer tenancy only,
  stated in the 2026-07-09 plan NOT-in-scope); typed DTO timestamps.
- R79 adapters (Claude Code memory_20250818 file adapter + Hermes provider
  SPI) — the distribution wedge once accuracy plateaus.

## Rules that bind you

- Promotion-provenance rule (STATUS header): evidence only from the packaged
  Postgres runtime on pinned real corpora with executed scorers; synthetic
  fixtures gate regressions, never promotions. Never fabricate a number.
- Accuracy/UX > cost > perf/latency (owner authorized answer-model budget
  until SOTA; cost optimization comes after).
- Six verbs + tri-domain contract are frozen; one substrate; no competitor
  code in the dependency tree (adapters = distribution only).
- Full local gate (AGENTS.md) + spec-drift green before any "done" claim;
  commits small with Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>.

> Update 2026-07-11 (FINAL v2 — supersedes everything above): plan of record is
> docs/reports/2026-07-11-prosumer-memory-campaign-report.md §9 (Memory-OS plan v2;
> §6/§8 remain as evidence context). Order: R0 embedder bakeoff FIRST (local
> bge-base/modernbert-embed-large/EmbeddingGemma vs API voyage-context-4/
> voyage-4-lite/gemini-embedding-001; API ships only on ≥3pt docs-QA win, CI>0, two
> seeds; chat lane stays local for privacy; voyage-code-3 code sub-bakeoff) → R1
> flip the Syndai docs gate → R2 chat n=300 seed 20260712 + full-500 confirm
> (Chain-of-Note v4, hot-plane profile block, temporal, HyDE) → R3 governance spec +
> hot/file planes (markdown exports = projection-not-source, owned regions) → R4
> coding lane + outcome write-back over the 63.6k events → R5 MemoryStress+FAMA
> longitudinal gate (demotion/consolidation adjudicated there) → R6 replacement
> wiring. Verified: voyage-context-4 real (vendor numbers only); OpenAI
> text-embedding-4 does NOT exist; fastembed has modernbert-embed-large, NOT
> gte-modernbert. Binding: five-plane architecture (hot ≤1k / warm verbatim / cold
> demotion-not-deletion / file plane / governance core); forgetting = demotion,
> hard-delete only for privacy; verbatim is the memory; deterministic writes.

> Update 2026-07-11 (FINAL v3 — R0 DONE, supersedes v2's "R0 first"): the embedder
> bakeoff is COMPLETE (`docs/build-log/2026-07-11-r0-embedder-bakeoff.md`; CI table
> `docs/build-log/artifacts/r0-embedder/r0-verdict-cis.json`). Verdicts: NO API
> embedder promotion (voyage-context-4 best case fails the ≥3pt/CI-floor/two-set
> bar); docs-lane winner modernbert-embed-large (plan-selected, `--embed-model
> modernbert` / `MEMPHANT_EMBEDDINGS=modernbert`, grammar in `embedder_from_id`);
> chat stays bge-small (ns ×3 models); qwen3 retired (CPU); code lane no API case
> at 40Q sample scale. THE control-arm finding: Syndai's own embedder on our stack
> scores .167 vs Syndai's .217 → the gate gap is heading-path context + fusion.
> **Next = R1: flip the Syndai docs gate** — levers in evidence order: (1) prepend
> heading-path/breadcrumb context to section bodies at ingest (Syndai does this,
> we don't), (2) fusion behavior at 3k-section scale (lexical families win RRF
> votes), (3) contextualized-embedding arm (local late-chunking approx vs
> voyage-context-4, same promotion bar). Re-run `scripts/gate_run_syndai.py` +
> `gate_run_memphant.py --embed-model modernbert` + `gate_compare.py` on BOTH
> golden sets (v2 = `syndai_docs_golden_v2.jsonl`, seed 20260712). Harness facts
> that bind analysis: ±1-question re-ingest variance at n=60 (stable-tiebreaker
> follow-up open); R@5==R@10 pack-budget censoring; run scripts take GATE_PORT.
> R0's 5 SDD incidents/fixes and open minors are ledgered in
> `.superpowers/sdd/progress.md`.

> Update 2026-07-12 (FINAL v4 — R1 DONE, supersedes v3's "R1 next"): **the Syndai docs
> gate FLIPPED** (`docs/build-log/2026-07-12-r1-docs-gate.md`): modernbert @ k=50/b8192
> beats Syndai .367/.400 vs .217/.267, pooled +0.142 [+0.058,+0.225], both sets excl0 —
> with the honest asterisk that this operating point feeds the reader ~14× Syndai's
> evidence volume; at k=10 we still lose. Falsified: breadcrumb/heading-path lever.
> Built + flag-gated (not promoted): resource contextual chunks (43b8702,
> `MEMPHANT_RESOURCE_CHUNKS`/`--resource-chunks`; attr +0.042 ns at depth). Landed
> hygiene: recall body-tiebreak determinism; with_scratch_db.sh isolation everywhere.
> **Next per §9 order: R2 chat round** (n=300 seed 20260712 → full-500 confirm:
> Chain-of-Note v4, hot-plane profile block, temporal re-measure, HyDE) — BUT the R1
> asterisk names a docs-lane fast-follow worth slotting first if owner agrees:
> **rank compression** (W8 cross-encoder seam server-side, or fusion re-ordering) to
> get k50 quality into k10 budgets — flips the gate at comparable volume, cuts reader
> cost ~5×, and is the same lever the R2 chat lane's pack-displacement failures point
> at. R6 replacement stays gated on that or explicit cost acceptance. Docs-lane recall
> recommendation for Syndai integration TODAY: k=50, budget_tokens=8192, modernbert.

> Update 2026-07-12 (FINAL v5 — R1.5 DONE, supersedes v4's decision point): rank
> compression measured (`docs/build-log/2026-07-12-r15-rank-compression.md`).
> Shipped default: recall_pool_depth=64 (k-invariance correctness contract).
> Cross-rerank: +0.158 excl0 attribution (campaign's biggest lever) but 13s/query
> CPU on full sections -> flag-gated, latency-retired; follow-ups named:
> truncated-input rerank, top-32 pool, smaller reranker, async — any 4-8x cut
> ships it. Chunks: 3rd ns, retirement candidate. R6 NOT unlocked (best
> comparable-volume arm floor exactly 0.000). NEXT per §9 order: **R2 chat round**
> (n=300 seed 20260712 → full-500 confirm: Chain-of-Note v4, hot-plane profile
> block, temporal re-measure, HyDE) — the reader-side levers R2 tests are exactly
> where the chat lane's headroom lives (R@10 .83-.94, QA .56). The rerank-latency
> follow-up can ride any later docs round; it is measurement-ready (flag + trace
> ms field all wired). Chat-lane regression baseline for R2: r15-docs/chat/
> reader-small-20260710.json (QA .550 on binary 800ac41).

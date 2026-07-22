# Tri-Domain SOTA Plan — 2026-07-21 (plan of record)

**Provenance.** Synthesized from an 8-team parallel research pass (library docs,
2026 web research, OSS repo study, MemPhant deep-read, Syndai cutover map,
test/eval audit, experimental proposals, devil's advocate). Full reports
preserved in the session scratchpad; every load-bearing claim below names its
source team. Supersedes the sequencing sections of
`docs/handoff/NEXT-SESSION-PROMPT.md` (the standing rules there remain binding
except where §6 amends the evidence ceremony). Owner priorities:
**accuracy/SOTA > cost > speed, best UX above all, KISS/DRY, pre-production.**

---

## 1. The verdict (read this if nothing else)

1. **"One substrate" is rebranded "one store, three profiles."** One Postgres
   schema, six verbs, five kinds, one trust/provenance model — and three
   *retrieval profiles* (chat, docs, code) whose configs (embedder, k,
   granularity, rerank, budget) are promoted independently per lane. The
   devil's-advocate finding is correct: the schema is shared, the retrieval
   products fork. Naming it stops us from pretending one config must win
   everywhere.
2. **The headline is the Syndai cutover, not a leaderboard.** LME-V2's
   leaderboard is empty because nobody watches it (96 stars). We still take the
   cheap first-mover shots (§5) because they cost days, not weeks — but nothing
   product-shaped waits on a benchmark again. This adjudicates the
   web-research-vs-devil's-advocate tension: **cutover is the spine; benchmarks
   are parallel, cheap, and never blocking.**
3. **Deep mode is a product feature first, benchmark arm second.** Explicit
   user action, streamed progress, cited partials, cancellable, hard ceilings
   (120 s / 24 iters / $0.30 — already coded). Fast stays the default with a
   sub-second budget. Nobody pays 100 s implicitly, ever.
4. **The evidence ceremony is reformed, not abandoned** (§6). Keep
   preregistration, append-only ledgers, the provenance rule, and
   no-sealed-tracks-until-exposed-green. Delete micro-dollar liability
   amendments — provider-side spend-capped keys give a harder guarantee at zero
   process cost. Amendment 14's own record (a DB-liveness fix *reverted* to
   preserve a frozen hash) is the proof the old ceremony inverted its purpose.
5. **Speed now matters externally.** OpenViking (ByteDance-backed, 27k stars in
   six months) is executing our exact thesis — unify memory + RAG + skills on a
   filesystem paradigm. Our Rust/Postgres/verbs substrate is ahead on
   governance and behind on distribution. The distribution wedge (§4.3) is no
   longer optional polish.
6. **Three foundations are unproven and must not be treated as built**
   (independent review, 2026-07-21, grounded in the T6 artifacts + STATUS):
   (a) **Deep has never produced one valid live pair** — every attempt aborted,
   the latest on pair 1 (`invalid_output`, 0 tool iterations). The A0
   mock-provider test passes while the real Azure provider still fails, so a
   mock test is NOT the Deep gate. (b) **The run-owned-Postgres controller
   does not exist** — §6.3 describes it as work to do, yet every paid lane
   depends on it; it is the true Week-0 critical-path item, not a footnote.
   (c) **The docs lever is latency-dead against a 4× deficit** — MemPhant
   currently loses docs retrieval 0.050 vs Syndai 0.217 (CI excludes zero) and
   the +0.158 rerank is retired at 13 s/query; rank-compression is hoped-for,
   unbuilt. These reorder the schedule (§8): **free gates and the infra
   controller come first; no paid rung opens until they are green.**

## 2. Answers to the open questions (authoritative)

### Q1 — One thing or separate things (agents / RAG / codebase)?

**One store, three profiles** (verdict §1.1). Evidence: every campaign win and
loss forked by lane (chat is reader-bound; docs won only at deep-recall volume;
code is untested at scale), while the schema, verbs, bitemporality, trust, and
tenancy never needed to fork. Per-profile promotion gates; cheap cross-lane
smokes so a win in one lane can't silently regress another.

### Q2 — How do the memory substrates map? (episodic, temporal, procedural, short, long)

All five are **already representable in the existing kinds + columns — no new
architecture** (codebase team confirms code matches STATUS ledger):

| User concept | MemPhant mechanism | State |
|---|---|---|
| Episodic (what happened) | `MemoryKind::Episodic`, verbatim bodies = ground truth | Built |
| Semantic / long-term facts | `Semantic` + `Belief` with `valid_at`/`invalid_at`/`superseded_by` | Built (validity columns per graphiti-lesson: rows, never a graph) |
| Procedural (how to do things) | `MemoryKind::Procedural` + outcome write-back (worked/failed → units) | Kind built; write-back = P5 later |
| Short-term / working | **Observation block**: reflect-maintained, versioned, prompt-cache-aligned scope summary served as a stable prefix | Dormant (scope_block storage exists, no verb) — activate, §4.2 |
| Temporal | Bitemporal columns everywhere + FSRS retrievability ranking later (fsrs-rs 6.6.1 over the event ledger) | Columns built; FSRS deferred until graded feedback exists |

### Q3 — Storage types: what lives where, and how do they relate?

**Canonical plane: Postgres (relational + pgvector), one source of truth.**
Everything else is a projection or an ingestion source — never a second
authority.

- **Vector**: pgvector. Exact scan is correct today (≤100k units/tenant); add
  HNSW + iterative scans (`relaxed_order`) when any tenant crosses ~100k.
  Quantization ladder is planned, not built: fp32 → halfvec → binary+rerank
  (docs team: all in pgvector already).
- **Lexical**: `tsvector` is the canonical schema — the docs team verified real
  BM25 extensions are **not portable** across Supabase+Neon (AGPL/allowlists).
  tsvector upgrades in place: Neon's lakebase_bm25 indexes standard tsvector;
  feature-detect via `pg_available_extensions`. Quality gap closes in the
  ranker (RRF fusion + optional `sparsevec` learned-sparse lane + rank-
  compressed cross-encoder), not in storage.
- **Flat .md files in a codebase** (CLAUDE.md / AGENTS.md / memory dirs):
  **a PROJECTION — a compiled artifact of canonical memory** with unit-id+hash
  footers. Human edits are hash-detected and re-ingested as ordinary retains
  through admission control → bidirectional sync with no file merges
  (experimental P4). This is also the Anthropic memory-tool surface (§4.3).
- **Flat files in cloud (Supabase Storage / S3)**: **DEFERRED with a named
  trigger** — activate the spec-25 object_store contract when (a) a single
  tenant's resource bodies exceed ~5 GB in PG, or (b) CaaS export/artifact
  needs land. Supabase Storage has **no versioning** (docs team) — history
  stays in our ledger regardless. Devil's advocate is right that it has zero
  users today.
- **Graph**: **rejected, fifth confirmation.** Mem0 dropped graph; Zep closed
  the operable version; mem0 retreated to ADD-only writes in April 2026. We
  adopt the two durable ideas without the engine: an entity-index *table*
  (recall-boost metadata only, never a write dependency) and validity-window
  *columns*.
- **KB**: not a storage type — it's the docs profile + `ResourceKind` +
  citations + receipts (§4.4).

### Q4 — The long-term latency/performance/cost call

**Two-speed UX, cache-aligned rendering, local-first models, metered honesty:**

- **Fast (default)**: sub-second recall; context injection p50 < 200 ms /
  p95 < 500 ms (Syndai's measured budget). Observation block gives the
  session-start context in one SQL read (memobase-pattern), no vector query.
- **Deep (explicit)**: bounded 120 s / $0.30, streamed progress + cited
  partials + cancel. LAFS-style dual operating points are also exactly what
  LME-V2 rewards — product truth and benchmark truth coincide.
- **Rerank**: the +0.158 QA lever ships only rank-compressed (top-16 fused ×
  256-token truncated pairs, ≤1.5 s p95, balanced/docs surfaces). Free
  pre-check before building: count retired-run flips reproducible within
  top-16 (<60 % → don't build).
- **Models**: local embeddings stay default (three bakeoffs: API embedders
  never cleared the bar). Any LLM step (observer/reflector, Deep) picks
  quality first at n=12, then downgrades to cheapest non-inferior.
- **Rendering**: Mastra's cache discipline is adopted wholesale — append-only
  stable prefixes, consolidation off the hot path, degenerate-output detection
  on every LLM condensation.
- **Cost model (CaaS)**: metered deduplicated ingested tokens + retrievals +
  Deep pass-through (the market's honest model per web-research); no
  graph-tier tax because there is no graph.

## Phase 0 — The real critical path (Week 0; free; gates everything paid)

Independent review established that every paid lane secretly depends on these,
and none exist today. Nothing paid opens until all three are green.

- **P0.1 — Run-owned Postgres controller** (§6.3 made real). The campaign
  controller supervises its own dedicated container/`pg_ctl` lifecycle and
  never shares the desktop Docker lifecycle that killed run-65981e4f. This is
  the fix for the failure mode that has recurred across the whole campaign.
- **P0.2 — Code-enforced PG-liveness preflight** (Amendment 14 prose → code):
  a `select 1` against the base DB before any billable row, so a vanished
  container fails at row 0, not mid-root. (Already drafted; lands here.)
- **P0.3 — Live-provider Deep smoke (1 question, ≤$0.30).** The A0 mock test
  proves the *pipeline* changes the answer set; it does NOT prove the real
  Azure provider emits valid tool calls. **UPDATE (investigation 2026-07-21):
  the provider parser is NOT the blocker** — the latest stream diagnostic
  (`f32fdb37`) shows the real Azure path working (200, tool calls reassembled,
  `production_parser_first_failure: null`, settled). The *actual* last abort
  (`diagnostic-dee83e37`) died at **ingestion 139/670** on
  `"contextual chunk span does not match its source body"`, before Deep ran.
  **OUTCOME (2026-07-21): DONE_WITH_CONCERNS — live plumbing PROVEN after fixing
  the real defect the smoke surfaced.** Vehicle (owner-confirmed): the ONE
  buried-evidence case via `memphant-eval run
  benchmarks/rung12-l4-exhaustive-sampled.yaml --l4-runtime-provider` with the
  sonnet Deep env — real OpenRouter provider through the FULL Deep pipeline
  (snapshot→workspace tool loop→validate→pack→trace), provider-enforced $0.30
  cap. The smoke found the reason **Deep had never produced one valid live pair**:
  a `read_file` tool defect (`deep_recall_openrouter.rs:1396` rejected
  `end_line > lines.len()` as `invalid_range` on 1-line episode bodies; two such
  rejections trip `malformed_response_limit=1` → `Partial/InvalidOutput`, empty
  evidence). The single-turn `f32fdb37` diagnostic sat one iteration too shallow
  to see it. Fixed (clamp `end` like `head -N`, TDD); **post-fix 7/7 live runs
  `status=Completed`, 5 tool iterations, 2 nominated sources, ~$0.017 settled** —
  the first reliable end-to-end live Deep pairs. The buried *case* still fails on
  quality (Sonnet nominates the decoy, a legitimate surface answer — A2/A3
  territory). Total spend $0.1288/10 runs, all ≤ $0.30/recall; settlement
  receipts recorded. **Paid lanes stay CLOSED** on the converging A1 verdict
  (below), not on plumbing. Proof: `docs/build-log/2026-07-21-p0.3-live-deep-smoke.md`
  + `docs/build-log/artifacts/p0.3-live-deep-smoke/`.
- **P0.4 — Ingestion chunk-span reliability (the real ingestion gate).**
  Root-caused this pass: both chunkers are proven correct (new
  `chunk_span_invariant_repro.rs` passes on all adversarial byte shapes), and
  every compile path uses one body for both mint and validation — so the
  conflict only arises on a **re-compile/retry against changed state** (the
  proof's own "failed jobs queued for retry" note). The offending input was
  cleaned from disk, so the fix shipped is **diagnostic capture** (the conflict
  now reports unit/chunk/span/lengths/divergence-point/both slices — commit
  `9e53d8c4`), turning the next occurrence from an unreproducible abort into a
  diagnosable one. **Before P0.3/A2 spend, run one ingestion of the LME-V2 dev
  corpus and confirm it reaches 670/670**; if the conflict recurs, the new
  message pins the exact source and the retry/re-compile path gets the real
  fix. This gate protects both the benchmark lane AND the Syndai cutover
  (same ingestion path).
  **OUTCOME (2026-07-21): RESOLVED — did not recur.** One full exposed
  ingestion of the pinned LME-V2 dev corpus reached **670/670 sources** through
  the packaged `MemoryService<PgStore>` runtime with `MEMPHANT_RESOURCE_CHUNKS=on`
  (the exact failing path) and **zero model calls** (`verify-no-model --fixture
  exact`, envelope `verified:true / paid_calls:0`). The reflect queue drained
  monotonically past the old 139/670 abort point to 670/670 with `err=0 dead=0`;
  worker stderr was clean (`drain completed=670`, no conflict line). No fix was
  required — the chunkers were already proven correct at mint time and no
  re-compile/retry path produced the divergence at full scale. Run on a run-owned
  ephemeral scratch Postgres (P0.1 discipline; source DB force-dropped, zero
  orphan clones). Proof:
  `docs/build-log/2026-07-21-p0.4-chunk-span-resolved.md` +
  `docs/build-log/artifacts/p1-t6/p0.4-chunk-span-resolved/`. This clears the
  ingestion precondition for the paid lanes (still gated behind the free A1
  verdict).

## 3. Phase A — Prove T6 (after Phase 0; free → $10)

Ascending evidence rungs; each is a kill-switch for the next. Sources: tests
team (rungs), devil's advocate (free-first discipline), codebase team (wiring).

- **A0 (free, 1–2 days): buried-evidence Rust test.** Seed a store where the
  answer unit misses Fast's whitelist; scripted mock provider nominates it;
  assert Deep's answer set differs from Fast's. Today **no test anywhere
  proves Deep changes the answer set** — this is the first artifact that does.
  Also: encode the PG-liveness preflight in the runner (Amendment 14 made it
  prose; make it code) and add a Deep leg to `e2e_probe.sh`.
- **A1 (free): Fast-miss trace classification — VERDICT BINDS THE WHOLE
  BENCHMARK LANE, not just Phase A.** Classify the ~74 Fast-miss dev questions:
  gold-present-but-unpacked vs gold-absent-from-pool. STATUS's own oracle-gap
  data (baseline QA 0.433, oracle ceiling only 0.584 — a +0.331 *unclosed*
  utilization gap) predicts most misses are present-but-unpacked, which Deep
  (a recall-depth lever) cannot fix. **If ≥70 % is present-but-unpacked: Deep
  drops to diagnostic status, the packing/ordering lever (rung 7) becomes the
  center of gravity, and D1/D3 (LME-V2, full-500) are DEFERRED — not run in
  parallel — because they chase recall depth when the bottleneck is
  utilization.** This is the cheapest single de-risk in the plan; it can
  invalidate half the benchmark roadmap for $0.
  **OUTCOME (2026-07-21): VERDICT FIRES AT ITS MAXIMUM — the depth lane is
  deferred.** FREE, zero model spend: the 178-question dev split classified
  through the product Fast pipeline (session, runtime-chunks, pool 64, k 10,
  8192 budget; same dataset sha `e4667bed`) into three buckets from the
  retrieval trace alone (`bench-lme --emit-trace-classification`). Of 166 scored
  questions (12 `_abs` set aside): **absent-from-pool = 0 (0.0 %)**,
  in-pool-unpacked = 64 (38.6 %), in-top-k = 102 (61.4 %). Present-but-unpacked-
  or-unread (B+C) = **166/166 = 100 %** — far past the ≥70 % threshold. **ZERO
  dev misses are recall-depth-bound**, so Deep (a depth lever) cannot fix a
  single one. Mechanism: pool median 47 ≈ every ingested session (gold is always
  in the pool at depth 64), but packed median is only 4 items under the 8192
  budget — **packing/ordering (rung 7) is the bottleneck, not depth.**
  Consequence (binding): **Deep → diagnostic status; packing/ordering (rung 7)
  becomes the center of gravity; D1 and D3 are DEFERRED (not run in parallel).**
  NB this run's r@10 (0.614) is below the pinned 2026-07-13 0.777 because it
  carries two bench-lme ingestion fixes the older report predates (`observed_at`
  RFC3339 + duplicate-session-id keying, both found by this run); absent=0 is
  robust to that (pool depth ≥ session count). Proof:
  `docs/superpowers/specs/2026-07-21-a1-fast-miss-classification-design.md` +
  `docs/build-log/2026-07-21-a1-fast-miss-classification.md` +
  `docs/build-log/artifacts/a1-fast-miss-classification/`.
- **RUNG 7 (packing/ordering — the A1-elevated center of gravity). DIAGNOSED +
  LEVER FOUND (2026-07-21, FREE).** The 64 in-pool-unpacked dev misses are
  **100 % `Budget` drops** (zero dedup, zero rerank, zero scan-depth): gold at
  median fused_rank 2 is budget-dropped because ~3230-tok whole-session bodies
  exhaust the 8192 pack budget (only ~4 items fit; probe-verified). The cause is
  **per-item cost, not total budget**: `packed_render` gave each item a render
  budget of its whole body, so chunk-render refilled it to ~4600 tok and hogged
  the budget. **Lever (`PackLevers.pack_render_cap`, default OFF): cap each
  item's chunk-render at 1200 tok.** Paired dev retrieval (166q, seed 20260713,
  FREE) vs the 8192 baseline: **r@10 0.6145 → 0.8494, Δ+0.2349 [95 % CI +0.169,
  +0.295]** — bigger than doubling the budget (16384: Δ+0.151 [+0.096, +0.211])
  AND at the SAME 8192 budget (tighter reader context, the opposite of the
  ns-harmful 16384-on-QA finding). Improves every stratum. **This is a
  RETRIEVAL win; reader-QA is a separate gated (paid) step.** **Two-seed rule
  SATISFIED:** seed 20260710 reproduces Δr@10 +0.2349 [+0.175, +0.301]
  identically (whole-split sample ⇒ seed-invariant, deterministic). Lever ships
  OFF; default-flip gated on paid reader-QA. Reconciles with
  [[memphant-packing-gate-verdict]] (that is the output-full Rerank branch; this
  is the Budget path). Proof:
  `docs/build-log/2026-07-21-rung7-packing-diagnosis.md` +
  `docs/build-log/2026-07-21-rung7-packing-lever.md` +
  `docs/build-log/artifacts/rung7-packing/` (PREREGISTRATION, paired reports,
  per-question drop-cause).
- **A2 (~$1–2.5 realistic, ≤$5.70 cap): the authorized n=12** on
  run-d2f4fcb3, babysat, on a run-owned Postgres (dedicated container, not
  the shared Docker Desktop lifecycle that killed run-65981e4f). Preregistered
  pass predicates already frozen (mean delta > 0, wins > losses, p50 ≤ 45 s,
  mean cost ≤ $0.10).
- **A3 (≤$9): bench-lme Deep wiring.** CORRECTION (review #3): this is NOT a
  ~10-line change. `bench_lme.rs` maps `RecallMode::Deep` to a label string
  only (line ~1089); there is no deep-provider install, no per-question async
  billable dispatch, no settlement/cancellation plumbing in the bench path.
  Cross-rerank is synchronous and in-process; Deep is async, billable,
  cancellable, externally settled. First do a **free zero-call mock-provider
  spike** of the dispatch+settlement into bench_lme behind a flag; only if that
  drops in cleanly commit the paid n=30 paired fast-vs-deep on LME-S targeted
  at Fast-miss questions. If the settlement plumbing resists, rescope — do not
  let A3 silently consume the cutover's week.
- **A4 (only after A2+A3 pass): preregister n≈100 — not 300** (most of the CI
  width at one-third of the ~200 CPU-hour construction bill); de-pin the
  controller's 12/24 constants; parallel construction across scratch DBs.

## 4. Phase B — Substrate & product activation (parallel with A; 1–2 weeks)

Everything here activates dormant machinery — no new architecture (codebase
team verdict). Each item carries an n=12-style falsification gate.

1. **B1 — Observation-block hot plane** (experimental P1; three converging
   external evidence lines). Reflect worker maintains a versioned,
   date-annotated observation generation per scope; served tri-surface: chat
   prefix + Deep workspace file 0 + .md projection header. Unbound from T6
   (the everyday-UX lever must not queue behind the flakiest gate). **SCOPE
   CORRECTION (review): this is NET-NEW feature work, not "activation."**
   `scope_block` exists only as a table name in the Postgres schema — no verb,
   no read path, no write path. The 1-week estimate must budget for building
   the verb + recall injection + reflect-worker generation swap, not just
   flipping a flag. Gate: n=12 from the displacement/reader failure classes;
   ≥+2 net flips.
2. **B2 — File plane as projection** (experimental P4): extend the dormant
   `compile` CLI; unit-id+hash footers; hash-detected human edits re-ingested
   via admission control. Gate: 4-edit-class round-trip + compile∘sync∘compile
   fixed point on spec-28 scopes.
3. **B3 — Distribution wedge**: Anthropic **memory-tool handler** (six
   commands over `/memories`, exact GA contract per OSS team) + Claude Code
   auto-memory shape (MEMORY.md index + topic files) + MCP resources mount.
   Tools+resources only — the 2026 MCP spec deprecates sampling/roots (docs
   team). This is the zero-integration adoption path for any agent, and the
   direct answer to OpenViking.
4. **B4 — Receipts + calibrated answers**: citation quote_hash verification
   end-to-end (experimental P11) and the LME-V2-blessed evidence-status →
   answer-policy protocol (supported / contradicts-premise / near-match /
   insufficient) in Deep's output contract. Post-Mem0/Zep-scandal, verifiable
   receipts are a positioning weapon no competitor has.
5. **B5 — Deletion list** (KISS; codebase team §f): retired heuristic rerank
   stage + trace fields, `RecallMode::Balanced` (behaviorally Fast, zero
   users), WS-0 spike stubs, l4-naming shims, spike dirs, synthetic rung YAML
   fleet (keep a regression subset), fix-or-delete the always-passing
   `memphant-eval compare` stub, mark schema-only tables (`trust_event`,
   `event_outbox`, `scope_block`, `retention_tier`) explicitly dormant
   in-repo. **Decision: contract InMemoryStore** to pure-logic unit tests;
   all contract evidence moves to scratch-PG (the InMemory/Pg divergence
   anti-pattern is documented and has already hidden bugs).
6. **B6 — CI honesty legs** (tests team): a Postgres service leg (52 ignored
   live tests + e2e probe currently never run in CI — all CI eval evidence is
   InMemory today), a fastembed-less leg, the `ops` eval lane, and an LME-S
   n=5 chain smoke to stop the full-500 chain from bit-rotting.

## 5. Phase C — Cutover (slices 0/1/3 start now; docs is the only gated slice)

Source: Syndai team. **Production corpora are near-empty** (knowledge 0/0,
files 0, facts 2, episodic 252; only `coding_execution_attempt_events` is real
at 64k rows/72 MB). **CORRECTION (review #4): "empty" means UNMEASURABLE, not
"easy."** Syndai spec 07 §158 is explicit — the adapter risk is event-ingest
throughput (10⁴–10⁵ events), not store size, and "a trace comparison against
an empty table proves nothing." A cutover can be contract-green and
recall-blind at the same time (the exact silent-fallback failure C0 fixes). So
the acceptance bar must be set BEFORE cutting: **C3 (64k coding events, real
volume) runs BEFORE C1**, so a volume-matched adversarial golden exists to
measure recall parity against. Where volume can't be bootstrapped, state
honestly that the slice is *correctness-only* and defer recall-quality to when
volume exists — never let "identical Conversations tab" stand in for retrieval
parity. The free re-embed window (OpenAI-1536 → local modernbert) is open only
while tables are empty. **Slices 0/3 do not wait for Phase A; C1 waits on C3's
golden.**

- **C0 — Rails** ✅ **LANDED (2026-07-21)**: rebuilt BOTH clients (the MemPhant
  Python SDK `bindings/python/memphant` AND the Syndai adapter, the latter
  committed in the Syndai repo off `Syndai/main`) against the strict landed
  contract — they sent `tenant_id`/`allowed_scope_ids` → 422 → silent legacy
  fallback (`context_loader` swallowed the 422 with `return None`). Now a
  `BoundContext`/`bind_context()` handshake (PUT `/v1/context-bindings`), no
  `tenant_id`, and the caller re-raises on a 4xx contract error (degrades only
  on transport fault, loudly). Drift test pinned to `openapi/memphant.v1.json`
  (oneOf-aware, teeth-verified); full local gate green; e2e_probe exercises the
  same binding flow live. Live Syndai wiring (real context binding) deferred —
  Syndai has no `subject_generation`/`agent_node` concept yet; dogfood default-
  off ⇒ nil blast radius. Proof: `docs/build-log/2026-07-21-c3-coding-backfill.md`,
  commits `d7939105` (SDK) + Syndai `a7f6ceeef`.
- **C1 — Episodic slice** (first real user value): cut the loader's episodic
  layer + recall/correct/reinforce/archive/forget to MemPhant; backfill 252
  rows; bar = hot-path SLO (p50 < 200 ms) + identical Conversations tab +
  two-user RLS leakage proof (the isolation model swaps from app-filters to
  tenant-RLS — must be proven, not assumed).
- **C3 — Coding-continuity backfill** (net-new value, no parity bar) ⚠️
  **MECHANISM LANDED, VOLUME BLOCKED-on-data (2026-07-21)**: the `retain(episode)`
  backfill path + the code-lane runner are now strict-contract-correct
  (`gate_runtime.ApiClient.bind_context()`; `ingest_attempt` nests
  `payload.episode`; ingest pinned to the spec by a unit test) — the backfill IS
  this path at `--limit-attempts 0`. Streaming-hook attachment point identified
  (`Syndai .../coding/events.py:append_coding_event`, one episode per attempt at
  the terminal event; live wiring deferred with C0's). **The ~64k events are
  unrecoverable locally** — `syndai-coding-local-db` starts healthy but the
  event/attempt tables are empty (historical rows wiped, no dump; last real
  extraction 359 attempts, since gone); local regen needs the full CaaS stack,
  prod is off-limits (AGENTS.md §18). So C3 ships correctness-only; the
  volume-matched adversarial golden is a runnable extract→mine→backfill→reader
  procedure that executes when a corpus exists, and IS the C1 acceptance bar.
  Proof: `docs/build-log/2026-07-21-c3-coding-backfill.md`, commit `4f90ef57`.
- **C2 — Docs/knowledge slice (LAST, gated)**: blocked first on a free
  half-day corpus re-pin (Syndai HEAD drifted 109→115 files; the gate
  currently cannot run at all — tests team), then retrieval-only hit@10
  head-to-head (~$1–5) before any reader spend, then the full pre-registered
  bar: **k=10 comparable-volume CI-clean win inside the 1.5 s ceiling** (the
  +0.142 flip stays asterisked at 14× volume and is not cutover evidence).
  Rank-compressed rerank (§2.Q4) is the named lever — **but review #8 flags it
  as likely dead-on-arrival: MemPhant currently loses docs retrieval 0.050 vs
  Syndai 0.217 (CI excludes zero) and the winning rerank is latency-retired at
  13 s.** Run the FREE kill-gate first: (a) count retired-run flips reproducible
  in top-16 (<60 % → don't build), AND (b) does rank-compressed rerank close
  ≥half the 0.050→0.217 gap on the pinned 4,870-section corpus? If either
  fails, **drop C2 from the roadmap now** rather than gating it to Week 3+ — the
  honest base rate is "won't win this quarter." Also implement the four spec-28
  fixture families as executable `syndai-trace-compare` fixtures (free, 1–2
  days) — currently spec prose, and the actual cutover-safety net.
- **Cost wins on cutover**: Jina API + OpenAI embedding egress eliminated
  (privacy win doubles as cost win); local embeddings free.

## 6. Evidence discipline v2 (binding; replaces the liability ceremony)

**Kept**: preregistration of predicates before runs; append-only run ledgers;
packaged-runtime + pinned-corpora + executed-scorer provenance; paired
bootstrap CIs; no sealed/official track until the identical pipeline is green
on an exposed full-scale run; never rerun a settled billable row; never
fabricate.

**Changed**:
1. Spend safety moves to **provider-side spend-capped keys** per campaign
   (hard guarantee) + the in-run $0.30/recall cap. **The AMENDMENT CEREMONY is
   retired; the SETTLEMENT ACCOUNTING is kept** (review #7). Spend-capped keys
   cap *total* spend but do not reconcile *per-run settlement* — and per-run
   settlement is exactly what has been failing (unsettled upper bound ran ~12×
   settled because runs abort mid-flight leaving reservations open). Keep the
   settle-on-abort receipt the invalidation proofs already compute
   (`deep_settled_micros`); retire only the per-failure amendment paperwork.
   An SOTA evidence trail needs "this aborted pair cost $Y, here's the receipt,"
   which a spend cap alone does not give.
2. **Infra faults are not evidence events.** Zero-billable-call infrastructure
   failures (vanished container, port squat) get code-enforced preflights and
   a retry without an amendment. Amendments are for *contract* changes only.
3. **Campaign Postgres is run-owned** — the controller supervises its own
   container/pg_ctl lifecycle; never shared with desktop Docker.
4. **Zero-cost rehearsal precedes every paid ladder rung** (the no-model
   verifier for mechanics; the buried-evidence test for Deep semantics).
5. Claim ladder unchanged: no "SOTA" language before the LME-S full-500
   protocol run; Pareto claims stay "Pareto frontier."

## 7. Phase D — SOTA shots (parallel, cheap, never blocking the cutover)

1. **D1 — LME-V2 dual operating point** (fast + deep). The adapter is one ABC
   (`insert`/`query` — OSS team); leaderboard verified empty 2026-07-21;
   baselines moved (AgentRunbook-C 74.9 % @ 108.3 s; Codex 69.9 % @ 177.2 s;
   best RAG 48.5 %). Pilot 50Q with a kill-switch (stop if fast < 45 % AND
   deep < 60 %), then paired full runs, then submit. The fast-end frontier
   slot is unoccupied — our 70 ms hot path is the differentiated entry.
2. **D2 — ForgetEval** (1,385 cases, MIT): mutation-time correct/forget/mark
   is *our architecture* — inscribe-time-only systems score 0 % on intent-aware
   deletion; mutation-time hooks reach 78–85 %. Second empty instrument;
   uniquely aligned; nobody else has the verbs. **PRECONDITION (review): the
   verbs exist; mutation *correctness* does not yet** — STATUS Task-4 is open
   with a documented history of stateless-identity guesses causing zero-match
   invalidations while stale facts stayed active. Verify mutation correctness
   on the existing Memora trajectories BEFORE claiming a ForgetEval slot.
3. **D3 — LME-S full-500**: the internal "SOTA-language" unlock. The whole
   chain exists (fetch → bench-lme → reader → official scorer); pre-commit its
   config; run when A-ladder and reader budget line up. Add the same-harness
   competitor re-runs (Mem0/Zep/Letta) published on Evalrank — the
   hindsight-validated credibility play (independent reproduction beats
   another self-run table).
4. **D4 — SWE instruments**: SWE-Explore is **externally blocked** (bundle
   omits issue texts/commits; our runner correctly fails closed) — watch
   upstream; the slot survives because nobody can run it. SWE-ContextBench
   has no public harness — implementing the paper's sequence protocol around
   SWE-bench Lite's docker eval as the first open reference is a
   medium-effort credibility play, sequenced **after** C3 gives us a real
   code-profile corpus.
5. Supporting-only (never headline): Memora, STALE, MemSyco, MemBench.

## 8. Ordering summary & kill-switches

**Schedule REORDERED (review #6): the cutover is the spine, so it goes first;
paid benchmark work fills idle capacity and never precedes it.** The prior
draft front-loaded a Deep-reliability fight (14 prior failures) for an empty
leaderboard while pushing first user value to Week 2 — incoherent against
"best UX above all." Corrected:

```
Week 0:  P0.1 run-owned Postgres controller ─ P0.2 liveness preflight ─
         P0.3 live-provider Deep smoke   (ALL free; gate everything paid)
         A0 buried-evidence test (free) ─ A1 Fast-miss classification (free)
            ↑ A1 verdict decides whether the paid benchmark lane opens AT ALL

Week 1:  C0 rails ─ C3 coding backfill+golden ─ C1 episodic slice   ← THE SPINE
         B5 deletions ─ B6 CI legs (free, unblock CI honesty)
         C2-prep corpus re-pin (free) ─ C2 free rerank pre-check (can kill C2)
         [only if P0.3 + A1 green:] A2 n=12 ($2.5)

Week 2:  B1 observation block (net-new; n=12 gate) ─ B2 file-plane projection
         B3 memory-tool handler (distribution wedge — OpenViking pressure)
         [only if A2 green:] A3 mock spike → paid n=30 ── A4 n≈100
         [only if A1 said depth-bound:] D1 LME-V2 50Q pilot ─ D2 ForgetEval
            (D2 needs mutation-correctness verified first)

Week 3+: C2 docs slice IFF free pre-check passed AND k=10 bar won
         D3 full-500 + Evalrank re-runs ─ D4 SWE-CB reference (post-C3)
```

Kill-switches (cheap, fire early): **A1 ≥70 % present-but-unpacked → Deep goes
diagnostic, D1/D3 DEFERRED, packing becomes the center of gravity;** P0.3 live
smoke fails → paid lanes stay closed, fix the provider parser first; **C2 free
pre-check <60 % flips reproducible in top-16 OR rerank doesn't close half the
0.050→0.217 deficit → drop C2 from the roadmap now, don't gate it;** D1 pilot
below floors → withdraw submission, keep dev evidence; B-gates failing → delete
the lever, keep the negative artifact.

## 9. What we are explicitly NOT doing

Graph engine (5th rejection, now externally vindicated); HyDE (2026 consensus
demoted it); LLM-arbitrated destructive writes (mem0's own retreat);
English-hardcoded lexical machinery; unverifiable README benchmark claims;
Supabase Storage before its trigger; n=300 before n≈100; BEAM 1M (operator
surface failed verification); building any new memory architecture — every
lever in this plan activates something that already exists.

## GSTACK REVIEW REPORT

| Review | Trigger | Why | Runs | Status | Findings |
|--------|---------|-----|------|--------|----------|
| Eng Review | `/plan-eng-review` | Architecture & tests (required) | 1 | issues_folded | 8 findings, 3 critical gaps, all folded into the plan |
| Outside Voice | Claude subagent | Independent 2nd opinion | 1 | issues_found | Read the T6 artifacts; found Deep-never-valid, schedule-vs-verdict incoherence, docs-lever-dead |

**OUTSIDE VOICE (Claude subagent):** independent review grounded in STATUS.md, the T6 invalidation proofs, and source. Eight findings; the load-bearing ones (all folded):
1. Deep has produced zero valid live pairs — mock A0 passes while the real provider aborts → added **Phase 0 P0.3 live-provider Deep smoke** as the gate before any paid rung.
2. 72.5/48.5 are third-party in-paper baselines, not MemPhant-transferable; 0.584 oracle-utilization ceiling is a packing problem Deep can't fix → **A1 verdict now binds the whole benchmark lane** (can defer D1/D3 for $0).
3. A3 bench-lme Deep wiring is not ~10 lines (async billable settlement) → rescoped to a free mock spike first.
4. "Empty corpora = easy" is really "unmeasurable" → **C3 (real volume) now precedes C1**; correctness-only cutover stated honestly.
5. Week-1 lanes weren't independent — all paid work depends on a run-owned Postgres controller that doesn't exist → **promoted to Phase 0 P0.1**.
6. Schedule contradicted the verdict (benchmarks first, cutover second) → **reordered: cutover is Week 1, paid benchmarks fill idle capacity**.
7. Retiring liability ceremony also retired settlement accounting → keep settle-on-abort receipts, retire only the amendment paperwork.
8. Docs lever (C2) is latency-dead against a 4× deficit → **free kill-gate added; drop C2 now if it fails** rather than gating to Week 3+.
Plus: B1 observation block is net-new (scope_block is a bare table name), not "activation"; D2 ForgetEval needs mutation-correctness verified first.

**CROSS-MODEL TENSION:** The outside voice argues the schedule should put the cutover strictly first and treat benchmarks as pure idle-fill; the original synthesis treated them as co-equal parallel tracks. Resolved toward the outside voice (cutover is the spine per owner's "best UX above all"), while keeping the two *free* benchmark gates (A0/A1) in Week 0 because they can invalidate the paid roadmap at zero cost — that is not "benchmark-first," it is "cheapest-kill-first."

**VERDICT:** ENG review complete, all findings folded. Plan is internally consistent: free gates + infra controller first, cutover as the spine, paid benchmark work gated behind cheap kill-switches. Ready to implement Phase 0.

NO UNRESOLVED DECISIONS

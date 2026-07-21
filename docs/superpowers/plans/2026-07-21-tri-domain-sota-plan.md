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

## 3. Phase A — Prove T6 (this week; free → $10)

Ascending evidence rungs; each is a kill-switch for the next. Sources: tests
team (rungs), devil's advocate (free-first discipline), codebase team (wiring).

- **A0 (free, 1–2 days): buried-evidence Rust test.** Seed a store where the
  answer unit misses Fast's whitelist; scripted mock provider nominates it;
  assert Deep's answer set differs from Fast's. Today **no test anywhere
  proves Deep changes the answer set** — this is the first artifact that does.
  Also: encode the PG-liveness preflight in the runner (Amendment 14 made it
  prose; make it code) and add a Deep leg to `e2e_probe.sh`.
- **A1 (free): Fast-miss trace classification.** Classify the ~74 Fast-miss
  dev questions: gold-present-but-unpacked vs gold-absent-from-pool. This
  decides whether Deep or packing/ordering is the binding lever — the ledgers
  suggest utilization (oracle 0.916 vs 0.584) and Deep only helps the
  gold-absent class. **If ≥70 % is present-but-unpacked, Phase A pivots to
  rung-7 packing and Deep drops to diagnostic status.**
- **A2 (~$1–2.5 realistic, ≤$5.70 cap): the authorized n=12** on
  run-d2f4fcb3, babysat, on a run-owned Postgres (dedicated container, not
  the shared Docker Desktop lifecycle that killed run-65981e4f). Preregistered
  pass predicates already frozen (mean delta > 0, wins > losses, p50 ≤ 45 s,
  mean cost ≤ $0.10).
- **A3 (≤$9): bench-lme Deep wiring** — the ~10-line change installing the
  deep provider in `bench_lme.rs` (mirror the cross-rerank wiring), then n=30
  paired fast-vs-deep on LME-S targeted at Fast-miss questions. Fastest
  paired-CI diagnostic per dollar.
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
   (devil's advocate is right that the everyday-UX lever must not queue behind
   the flakiest gate). Gate: n=12 from the displacement/reader failure
   classes; ≥+2 net flips.
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
at 64k rows/72 MB). The cutover is pipeline-vs-pipeline on goldens, not a data
migration — and the free re-embed window (OpenAI-1536 → local modernbert) is
open only while tables are empty. **Slices 0, 1, 3 do not wait for Phase A.**

- **C0 — Rails**: rebuild the Syndai adapter against the strict landed
  contract (it currently sends `tenant_id`/`allowed_scope_ids` → 422 → silent
  legacy fallback, the exact banned failure mode). Add a drift test pinned to
  `openapi/memphant.v1.json`. Dogfood file memory + facts at nil blast radius.
- **C1 — Episodic slice** (first real user value): cut the loader's episodic
  layer + recall/correct/reinforce/archive/forget to MemPhant; backfill 252
  rows; bar = hot-path SLO (p50 < 200 ms) + identical Conversations tab +
  two-user RLS leakage proof (the isolation model swaps from app-filters to
  tenant-RLS — must be proven, not assumed).
- **C3 — Coding-continuity backfill** (net-new value, no parity bar): 64k
  events → `retain(episode)` backfill (<1 h) + streaming retain hook on the
  existing durable-jobs path. This becomes the code-profile corpus that the
  40Q golden can't provide (distribution drift: local-dev-mined vs prod).
- **C2 — Docs/knowledge slice (LAST, gated)**: blocked first on a free
  half-day corpus re-pin (Syndai HEAD drifted 109→115 files; the gate
  currently cannot run at all — tests team), then retrieval-only hit@10
  head-to-head (~$1–5) before any reader spend, then the full pre-registered
  bar: **k=10 comparable-volume CI-clean win inside the 1.5 s ceiling** (the
  +0.142 flip stays asterisked at 14× volume and is not cutover evidence).
  Rank-compressed rerank (§2.Q4) is the named lever. Also implement the four
  spec-28 fixture families as executable `syndai-trace-compare` fixtures
  (free, 1–2 days) — they are currently spec prose, and they are the actual
  cutover-safety net.
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
   (hard guarantee) + the in-run $0.30/recall cap. Micro-dollar liability
   amendments are retired.
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
   uniquely aligned; nobody else has the verbs.
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

```
Week 1:  A0+A1 (free) ─┬─ A2 n=12 ($2.5) ─ A3 n=30 deep diag ($9)
                       ├─ B1 observation block (n=12 gate)
                       ├─ B5 deletions + B6 CI legs
                       └─ C0 rails + C2-prep corpus re-pin (free)
Week 2:  C1 episodic slice ── C3 coding backfill
         B2 file-plane projection ── B3 memory-tool handler
         D1 LME-V2 50Q pilot (kill-switch) ── D2 ForgetEval adapter
Week 3+: C2 docs slice iff k=10 bar won (rank-compressed rerank lever)
         A4 n≈100 iff A2+A3 passed ── D3 full-500 + Evalrank re-runs
         D4 SWE-CB reference harness (post-C3)
```

Kill-switches: A1 ≥70 % present-but-unpacked → pivot A to packing; D1 pilot
below floors → withdraw submission, keep dev evidence; C2 bar unmet after
rank-compression → docs cutover deferred honestly, k=50 offered to Syndai as
an async-surface config only; B-gates failing → delete the lever, keep the
negative artifact.

## 9. What we are explicitly NOT doing

Graph engine (5th rejection, now externally vindicated); HyDE (2026 consensus
demoted it); LLM-arbitrated destructive writes (mem0's own retreat);
English-hardcoded lexical machinery; unverifiable README benchmark claims;
Supabase Storage before its trigger; n=300 before n≈100; BEAM 1M (operator
surface failed verification); building any new memory architecture — every
lever in this plan activates something that already exists.

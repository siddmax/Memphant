# MemPhant Memory Campaign Report — 2026-07-11

**Audience note (the reframe this report is built around):** MemPhant serves
PROSUMERS — one person's assistant remembering their life and work across months:
thousands of chat sessions, hundreds of notes/docs, zero configuration. Not
million-doc enterprise IR. Every verdict below is re-read through that lens, and the
next round is designed for it. "Long memory" (months-scale continuity) stays a
first-class goal; billion-scale machinery does not.

## 1. What we did, and why

Sequence (all on `main`, every number from the packaged Postgres runtime, pinned
LongMemEval-S, executed reader/judge — the promotion-provenance rule):

1. **Runtime proof** → REST/MCP/CLI/worker on Postgres, e2e probe green.
2. **n=30 reader round** → first honest QA numbers (0.43); rerank measured harmful.
3. **n=100 scaled round** (`2026-07-10-scaled-reader-campaign.md`) → rerank-off
   re-confirmed with power; the n=30 "turns falsified" verdict overturned
   (small-sample artifact) — first real-evidence promotion.
4. **Rung-4 closure** (`2026-07-10-runtime-chunks-campaign.md`) → contextual chunks
   built into the runtime write path (reflect-stage ≤4-turn windows, headers, parent
   linkage) + chunk-aware packing; ΔQA +0.110 [+0.020,+0.190]; **confirmed on a
   virgin seed (+0.130 [+0.040,+0.220]) and a second reader/judge lattice**. The one
   promoted default of the campaign, and it survived every audit.
5. **11-lens research fleet** → synthesis into the canonical-plan addendum; devil's
   advocate forced the held-out validation (it passed) and banned unearned "SOTA"
   language.
6. **Accuracy wave W1–W10** (`2026-07-11-accuracy-wave.md`) → 3 default-path
   honesty fixes (server actually embeds now; SQL-side vector scoring; fusion
   substring hacks deleted) + 8 flag-gated levers, measured as paired singles and a
   pre-registered combo on two seeds. **No lever cleared the bar; none promoted.**
7. **Syndai replacement gate** (`2026-07-11-syndai-gate.md`) → engine-vs-engine on
   108 seeded docs, 60 mined span-grounded questions: **HOLD** — Syndai 0.217 vs
   MemPhant 0.050 (CI excludes zero).

## 2. Models and harness (what read, what judged, what it cost)

- **Reader selection was measured, not assumed:** claude-haiku-4.5 ≈ gpt-5.6-terra
  (identical on 29/30), gpt-5.6-luna worse (−0.100). Campaign reader:
  `gpt-5.6-terra@medium`. After the codex CLI quota outage we built
  `--engine openrouter`; judge moved to `claude-sonnet-5` (different family than the
  reader — kills self-preference bias). All promoted deltas are same-reader,
  same-judge, PAIRED; cross-lattice replication backs the one promotion.
- **Judging:** normalized containment first; LLM judge only on containment misses
  (~20–27% of rows); abstention by exact-match. Known bias: containment penalizes
  abstraction — one reason extracted-facts levers judge poorly (see §4).
- **Cost discipline:** sha256 reply cache keyed by engine+model+effort+prompt; every
  re-score of unchanged prompts is free. Wave rounds ran ~100–140 fresh calls per
  config (~$4/config via OpenRouter).

## 3. The numbers that matter

| Claim | Evidence | Status |
|---|---|---|
| Contextual chunks lift QA | +0.110/+0.130 on two seeds, two lattices, CIs excl 0 | **PROMOTED (default on)** |
| Heuristic rerank hurts | ΔR@5 −0.128, ΔR@10 −0.074, CIs excl 0 | **Default off** (disable-when) |
| 8 wave levers (xrr, v3, quota, temporal, facts, pool, embed, sibling) | all QA CIs include 0 at n=100; pooled combo +0.020 [−0.040,+0.080] n=200 | Built, flag-gated, **not promoted** |
| Fact extraction improves retrieval | ΔR@10 +0.074 [+0.021,+0.138] — wave's only significant signal | Real, but pack displacement blocks QA (§4) |
| Doc lane loses to Syndai stack | 0.050 vs 0.217, Δ CI excl 0 | **HOLD on replacement** |

QA trajectory on our harness: 0.43 → 0.56–0.61 across seeds. That is GPT-4o
full-context-baseline territory; the LongMemEval paper's optimized-retrieval band
(0.70–0.73) is the open target. Independent re-runs place the "90%+" vendors at
0.58–0.72 — we are inside the reproducible band, at its bottom. No external claim
is made or permitted until a full-protocol run exists.

## 4. Why the paper-backed levers measured null (detailed analysis)

- **The binding constraint moved.** R@10 is 0.83–0.94 while QA is 0.59: retrieval
  is no longer the bottleneck; reading/composition is. Flip-level decomposition of
  44 failures: **21 reader-with-adequate-pack, 16 pack displacement, 7 judge
  subjectivity.** Retrieval levers (cross-encoder, pool, embedder) target the small
  fraction; that is why they moved retrieval but not QA.
- **Our baseline already contains the big lever.** Published gains are mostly vs
  naive-RAG baselines. Hybrid FTS+vector+RRF+contextual-chunks IS the paper stack;
  what remains are +2–5pt effects.
- **n=100 cannot see +2–5pt** (CI half-width ±0.062). The nulls are mostly
  "unresolved," not "false" — xrr and v3 routing were mechanism-confirmed at the
  stratum level (each one's gain sits exactly where predicted, and each one's
  regression is the other's gain stratum).
- **Fact-rows displace verbatim context.** Facts packs: 8.2 items but FEWER tokens
  than baseline; 48.7% of items <150 chars. External corroboration is now strong:
  the verbatim-vs-extracted ablation (arXiv 2601.00821, +22pt for verbatim on
  LongMemEval) and independent practitioner consensus (incl. the mem0 junk-drawer
  autopsy: 10,134 entries, 97.8% junk, one hallucination amplified to 808 copies).
  **Verdict adopted: verbatim chunks are the memory; extraction feeds retrieval
  KEYS and profile state, never pack content.**

## 5. The prosumer lens (what "magic" actually is, per evidence)

Practitioner consensus (~40 sources, `prosumer-practitioners` research):
**magic = corrections that stick, preference continuity without re-briefing,
freshness (stale facts age out), and provenance you can tap.** Complaints:
context bleed, junk-drawer bloat, silent self-rewriting, absurd extracted facts.

Where MemPhant already aligns (validated choices, keep):
- **Deterministic no-LLM writes** — the direct antidote to the mem0 failure mode;
  also instant + free at ingest.
- **Supersedence on honest subject keys** (W6 machinery) — "corrections that stick."
- **Citations end-to-end** — provenance chips are the most-praised least-shipped UX;
  Syndai mobile already renders them.
- **Bitemporal validity fields + W5 grounding** — practitioners and papers both
  demand explicit valid-from/to over recency hacks.
- **Prosumer scale kills the don't-chase list:** ANN indexes, quantization,
  GraphRAG, RAPTOR, sharding — irrelevant below ~100k memories; exact search wins
  on accuracy. Our earlier "no HNSW" finding is a non-problem at this scale.

Where we are behind the magic bar: reader-side composition (the 21/44 class),
preference continuity (profile-shaped, not fact-rows in packs), and the doc lane
(a prosumer's few hundred notes/docs still deserve better than 0.050).

## 6. Next round — pre-registered, prosumer-first

**Measurement design (the exact-n answer):** dev at **n=300** (seed 20260712;
detects +0.035, CI half-width ±0.036), confirm winners on the **full 500** — the
200 never-seen questions are a built-in virgin held-out (±0.044 directional check)
and the full-500 CI (±0.028) is the promotion bar. n=300 is the largest dev set
that leaves a true held-out inside the 500-question corpus. Promotion =
pre-registered lever + full-500 CI excluding zero + virgin-200 agreement +
cross-lattice spot-check. The full-500 run doubles as the protocol run that
finally permits external comparison, using the canonical LongMemEval judge prompt.

Levers, ranked by evidence × prosumer fit:
1. **R1 Chain-of-Note reader (prompt v4):** per-item relevance notes before
   synthesis + structured output; +10pt reported on this benchmark family; aims at
   the 21/44 class. Keep v3's routing + calibrated abstention.
2. **R2 Preference profile block:** W6's honest keys feed a compact profile
   (chain-heads of preference/attribute supersedence chains) injected as ONE pack
   item — the ChatGPT-pole hybrid; kills fact-row displacement while shipping
   preference continuity. (Facts never enter packs individually again.)
3. **R3 Contextualized embedder:** voyage-context-3 primary (bakes heading/session
   context into chunk vectors — the exact mechanism the Syndai gate diagnosed);
   qwen3-embedding-0.6b as the local/free fallback arm; symmetric query prefixing.
   Doc lane first (re-run the Syndai gate with it), chat A/B second.
4. **R4 Temporal re-measure:** fix the redundant date-prefix muting, then re-arm
   W5 at n=300 (temporal is every system's weakest axis; ours is built).
5. **R5 Query-side paraphrase fix:** HyDE/query-rewrite A/B (cheap, targets the
   generic-query→specific-memory miss that owns the preference stratum).
6. **R6 Months-scale hygiene (long-memory plus):** write-gating at ingest ("does
   this change future behavior?"), tiered similarity resolution
   (auto-merge/review/new) — the levers practitioners say decide month-6 quality;
   measured on the longitudinal suite, not LME-S.

Explicitly NOT next round: replacement wiring (HOLD stands until R3 flips the doc
gate), ANN/scale machinery, graph engines, LLM-at-ingest.

## 7. Standing rules (unchanged, now with teeth)

Promotion-provenance + two-seed/virgin-subset confirmation; same-lattice pairing;
"SOTA" banned until the full-500 protocol run; accuracy/UX > cost > perf/latency;
verbatim is the memory, extraction is metadata; deterministic writes stay.

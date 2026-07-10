# SOTA Campaign Handoff — 2026-07-10

Current STATUS mirror: RUNTIME COMPLETE — BENCHMARK EVIDENCE PENDING

Written at session pause (context budget). Everything below is committed on
`main` in both repos; nothing is pushed.

## What is DONE (verified, with proof pointers)

1. **Evidence reset** (`6ff62b0`): fabricated 2026-07-03/04 promotions reopened;
   promotion-provenance rule in STATUS/27 (synthetic fixtures gate regressions,
   never promotions); scorecards marked `invalid_synthetic_fixture`.
2. **Durable authenticated runtime** (`f3b548e`..`d17c026`): full `MemoryStore`
   seam; `MemoryService<S>` shared by REST/MCP/CLI/worker; PgStore on sqlx 0.9;
   API keys w/ max_trust clamping; typed jiff clock; content-hash subject keys
   (auto-keys never supersede); forgotten-source tombstones; rmcp 2.2 MCP
   (persistent stdio, camelCase); CLI memory verbs + admin; embedding seam
   (fastembed bge-small 384d); tri-domain retain `episode|resource|unit` with
   code `revision` identity; honest website; Syndai adapter/contract sync.
   Proof: `scripts/e2e_probe.sh` → ALL CHECKS PASSED (durability across
   restart, cross-tenant 404, forget w/o resurrection, degraded
   read-your-own-writes); `docs/build-log/2026-07-10-runtime-postgres-proof.md`.
3. **Real benchmark lane + first honest numbers** (`9abb0bb`, `db2034b`):
   `memphant-eval bench-lme` on pinned LongMemEval-S (sha256 in
   `benchmarks/manifests/`), retrieval-only, Postgres runtime, 30q stratified
   seed 20260710: **R@5 0.500 / R@10 0.607**. Paired ablations: vector helps
   (−7.1 @10 when off), edges/decomposition/exhaustive zero, **rerank harmful**
   (+14.3 R@5 when off, CI excl. 0) → rerank off by default (`7dad881`).
   Rungs 4–13/15 adjudicated open with per-rung evidence notes; 14 retired.
   `docs/build-log/2026-07-10-real-retrieval-campaign.md`.
4. **Reader-scored round DONE** (`f3e424e`, `d75ce0a`): claude-haiku reader +
   containment/LLM judge over top-10 evidence, 107 CLI calls, cached re-scoring.
   Session baseline QA accuracy **0.433**; rerank-off **0.467** (+3.3 ns,
   directionally consistent with retrieval harm) → **rerank-off default stands
   on end-to-end evidence**. Turn-window ingestion lifted R@5 (+10.7 ns) but
   DROPPED QA 6.7 pts (knowledge-update .60→.20: fragments lose update
   context) → **session granularity stays default; the "granularity is the
   biggest lever" hypothesis is falsified as tested** — the next lever family
   is per-axis (update-chain surfacing, temporal windows, evidence-pack size),
   see the ranked NEXT in `docs/build-log/2026-07-10-reader-campaign.md`.
   Prior retrieval numbers reproduced exactly (0.500/0.607; rerank +0.143 CI
   excl. 0). No rung advanced; STATUS notes refined.

## What is LEFT / NEXT (ranked), and WHY

1. **Per-axis QA levers, reader-scored, until plateau** — 4-turn windows are
   falsified; try contextual chunks WITH session-context headers (rung 4 axis,
   keeps update context), update-chain/temporal surfacing for knowledge-update
   and temporal strata (the weakest), and evidence-pack size k. Bump sample to
   100q once a lever looks real (n=28-30 CIs are wide); same seed for pairing.
2. **Rung evidence with the right corpora** — 10 (procedural) and outcome axes
   need STATE-Bench-style tasks, not chat QA; 11 (DSR) needs the longitudinal
   suite; 15 needs an OP-Bench restraint check; 13 needs archived-trace
   training data (bench lane now produces real traces). Doc 27 §2 holds the
   gates; the provenance rule holds the bar.
3. **STATE-Bench first-mover run** — the spec's PRIMARY target; empty
   leaderboard; needs the paired memory-on/off ablation to attribute the delta.
4. **Syndai RAG/KB replacement gate** — replace `backend/src/features/knowledge/`
   (12.7k LOC: knowledge_sources/sections/chunks, halfvec+BM25+RRF+Jina rerank)
   ONLY after MemPhant beats it on a golden set built from Syndai's own
   knowledge corpus (mined cases per doc 12 provenance rules). Same gate for
   episodic memory (21k LOC) and the coding-continuity lane
   (62k `coding_execution_attempt_events` = the data-rich corpus; extraction/
   backfill design in the 2026-07-09 plan, Phase 2(b)). Wire the dogfood flag
   (`memphant_file_memory_dogfood_enabled`) against a deployed instance first.
5. **Reader-scored honesty ceiling** — our QA numbers use haiku over top-k
   evidence; published 86–94% LME numbers use frontier readers. Never compare
   across readers; when claiming externally, run the official scorer with a
   pinned frontier reader and report cost.
6. **Deferred deliberately** (unchanged): RLS policies + non-owner runtime role;
   typed DTO timestamps; R79 adapters (Claude Code memory tool + Hermes SPI —
   the distribution wedge, next after accuracy); learned rerank/FSRS fitting;
   graph engines (rung 14 retired); dead `include_trace` flag cleanup.

## Standing decisions (do not silently re-litigate)

- Six verbs + tri-domain contract day one; one substrate, per-domain indexes
  earn promotion by paired ablation on real corpora.
- Accuracy/UX > cost > perf/latency (owner, 2026-07-10: answer-model budget
  authorized until SOTA; optimize cost after).
- No competitor code in the dependency tree; adapters are distribution.
- Every promotion cites an artifact produced by the Postgres runtime on pinned
  real data. `scripts/e2e_probe.sh` and the full local gate must stay green.

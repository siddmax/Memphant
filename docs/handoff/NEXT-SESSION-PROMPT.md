# MemPhant Campaign Handoff — paste this to resume (rewritten clean 2026-07-12, supersedes all v1–v5 accretions; history in git)

You are resuming the MemPhant SOTA campaign in `/Users/sidsharma/Memphant` (sibling
`/Users/sidsharma/Syndai`; spec mirror must stay drift-free: `python3 scripts/check_spec_drift.py`
after any spec edit + rsync of `docs/superpowers/specs/memphant/`). Execution style: **SDD**
(subagent-driven: sonnet/opus implementers with briefs in `.superpowers/sdd/briefs/`, per-task
review, whole-branch review per wave; ledger `.superpowers/sdd/progress.md` is the recovery map).
Plan of record: `docs/reports/2026-07-11-prosumer-memory-campaign-report.md` §9, as amended by the
build-logs below. Owner priorities: **accuracy/UX > cost > perf/latency**; prosumer magic
(corrections stick, preference continuity, freshness, provenance you can tap); tri-domain
capability — **agents (chat), RAG (docs), codebase** — on ONE substrate; end state = inserted into
Syndai agents, offered as CaaS, and available to any agent (MCP + file adapters) at scale.
Pre-production: anything may be rewritten; no backwards compat; never fabricate a number.

## 1. State of truth (evidence-anchored; every claim has a build-log + CI artifact)

- **Runtime**: complete and proven (Postgres REST/MCP/CLI/worker, e2e probe green, scratch-DB
  isolated harnesses — `scripts/with_scratch_db.sh`; job-debris failure mode structurally gone).
- **Chat lane (agents)**: QA 0.56 at n=100 (terra/sonnet-5 lattice); R@10 0.83–0.94 → the lane is
  READER/COMPOSITION-bound, not retrieval-bound (embedder swaps ns ×3; rerank ns at n=100).
  Promoted default: reflect-stage contextual chunks (+0.110 excl0, two seeds). Baseline artifact
  for the next round: `docs/build-log/artifacts/r15-docs/chat/reader-small-20260710.json` (QA
  .550, binary 800ac41).
- **Docs lane (RAG)**: **the Syndai gate is FLIPPED** at deep-recall (modernbert, k=50/b8192):
  pooled +0.142 [+0.058,+0.225], both golden sets individually CI-clean
  (`docs/build-log/2026-07-12-r1-docs-gate.md`). Honest asterisk: ~14× reader-input volume; at
  k=10 we lose −0.100. R1.5 then proved **ordering is the bottleneck**: server-side cross-encoder
  = +0.158 excl0 at k=10 (v2 R@5 .250 > Syndai .200) but 13s/query CPU → **accuracy-validated,
  latency-retired, flag `MEMPHANT_CROSS_RERANK`**
  (`docs/build-log/2026-07-12-r15-rank-compression.md`).
- **Code lane**: 40Q mined fixture + runner exist (privacy-gated artifacts; lock committed);
  voyage-code-3 vs local ns at sample scale. Greenfield beyond that — first-mover space.
- **Embedders (R0, `docs/build-log/2026-07-11-r0-embedder-bakeoff.md`)**: modernbert-embed-large =
  docs embedder; bge-small = chat default; NO API embedder cleared the ≥3pt/CI-floor/two-set bar
  (voyage-context-4 came closest); qwen3-0.6b retired (CPU); grammar: `MEMPHANT_EMBEDDINGS` /
  `--embed-model` via `embedder_from_id`.
- **Shipped defaults this campaign**: contextual chunks (episodes), rerank-off (heuristic),
  body-tiebreak determinism, `recall_pool_depth=64` (k-invariance contract, tombstone-tested).
  Flag-gated with evidence: cross-rerank (latency), resource chunks (3× ns — retirement
  candidate), W4–W8 wave levers. Falsified: breadcrumb/heading-path prefix, budget-16384-alone
  (chat), prompt v2 global, w8 windows, qwen3.
- **SOTA context**: our 0.56 chat QA sits at the bottom of the reproducible band (independent
  re-runs place vendors at 0.58–0.72; the paper's optimized band 0.70–0.73). "SOTA" language is
  BANNED until the full-500 protocol run (R2) — that run doubles as the unlock.

## 2. Next: R2 — chat/agent lane to the SOTA band (the campaign's center of gravity)

**Why**: chat is the largest lane by prosumer usage, it is reader-bound (exactly where our levers
haven't been spent yet), and R2's full-500 run is the protocol run that legitimizes external
claims. The flip-analysis failure classes at n=100 were: 21/44 reader-with-adequate-pack,
16/44 pack displacement, 7/44 judge — R2's levers target the first two directly.

**Measurement design (pre-register before any run)**: dev at **n=300, seed 20260712** (detects
+0.035; CI ±0.036), confirm winners on the **full 500** (virgin-200 subset = built-in held-out;
full-500 CI ±0.028 is the promotion bar). Promotion = pre-registered lever + full-500 CI excl 0 +
virgin-200 sign agreement + cross-lattice spot-check. Use the canonical LongMemEval judge prompt
for the protocol run. Baselines re-run on the R2 binary (same-lattice, same-binary pairing).

**Levers, ranked (build as SDD tasks, measure as paired singles → pre-registered combo)**:
1. **Chain-of-Note reader v4** (`--prompt-version 4` in `scripts/run_reader.py`): per-item
   relevance notes before synthesis + structured output; keep v3's routing + calibrated
   abstention as a comparison arm. Targets the 21/44 reader class.
2. **Hot-plane profile block**: W6 supersedence chain-heads (keys `{scope}:{family}:{phrase}`,
   value-free) compiled into ONE deterministic pack item (≤1k tokens). Kills fact-row displacement
   while shipping preference continuity — the ChatGPT-pole hybrid. This is also the seed of the
   R3 hot plane; build it as a runtime feature (flag-gated), not a harness hack.
3. **Temporal re-measure**: W5 machinery with the redundant date-prefix muting fixed; soft
   temporal boost + dated packs; temporal is every system's weakest axis and ours is built.
4. **HyDE / query rewrite A/B**: cheap; targets generic-query→specific-memory misses that own the
   preference stratum. Reader-side, no write-path change.
5. (Diagnostic arm) cross-rerank at n=300: it was ns at n=100 with the old pools; with
   recall_pool_depth=64 the picture may differ — one arm, cheap, settles it.

**How**: SDD tasks T0(profile block, opus) → T1(reader v4 + HyDE harness, sonnet) → T2(temporal
fix, sonnet) → runs (n=300 singles ≈ 3× the n=100 wall-clock; budget ~$12–15/config reader) →
combo → full-500 protocol run (~$40–60) → adjudicate → build-log + STATUS eighth-pass.

## 3. R2.5 — rank-compression ship path (docs lane; interleave during R2's long runs)

**Why**: converts the flipped gate into a comparable-volume, cost-honest win — the pre-registered
**R6 unlock rule** (k=10/b8192 beats Syndai pooled CI-clean) fires the moment it ships. It is the
single highest-leverage small engineering item we hold (+0.158 excl0 already banked).

**How (in order of expected value/effort)**:
1. **Truncated-input rerank**: verify what fastembed's TextRerank actually processes — 13s/64
   pairs (~200ms/pair) suggests full-length inputs; cap candidate text at ~512 tokens (the model's
   max anyway) and re-measure. Likely 3–10× for free.
2. **Rerank top-32** (post-L1, gold mass is ≤32): halves cost.
3. **Smaller reranker arm** if still >1.5s.
4. **Async second-pass** as the UX fallback (Syndai knowledge panels tolerate it).
Gate: same lattice, L1X-truncated vs Syndai pooled; ship + unlock R6 on CI-clean. Include
`cross_rerank_ms` p50/p95 in provenance (and persist chars/question aggregate — review note).

## 4. Then, per plan of record (each its own SDD wave)

- **R3 — governance core + hot/file planes**: typed write-router (knowledge=supersession /
  episodic=decay-to-cold / procedural=evidence-gated / preference=chain-head→hot), demotion-not-
  deletion cold plane, file plane as PROJECTION (MEMORY.md/AGENTS.md/learnings.md exports with
  owned regions; evidence-derived, terse — LLM prose dumps measurably hurt), multi-agent
  principal-scoped access model (the CaaS prerequisite). Spec amendment first, then hot plane
  (generalize R2's profile block) + one export/ingest MVP behind flags.
- **R4 — coding lane at full scale**: golden set over the full local event corpus (template:
  `scripts/code_lane_*`; exclude gap attempts — measured 0% locally, verify per-corpus), plus
  **post-action outcome write-back** (worked/failed/invalidated-assumptions → procedural memory)
  — this is "better codebase memory than Syndai" (they have nothing here) and the coding-agent
  wedge. Measure retrieval + a continuity task suite; promotion rules unchanged.
- **R5 — longitudinal gate**: MemoryStress (1000 sessions/10 months) + FAMA stale-penalizing
  scoring; demotion/decay + additive consolidation (citation-anchored reflection nodes; originals
  never rewritten; offline Qwen3-8B consolidator behind an HHEM/NLI grounding check) adjudicated
  THERE, never on LME-class benches (95.4 vs 38.3 divergence). This is the month-6 magic bar.
- **R6 — insertion & scale** (unlocks via R2.5's rule or explicit owner cost acceptance):
  1) Syndai dogfood flag (`memphant_file_memory_dogfood_enabled`) against deployed MemPhant —
  docs lane first, episodic next; 2) CaaS hardening: RLS + non-owner runtime role (known queued
  fix), per-tenant quotas/limits, deployment profiles (compose/supabase/neon exist from WS-H),
  usage metering; 3) all-agents distribution: MCP surface (exists) + Claude-Code
  `memory_20250818` file adapter + Hermes provider SPI (R79) — adapters are distribution only,
  never dependencies.

## 5. Standing rules (unchanged, binding)

Promotion-provenance (packaged Postgres runtime, pinned corpora, executed scorers); paired
bootstrap, two-set/two-seed or pooled CI excl 0; same-lattice AND same-binary pairing (rebuild →
re-run baselines); reader `openai/gpt-5.6-terra@medium`, judge `anthropic/claude-sonnet-5`,
`--engine openrouter` via `doppler run --project syndai --config dev --`; scratch DBs only;
verbatim is the memory (extraction = keys/metadata only); deterministic writes (no LLM at
ingest); "SOTA" banned until the R2 protocol run; six verbs + tri-domain contract frozen; no
competitor code in the tree; commits small with `Co-Authored-By: Claude Fable 5
<noreply@anthropic.com>`, explicit paths only if owner WIP present, never push without owner call.

## 6. Carry-list (fold opportunistically into R2/R2.5 tasks; none block)

Packing tier reconciliation (chip task_5c5d8184 — spawned twice, both sessions deleted; the
conservative gate from R1.5-T0 holds meanwhile); feature-off tests dead under `--all-features` CI
(needs a fastembed-less CI leg — recurring); Retry-After clamp ≤60s; `FastEmbedModel::parse` vs
`fastembed_arm` dedup + local-arm drift pin; trim-vs-span exactness (chunks, inherited from
episode twin); W9 tail cap >32 windows; fence-aware chunk header; `build_service` panic→error on
misconfigured flag; worker skips reranker load; chat-score missing-baseline warning; persist
chars/question + rerank p50/p95 into provenance.

## 7. Ops gotchas (cost hours before; don't relearn)

- Keys: `VOYAGE_API_KEY` in Doppler **tacitry/dev**; GEMINI/OPENAI/OPENROUTER in syndai/dev.
- Build campaign binaries from a **clean git worktree** at the measured commit (owner WIP floats
  in the main tree); pass `SERVER_BIN/WORKER_BIN/CLI_BIN` + `GATE_PORT` to run scripts.
- zsh does NOT word-split unquoted vars — use explicit function args in queue one-liners.
- Background Agent dispatches occasionally misfire (instant garbage return) → re-dispatch or
  SendMessage-nudge.
- Docker Desktop's :5432 proxy can wedge after heavy churn (`docker restart memphant-postgres-1`);
  check nothing squats ::1:5432 (`lsof -iTCP:5432`).
- First model use downloads weights at server boot (gate runner waits 10min + logs); keep
  `.fastembed_cache/` warm.
- Reader cache dir `docs/build-log/artifacts/r0-embedder/reader-cache` is shared across all
  waves — reuse it; re-scores of unchanged evidence are free.

## 8. npcsh/npcpy source audit (2026-07-12) — verdict SKIP; three folds, no new workstream

Full-source audit (5 parallel deep-dives) of npc-worldwide npcsh/npcpy/npcrs + their paper
(arXiv 2603.20380). Nothing competes with any MemPhant lane: no rerank, no packing/admission,
brute-force cosine over JSON-in-BLOB, recency-only memory injection, zero recall evals, and three
live store-divergence bugs of exactly the class our store contract + testkit forbids. Detail in
auto-memory `npcsh-npcpy-verdict.md`; do not re-investigate. Fold-ins (opportunistic; none block
R2):
1. **Extraction rule-prompt content** (npcpy `llm_funcs.py` ~1415–1450: ban speaker attribution,
   interaction mechanics, greetings, generic truisms; demand nuance-preserving statements +
   source excerpts + explicit/inferred tag; return-EMPTY few-shot negatives) → applies ONLY where
   we already run LLMs — the R5 consolidator prompt and keys/metadata extraction. Verbatim-is-
   the-memory and no-LLM-at-ingest stand; their retry-until-facts-appear anti-pattern is the
   cautionary pairing (never pressure a model out of returning empty).
2. **SHA-256 content-hash-gated incremental re-extraction + noise-file heuristic** (lockfiles,
   avg line length, printable ratio) → R3 file-plane ingest MVP and R4 code-lane corpus refresh.
3. **Approval-lifecycle edit history (initial vs final memory, approve/reject/edit labels) as
   training signal for a learned admission gate** → R3 governance back pocket; pairs with the
   packing-tier reconciliation carry item.
Carry-list add: an e2e assertion that the recalled band is NON-EMPTY in the final packed context
(their Rust port templates memory into the system prompt but nothing ever populates it — silent
zero-recall, shipped, untested).
Citable numbers (small-model ≤35B caveat): tool-call accuracy ~95%→~25% as tool catalog grows
1→8 (keep the MCP surface lean); failed-attempt context poisons retries (delegation retry gain
~3pp vs ~20pp for search) — independent support for suppression-heavy packing.

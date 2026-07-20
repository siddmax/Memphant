# MemPhant Campaign Handoff — paste this to resume (rewritten 2026-07-19; supersedes the PMU pointer and the 2026-07-12 R2→R6 prompt; history in git)

Current STATUS mirror: RUNTIME COMPLETE — BENCHMARK EVIDENCE PENDING

You are resuming the MemPhant SOTA campaign in `/Users/sidsharma/Memphant` (sibling
`/Users/sidsharma/Syndai`; spec mirror stays drift-free: `python3 scripts/check_spec_drift.py`).
Execution style: **SDD** (subagent implementers, briefs in `.superpowers/sdd/briefs/`, per-task
review; ledger `.superpowers/sdd/progress.md`). Owner priorities: **accuracy > cost > speed**;
tri-domain — **agents (chat), RAG (docs), codebase** — on ONE substrate; end state = Syndai
cutover done, CaaS offered, any-agent distribution (MCP + file adapters). Pre-production: anything
may be rewritten; never fabricate a number.

**THE FOCUS RULE (owner directive, 2026-07-19).** One active campaign per lane, no detours. The
PMU/MemSyco weeks produced elite-discipline evidence on a benchmark nobody watches — do not
repeat that. Any new benchmark, official track, or research rabbit-hole needs an explicit owner
call first. Sequencing is fixed: **P0 repo green → P1 feature tests (n=12) → P2 cutovers
(Syndai RAG + CaaS) → P3 public SOTA proof (SWE + LME-V2, leaderboard-submitted).**

## 1. What happened before and what worked (compressed; details in the linked artifacts)

- **Runtime is complete and proven** (Postgres REST/MCP/CLI/worker, e2e green, scratch-DB
  harnesses; bitemporal recall, exact-unit state-aware mutations, chain-heads, fail-closed
  zero-target — all runtime-proven). STATUS ledger: `docs/superpowers/specs/memphant/STATUS.md`.
- **PMU campaign complete (2026-07-18)**: MemPhant 298/300 answer accuracy / 300/300 preference
  use vs RawDialogue 257/291; paired bootstrap lower bounds +0.1000/+0.0133; recall p95 70.61 ms.
  Dev-evidence only; official track burned. Canonical record:
  `docs/handoff/2026-07-18-memsyco-personalized-use-sota-handoff.md`; scorecard under
  `.../candidate-d3c4475f-v12-aggregate/FULL300-SCORECARD.json`. Approved claim wording lives
  there — nothing stronger.
- **What worked (keep doing):** the 12-case dev-pack → sealed independent confirmation →
  full-run loop; one narrow deterministic lever at a time (all 7 retained PMU levers are
  extraction/canonicalization/packet seam fixes); trace-first diagnosis (fix the first failing
  stage); rejecting graph memory / learned gates / decay four separate times — externally
  vindicated (Mem0 dropped its graph; Mem0 scores BELOW no-context on SWE-ContextBench).
- **What failed (stop doing):** spending elite evidence on low-mindshare benchmarks; opening
  official/sealed tracks before the shared pipeline survived full-scale exposed pressure (three
  MemSyco tracks + PMU official burned on first extractor failures); producing evidence that
  never lands on any public surface; leaving the repo gate red.
- **Docs lane mechanism validated (2026-07-13)**: balanced admission + Voyage rerank-2.5 top-8 on
  the corrected 4,870-section corpus — R@10 0.050→0.283 (v1) / 0.100→0.417 (v2), supported
  answers 0.083→0.350 / 0.117→0.417, CIs exclude zero, p95 ≈ 0.9–1.0 s, zero degraded rows
  (`docs/build-log/2026-07-13-rag-retrieval-admission.md`). R6 replacement unlock NOT yet fired.
- **Chat lane**: QA 0.56–0.584 dev (below the honest reproducible band 0.58–0.72). The R2
  full-500 protocol run — declared the unlock on 2026-07-12 — was never run. It is now P3.1.
- **Verified external landscape (2026-07-18/19, two adversarial deep-research passes, primary
  sources):** no vendor has credible SOTA — Supermemory 95% (favorable-judge R@15), Mastra 94.87%
  (self-run), Zep 71→90% (self; 63.8% independent) are all self-reports with copied competitor
  numbers; LoCoMo is discredited (6.4% wrong keys, Mem0–Zep denominator scandal → Zep forced to
  75.14%±0.17). Credible neutral instruments: **LongMemEval-V2** (451Q, 5 abilities, 25M–115M
  tokens; best = authors' coding-agent AgentRunbook-C 72.5% vs strongest RAG 48.5%; official
  leaderboard EMPTY), **SWE-ContextBench** (third-party n=99: Supermemory 30.3% resolved best;
  Mem0 24.24% < no-context 26.26%), **SWE-Explore** (agentic explorers ≫ classical retrieval;
  line-level coverage is the axis), **MemBench** (ACL 2025). The de facto evidence bar (set by
  the Mem0–Zep fallout): uniform configs/prompts, standardized judge, multi-run variance,
  faithful ingestion, leaderboard or third-party run. Nobody has sealed holdouts — our
  discipline already exceeds the bar; the gap was benchmark allocation, not method.

## 2. Standing rules (binding)

All prior promotion rules hold: packaged-runtime + pinned-corpora + executed-scorer provenance;
paired bootstrap, two-seed/pooled CI excluding zero; same-lattice AND same-binary pairing;
scratch DBs only; verbatim-is-the-memory; deterministic writes; "SOTA" language banned until the
P3.1 protocol run; commits small with `Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>`;
never push without owner call. New rules from this rewrite:

1. **n=12 micro-eval gate (owner directive).** Every new lever/feature runs a ~12-case exposed
   dev pack vs control FIRST (the PMU dev-pack pattern — it works). Pass → n≈100–300 paired
   confirmation for promotion; full-scale runs are for claims only. No integration without the
   n=12 pass. Losing levers: delete code, keep the negative-result artifact.
2. **No official/sealed track opens until the identical pipeline is green on a full-scale
   exposed run.** (The rule the MemSyco burns paid for.)
3. **Mindshare rule.** Headline claims only on LME-S/V2, SWE-ContextBench/SWE-Explore, or a
   board-backed instrument. Memora/STALE/MemSyco/MemBench are supporting evidence. LongEval-RAG
   and Re2Bench could NOT be confirmed to exist — never plan spend on them without verifying.
4. **Submission is part of done.** A P3 campaign is complete when its result is submitted
   (official board) or published on **Evalrank** (Syndai's leaderboard feature —
   `backend/src/features/evalrank/` — our public board for benches without an official one:
   publish harness, configs, variance, and same-harness competitor re-runs there).

## 3. Phase plan

### P0 — Green the repo (days; blocks everything)

Fix the exact red predicates preserved in the PMU `POSTFLIGHT-VERIFICATION.json`: five Python
contract failures, six Clippy findings, three stale `ReflectInput` fixtures, one e2e
retain-response mismatch. Land the dirty worktree in reviewed commits (owner call on push). Run
the full `AGENTS.md` gate green. Nothing external ships and no cutover merges while red.

### P1 — Feature-test wave (n=12 gates; techniques from the verified 2026-07 research)

Each lever: named failure class → 12-case pack vs control → promote/delete. Ranked by lane:

**Chat/agents (failure classes from the flip analysis: 21/44 reader-with-adequate-pack, 16/44
pack displacement):**
- T1 **Observation-block hot plane** (Mastra OM pattern, verified mechanics: background
  compression of history into dense date-annotated observations; retrieval-free block +
  retrieval hybrid). Our `reflect` loop is the natural owner; generalizes the R2 profile-block
  lever. The single highest-external-evidence chat lever (OM self-reports beating the oracle).
- T2 **Chain-of-note reader v4** + calibrated abstention comparison arm.
- T3 **Temporal re-measure** (date-prefix muting fixed; soft temporal boost; dated packs).
- T4 **HyDE/query rewrite A/B** (cheap; preference-stratum misses).
- T5 (diagnostic) cross-rerank at n=300 with `recall_pool_depth=64`.

**File scale (the verified winner shape: agentic exploration over files beats top-k RAG by ~24
pts at trajectory scale — LME-V2 72.5 vs 48.5):**
- T6 **Agentic deep mode** (AgentRunbook-C pattern: trajectories/resources as files + workflow
  doc + memory manifests + helper scripts; agent gathers evidence at query time). Maps to
  dormant rung-12 L4 exhaustive — activate as `mode=deep` over the substrate. Latency 100 s+
  acceptable in deep mode only; hot path unchanged.
- T7 **L0/L1/L2 multi-resolution resource summaries** (`04` §6.1, dormant; the
  scan-20-abstracts-load-3 coding-agent case).

**Code lane:**
- T8 **Outcome write-back** (worked/failed/invalidated → procedural units; ReasoningBank-shaped
  `04` §4.1 payload already spec'd). External signal: Letta skill-learning reports +9/+15.7 abs
  on Terminal Bench 2.0 (vendor-reported; direction matches our design).

**Temporal/longitudinal (years-scale; "sleep consolidation"):**
- T9 **Sleep-time anticipatory consolidation**: idle-time `reflect` pre-computes the observation
  block / runbook summaries. Verified boundaries (Letta paper, 3-0): pays off only when ≥~10
  queries amortize one context, and loses to plain test-time scaling at high budgets — gate on
  amortization math, chat lane first. (`04` §9 reflect IS this loop; the lever is the
  anticipatory-output half.)
- T10 **Active freshness** (churn classes on volatile facts, `04` §8.1, dormant) — the
  years-scale staleness answer; pilot on aged-fact packs.
- T11 **DSR decay fold pilot** (`fsrs-rs` over the review ledger, rung 11) — internal
  MemoryStress-style longitudinal suite only; FSRS must beat plain exponential or stay off.
  External evidence for FSRS-in-agents: none survived verification — treat as hypothesis.

Architecture audit verdict (2026-07-19): the substrate already specifies everything the verified
landscape rewards — five kinds, bitemporal + supersession, retention tiers hot/warm/cold,
write-time admission control, consolidation with stage checkpoints, event-sourced
confidence/decay ledgers. **No new architecture is needed for P1–P3; the work is measured
activation of dormant mechanisms.** Watch-only: learned consolidators (Auto-Dreamer, +7 pts,
unreplicated preprint) stay data-gated per rung 13.

### P2 — Cutovers (Syndai RAG + CaaS; internal gates, NOT public claims)

- **Docs/RAG**: hierarchy parity → comparable-volume full-corpus MemPhant-vs-Syndai on both
  exposed sets (k=10/b8192; fires the pre-registered R6 unlock on CI-clean) → live restraint
  10/10 → chat non-regression → sealed version-disjoint holdout → flip
  `memphant_file_memory_dogfood_enabled` on the Syndai docs surface, then episodic.
- **CaaS/codebase**: resume `docs/superpowers/plans/2026-07-15-syndai-canonical-memory-cutover.md`
  task-by-task (subject-bound identity → strict public contracts → provenance/idempotency → …);
  spec-28 fixture families (arch-decision honored, compaction rehydrate, cross-agent transfer,
  composite) are the acceptance gate; parity-or-better under the 2,500-token cap; six-table drop
  only after per-surface proof. T6/T7/T8 feed this lane.
- Exit: Syndai reads memory only through public MemPhant contracts; legacy tables dropped.

### P3 — Public SOTA proof (small runs first; submission = done)

1. **LME-S full-500 protocol run** (the internal "SOTA-language" unlock) + **same-harness
   competitor re-run** (Mem0, Zep CE/Graphiti, Letta) under uniform configs, standardized judge,
   10-run variance — published on Evalrank with the harness. This is the Mem0–Zep lesson
   weaponized: we run the controlled comparison.
2. **SWE-ContextBench Lite** (99 related tasks; match the paper's Claude Sonnet 4.5 setup;
   ~20-task subset first). Floor: beat no-context 26.26% resolved. Target: beat Supermemory
   30.3% → "outperforms every evaluated memory product on a third-party coding-memory
   benchmark." Then **SWE-Explore** (line-level localization under budget) with T6. No official
   boards found for either → Evalrank.
3. **LongMemEval-V2** — the scale flagship (replaces "BEAM 1M": BEAM's operator/submission
   surface failed verification and its board is competitor-associated per `27` §1; LME-V2 is
   neutral, 25M–115M tokens, and its empty official leaderboard is a first-mover slot). Verified
   submission mechanics: Google Form (`forms.gle/rxUpiuRKDERqpqSi9`), tarball with
   SYSTEM_DESCRIPTION.md + code + `submission_overview.json`; **paired web+enterprise FULL runs
   per operating point, every question covered** — a 12-case pilot is private-only. Ranking =
   LAFS Gain (accuracy-latency frontier), which fits us: submit TWO operating points — fast
   (RAG hot path, sub-second) and deep (T6 agentic mode). Path: Small-tier ~50Q dev subset →
   full paired runs → submit. Reference bars: strongest RAG 48.5%, Codex 69.3%, AgentRunbook-C
   72.5% at 108–140 s/query.
4. Supporting (never headline): full STALE if free capacity; Memora only if the next causal
   slice improves 53.49; MemSyco held until a new version/holdout exists.

## 4. Claim ladder (exact wording gates)

- Now: only the approved PMU dev-evidence sentence (see the 07-18 handoff).
- After P3.1: "full-protocol LongMemEval-S score X±σ; controlled same-harness comparison vs
  Mem0/Zep/Letta under uniform configs" (never "global SOTA").
- After P3.2: "outperforms all evaluated memory products on SWE-ContextBench related tasks
  (third-party benchmark)."
- After P3.3 acceptance: "first published result on the official LongMemEval-V2 leaderboard."
- Pareto wins are claimed as "Pareto frontier," never "best accuracy" (`27` §6 rules apply).

## 5. Ops gotchas (carried; cost hours before)

- Keys: `VOYAGE_API_KEY` in Doppler **tacitry/dev**; GEMINI/OPENAI/OPENROUTER in syndai/dev.
- Build campaign binaries from a **clean worktree** at the measured commit; pass
  `SERVER_BIN/WORKER_BIN/CLI_BIN` + port env to runners. `CARGO_TARGET_DIR=/tmp/...`,
  `CARGO_BUILD_JOBS=1`, `CARGO_INCREMENTAL=0` must survive the Doppler boundary.
- One campaign lane, no completion cache, fresh scratch DB per arm, drop at exit; memory arms
  sequential (`tuple concurrently updated` is infra contention, not a result).
- Before claiming "stopped": check process TREES, tmux, AND `launchctl list | rg memphant`
  twice, 3 s apart — delayed Codex app-server children and launch agents restart controllers.
- Artifact trees can vanish from canonical paths while inodes live under `/.vol` — stat/find
  before recreating anything; mirror + `MIRROR.json`, never mutate the source.
- Reader cache `docs/build-log/artifacts/r0-embedder/reader-cache` is shared — reuse it;
  re-scores of unchanged evidence are free.
- First model use downloads weights at server boot (10 min); keep `.fastembed_cache/` warm.
- Absolute `MEMPHANT_STRUCTURED_STATE_PROMPT_PATH` required — the extractor prompt is hash-bound.

## 6. Carry-list (fold opportunistically; none block)

Packing tier reconciliation (conservative gate holds); fastembed-less CI leg for feature-off
tests; Retry-After clamp ≤60 s; `FastEmbedModel::parse` vs `fastembed_arm` dedup; trim-vs-span
exactness; W9 tail cap; fence-aware chunk header; `build_service` panic→error; worker skips
reranker load; chat-score missing-baseline warning; persist chars/question + rerank p50/p95 into
provenance; resource-chunks retirement decision (3× ns).

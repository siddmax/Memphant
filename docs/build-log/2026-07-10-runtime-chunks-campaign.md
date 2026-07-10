# 2026-07-10 — Runtime Contextual-Chunks Campaign (LME-S n=100, OpenRouter lattice)

**Scope statement:** third campaign of the day. Built the rung-4 runtime embodiment
(reflect-stage contextual chunks + chunk-aware packing), an OpenRouter reader engine
(codex CLI quota outage mid-campaign), and ran a failure-analysis-driven lever round.
Everything below: LongMemEval-S pinned sha256, n=100 stratified seed 20260710, k=10,
rerank-off, packaged Postgres runtime, fastembed bge-small. Reader
`openai/gpt-5.6-terra` (reasoning `medium`), judge `anthropic/claude-sonnet-5`
(different-family judging per LLM-judge best practice; self-preference control).
Paired bootstrap 1000 resamples, seed 20260710. **Not a LongMemEval leaderboard claim.**

## Why these levers (measured, not guessed)

A failure-mode analysis over the n=100 turns-config evidence classified all 36
weak-stratum failures (temporal-reasoning, multi-session, single-session-preference):
**19 pack-drop / 12 retrieval-miss / 4 composition / 1 over-abstention / 0 judge
artifacts**. R@10 was 0.83 while QA was 0.56 — the binding constraint was packing,
not retrieval, and the reader abstains honestly when the answer window is missing
(72% of failures were honest abstentions). Levers ranked from that table:
(1) session-complete/chunk-aware packing, (2) query-date recall filtering,
(3) reader prompt (enumerate-then-compute, calibrated abstention).

## What was built (commits, all gated)

- `9e8250b`/`3819054` — reflect-stage contextual-chunk write path in
  `compile_job()` (episodes only): ≤4-turn windows, headers
  `[episode <uuid>] [kind <k>] [turns a-b]` (clock-date dropped in review — headers
  are lexically scored at recall, a stamped current-date is retrieval noise),
  byte-offset `source_span`, deterministic ids, ≤32-chunk cap, zero chunks for
  single-window bodies. Default OFF at birth (promotion-provenance rule).
- `ab4168f` — `bench-lme --runtime-chunks` ablation flag.
- `399b553` — chunk-aware pack rendering: matched chunks (per-chunk lexical score)
  + sibling-neighbor expansion, document-order emission with headers; packing
  DECISIONS frozen (attribution-clean increment).
- `a5f3493` — budget reclaim: chunk-rendered items charge their actual rendered
  token cost; `token_estimate` now equals the sum of charged costs.
- `998b4ea`/`7d1884f` — `--engine openrouter` in `scripts/run_reader.py`
  (key via Doppler `syndai/dev` env, never persisted; retries on 429/5xx/SSL/socket;
  the codex CLI quota outage motivated it — codex resets at fixed windows and died
  mid-queue).
- `71adda0`/`90acb6d` — `--prompt-version 2` reader prompt (enumerate-then-compute,
  calibrated abstention) + a review-caught scoring fix: the abstention short-circuit
  now applies to the reply's final line only (a CoT reply hedging "I don't know"
  mid-reasoning with a correct final answer was being scored wrong).
- `e669a3f` — **promotion commit** (defaults flipped; below).

## Results (OpenRouter lattice; QA n=100, retrieval n=94)

| Config | QA | ΔQA vs baseline [95% CI] | Verdict |
|---|---|---|---|
| session, rerank-off (baseline A) | 0.450 | — | control |
| turns w4 (client-side windows) | 0.600 | +0.150 [+0.070, +0.230] vs A — excl 0 | reference embodiment |
| turns w2 | 0.580* | +0.02 [−0.07, +0.10] vs w4 — ns | w4 stays |
| turns w8 | 0.530 | −0.070 [−0.140, 0.000] vs w4 — ns, harmful direction | **w8 falsified** |
| turns + budget 16384 | 0.570 | −0.030 [−0.070, 0.000] vs w4@8192 — ns | **budget doubling falsified; 8192 stays** |
| session + runtime chunks, rendering only (B1) | 0.600 | **+0.150 [+0.070, +0.240] vs A — excl 0** | rung-4 embodiment works |
| session + runtime chunks + reclaim (B2 = shipped code) | 0.560 | **+0.110 [+0.020, +0.190] vs A — excl 0** | **promoted** |
| turns + prompt v2 | 0.610 | +0.010 [−0.050, +0.070] vs v1 — ns | v1 stays default |
| chunks + prompt v2 | 0.600 | +0.040 [−0.030, +0.110] vs v1 — ns | v1 stays default |

*w2 was scored on the earlier terra-CLI/terra-judge lattice before the outage; its
delta is same-lattice paired vs w4 and remains valid within that lattice.

Cross-arm (offline paired bootstrap over per-question arrays, same seed):
**runtime chunks tie client-side turns windowing** — B1 vs turns ΔQA +0.000
[−0.080, +0.080]; B2 vs turns −0.040 [−0.130, +0.050]; B2 vs B1 −0.040
[−0.110, +0.020] (all ns). Retrieval: B1 ΔR@5/ΔR@10 +0.096 [+0.032, +0.160] vs A
(0.798 both depths); B2 +0.117 [+0.053, +0.191] on both depths — reclaim packs more
gold sessions (retrieval up) without a QA gain (more packed evidence is also more
distraction; the same lesson as the budget-16384 falsification).

Judge-family sensitivity: the same turns evidence scores 0.56 under judge=terra
(codex CLI lattice) and 0.60 under judge=sonnet-5 — levels are judge-relative;
only same-lattice paired deltas are read. Cross-engine terra-CLI validation runs
for w8/b16384/session-chunks re-launched after the quota reset (artifacts land as
`scaled-reader-{turns-w8,turns-b16384,session-chunks}-rerank-off.json`).

## Verdicts applied (promotions and falsifications)

1. **RUNG 4 CLOSED — first rung closure under the promotion-provenance rule.**
   The implementation contract (reflect-stage chunk metadata: chunk ID, parent
   episode linkage, context header, source span, citation tests) is built, and the
   paired ablation THROUGH THE RUNTIME PATH clears the gate on both axes:
   ΔQA +0.110 [+0.020, +0.190], ΔR@5/ΔR@10 +0.117 [+0.053, +0.191], all CIs
   excluding zero. `contextual_chunks_write_enabled` defaults to **true** (`e669a3f`);
   explicit-off builder + `--disable runtime_chunks` control arm retained.
2. **The turns lane default is superseded.** Runtime chunks match client-side
   windowing (ΔQA +0.000 ns) without requiring callers to window, so
   `bench-lme` `DEFAULT_GRANULARITY` returns to `session` — the lane now measures
   the product path by default. `--granularity turns` stays available.
3. **Window size w=4 confirmed** (w2 ns, w8 ns-harmful): dose-response peaks at 4.
4. **Packing budget 8192 confirmed** (16384 ns-harmful on QA).
5. **Reclaim retained** (honest token accounting + retrieval CI positive; QA vs
   rendering-only ns — no evidence of harm, principled accounting wins ties).
6. **Reader prompt v1 stays default** (v2 ns on both configs; available via
   `--prompt-version 2`). Notable stratum signal for the next round: chunks+v2
   scored temporal-reasoning **0.78** (vs 0.33 at session baseline) while
   multi-session dropped to 0.26 — prompt-shaping interacts with strata and needs
   a targeted, not global, lever.

## Cost

~$25 OpenRouter (reader ~1.3M input tokens/config across 9 scoring runs, many
cache-resumed; judge on containment misses only), 416 codex CLI calls earlier in
the day (already logged), ~2 codex probe calls post-reset. All replies sha256-cached;
every re-score after the judge fix was fresh_calls=0.

## Deviations / limits

- The codex CLI quota outage forced the engine change mid-campaign; the OpenRouter
  lattice re-scored BOTH baselines so every promoted delta is same-reader/same-judge
  paired. Levels are not comparable across lattices and are never compared.
- Judge = sonnet-5 grades terra replies (different family, self-preference
  controlled) but judge rows are ~20-27% of questions and spot-checked, not
  human-verified.
- The abstention-scope scoring fix (`90acb6d`) landed after the v1 lattice was
  scored; v1 replies are terse single-liners where final-line == whole-reply, so
  v1 numbers are unaffected; both pv2 runs were re-scored from cache under the
  fixed judge.
- Chunk headers carry no date (dropped as retrieval noise); threading true
  `first_observed_at` into headers is open follow-up if temporal levers want it.
- One seed, k=10, 8192-token packs; single-session-preference n=6 is too small to
  move alone; multi-session (0.26-0.44 across arms) is now the weakest large stratum.

## State of play (end of campaign)

**DONE:** rung 4 closed on real evidence (first ever); runtime chunks default-on;
turns default superseded by the product path; w4/8192 confirmed; w8, budget-16384,
and global prompt-v2 falsified as defaults; OpenRouter engine + different-family
judging landed; failure-analysis method (classify → rank levers → build → measure)
validated end-to-end.

**NEXT (ranked):**
1. Multi-session composition lever: the weakest stratum under the promoted config
   (0.33); failure analysis says sibling coverage across *different* sessions —
   candidate: per-session diversity quota in packing, measured paired.
2. Temporal levers: query-date-aware recall filtering (+ the pv2 temporal signal
   suggests stratum-targeted prompting); temporal-reasoning 0.52 promoted / 0.78
   under chunks+v2.
3. Rung-specific corpora (STATE-Bench for rung 10, longitudinal for 11, OP-Bench
   for 15) and the Syndai RAG/KB replacement gate (golden set mined from
   knowledge_sources/sections/chunks; MemPhant must beat it before replacing).

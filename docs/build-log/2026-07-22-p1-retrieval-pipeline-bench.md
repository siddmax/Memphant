# P1 Retrieval-Pipeline Benchmark — 2026-07-22

Rigorous per-stage benchmark of the retrieval pipeline (embedders / retrieval
algorithms / rerankers) across accuracy, cost, latency, on ONE shared
hard-adversarial LongMemEval-S fixed pool. Owner priority accuracy > cost > speed.

Full results table + cost analysis: `artifacts/p1-retrieval-bench/RESULTS.md`.
Reproducible scripts (committed): `build_adversarial_set.py`, `harness.py`,
`run_piece1.sh`, `run_piece3.sh`, `cost_analysis.py`. Corpus text + 173MB pool.json
+ vector/response caches gitignored; per-arm score JSONs committed under `scores/`.

## Headline findings

1. **Reranking is the highest-ROI stage AND the only recurring cost.** +0.111 recall@5
   / +0.223 MRR over no-rerank on the n=72 adversarial set. Voyage rerank-2.5 wins every
   accuracy axis and is fastest (R@5 0.944, MRR 0.858, 705ms) at ~$4.50/1k queries;
   Cohere v4-pro is the value pick (indistinguishable at 0.006 MRR, 44% cheaper $2.50/1k);
   local MiniLM is the $0 floor (0.889/0.804 but 9.8s/query CPU). Cohere v4-fast is a
   ranking trap (max R@5 but MRR 0.790 < free local).
2. **Embedders separate on raw retrieval but the reranker erases the gap.** gemini-embedding-2
   0.931 / voyage-4 0.917 vs free bge 0.833 retrieval-only; all → ~0.889 after reranking.
   Embedder cost is a one-time index ($0.57 voyage-4 / $1.88 gemini-2 per 33k chunks), ~$0/query.
   So: if you rerank, free bge is right; pay for a better embedder only in a no-rerank path.
3. **Convex fusion (0.847) > RRF (0.833).** Confirms Bruch et al. MMR (0.750) and
   instruction-prompting (0.694) both HURT single-gold recall; cheap context-prepend no-op.

## What was built (all committed)

- **T1 adversarial pool** (`build_adversarial_set.py`): 80q (72 scored + 8 abstention),
  ~100 docs/q, 33,224 unique chunks, mixed BM25/embedding hard-negative mining, LLM
  duplicate-leak audit + gold "contributes" verification, 0 guard violations. Regenerate
  via Doppler (OpenAI mining embeds + OpenRouter claude-sonnet-5 verify, all cached).
- **T2 chunk-granularity cross-rerank** (`CrossRerankGranularity`, `MEMPHANT_RERANK_GRANULARITY`,
  `--rerank-granularity`) + the **sub-chunk cap** (`RERANK_CHARS_PER_TOKEN`,
  `sub_split_for_rerank`, `RERANK_MAX_SUBCHUNKS_PER_CANDIDATE`). Measured ~80% of prod
  contextual_chunks exceed the 512-token wall; the cap sub-splits at rerank read-time
  (zero embedder cost, zero storage, local-CPU-only; 30s→11s/query). Full TDD coverage.
- **T3/T4 seam tools** (`embed-pool`, `rerank-pool` in `pool_tools.rs`) run every arm through
  the PRODUCTION `embedder_from_id` / `build_cross_reranker` seams. New embedder arms
  `jina-v5-small` + `gemini-embedding-2` (mirrored api_embeddings shapes, live-probed).
  Bench-only throttle (`POOL_RERANK_SLEEP_MS`) + per-question rerank retry for hosted
  rate-limits — prod one-shot behavior unchanged.
- **Harness** (`harness.py`): BM25/RRF/convex/MMR/dense+chunk-maxpool retrieval, context-prepend,
  ColBERT MaxSim (V6), recall/MRR/cov scoring, exact sign test + bootstrap CI, all under
  `harness.py selftest`.

## Incidents & how they were handled

1. **Two process-exit interruptions** killed background jobs mid-run. Caches (LLM verify,
   mining embeds, model weights) persist, so builder re-runs were cheap; the T2 subagent's
   work was verified intact on resume. Lesson: commit early + often (11 commits this session).
2. **Adversarial-set answer-leak on numeric answers:** a bare "2" appears in ~40% of docs, so
   the string-absence guard was near-useless. Fixed with an LLM duplicate-audit ("could this
   session ALONE yield the labeled answer?") that waives coincidental mentions and removes true
   unlabeled duplicates — the load-bearing check became the gold "contributes" verification.
3. **OpenRouter empty-content on tiny max_tokens:** reasoning-on models spent the whole budget
   on reasoning → None content. Fixed with `reasoning: {enabled: false}` + retry.
4. **Over-parallelization repeatedly starved CPU-bound jobs** (local MiniLM rerank, `small`
   embed, ColBERT MaxSim all fighting). Managed with SIGSTOP/CONT pausing and serializing the
   heavy jobs. Lesson: cap concurrent CPU-bound jobs to ~cores.
5. **Cloudflare 1010 block** on Jina from the default python-urllib UA → added a curl UA header.
6. **Hosted reranker rate limits:** Voyage 2M-tok/min and ZeroEntropy 429 both tripped by the
   tight 72-question loop → bench throttle + retry/backoff (bench-only).
7. **jina-v5-small free-tier token rate limit** (100k tok/min) blocked its full-pool embed after
   640 chunks — recorded as an honest gap, not chased.
8. **ColBERT V6 pure-Python MaxSim** is ~5min/question (no numpy, stdlib-only per plan) → full
   80q was ~6h. Ran the pre-registered 24-question paired subset instead (directional).

## Reconciliation with prior findings

- **R0 embedder bakeoff** ("chat lane not embedder-bound"): replicated at higher power — the
  reranker erases embedder differences. The new nuance: embedders DO separate on retrieval-only.
- **Reranker spike** (fixed-pool, predicted chunking helps): the spike forced answers deep;
  live LME-S answers are often shallow, so on the live A/B chunk-granularity didn't help. The
  adversarial set (harder, buried) is the binding comparison; the sub-chunk cap makes the
  premise actually hold on prod-sized chunks.
- **MiniLM-L6-int8 as local reranker winner** stands: $0, R@5 0.889/MRR 0.804, Apache/CPU — the
  free floor. Hosted arms buy +0.055 R@5 for recurring $.

## Status

DONE_WITH_CONCERNS. All three pieces measured and committed with the cost lens; production
chunk-rerank cap landed + tested. Concerns: (1) V6 ColBERT on 24q subset only (directional);
(2) jina-v5-small rate-limited, not completed; (3) all numbers are the retrieval-frontier
screen — a DEFAULT flip still needs paid reader-QA (n≥100). Keys handled env-only, never
committed (value-anchored sweep on every commit).

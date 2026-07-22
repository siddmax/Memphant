# P1 Retrieval-Pipeline Benchmark — Results

Retrieval-frontier screen (owner P1). ONE shared hard-adversarial LME-S fixed pool,
reused across all three pieces. **Framing:** rung-7/A1 evidence says packing (not
retrieval ordering) dominates current-default misses; this benchmark pre-positions
for post-packing headroom. A retrieval win here is NOT an end-metric claim — the
binding accuracy gate stays paid reader-QA (n≥100).

## The shared adversarial set (built once, T1)

- **80 questions** (72 scored + 8 abstention), seed 20260722, from LongMemEval-S.
  Strata: 24 multi-session, 24 temporal-reasoning, 16 knowledge-update, 8 deep-buried
  single-session, 8 abstention.
- **~100 docs/question** (97–100): the question's own haystack sessions (same-user
  topical near-duplicates) + hard negatives mined half by BM25, half by embedding
  cosine to the gold. **33,224 unique chunks**, 1200-char windows; median gold
  answer buried **1195 chars** in (33 golds past 2000).
- **Guards (0 violations):** no answer-string leak into non-gold (LLM-audited for
  numeric/short answers — a bare "2" appears in ~40% of docs, so the load-bearing
  check is the LLM "contributes" gold verification, not string-absence); gold
  verified 72/72 (35 string-located + 37 LLM); abstention pools spot-checked so the
  top-5 most-similar docs do not answer.
- Corpus text + 173MB pool.json + caches gitignored; builder + harness committed.
  Regenerate: `doppler run -p syndai -c dev -- python3 build_adversarial_set.py <lme-s> --out pool.json`.

## Metrics & method

Realistic flow per question: embed ~100 docs (chunked) → retrieve top-48 → rerank to
top-5. **Primary = recall@5** (any gold in top-5) + **MRR** of first gold. R@48 is the
retrieval screen but is **near-ceiling (1.000) on a 100-doc pool**, so per the
pre-registered freeze rule the decision metric is **MRR** (tie → R@16 → cheaper →
local). gold_cov@5 = fraction of a multi-gold question's golds in top-5. Paired
(same questions/seed); n=72 scored → exact sign test + bootstrap CI (seed 20260722);
deltas needing <~8 one-sided flips stay directional.

---

## Piece 0 — production chunk-granularity rerank (T2, landed)

Measured that **~80% of prod `contextual_chunks` exceed the 512-token BERT wall**
(median ~945 est. tokens, max ~2546; the adaptive window caps chunk COUNT at 32, not
token length). So the first chunk-granularity landing still truncated long chunks.
Fixed with a rerank-time sub-split (≤ max_length×3 chars, max-pool, ≤16 sub-chunks/
candidate). **Cost boundary:** rerank-time only — zero embedder cost, zero stored
bytes, local-CPU only; latency ~30s→~11s/query vs uncapped.

**T2-S6 live A/B** (bench-lme n=20 seed=3, MiniLM byo, scratch PG; `t2-ab/`):

| granularity | recall@5 | recall@10 | cross_rerank median | note |
|---|---|---|---|---|
| body (default) | **0.526** | 0.526 | 1768 ms | |
| chunk (capped) | 0.474 | 0.474 | 10758 ms | −1 multi-session q, within n=6 noise, ~6× latency |

Honest reconciliation vs the fixed-pool spike (which predicted chunking helps): the
spike FORCED answers deep to stress the 512-token wall; live LME-S answers are often
shallow enough that body-rerank already sees them. The binding granularity comparison
is the n=72 adversarial set below, not this n=20 smoke.

---

## Piece 1 — embedders

Same retrieval (RRF hybrid, v2) + same reranker (MiniLM chunk) for every arm on the
n=72 adversarial set. **recall@48 = 1.000 for every arm** (gold always in-pool at 100
docs) — reported once here, not per row; the discriminator is MRR / recall@5.

### Local-arm projection gate (pre-registered: retire if full-pool embed projects >2h)

| arm | 200-chunk smoke | full-pool (33,224) projection | verdict |
|---|---|---|---|
| small (bge-small-en-v1.5, 384d) | — | **34 min measured** | RUN |
| modernbert-embed-large (1024d) | 124 s | ~5.7 h | **RETIRE** (CPU cost, per R0) |
| embeddinggemma-300m (768d) | 109 s | ~5.0 h | **RETIRE** (CPU cost, per R0) |

modernbert/gemma retired from the full run on CPU-cost grounds (honors R0's finding);
a bounded subset run is a follow-up if their frontier position matters.

### Results (n=72, MiniLM chunk-rerank, recall@5 primary)

| embedder | provider | recall@5 | MRR | gold_cov@5 | query-embed lat | $/1M tok | note |
|---|---|---|---|---|---|---|---|
| small (bge, local) | local | 0.889 | 0.804 | 0.784 | ~7 ms | free | anchor; +0.056 R@5 / +0.169 MRR over no-rerank |
| _(no rerank, small)_ | — | 0.833 | 0.635 | 0.653 | — | free | retrieval-only reference |
| openai-3-small | API | _pending_ | | | | $0.02 | |
| gemini-embedding-001 | API | _pending_ | | | | $0.15 | |
| gemini-embedding-2 | API | _pending_ | | | | $0.20 | new arm |
| jina-v5-small | API | _pending_ | | | | ~free tier | new arm (1024d) |

_(API arms running full-pool; table fills on completion.)_

---

## Piece 2 — retrieval algorithms

_(pending — fixed on the best piece-1 embedder; V0 dense / V1 BM25 / V2 RRF / V3 convex
/ V4 instruction-prompt / V5 context-prepend / V6 ColBERT MaxSim / V7 MMR.)_

## Piece 3 — rerankers

_(pending — local: none/MiniLM-chunk/bge-chunk; hosted GATED on owner Cohere/ZeroEntropy/
Voyage keys — PARTIAL until pasted.)_

## Costs & caveats

- Pool build: OpenAI mining embeds (~$0.05, cached) + OpenRouter claude-sonnet-5
  verify (~$1, cached). Piece-1 API embeds: pending (bounded, 200M free Voyage tier
  n/a without key).
- n=72 gives the paired comparisons real power (small's +0.169 MRR from rerank is well
  outside noise), but individual embedder deltas may be directional — CIs reported per
  comparison.
- Every "winning config" here is a retrieval-frontier screen result; a DEFAULT change
  still requires the separate paid reader-QA gate.

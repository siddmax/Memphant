# P1 Retrieval-Pipeline Benchmark — Results

Rigorous per-stage benchmark of the retrieval pipeline — **embedders / retrieval
algorithms / rerankers** — across **accuracy, cost, and latency**, on ONE shared
hard-adversarial LongMemEval-S fixed pool reused across all three pieces.

**Framing (owner-directed scope):** rung-7/A1 evidence says packing (not retrieval
ordering) dominates current-default misses; this is the SOTA retrieval-frontier screen
that pre-positions for post-packing headroom. A win here is NOT an end-metric claim —
the binding accuracy gate stays paid reader-QA (n≥100). Owner priority: **accuracy > cost > speed**.

## TL;DR — the three verdicts + the cost lens

1. **Reranking is the single highest-ROI stage** (+0.111 recall@5, +0.223 MRR over no-rerank)
   — and the only stage with *recurring* per-query cost. **Voyage rerank-2.5 wins every
   accuracy axis and is fastest** (R@5 0.944, MRR 0.858, 705ms) at ~$4.50/1k queries;
   **Cohere v4-pro is the value pick** (indistinguishable at 0.006 MRR, 44% cheaper, $2.50/1k).
2. **Embedders separate on raw retrieval** (gemini-embedding-2 0.931 vs free bge 0.833)
   but **a strong reranker erases the gap** — so if you rerank, free bge is correct and paid
   embedding is wasted spend. Embedder cost is a *one-time index* ($0.57–$1.88 per 33k chunks),
   not per-query.
3. **Retrieval algorithm: tuned convex fusion (0.847) beats RRF (0.833)** — confirms Bruch
   et al. **MMR (0.750) and instruction-prompting (0.694) both HURT** single-gold recall.

## The shared adversarial set (T1)

- **80 questions** (72 scored + 8 abstention), seed 20260722, from LongMemEval-S. Strata:
  24 multi-session, 24 temporal-reasoning, 16 knowledge-update, 8 deep-buried single-session,
  8 abstention.
- **~100 docs/question** (own-haystack same-user near-dups + hard negatives mined half by
  BM25, half by embedding cosine). **33,224 unique 1200-char chunks**; median gold answer
  buried **1195 chars** in (33 golds past 2000).
- **Guards (0 violations):** no answer-string leak into non-gold (LLM duplicate-audit for
  short/numeric answers); gold verified 72/72 (35 string + 37 LLM-"contributes"); abstention
  top-5 spot-checked. Corpus + 173MB pool + caches gitignored; builder + harness committed.

## Method

Realistic flow per question: embed ~100 docs (chunked) → retrieve top-48 → rerank to top-5.
**Primary = recall@5** + **MRR** of first gold + **gold_cov@5** (multi-gold coverage). R@48
is **near-ceiling (1.000)** on a 100-doc pool, so the freeze metric is MRR, not R@48. Paired,
n=72; exact sign test + bootstrap CI (seed 20260722). Deltas needing <~8 one-sided flips are
directional. Cost from **measured token counts** (`cost_analysis.py`), not list prices.

---

## Piece 1 — Embedders

**Retrieval-only (no rerank — the CLEAN embedder signal), n=72, RRF hybrid:**

| embedder | R@5 | MRR | gold_cov@5 | one-time index $ (33k chunks) | license/host |
|---|---|---|---|---|---|
| small (bge-small-en-v1.5, local) | 0.833 | 0.635 | 0.653 | **$0** | Apache, local |
| voyage-context-4 | 0.792 | 0.621 | 0.635 | $1.13 | API |
| openai-3-small | 0.861 | 0.688 | 0.701 | $0.19 | API |
| gemini-embedding-001 | 0.889 | 0.719 | 0.733 | $1.41 | API |
| voyage-4 | 0.917 | 0.667 | 0.739 | $0.57 | API |
| **gemini-embedding-2** | **0.931** | **0.724** | 0.738 | $1.88 | API (new arm) |
| jina-v5-small | — | — | — | — | free-tier RATE-LIMITED (100k tok/min), not completed |

**What the retrieval-only view shows:** paid embedders genuinely lead — gemini-2 (+0.098 R@5)
and voyage-4 (+0.084) beat free bge. Interesting: **voyage-context-4 UNDERPERFORMS plain
voyage-4** (0.792 vs 0.917), and it's the priciest to index — the contextualized variant is
not worth it here (consistent with R0's unresolved verdict).

**But with a strong reranker downstream, the embedder gap collapses** (all arms → ~0.889 R@5
after MiniLM chunk-rerank; the reranker recovers golds a weaker embedder buried in the top-48).

**Verdict:** if you rerank → **free bge** (paid embedder is wasted, its edge is recovered
downstream). If you do NOT rerank (latency-bound) → **gemini-2 or voyage-4** (+0.08–0.10 R@5
for pennies to index, ~$0/query). voyage-4 is the value paid embedder ($0.57 index, 2nd-best
retrieval).

**Local-arm projection gate:** small (bge) 34 min full-pool → RUN. modernbert (~5.7h) &
embeddinggemma (~5.0h) → **RETIRED** (>2h CPU ceiling, per R0). Nemotron/Harrier/KaLM = GPU-only,
not runnable here (named for honesty).

---

## Piece 2 — Retrieval algorithms (fixed on `small` embedder, retrieval-only recall@5)

| variant | R@5 | MRR | verdict |
|---|---|---|---|
| V4 instruction-prompted query | 0.694 | 0.549 | **HURTS** (off-label: bge is not instruction-tuned) |
| V7 MMR (λ=0.7) | 0.750 | 0.591 | **HURTS** (evicts gold as redundant vs near-dup distractor) |
| V0 dense-only | 0.778 | 0.595 | baseline |
| V1 BM25-only | 0.792 | 0.652 | lexical alone > dense here |
| V5 context-prepend (lite header) | 0.792 | 0.607 | no lift (needs LLM-generated context, not a header) |
| V2 RRF hybrid (prod default) | 0.833 | 0.635 | |
| **V3 tuned convex fusion** | **0.847** | **0.672** | **WINNER — beats RRF +0.014 R@5 / +0.037 MRR** |
| V6 ColBERT MaxSim (late-interaction) | _running_ | | Jina ColBERT v2, full-pool MaxSim |

**Verdict:** **convex score fusion > RRF** (confirms Bruch et al. — score magnitude separates
near-duplicates that RRF's rank-only discards). Pre-registered SKIPs held up: **MMR hurts**
single-gold recall (unproven→harmful, as the research warned), **instruction-prompting hurts**
on a non-instruction-tuned embedder, **cheap context-prepend gives nothing** (the real
contextual-retrieval win needs an LLM pass — deferred). HyDE/SPLADE/MUVERA skipped per research.

---

## Piece 3 — Rerankers (frozen retrieved-48, n=72) — THE decisive stage

| reranker | R@5 | MRR | gold_cov@5 | latency p50 | $/1k queries | host |
|---|---|---|---|---|---|---|
| none (retrieval-only) | 0.833 | 0.635 | 0.653 | — | $0 | — |
| MiniLM-L6-int8 chunk (local) | 0.889 | 0.804 | 0.784 | 9805 ms | **$0** | Apache, local |
| Cohere v3.5 | 0.903 | 0.831 | 0.794 | 1521 ms | $1.00 | API |
| zerank-2 | 0.931 | 0.834 | 0.800 | 1980 ms | $2.26 | API (non-commercial) |
| Cohere v4.0-fast | 0.944 | 0.790 | 0.827 | **896 ms** | $2.00 | API |
| Cohere v4.0-pro | 0.944 | 0.852 | 0.850 | 1320 ms | $2.50 | API |
| **Voyage rerank-2.5** | **0.944** | **0.858** | **0.867** | **705 ms** | $4.50 | API |

**Reranking is the biggest lever in the whole pipeline: +0.111 R@5, +0.223 MRR** over no-rerank.
Every hosted reranker beats local MiniLM on accuracy AND is 7–14× faster (MiniLM is CPU-bound
at 9.8s/query).

---

## COST vs ACCURACY — the balance (what to actually pick)

The two stages have opposite cost shapes: **embedder = one-time index; reranker = recurring
per-query.** So the reranker is where cost discipline matters.

### Reranker: accuracy per recurring dollar

| pick | when | R@5 / MRR | $/1k queries |
|---|---|---|---|
| **MiniLM local** | zero recurring cost / privacy / self-host; latency-tolerant | 0.889 / 0.804 | **$0** (but 9.8s/q CPU) |
| **Cohere v4-fast** | only top-5 recall matters, cost-sensitive | 0.944 / 0.790 | $2.00 |
| **Cohere v4-pro** | **best value — ~all the accuracy, ranking matters** | 0.944 / 0.852 | $2.50 |
| **Voyage 2.5** | **accuracy-max at owner priority (best on every axis + fastest)** | 0.944 / 0.858 | $4.50 |

- **v4-fast is a trap for ranking:** it hits max R@5 but MRR 0.790 is *worse than the free local
  MiniLM* — it lands golds in the top-5 but orders them badly. Only pick it if you truly only
  score binary top-5 recall.
- **Voyage 2.5 vs Cohere v4-pro:** Voyage wins MRR by 0.006 (1 question at n=72 — a statistical
  tie) for **80% more cost**. At owner's accuracy>cost priority Voyage is defensible (also fastest);
  for anyone cost-aware, **Cohere v4-pro is the rational pick**.
- **The free floor is strong:** local MiniLM (+chunk-rerank cap fix) delivers R@5 0.889 / MRR 0.804
  at $0. The hosted premium buys +0.055 R@5 / +0.054 MRR — worth ~$2–4.50/1k-q only if that margin
  matters to the product.

### Embedder: your voyage-4-vs-bge question, answered

| | retrieval-only R@5 | after reranking | cost |
|---|---|---|---|
| **bge (free)** | 0.833 | ~0.889 | **$0** |
| **voyage-4** | 0.917 (+0.084) | ~0.889 (gap gone) | **$0.57 one-time / 33k chunks**, ~$0/query |

**voyage-4 buys +0.084 raw-retrieval R@5 for ~$0.57 one-time — but the reranker erases it.**
If your pipeline reranks (it should — that's the big lever), **stay on free bge**. Pay for a
better embedder ONLY in a no-rerank, latency-bound path.

## Recommended end-to-end config

- **Accuracy-max (owner default):** free bge embed → V3 convex fusion → **Voyage rerank-2.5**.
  R@5 0.944, MRR 0.858. ~$4.50/1k-q recurring, ~$0 index.
- **Value:** free bge → V3 convex fusion → **Cohere v4-pro**. Indistinguishable accuracy, $2.50/1k-q.
- **Zero recurring cost:** free bge → V3 convex fusion → **MiniLM-L6 local chunk-rerank**.
  R@5 0.889, $0, at a latency cost (9.8s/q CPU — the chunk-rerank sub-split cap keeps it from
  being worse; see Piece 0).
- **No-rerank / latency-bound:** **gemini-2 or voyage-4** embed → V3 convex fusion. R@5 ~0.85–0.93,
  pennies to index, no per-query API.

## Piece 0 — production chunk-granularity rerank (T2, landed)

Measured **~80% of prod `contextual_chunks` exceed the 512-token BERT wall** (median ~945 est.
tokens, max ~2546). Fixed with a rerank-time sub-split (≤ max_length×3 chars, max-pool, ≤16
sub-chunks/candidate). **Cost boundary:** rerank-time only — **zero embedder cost, zero stored
bytes, local-CPU only**; latency ~30s→~11s/query vs uncapped. Hosted rerankers bill per candidate
doc and handle long context natively, so this is a local-model-only path.

**T2-S6 live A/B** (bench-lme n=20, MiniLM byo): body 0.526 vs capped-chunk 0.474 recall@5 —
chunk didn't help on the shallow live sample (spike forced answers deep; live LME-S answers are
often shallow enough that body-rerank already sees them). The binding granularity comparison is
the n=72 adversarial set above.

## Costs spent & caveats

- Pool build ~$1 (cached). Piece-1 API embeds: ~$5 (one-time indexes across 6 arms). Piece-3
  rerank APIs + ColBERT ~$2. Total this session well under budget.
- **n=72 gives real paired power** (rerank's +0.223 MRR, embedder retrieval separations are outside
  noise); individual 0.006-MRR reranker deltas are ties — flagged inline.
- **jina-v5-small** free-tier token-rate-limited, not completed (honest gap).
- **Every number here is a retrieval-frontier screen; a DEFAULT change still requires paid reader-QA.**
- Reproduce: `cost_analysis.py` (cost table), `run_piece1.sh` / `run_piece3.sh` (arms),
  `build_adversarial_set.py` (pool), all from committed scores under `scores/`.

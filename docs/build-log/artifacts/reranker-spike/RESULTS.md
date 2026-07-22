# Reranker fixed-pool micro-benchmark results (2026-07-22)

8 fixed LME-S pools (gold + 43–56 real distractors), gold-rank → MRR / R@k.

| Reranker | MRR | R@1 | R@5 | R@10 | lat/query | cost/query | self-host |
|---|---|---|---|---|---|---|---|
| cohere-rerank-v4.0-fast | 0.704 | 0.62 | 0.75 | 0.88 | 419 ms | $0.002 | API |
| zerank-2 | 0.672 | 0.62 | 0.75 | 0.88 | 318 ms | ~$0.0004 | API, non-commercial |
| ms-marco-MiniLM-L6 int8 (local) | 0.660 | 0.62 | 0.62 | 0.75 | 605 ms | free | Apache CPU |
| bge-reranker-base (local) | 0.569 | 0.38 | 0.75 | 0.75 | 6384 ms | free | Apache but slow |
| cohere-rerank-v4.0-pro | 0.560 | 0.50 | 0.62 | 0.75 | 699 ms | $0.0025 | API |
| cohere-rerank-v3.5 | 0.410 | 0.38 | 0.38 | 0.38 | 417 ms | $0.001 | API |

n=8 → 1 question ≈ 0.12 MRR. Directional. Reproduce: `rr_api_score.py` (API arms) +
`cargo test -p memphant-runtime --features fastembed --lib rerank_fixed_pool_accuracy
-- --ignored --nocapture` with `MEMPHANT_RR_POOLS=rr_pools.json` (local arms).
Keys via env (COHERE_API_KEY / ZEROENTROPY_API_KEY); pools built from LME-S seed 3.

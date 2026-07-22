# Reranker fixed-pool micro-benchmark results (2026-07-22)

Every reranker scores the SAME pools (gold at index 0) → gold-rank → MRR/R@k.
Local arms via `rerank_fixed_pool_accuracy` / `rerank_chunked_pool_accuracy`
(memphant-runtime `#[ignore]` tests); API arms via `rr_api_score.py`. Keys via env
(COHERE_API_KEY / ZEROENTROPY_API_KEY), never committed. Pools built from LME-S seed 3;
corpus text NOT committed — regenerate with the builders.

## Test generations
- v1 (`build_pools.py`): 8 pools, whole sessions truncated to 1500 chars — INVALID
  (truncation cut the answer out of gold in 6/8). Ignore its numbers.
- v2 (`build_pools_v2.py`): 12 pools of 48 ~1200-char CHUNKS, gold chunk contains the
  answer 12/12. The valid chunk-level test.
- v3 (`build_pools_v3.py`): 12 pools of 48 FULL sessions (9–22 KB). The long-doc test.
- v4 (`build_pools_v4.py`): v3 docs pre-chunked; rerank all chunks, max-pool to doc.

## v2 (48 chunks) — valid, chunk-level
| Reranker | MRR | R@10 | lat/query | cost | self-host |
|---|---|---|---|---|---|
| zerank-2 | 0.944 | 1.00 | 265 ms | ~$0.0004 | non-commercial |
| bge-base (local) | 0.927 | 1.00 | 4813 ms | free | Apache, slow |
| MiniLM-L6 int8 (local) | 0.926 | 1.00 | 391 ms | free | Apache CPU |
| cohere-v4.0-pro | 0.925 | 1.00 | 646 ms | $0.0025 | API |
| cohere-v4.0-fast | 0.921 | 0.92 | 376 ms | $0.002 | API |
| cohere-v3.5 | 0.866 | 0.92 | 390 ms | $0.001 | API |

## v3 (48 FULL docs) — long-document stress
| Reranker | MRR | R@10 | lat/query | note |
|---|---|---|---|---|
| cohere-v4.0-pro | 1.000 | 1.00 | 1342 ms | long-context |
| cohere-v4.0-fast | 0.958 | 1.00 | 721 ms | long-context |
| cohere-v3.5 | 0.958 | 1.00 | 1164 ms | long-context |
| zerank-2 | 0.872 | 1.00 | 1592 ms | chunks internally |
| MiniLM-L6 int8 (local) | 0.572 | 0.67 | 734 ms | 512-tok wall |
| bge-base (local) | 0.570 | 0.83 | 7077 ms | 512-tok wall |

## v4 (chunk the full docs, max-pool) — recovers local accuracy
| Model | v3 direct MRR | CHUNKED MRR |
|---|---|---|
| MiniLM-L6 int8 (local) | 0.572 | 1.000 |

n=8–12 → directional. Workload is easy (11/12 golds trivially top-1 on valid tests) —
needs a harder adversarial set to separate the top models. Root cause of the local
collapse on full docs: MiniLM/bge are BERT cross-encoders with max_position_embeddings
= 512 (~2000 chars); max_length=2048 is ignored. Fix = rerank contextual_chunks, not
the whole unit body.

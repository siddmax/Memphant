# 2026-07-13 — Cross-rerank partial-corpus diagnostic

## Result

The `modernbert` + `fastembed:bge-reranker-base` screen used candidate limit
32, max length 128, batch size 256, recall `k=10`, and an 8,192-token budget.

| exposed set | n | R@5 | R@10 | reranker p50 | reranker p95 |
|---|---:|---:|---:|---:|---:|
| v1 | 60 | 0.100 | 0.250 | 1,080.0 ms | 1,193.2 ms |
| v2 | 60 | 0.183 | 0.250 | 1,186.5 ms | 1,269.25 ms |
| pooled | 120 | 0.142 | 0.250 | 1,146.5 ms | 1,257.05 ms |

All 120 rows completed with zero reranker failures, degraded responses,
fallbacks, or skipped rows. Artifacts live under
`docs/build-log/artifacts/unified-sota-20260713/task3-rerank/`.

## Claim boundary

This is a development diagnostic, not replacement evidence. It indexed the
3,257-section golden-mining subset (`sha256:e7ceb151...`) rather than the full
4,870-section common corpus (`sha256:82814a4c...`), omitting 1,613 sections.
It also captured only server `cross_rerank_ms`; it did not capture end-to-end
recall wall time, so it cannot establish the replacement ceiling of recall p95
at most 1,500 ms.

The required parity measurement is a fresh baseline and candidate rerun on all
4,870 pinned sections through the current common-corpus contract. Both arms
must use identical corpus revision, goldens, `k`, packing budget, binaries, and
reader lattice. Promotion uses the newly recorded end-to-end recall p95 (full
recall POST plus trace GET), never reranker-only latency.

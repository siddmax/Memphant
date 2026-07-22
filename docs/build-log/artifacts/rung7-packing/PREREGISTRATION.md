# Rung-7 packing lever — preregistration (§6)

Registered BEFORE reading either arm's paired result.

## Hypothesis
The 64 in-pool-unpacked LME-S dev Fast-misses are 100 % `Budget` drops
(diagnosis: `2026-07-21-rung7-packing-diagnosis.md`). Reducing per-item budget
pressure lets top-ranked gold sessions (median fused_rank 2) into the packed
top-k, raising recall@k.

## Arms (each paired vs the budget-8192 baseline `dev-fast-retrieval.json`)
1. **budget-16384** — global budget doubled (control; zero code). Prior overall
   QA evidence is ns-harmful, but retrieval@k on this subset is unmeasured.
2. **pack-render-cap=1200** — per-item chunk-render cap so no single big body
   hogs the 8192 budget (the surgical, accuracy-and-cost-positive lever).

Config for both: `--sample 178 --seed 20260713 --k 10 --disable rerank
--pool 64 --embed-model small`, session granularity, runtime chunks on. Run on a
run-owned scratch Postgres. Dataset sha256 `e4667bed…d3a475`.

## Pass predicate (retrieval, FREE)
An arm PASSES if the bootstrap 95 % CI on **Δrecall@10** (the primary; Δrecall@5
secondary) **excludes zero AND is positive** (`ci_excludes_zero == true` and
`mean > 0`), from the built-in seeded bootstrap in `paired_vs_baseline`.

## Promotion rule
Promotion-to-default requires the retrieval win to **hold on a second seed**
(two-seed rule, binding). A retrieval win does NOT by itself flip any reader
default — reader-QA confirmation is a separate, gated (paid) step, and prior
overall-QA evidence against budget-16384 means the render-cap arm (which lowers
reader tokens) is the preferred promotion candidate if both win on retrieval.

## Kill-switch
An arm whose Δrecall@10 CI includes zero is REJECTED; its paired report is kept
as the negative artifact. If NEITHER arm moves the 64, the bottleneck is deeper
than packing budget (redirect to the reader-utilization layer, bucket C).

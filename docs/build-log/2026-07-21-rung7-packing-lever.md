# Rung-7 packing lever — the per-item render cap recovers budget-starved gold

## Result (seed 20260713, dev, retrieval-only, FREE)

The diagnosis (`2026-07-21-rung7-packing-diagnosis.md`) showed the 64
in-pool-unpacked LME-S dev Fast-misses are **100 % `Budget` drops**: top-ranked
gold sessions (median fused_rank 2) are budget-dropped because a few large
whole-session bodies (~3230 tok median) exhaust the 8192-token pack budget. Two
levers, each paired vs the budget-8192 baseline (`dev-fast-retrieval.json`,
r@10 = 0.6145), 166 scored questions:

| arm | r@10 | Δr@10 vs baseline | 95 % CI | reader tokens |
|---|---|---|---|---|
| baseline (budget 8192) | 0.6145 | — | — | 8192 |
| budget 16384 | 0.7651 | **+0.1506** | [+0.0964, +0.2108] | doubled |
| **pack-render-cap 1200** | **0.8494** | **+0.2349** | **[+0.1687, +0.2952]** | same 8192, tighter per item |

(r@5 == r@10 in every arm — no gold sits at ranks 6–10 in this set.)

**The per-item render cap wins bigger than doubling the budget (+0.235 vs +0.151)
while keeping the same 8192 budget.** Both CIs exclude zero; the cap arm passes
the preregistered predicate (`PREREGISTRATION.md`) by the widest margin.

Per stratum (r@10, baseline → cap), the cap improves **every** stratum:

| stratum | base | cap | n |
|---|---|---|---|
| temporal-reasoning | 0.543 | 0.891 | 46 |
| multi-session | 0.585 | 0.805 | 41 |
| single-session-user | 0.455 | 0.818 | 22 |
| knowledge-update | 0.889 | 1.000 | 27 |
| single-session-assistant | 0.895 | 0.947 | 19 |
| single-session-preference | 0.182 | 0.364 | 11 |

## Why the cap beats the bigger budget

The pathology is per-item cost, not total budget. `packed_render` set each
item's chunk-render budget to `whole_body.min(request_budget)` (`lib.rs:8122`),
so a chunk-matched session refilled (via sibling expansion) to nearly its whole
~4600-token self and hogged the 8192 budget — only ~4 items fit. Doubling the
budget to 16384 just lets ~2 more whole sessions in (+0.151). Capping each item
at 1200 tokens renders matched-chunk-plus-neighbours compactly, so ~6–7 gold
sessions fit in the SAME 8192 budget (+0.235). More distinct gold sessions in
the packed top-k = higher recall@k. The cap is strictly better than the bigger
budget on both accuracy (larger Δ) and cost (unchanged budget, tighter reader
context — the opposite of the ns-harmful 16384-on-QA finding in STATUS).

## Lever (default OFF)

`PackLevers.pack_render_cap: Option<usize>` (commit `1918ce5e`). `Some(cap)`
bounds each packed item's chunk-render budget at `cap`; `None` =
`whole_body.min(request_budget)`, byte-identical to before (the pack-lever
contract — 84/84 core lib tests unchanged with it off). Threaded
core → `MemoryService::with_pack_render_cap` → bench-lme `--pack-render-cap <n>`.
TDD: `pack_render_cap_reclaims_budget_and_admits_second_item`.

## Scope and honesty

- This is a **retrieval recall@k** win, measured FREE on the retrieval trace. It
  is NOT a reader-QA result. Prior overall-QA evidence against a bigger budget
  (STATUS: 16384 ns-harmful, −0.030) does not transfer to the cap (which lowers,
  not raises, reader tokens) — but a reader-QA confirmation is a separate, gated
  (paid) step before any reader/default flip.
- `cap = 1200` is the single value tested; it won decisively, so no sweep was
  run (a finer sweep is a follow-up, not a blocker). The lever ships OFF; turning
  it on by default is gated on the second-seed hold (below) and, for the reader
  lane, on a paid QA confirmation.
- Reconciles with [[memphant-packing-gate-verdict]] (that verdict is the
  output-full Rerank branch; this is the Budget path) — no conflict.

## Promotion status

- Seed 20260713: **PASS** (Δr@10 +0.2349, CI excludes zero). 
- Seed 20260710 (two-seed rule): _[pending — second-seed baseline + cap arm
  running; fill on completion]_.

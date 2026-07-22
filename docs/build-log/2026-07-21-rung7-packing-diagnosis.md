# Rung-7 — in-pool-unpacked drop-cause diagnosis (FREE)

## Verdict — the 64 in-pool-unpacked dev misses are 100 % BUDGET drops

The A1 classification (`2026-07-21-a1-fast-miss-classification.md`) split the 166
scored LME-S **dev** Fast-misses into buckets and found 64 (38.6 %)
`in_pool_unpacked`: gold is in the candidate pool but never reaches the packed
top-k. This diagnosis answers **why** for all 64, from the retrieval trace alone
(no reader, no model spend).

**Every one of the 64 is a `Budget` drop.** Not ordering, not subject-dedup, not
scan depth:

| signal | value |
|---|---|
| `gold_drop_reason` | **budget: 64 / 64** (zero Duplicate, zero Rerank, zero never-reached) |
| gold `fused_rank` | median **2**, min 1, max 35; **48/64 ≤ 5**, **59/64 ≤ 10** |
| `packed_size` | median **4**, min 2, max 8 — **64/64 below k=10** |

Artifacts: `docs/build-log/artifacts/rung7-packing/dev-drop-cause.jsonl`
(per-question), `dev-drop-cause-summary.json`, `dev-fast-retrieval.json`
(r@10 = 0.6145 — reproduces the A1 pinned value exactly).

## Mechanism (probe-verified, not inferred)

An env-gated probe (`RUNG7_PROBE`, reverted after) logged the greedy-fill order,
per-item token cost, and every budget-drop on a seeded 30-q sample (16/16
in-pool-unpacked = budget, gold median rank ~2, same as the full run). The order
IS pure `fused_score desc`, as expected. The pathology is **greedy first-fit ×
large bodies × tight budget**:

- LME-S session bodies are large: median **≈ 3230** conservative-est tokens,
  p90 ≈ 5197 (measured on the raw dev haystacks).
- The pack admits the top fused item (~4500 charged tokens even after
  chunk-render — see below), then the 8192-token budget has only ~3600 left.
- Every subsequent large-body candidate — **including gold at fused rank 1–3** —
  fails `acc.token_estimate + unit_tokens ≤ budget` and is `Budget`-dropped.
  Only a couple of unusually *small* lower-ranked items squeeze into the crumbs;
  the budget saturates at ~8180/8192 after 2–7 items. That is the whole
  `packed_size` median of 4.

Concretely (probe, question `1c549ce4`): order[0] admitted at charged 4556;
order[1] (4916 tok) and order[2] (5552 tok) both budget-dropped with
`packed_so_far=1`; the budget then saturated at 8179 and dropped 38 of 45
candidates.

### Why chunk-render did not save it

Runtime chunks are ON (rung 4), yet order[0]'s 4638-token body was charged 4556
— barely reduced. Root cause: `packed_render` sets the per-item render budget to
`whole_body_tokens.min(request_budget_tokens)` (`lib.rs:8122`). For a 4638-tok
body under an 8192 request budget that is **4638** — the whole body — so
`select_chunk_mask`'s sibling-expansion (Phase B) refills the item back to nearly
the entire session. Chunk-rendering compresses nothing when the per-item budget
is the whole body. This is the surgical lever candidate (a per-item render cap),
distinct from simply raising the global budget.

## Reconciliation with prior verdicts

- **[[memphant-packing-gate-verdict]]** (the coarse `rank_based_ordering_active`
  gate measured-permanent, 276/276): that measurement was the **output-full
  Rerank replacement** branch on the cross-rerank arm. It does **not** cover the
  `Budget` drop path, which fires earlier and is the sole cause here. No conflict
  — different mechanism, orthogonal evidence.
- **STATUS budget ablation** (doubling to 16384 is ns-harmful on **QA overall**,
  −0.030, n=100): that is the reader/QA axis on the full set. This diagnosis and
  the paired arm that follows measure **retrieval recall@k on exactly the 64
  in-pool-unpacked dev questions** — a different axis and a different subset,
  which the plan's decision tree explicitly allows to reopen budget. Any budget
  win here is a *retrieval* win; a reader-QA confirmation stays a separate, gated
  step (prior overall-QA evidence is against it, so promotion-to-default requires
  its own reader evidence).

## Method (reuse, FREE)

`bench-lme --emit-trace-classification` extended (commit `b334f30c`) to record,
per question, the best-ranked gold pool unit's `fused_rank`/`fused_score`, its
pack `dropped_items` reason, and `k` — via the pure, unit-tested
`classify_gold_drop_cause`. Run on a run-owned scratch Postgres (P0.1,
`with_scratch_db.sh`), product Fast config `--sample 178 --seed 20260713 --k 10
--disable rerank --budget-tokens 8192 --pool 64 --embed-model small`, dataset
sha256 `e4667bed…d3a475` (the pinned dev split). Zero model spend.

# A1 — Fast-miss trace classification (result)

**Plan:** `docs/superpowers/plans/2026-07-21-tri-domain-sota-plan.md` §3.A1 (verdict
binds the whole benchmark lane) + §8 kill-switches. **Cost:** FREE, zero model
spend. **Design:** `docs/superpowers/specs/2026-07-21-a1-fast-miss-classification-design.md`.

## Verdict — the depth lane is deferred (kill-switch fires at its maximum)

Classifying the 178-question LME-S **dev** split (166 scored, 12 `_abs` set aside)
by where the gold evidence sits relative to the Fast pipeline, from the retrieval
trace alone:

| Bucket | n | share | lever |
|---|---|---|---|
| **A — absent-from-pool** (recall never surfaced gold) | **0** | **0.0 %** | recall depth — Deep-fixable |
| **B — in-pool-but-unpacked** (gold in pool, packed out) | 64 | 38.6 % | packing/ordering (rung 7) |
| **C — in-top-k** (retrieval hit) | 102 | 61.4 % | reader utilization |

**Present-but-unpacked-or-unread (B+C) = 166/166 = 100 %** — far past the binding
≥70 % threshold. **Zero dev misses are recall-depth-bound.** Deep (a recall-depth
lever) cannot fix a single dev Fast-miss.

**Binding consequence (plan §3.A1, §8):** Deep drops to **diagnostic status**; the
**packing/ordering lever (rung 7) becomes the center of gravity**; **D1 (LME-V2)
and D3 (LME-S full-500) are DEFERRED — not run in parallel** — because they chase
recall depth when the bottleneck is utilization.

## Why (mechanism)

Pool size median **47** (min 41, max 61) ≈ every ingested session, because at
`recall_pool_depth=64` the exact/lexical/vector/temporal channels surface all
sessions into the pool. So gold is essentially always in the pool → absent=0 is
structural, not a fluke. But the packed set is median **4 items** (max 9) under
the 8192-token budget: **packing/ordering is the throughput limit.** 64 questions
have gold in the 47-item pool that never reaches the packed top-k. This is the
STATUS oracle-gap prior (reader 0.584 vs oracle 0.916, +0.331 unclosed) made
concrete at the retrieval layer.

## Method (reuse, FREE)

`memphant-eval bench-lme --emit-trace-classification` on a run-owned scratch
Postgres (P0.1), product Fast config (`--sample 178 --seed 20260713 --k 10
--disable rerank --budget-tokens 8192 --pool 64 --embed-model small`, session
granularity, runtime chunks on). Per question: fetch the full trace
(`store.trace_by_id`), map every **pool** candidate unit → episode via
`fetch_units_by_ids` + `source_episode_id` (NOT trace citations, which cover only
packed units and would undercount bucket B), then session-map against
`answer_session_ids`. Pure `classify_question` + `FastMissBucket` unit-tested.

## Honesty header + provenance

Same dataset sha256 `e4667bed29565884b827ca0a75fbbec8d15f772c96011bb058ea5e2863d3a475`
as the pinned 2026-07-13 dev report. This run's r@10 is **0.614**, below that
report's **0.777**, because this run carries two bench-lme ingestion fixes that
the older report predates and that A1's execution surfaced:

1. **`observed_at` RFC3339** — the 2026-07-09 context-binding cutover
   (`0af44ad4`) wired `observed_at` to `"{}T00:00:00Z".format(haystack_dates)`,
   but upstream dates are `YYYY/MM/DD (Day) HH:MM`, so `retain` rejected every
   row. The pinned report ran on the pre-cutover retain (no `observed_at` field).
   Fixed via `normalize_haystack_date` (preserves real HH:MM).
2. **Duplicate-session-id keying** — LME haystacks can repeat a session_id at two
   timestamps (dev `001be529` has `sharegpt_SYbLHTK_0` twice); the retain
   idempotency key `lme:{session_id}:{turn}` collided. Fixed by keying on the
   haystack position.

**The absent=0 verdict is robust to the recall-rate difference:** pool depth (64)
≥ per-question session count (≤61), so gold enters the pool regardless of the
temporal/dedup differences that move the *packed* recall rate. Even at 0.777, the
misses are pool-present.

**Artifacts:** `docs/build-log/artifacts/a1-fast-miss-classification/`
(`dev-fast-miss-classification.jsonl` per-question buckets;
`dev-fast-retrieval.json` full report + honesty header).

# A1 — Fast-miss trace classification (design)

**Date:** 2026-07-21 · **Plan:** `docs/superpowers/plans/2026-07-21-tri-domain-sota-plan.md` §3.A1 · **Cost:** FREE (no reader/model spend)

## Goal

Classify every scored LME-S **dev** question (178 total; 166 scored, 12 `_abs`
excluded from the miss denominator) by where its gold evidence sits relative to
the Fast recall pipeline, to decide whether Deep (a recall-**depth** lever) can
move the bottleneck. The verdict **binds the whole benchmark lane** (kill-switch,
plan §8).

## Three buckets (all trace-derived, zero model spend)

Per scored question, using the Fast `RetrievalTrace`:

| Bucket | Predicate | Lever |
|---|---|---|
| **A absent-from-pool** | no candidate in `trace.candidates` maps to a gold session | recall **depth** — Deep *can* help |
| **B in-pool-but-unpacked** | a gold candidate is in `trace.candidates` but its unit is not in the packed top-k | **packing/ordering** (rung 7) — Deep cannot help |
| **C in-top-k (hit)** | a gold-bearing item is in the packed top-k (`first_answer_rank ≤ k`) | **reader utilization** — Deep cannot help |

A question is a **retrieval-miss** iff it is not bucket C. The plan's "~74
Fast-miss" is the *reader*-miss count (178×(1−0.584)); the *retrieval*-miss count
on the pinned dev Fast run is ~37 (r@10 0.777 over 166 scored). A1 measures the
retrieval layer directly and reports the reader-utilization layer (bucket C) as
the third, largest "present-but-not-used" share, faithful to the oracle-gap prior
(reader 0.584 vs oracle 0.916, +0.331 unclosed).

## Binding verdict

`present-but-unpacked-or-unread = B + C`. Per plan §8: **if ≥70 % of scored
questions are NOT bucket A** (i.e. Deep's depth cannot be the fix), Deep drops to
diagnostic, packing (rung 7) becomes center of gravity, and D1/D3 defer. Report
the exact A/B/C split with per-question classification either way.

## Mechanism (reuse, do not rebuild)

Reuse the `bench_lme.rs` ingestion + Fast recall pipeline verbatim (session
ingestion, worker drain, `recall_internal` mode=Fast) on a run-owned scratch
Postgres. After each recall:

1. fetch the full trace via `store.trace_by_id(context, response.trace_id)` —
   gives `candidates` (**full pool**, not just packed) + `context_items` (packed).
   NB `RecallResponse.candidate_whitelist` is the *packed* set (`items` unit-ids),
   NOT the pool — must use `trace.candidates`.
2. `store.fetch_units_by_ids(context, pool_unit_ids)` → each unit's
   `source_episode_id` (the candidate trace carries only `unit_id`; the unit
   carries the episode). This maps every **pool** unit → episode, so bucket B
   (gold in pool but unpacked, hence uncited) is not undercounted.
3. episode → session via the run's `episode_sessions` map (same map bench-lme
   already builds for scoring); session ∈ `answer_session_ids` ⇒ gold.

Emit one JSONL row per question: `{question_id, question_type, bucket, gold_in_pool,
gold_in_topk, first_answer_rank, pool_size, packed_size}` + an aggregate A/B/C
split and the verdict boolean.

## Where the code lives (KISS)

A new `bench-lme --emit-trace-classification <path>` flag: inside the existing
recall loop (`response.trace_id` and `episode_sessions` already in scope), when
set, fetch the trace + units and write the classification row. Mirrors the
existing `--emit-qa` pattern — ~40 lines reusing in-scope state, no new harness,
no reader, no trace-schema change.

## Test (TDD)

A pure classifier fn `classify_question(pool_sessions, topk_sessions,
answer_session_ids) -> Bucket` with a unit test covering all three buckets +
the empty-pool edge — mirrors `bench_lme::score_question`'s test style.

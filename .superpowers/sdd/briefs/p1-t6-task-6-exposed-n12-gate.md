## Task 6: Build and run the exposed n=12 LongMemEval-V2 feasibility gate

**Start only after:** Task 5 is independently approved and the full packaged repository gate is green.

**Purpose:** Decide whether explicit Deep recall is accurate enough, fast enough, and cheap enough to justify a larger exposed confirmation. This is a feasibility screen, not a leaderboard, SOTA, product-default, or ledger-promotion claim.

### Frozen upstream and operating point

- LongMemEval-V2 code commit: `be15ea6e995462f3391c1a610892df3f67dfa7bd`
- Dataset revision: `f152293e235517d504809563c833d7190b8c713b`
- Dataset questions SHA-256: `0a3ae5ebea938c24d7800e1e0b0828e08ae1646f939a53853b2b8cdc08e292b7`
- Medium haystack SHA-256: `4756d5126347f0d18f045bb6c47b08cb3b23e9db24386cc48a9b2879e7969b59`
- Tier: Medium only. Its pinned haystacks contain 387-500 trajectories (mean 498.08), so this screen measures the hard long-history use case for which explicit Deep exists.
- Reader/judge: the locked official protocol (`Qwen/Qwen3.5-9B` fixed reader and official `gpt-5.2` judge) with identical prompt/endpoint/config hashes across arms.
- Memory context limit: official 32,768 tokens, with zero accepted truncations.
- Deep limits for the selected Sonnet treatment: 120,000 ms wall, 24 completed model responses, 96,000 cumulative input tokens, 300,000 micro-USD liability, 4,096 maximum completion tokens.
- Deep provider routing: exact model, Azure only, ZDR, data-collection deny, require-parameters, streaming cancellation, no cross-model fallback.

### Answer-blind deterministic case selection

The selector may parse only `id`, `domain`, and `question_type`. It must reject access to `question`, `answer`, `eval_function`, `image`, gold/reference fields, trajectories, or prior run outputs while selecting.

Map question types to five abilities:

- `static-environment` -> `static_state`
- `dynamic-environment` -> `dynamic_state`
- `procedure` -> `workflow_knowledge`
- `errors-gotchas` -> `environment_gotchas`
- every `*-abs` type -> `premise_awareness`

Derive the seed as SHA-256 of the NUL-separated dataset revision, reviewed planning base commit, and selector version:

```text
f152293e235517d504809563c833d7190b8c713b\0f2f9d772b5dabbe0d93202d6d5480069c209bbcb\0p1-t6-feasibility-v1
```

Expected seed SHA-256: `1d5ce2760cf354b45c102bab25c3a31bbff6f96f8a36425480da54473348e4dd`.

1. For each domain x ability stratum, select the row minimizing SHA-256 of `seed\0base\0domain\0ability\0id`, breaking a hash collision by ID. This produces ten cases.
2. From the remaining rows, enumerate one web plus one enterprise case with different abilities. Select the pair minimizing SHA-256 of `seed\0extra_pair\0web_id\0enterprise_id`, breaking a collision by the two IDs.
3. Sort the final rows by domain, ability, ID. Require six cases per domain and ability counts differing by at most one.

The selector must reproduce exactly:

| Domain | Ability | Question type | ID |
|---|---|---|---|
| enterprise | dynamic_state | dynamic-environment | `658fa827` |
| enterprise | environment_gotchas | errors-gotchas | `8e21c6e5` |
| enterprise | premise_awareness | static-environment-abs | `6fdda2fc` |
| enterprise | static_state | static-environment | `19367bc7` |
| enterprise | static_state | static-environment | `aedd338d` |
| enterprise | workflow_knowledge | procedure | `52dd33bb` |
| web | dynamic_state | dynamic-environment | `dae9f7e9` |
| web | environment_gotchas | errors-gotchas | `f2b221fd` |
| web | premise_awareness | static-environment-abs | `21f3228c` |
| web | premise_awareness | static-environment-abs | `b05cf470` |
| web | static_state | static-environment | `2c45ecbb` |
| web | workflow_knowledge | procedure | `86fa86eb` |

Expected canonical selection SHA-256: `ffe151038e3dc54c8132b58a2d39575db9ee37d0ead8f873afda67a6e35c2bea`.

### Arms and run order

Build one manifest-driven materializer/runner; do not loop the current single-question script by hand.

- Materialize every selected question once from the pinned official functions, with exact Fast/Deep corpus-pairing hashes and an assertion that no gold/evaluator field entered memory.
- Build packaged server/CLI binaries once and fingerprint them. Every question/arm uses a fresh scratch database and the same binaries, corpus, reader, judge, prompt, and memory-context budget.
- Run one Fast control per question.
- Run exactly one selected Deep treatment per question: `anthropic/claude-sonnet-5` on Azure, paired with one Fast control. Luna and Sol remain inactive researched metadata, not executable arms.
- Randomize neither case nor arm order after observing output. A deterministic rotation derived from the selection hash may interleave arms to reduce time-of-day bias, but the rotation must be generated and archived before the first score.
- Archive failures and partials; never rerun a completed billable row to improve a result. Infrastructure-invalid rows may be retried only under a predeclared rule that proves no generation was accepted/billed and preserves the original attempt.

### Primary metric and failure treatment

- Primary metric: pinned official per-question binary `score`, aggregated as the paired mean Deep-minus-Fast delta over all 12 cases.
- Also report wins, ties, and losses versus Fast, by-domain and by-ability scores, but do not optimize or gate on subgroup results at n=12.
- Any Deep unavailable/provider-error/invalid-output/capped/partial response, missing trace, unaccounted generation, unsettled liability, reader context truncation, security/write invariant failure, or missing pair scores `0` for the Sonnet treatment and makes the paired gate operationally infeasible. Preserve the row.
- The Fast row is never rerun; it pairs only with the selected Sonnet treatment.

### Pre-registered UX/cost feasibility predicates

The selected Sonnet treatment is feasible only if all predicates hold:

1. All 12 pairs and all proof hashes are present; no persistent recall-time writes, auth/policy regressions, cap/infra failures, unsettled liability, or leaked gold fields.
2. Paired official mean score delta is strictly positive and wins exceed losses.
3. Deep memory-query p50 is at most 45 seconds, p95/max is at most 90 seconds, and no request reaches the 120-second hard cap.
4. Mean settled per-query Deep cost is at most USD 0.10, p95/max at most USD 0.20, and no row approaches the USD 0.30 hard cap through hidden/unsettled liability.
5. Official memory context is never truncated.

These are explicit-Deep thresholds, not Fast-default targets. A candidate that is accurate but slower or costlier fails the product feasibility screen.

### Candidate decision

- If Sonnet is infeasible, preserve the negative immutable artifact, stop T6, and do not update the live ledger.
- Luna or Sol may run only after a fresh answer-blind amendment and a new output root; they are not automatic fallback or ranking arms.
- Treat a one-question score difference at n=12 as exploratory, not statistical superiority.
- Before any larger run, write a separate immutable n=100-300 confirmation manifest with a fresh answer-blind selection, non-inferiority margin, paired confidence procedure, fixed/adaptive stopping rule, and total spend ceiling. Do not derive those choices from hidden answers or subgroup outcomes.

### Required artifacts and verification

Archive under `docs/build-log/artifacts/p1-t6/`:

- selection source/manifest and selector tests;
- pinned input hashes and per-question pairing proofs;
- run-order manifest frozen before scoring;
- commit/binary/provider/model/prompt/config/workspace hashes;
- every Fast and Deep public response, trace, citation IDs, status/stop reason, generation IDs, settled/unsettled accounting, latency, and cost;
- official per-question rows and aggregate proof with explicit predicate results;
- scratch database identities and zero-write/security proof.

Run the full `AGENTS.md` repository gate at the measured commit before any conclusion. An n=12 pass authorizes only the larger exposed confirmation; it does not authorize a SOTA statement, official/sealed submission, default Deep routing, ledger checkbox, merge, push, or P3 spend.

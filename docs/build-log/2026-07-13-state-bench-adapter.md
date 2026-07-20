# STATE-Bench v0.8.0 acquisition and native-score adapter

Date: 2026-07-13

## Decision

STATE-Bench is publicly runnable and useful as the procedural-memory lane of the
campaign. It is not sufficient by itself to justify replacing MemPhant's temporal
memory, RAG, or codebase retrieval systems. It measures whether reusable learnings
improve tool-using agents on synthetic enterprise workflows; it does not directly
measure temporal fact resolution, knowledge-base citation quality, or repository
code retrieval.

Use it as one decision-grade gate in the benchmark portfolio. Require gains on the
official held-out Agent Learning Track without regressions in the separate temporal,
RAG/KB, and code-agent lanes before making a unified-system replacement call.

## Official release contract

- Repository: <https://github.com/microsoft/STATE-Bench>
- Release: `v0.8.0`, published 2026-06-25
- Commit: `e2c8d7af51ef48fbbea51bb2ce1fb859af36b423`
- License: MIT
- Track: Agent Learning Track
- Public learning corpus: 100 trajectories per domain, 300 total
- Held-out evaluation: 50 tasks per domain, 150 total
- Official runs: five per task
- Retrieval contract: read-only `retrieve_learnings`, top-k fixed at 3
- Locked simulator and judge: GPT-5.4; judge reasoning effort `high`
- Native headline metrics: task completion pass@1, task completion pass^5,
  mean UX score, and mean reported cost per task

Primary sources:

- <https://github.com/microsoft/STATE-Bench/tree/v0.8.0>
- <https://github.com/microsoft/STATE-Bench/blob/v0.8.0/docs/AGENT_LEARNING_TRACK.md>
- <https://github.com/microsoft/STATE-Bench/blob/v0.8.0/state_bench/configs/eval_protocols/gpt54.json>
- <https://opensource.microsoft.com/blog/2026/05/19/introducing-state-bench-a-benchmark-for-ai-agent-memory/>

## Adapter boundary

`benchmarks/manifests/state_bench.lock.json` pins the official release, native
metric sources, protocol, counts, and an aggregate content digest over all 450 task
definitions, 450 task environments, three split manifests, and 300 train
trajectories.

`scripts/run_state_bench.py` deliberately does not reimplement a metric. It:

1. acquires or accepts the pinned checkout;
2. fails on revision, scorer, protocol, task, environment, split, or train-corpus
   drift;
3. requires every one of the five run directories and every held-out trajectory;
4. rejects task errors, unscored completion, invalid UX scores, and wrong protocol
   stamps;
5. invokes `state_bench.scripts.compute_metrics` unchanged for every domain; and
6. verifies the official metric and per-task outputs before writing a proof file.

The extra completeness check is necessary because the upstream aggregator warns and
skips a missing run directory, which can otherwise produce a plausible metric over
fewer than the official five runs. The wrapper only makes that official-run
requirement fail closed; it does not change scoring.

## Verification

No provider or paid model calls were made.

```text
python3 scripts/run_state_bench.py \
  --official-repo /tmp/memphant-task6-research/state-bench \
  --dry-run

{"benchmark":"STATE-Bench","domains":{"customer_support":50,"shopping_assistant":50,"travel":50},"protocol_id":"state_bench_v0.8.0_gpt54","revision":"e2c8d7af51ef48fbbea51bb2ce1fb859af36b423","status":"audit-ok-no-model-calls"}

python3 -m pytest \
  tests/test_state_bench_contract.py \
  tests/test_temporal_benchmark_contract.py -q

14 passed
```

A synthetic 5-run x 50-test-task x 3-domain smoke corpus was then passed through
the actual upstream metric entrypoint. It produced 50 per-task artifacts per domain,
`num_runs=5`, pass@1 `1.0`, pass^5 `1.0`, and UX `5.0`, proving the native adapter
end to end without launching an agent, simulator, or judge.

## Remaining paid evaluation

The acquisition and scoring path is ready. A real official result still requires
the benchmark's locked GPT-5.4 simulator and judge plus an agent under test across
150 held-out tasks and five runs. That spend should only begin after the MemPhant
learning adapter is frozen and a no-oracle audit confirms learning extraction reads
only the 300 public train trajectories.

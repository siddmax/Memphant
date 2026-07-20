# STATE-Bench MemPhant arm contract

Date: 2026-07-13

## Outcome

The MemPhant Agent Learning Track arm is specified and runnable without any
benchmark metric reimplementation. No agent, simulator, judge, or other paid model
call was launched.

The official v0.8.0 training corpus maps to 2,842 raw tool-attempt episodes:

| Domain | Attempts | Successful executions | Explicit tool failures | Held-out tasks/run |
| --- | ---: | ---: | ---: | ---: |
| Travel | 1,454 | 1,434 | 20 | 50 |
| Customer support | 716 | 712 | 4 | 50 |
| Shopping assistant | 672 | 665 | 7 | 50 |
| Total | 2,842 | 2,811 | 31 | 150 |

The attempt classification is deliberately mechanical and source-faithful:

- a non-empty `result.error` is `tool_attempt.failure` and receives a `failure`
  mark;
- every other completed tool call is `tool_attempt.success` and receives a
  `success` mark.

A business result such as `status: rejected` remains a successful tool execution.
This avoids teaching the system that a correct policy denial is an infrastructure
or tool-use failure.

## Data boundary

The builder reads only:

- `datasets/train_task_trajectories/<domain>/*.json`, whitelisting each file's
  `conversation` field; and
- `state_bench/domains/<domain>/splits/train_test.json`, using train IDs to prove
  corpus coverage and test IDs only for non-content coverage hashes.

It never opens held-out task definitions or task environments. It also ignores all
non-conversation fields on train fixtures. Test requirements, state assertions,
answers, judge results, and scorer fields therefore cannot enter retained evidence,
training recall queries, the inference agent, or runner commands.

## Packaged runtime path

`scripts/build_state_bench_memphant_arm.py` uses the packaged server's shared
`MemoryService` verbs:

1. provision one isolated tenant/scope/actor per domain;
2. retain one raw episode per tool attempt with its original user context,
   assistant context, tool name, arguments, and result;
3. synchronously reflect each domain through `/v1/reflect`;
4. recall top-3 relevant evidence for the attempt intent without including its
   result in the query; and
5. record the preserved success/failure label through `/v1/mark` against the
   exact recall trace and used unit IDs.

The private checkpoint is atomically replaced after every mutation and mode `0600`.
It stores the tenant credentials required for same-database resume, plus every
episode ID and mark trace. Final proof is refused unless every expected attempt has
an episode, a reflect result, an accepted mark with the correct outcome, and a
hashed used-ID set.

The final public proof excludes API keys. It freezes hashes for the packaged
binaries, OpenAPI artifact, builder, inference agent, shared runtime harness,
official agent hook/base, source corpus, queries, protocol, and runner inputs.

## Held-out injection contract

`benchmarks/state_bench/memphant_memory_agent.py` subclasses the official
`StateBenchAgent` and implements only `retrieve_learnings(query, top_k=3)`.
It rejects any other top-k, performs read-only exhaustive MemPhant recall, rejects
degraded or malformed output, and returns at most three strings through
STATE-Bench's official retrieval tool. It does not read `task_summary`, state
requirements, task requirements, task files, or task environments.

The frozen official commands use:

- protocol `state_bench_v0.8.0_gpt54`;
- split `test`;
- `MemphantMemoryAgent`;
- five runs;
- retrieval top-k 3;
- canonical answer-model label `openai/gpt-5.6-sol-pro-20260709`; and
- high answer-model reasoning.

This schedules exactly 750 held-out agent jobs: 3 domains x 50 tasks x 5 runs.
Those jobs were not launched.

## Verification

```text
python3 scripts/build_state_bench_memphant_arm.py \
  --official-repo /tmp/memphant-task6-research/state-bench \
  --fixture tests/fixtures/state_bench_learning_small.json \
  --dry-run --out /tmp/state-bench-fixture.json

fixture: 2 attempts, one success, one failure, one test ID

python3 scripts/build_state_bench_memphant_arm.py \
  --official-repo /tmp/memphant-task6-research/state-bench \
  --dry-run --out /tmp/state-bench-official.json

official: 2,842 attempts, 2,811 success, 31 failure, 150 held-out IDs

python3 -m pytest \
  tests/test_state_bench_memphant_arm.py \
  tests/test_state_bench_contract.py \
  tests/test_temporal_benchmark_contract.py -q

21 passed
```

The full evidence build remains intentionally unexecuted. It needs a durable,
migrated Postgres database because the three domain memories must remain available
while the later official 750-job run uses the read-only retrieval adapter.

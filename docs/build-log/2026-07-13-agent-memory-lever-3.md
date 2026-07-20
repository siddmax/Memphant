# Agent-memory lever 3: sibling evidence expansion

## Error decomposition

The frozen 178-question development baseline had 74 misses. Oracle and
retrieval controls split them into 31 retrieval-hit/oracle-hit utilization
misses, 30 retrieval-miss/oracle-hit misses, and 13 oracle misses. No-memory
answered only one MemPhant miss. Within the 31 utilization misses, 26 packed
an official answer session but not a chunk containing the literal answer:
multi-session 10, temporal-reasoning 9, knowledge-update 4, and
single-session-assistant 3. The pinned machine-readable decomposition is
`docs/build-log/artifacts/unified-sota-20260713/task2-error-decomposition.json`.

This made the existing `--sibling-gather` packing seam the single selected
lever. The predeclared development bar was at least three net QA hits
(107/178), positive net conversion inside the 26-question target bucket, and
zero gold-abstention regressions.

## Result

Rejected. On a fresh migrated scratch Postgres database, sibling gathering
changed all 178 evidence bodies and all 26 target bodies, while retrieval
session inclusion and Recall@5/Recall@10 remained exactly 0.777108. It added
the literal answer to none of the 26 target packs.

The current-evaluator baseline was replayed from cache at 104/178. The frozen
Sol Pro high v1 candidate scored 103/178 (0.578652), a paired delta of
-0.005618 with 95% CI [-0.022472, 0.011236]. There was one miss-to-hit and two
hit-to-miss flips, zero target-bucket conversions, and one gold-abstention
regression (`80ec1f4f_abs`). Reader, parser, and judge errors were zero.

The lever fails every promotion condition. No new runtime route was added or
therefore needed deletion; the pre-existing sibling-gather flag remains
default-off. Confirmation data was not evaluated or inspected.

## Artifacts and fingerprints

- Packaged eval binary: `221d4399b81ded522c5b68a8e001ccf2afd3f39de3c0c4fbe5c919a1b443b5b0`
- Retrieval report: `docs/build-log/artifacts/unified-sota-20260713/development-memphant-sibling-gather-retrieval.json` (`ccea6dcf62ae57e1563f93b21e295f32a4d8d3127f658cce9a0beb8135707f2e`)
- Evidence: `docs/build-log/artifacts/unified-sota-20260713/reader-evidence-development-memphant-sibling-gather.jsonl` (`04cf0e669bb9dfad69722488618ffba8bff601fe185e9102ffe1665481e20642`)
- Current-evaluator baseline: `docs/build-log/artifacts/unified-sota-20260713/reader-solpro-memphant-current-evaluator.json` (`d2e8af757b2b5b540d90ad3cd800cba99ba3dc6cc3eccabe811785d6d64ac367`)
- Candidate reader report: `docs/build-log/artifacts/unified-sota-20260713/reader-solpro-memphant-sibling-gather.json` (`d00aa54b22bd2dd09bdbe677d467bc0b920a8773827b45041f96af4764887da6`)
- Evaluator fingerprint: `bdcff6e35a6b5e773be25da0207e645a2cea0a1c15553abd0aa3fcd9fd6e9e03`
- Evaluator source: `f527a80888ed34b2239c774a6aba0089f557cc2fc6e90cc9a3dd52419b52b3c7`
- Question set: `f5d9f0b9cae0d237453c43a26d66a7c1b5660c518704a71a8ebe00719b2864b1`

Retrieval command:

```text
target/release/memphant-eval bench-lme --database-url postgres://memphant:memphant@localhost:5432/memphant_scratch_82982_1783940428 --data benchmarks/data/longmemeval_s.development.json --sample 178 --seed 20260713 --k 10 --disable rerank --budget-tokens 8192 --pool 64 --embed-model small --sibling-gather --emit-qa docs/build-log/artifacts/unified-sota-20260713/reader-evidence-development-memphant-sibling-gather.jsonl --baseline docs/build-log/artifacts/unified-sota-20260713/development-memphant-retrieval.json --out docs/build-log/artifacts/unified-sota-20260713/development-memphant-sibling-gather-retrieval.json
```

Reader command:

```text
scripts/run_reader.py --engine openrouter --model openai/gpt-5.6-sol-pro --judge-model openai/gpt-5.6-sol-pro --reasoning-effort high --prompt-version 1 --evidence docs/build-log/artifacts/unified-sota-20260713/reader-evidence-development-memphant-sibling-gather.jsonl --retrieval-report docs/build-log/artifacts/unified-sota-20260713/development-memphant-sibling-gather-retrieval.json --baseline docs/build-log/artifacts/unified-sota-20260713/reader-solpro-memphant-current-evaluator.json --out docs/build-log/artifacts/unified-sota-20260713/reader-solpro-memphant-sibling-gather.json --label task2-sibling-gather-v1 --cache-dir docs/build-log/artifacts/unified-sota-20260713/reader-cache/solpro --max-calls 10 --seed 20260713
```

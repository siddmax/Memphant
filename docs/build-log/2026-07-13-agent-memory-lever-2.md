# Agent-memory lever 2: temporal grounding

## Verdict

Retire the existing broad `--temporal-grounding` lever for accuracy. On the
178-question development split it changed 133 evidence packs, including 117
session-set changes, but the frozen Sol Pro high v1 reader fell from 104/178
(0.584270) to 103/178 (0.578652). The paired delta was -0.005618 with 95% CI
[-0.039326, 0.028090]: four miss-to-hit flips and five hit-to-miss flips.
Temporal-reasoning QA was unchanged at 28/50.

The retrieval mechanism did activate: Recall@5 and Recall@10 rose from
0.777108 to 0.789157, a paired +0.012048 with 95% CI
[-0.012048, 0.042169]. It added three non-abstention hits and lost one, but
abstention correctness fell from 7/12 to 5/12. Only one new retrieval hit
converted to a reader win; the two new temporal-reasoning hits did not.
Reader, parse, and judge errors were all zero.

No confirmation question was evaluated or inspected. This is a development
rejection, not a promotion or SOTA claim.

## Exact runs

Retrieval and evidence, through an ephemeral migrated Postgres database:

```text
target/release/memphant-eval bench-lme --database-url postgres://memphant:memphant@localhost:5432/memphant_scratch_75592_1783935679 --data benchmarks/data/longmemeval_s.development.json --sample 178 --seed 20260713 --k 10 --disable rerank --budget-tokens 8192 --pool 64 --embed-model small --temporal-grounding --emit-qa docs/build-log/artifacts/unified-sota-20260713/reader-evidence-development-memphant-temporal.jsonl --baseline docs/build-log/artifacts/unified-sota-20260713/development-memphant-retrieval.json --out docs/build-log/artifacts/unified-sota-20260713/development-memphant-temporal-retrieval.json
```

Reader and judge:

```text
scripts/run_reader.py --engine openrouter --model openai/gpt-5.6-sol-pro --judge-model openai/gpt-5.6-sol-pro --reasoning-effort high --prompt-version 1 --evidence docs/build-log/artifacts/unified-sota-20260713/reader-evidence-development-memphant-temporal.jsonl --retrieval-report docs/build-log/artifacts/unified-sota-20260713/development-memphant-temporal-retrieval.json --baseline docs/build-log/artifacts/unified-sota-20260713/reader-solpro-memphant.json --out docs/build-log/artifacts/unified-sota-20260713/reader-solpro-memphant-temporal.json --label task2-temporal-grounding-v1 --cache-dir docs/build-log/artifacts/unified-sota-20260713/reader-cache/solpro-temporal-v1 --max-calls 400 --seed 20260713
```

## Fingerprints

- Packaged eval binary: `221d4399b81ded522c5b68a8e001ccf2afd3f39de3c0c4fbe5c919a1b443b5b0`
- Development dataset: `e4667bed29565884b827ca0a75fbbec8d15f772c96011bb058ea5e2863d3a475`
- Question set: `f5d9f0b9cae0d237453c43a26d66a7c1b5660c518704a71a8ebe00719b2864b1`
- Retrieval report: `f9ecad6849e587130492089a6b24ce246fb2d9db4a08469a316296326bb433a1`
- Reader evidence: `a0dc4851454a7f32246819b7da7cde77cea4b1f6eaf5a57c079982660a98e9f6`
- Reader report: `3fdbc02537feaa3d5018680925736fdc103d7e5be65afc9315dcfeed7a63119e`
- Evaluator: `0b63e36081bb3bab6ebc52ac6c8acd76ab4debcf47d6eea53bac830a41f50354`

## Narrow next experiment

The small retrieval signal warranted one representation-only experiment, not
another broad temporal run. A deterministic transformer prepended, only to the
50 temporal-reasoning rows, one compact calendar fact per evidence body:
`before`, `on`, `after`, or explicit `unknown`, plus absolute calendar days.
It changed 340 bodies while preserving every question ID, evidence count,
session ID, rank/order, inclusion decision, and every non-temporal row.
Its focused contract passed 2/2 before the full transformation.

The unchanged frozen reader scored exactly the baseline: overall 104/178
(0.584270), temporal-reasoning 28/50 (0.560000), and every other stratum
unchanged. There were zero correctness flips and zero abstention regressions;
five temporal answers changed text without changing correctness. The run used
54 fresh and 232 cached calls with zero reader, parse, or judge errors. It
failed the required temporal-improvement gate, so the transformer and its
runtime route were deleted. Query-relative date-prefix representation is now
retired; these artifacts remain as rejection proof:

- Transform report: `docs/build-log/artifacts/unified-sota-20260713/validity-transform-v1.json` (`b05118e8d089db8957f4090af1ae6737ebe2bf59d7ab08f2c19683fa9375c696`)
- Transformed evidence: `docs/build-log/artifacts/unified-sota-20260713/reader-evidence-development-memphant-validity-v1.jsonl` (`5f0e82fdc24e9bfc0e6503a95aa37d9d548b007dd00eb94f125a8fd0f2794db7`)
- Reader report: `docs/build-log/artifacts/unified-sota-20260713/reader-solpro-memphant-validity-v1.json` (`aecf651185a2dccb2d3734cce5bf680945639a404db573fb13e902d041336488`)
- Transformer version/hash: `query-relative-validity-v1` / `0ff032f5650817a89e35d778030a5f34378490971d6d547d59bdca20f39c0c10`
- Evaluator fingerprint: `0b63e36081bb3bab6ebc52ac6c8acd76ab4debcf47d6eea53bac830a41f50354`

Historical transform command (the rejected transformer is no longer present):

```text
python3 scripts/transform_lme_temporal_validity.py --input docs/build-log/artifacts/unified-sota-20260713/reader-evidence-development-memphant.jsonl --output docs/build-log/artifacts/unified-sota-20260713/reader-evidence-development-memphant-validity-v1.jsonl --report docs/build-log/artifacts/unified-sota-20260713/validity-transform-v1.json
```

Reader command:

```text
scripts/run_reader.py --engine openrouter --model openai/gpt-5.6-sol-pro --judge-model openai/gpt-5.6-sol-pro --reasoning-effort high --prompt-version 1 --evidence docs/build-log/artifacts/unified-sota-20260713/reader-evidence-development-memphant-validity-v1.jsonl --retrieval-report docs/build-log/artifacts/unified-sota-20260713/development-memphant-retrieval.json --baseline docs/build-log/artifacts/unified-sota-20260713/reader-solpro-memphant.json --out docs/build-log/artifacts/unified-sota-20260713/reader-solpro-memphant-validity-v1.json --label task2-validity-representation-v1 --cache-dir docs/build-log/artifacts/unified-sota-20260713/reader-cache/solpro --max-calls 200 --seed 20260713
```

# Agent-memory lever 4: question-only retrieval query rewrite

## Frozen experiment

The candidate rewrote only the standalone question used for retrieval. The
reader, if reached, would still receive the original question. The structured
rewrite used `openai/gpt-5.6-sol-pro` at high reasoning effort, with no history
or gold data. Its system-prompt hash was
`02fba76273166dfbd4883dfbb411bb5c51dece49ee6a016eb945cd45872e2903` and
its JSON-schema hash was
`9c2fe9762111c1e93a19017a10e9403fb537083419ba45c46312aafe8b95aaab`.
All 178 development rewrites were frozen before retrieval.

The predeclared mechanism gate required at least three net answer-session
Recall@10 gains and zero gold-abstention regressions. The decision also used
the previously corrected metric: a selected evidence session counts as
answer-bearing only when its full parent session contains the normalized
literal gold answer. Confirmation data was not evaluated or inspected.

## Result

Rejected before reader QA. On a fresh migrated scratch Postgres database, the
benchmark's official answer-session Recall@10 increased from 129/166 to
136/166: 12 gains, five losses, net +7. That proxy improvement did not recover
answer-bearing sessions. Corrected full-parent literal-answer coverage fell
from 78 to 76: four gains and six losses, net -2. Gold-abstention correctness
also fell from 7/12 to 6/12, with regression `88432d0a_abs` and no gain.
Inside the 30 retrieval-miss/oracle-hit target rows, the official proxy gained
10 sessions, but corrected answer-bearing coverage gained only two and lost
one (net +1). The broad rewrite therefore recovered one target parent while
causing three net answer-bearing losses elsewhere.

The route therefore fails the meaningful retrieval target and the locked
zero-abstention condition. No paid reader run was made. HyDE was not run
because the query rewrite did move the predeclared Recall@10 mechanism; adding
another query-generation branch after observing this result would violate the
frozen branch rule. Candidate-only implementation and tests were retired.

This result is a direct warning against making the product decision from the
benchmark's session-ID proxy alone: that metric improved by seven while actual
answer-bearing parent selection worsened by two.

## Artifacts and fingerprints

- Rewrite manifest: `docs/build-log/artifacts/unified-sota-20260713/query-rewrites-solpro-v1.json` (`f38855aff37122e4c9868c95a2422c4ab200e42ff97992db125102f8c32c534e`)
- Rewritten development dataset: `benchmarks/data/longmemeval_s.development.query-rewrite-solpro-v1.json` (`8182183c360ff2efac8e4d24d179427d55ea3a387d2d71521a5ed37c97db9e17`)
- Retrieval report: `docs/build-log/artifacts/unified-sota-20260713/development-memphant-query-rewrite-retrieval.json` (`31f75b7e6a1e75ea8f59c184c0eed67a069ac257c70d1ebd946b86fc7ec2f50c`)
- Raw evidence: `docs/build-log/artifacts/unified-sota-20260713/reader-evidence-development-memphant-query-rewrite-raw.jsonl` (`a68614da1d1a3604df09c663e5df32fd70f2d0db6deaab335420f1dc185b2372`)
- Original development dataset: `e4667bed29565884b827ca0a75fbbec8d15f772c96011bb058ea5e2863d3a475`

Retrieval command:

```text
target/release/memphant-eval bench-lme --database-url postgres://memphant:memphant@localhost:5432/memphant_scratch_59185_1783943830 --data benchmarks/data/longmemeval_s.development.query-rewrite-solpro-v1.json --sample 178 --seed 20260713 --k 10 --disable rerank --budget-tokens 8192 --pool 64 --embed-model small --emit-qa docs/build-log/artifacts/unified-sota-20260713/reader-evidence-development-memphant-query-rewrite-raw.jsonl --out docs/build-log/artifacts/unified-sota-20260713/development-memphant-query-rewrite-retrieval.json
```

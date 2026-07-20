# High-accuracy reader lattice calibration

Date: 2026-07-13

Plan: `docs/superpowers/plans/2026-07-12-unified-temporal-memory-sota.md`, Task 1.

## Decision

Reserve GPT-5.6 Sol Pro at high reasoning as the accuracy reference, fixed
promotion judge, and sealed/finalist reader. Use GPT-5.6 Luna Pro at high
reasoning as the normal development reader and retain Claude Sonnet 5 at high
reasoning only as the cross-family robustness reader.
Every development winner must still be rerun against its baseline with Sol Pro
before promotion; Sonnet may select experiments but cannot authorize a final
claim by itself. GPT-5.6 Sol Pro remains the fixed judge for every promotion
comparison in this lattice.
The dated model snapshots are recorded in
`benchmarks/manifests/reader_lattices.v1.json`.

This is an accuracy decision, not a cost decision. On identical MemPhant
evidence, Sol Pro scores 0.5843 versus 0.5337 for the previous Terra-medium
lattice (paired delta +0.0506, question bootstrap CI [+0.0225,+0.0843]) and
0.5393 for Sonnet 5 (+0.0449 [+0.0169,+0.0787]). Accuracy is not tied, so the
latency tiebreak is not invoked. Cost optimization is deferred until the SOTA
answer/evidence artifacts are frozen.

## Cheaper-reader challenge

The current evaluator replayed the Sol and Sonnet artifacts entirely from
cache, then ran `z-ai/glm-5.2` at high reasoning on all 178 identical evidence
rows with the same Sol Pro judge.

| Reader | Correct | Accuracy | Reader/parse/judge errors |
|---|---:|---:|---:|
| Sol Pro high | 104/178 | **0.5843** | 0/0/0 |
| Sonnet 5 high | 96/178 | 0.5393 | 0/0/0 |
| GLM-5.2 high | 94/178 | 0.5281 | 0/4/0 |

Against Sol, Sonnet's paired delta is -0.0449 [95% question-bootstrap CI
-0.0787,-0.0169] and GLM's is -0.0562 [-0.0899,-0.0225]. Sol wins eight
Sonnet rows and ten GLM rows; neither cheaper reader wins a row that Sol misses.
GLM and Sonnet are statistically tied (-0.0112 [-0.0449,+0.0225]), but GLM is
two answers lower and promotion-ineligible. Its four failures are real model or
provider-contract failures: two truncated JSON objects, one plain-text response
despite strict structured output, and one abstention with a non-null answer.
Even scoring all four as correct leaves GLM at 98/178, below Sol's 104/178.

The GLM run consumed approximately $4.10 including Sol judging. Current
OpenRouter list prices at execution were $2/$10 per million input/output tokens
for Sonnet 5 and $0.93/$3 for GLM-5.2. GLM is rejected as the development reader
until a future pinned-provider arm proves zero contract failures and higher
accuracy; no retry is justified by this result.

The user-requested final screen then tested `openai/gpt-5.6-luna-pro` and
`google/gemini-3.5-flash` on the same 178 rows with the same Sol judge:

| Reader | Correct | Accuracy | Reader/parse/judge errors | Eligible |
|---|---:|---:|---:|---|
| Luna Pro high | 100/178 | **0.5618** | 0/0/0 | yes |
| Gemini 3.5 Flash high | 97/178 | 0.5449 | 0/19/0 | no |

Luna wins six rows Sonnet misses while Sonnet wins two rows Luna misses, for a
net +4 answers. Luna remains four answers below Sol and never wins a Sol miss;
Sol therefore remains the accuracy authority. Luna replaces Sonnet for normal
development because it is more accurate on this evidence and cheaper at the
recorded OpenRouter prices. Sonnet remains the cross-family sign check. Flash
is rejected: its 19 truncated strict-JSON replies make it fail closed even
though its scored accuracy is competitive. No retry or larger-output exception
is added to rescue a contract-invalid arm.

Current artifacts:

- `reader-model-screen-solpro.json` (`aaa921f5...de26`)
- `reader-model-screen-sonnet5.json` (`e1c50c20...ad85`)
- `reader-model-screen-glm52.json` (`31cb6290...182`)
- `reader-model-screen-luna.json`
- `reader-model-screen-flash.json`

## Controls

| Reader | No memory | MemPhant | Answer-session oracle |
|---|---:|---:|---:|
| Sol Pro high | 0.0674 | **0.5843** | **0.9157** |
| Sonnet 5 high | 0.0674 | 0.5393 | 0.8933 |
| Terra medium | 0.0674 | 0.5337 | 0.8708 |

Every arm scores all 178 historically exposed cleaned LongMemEval questions,
has zero reader/parse/judge errors, and is bound to the same question-set hash.
No confirmation question was read or scored.

The primary MemPhant-minus-no-memory delta is +0.5169
[+0.4438,+0.5899]. The answer-session-oracle-minus-MemPhant gap is +0.3315
[+0.2584,+0.4045]. These question-level bootstrap intervals are diagnostics,
not sealed promotion claims; shared benchmark sessions require the stronger
cluster-aware protocol at the claim gate.

## What the oracle gap says

Sol Pro's oracle scores by stratum are 1.000 knowledge update, 0.851
multi-session, 0.947 assistant, 0.455 preference, 1.000 user, and 0.980
temporal. The corresponding MemPhant values are 0.821, 0.383, 0.842, 0.273,
0.696, and 0.560.

Among 74 MemPhant misses, 36 occur even though an answer-bearing session is in
the packed top ten: 23 conservative abstentions and 13 wrong answers. The
largest recoverable oracle gaps are multi-session (+0.468) and temporal
(+0.420). Preference remains weak even under oracle evidence, so it needs both
better retrieval/profile projection and a preference-specific composition
policy. This evidence rejects a universal reader-only rewrite as the main next
move.

## Official comparability track

The separately pinned upstream evaluator (`xiaowu0162/LongMemEval` commit
`9e0b455f4ef0e2ab8f2e582289761153549043fc`, metric model
`gpt-4o-2024-08-06`) scores the same final-answer-only Sol Pro MemPhant output
at 0.5787. The repaired promotion evaluator scores 0.5843. The official number
is reported for leaderboard comparability only and is permanently ineligible
to drive MemPhant promotion.

As a same-family-bias check, the official GPT-4o evaluator also scored the
complete Sonnet 5 and Terra-medium answer artifacts. Both scored 0.5337 with
zero API errors. Sol Pro's paired lead is +0.0449 with a question-bootstrap
95% interval of [+0.0169,+0.0787] against each. The independent evaluator
therefore confirms rather than reverses the primary reader selection; this is
still development evidence, not a sealed SOTA result.

## Runtime evidence

The packaged Postgres runtime ran in an ephemeral migrated scratch database.
Its 178-row development arm has zero degraded rows and retrieval
R@5=R@10=0.7771. The evidence SHA-256 is
`b7491da86fbfda01501641465f9bc7ab2d578ae359141c47ac91bb1fcba26120`.

## Verification

```text
python3 -m pytest tests/test_run_reader_contract.py \
  tests/test_build_lme_reader_controls.py tests/test_gate_compare.py \
  tests/test_run_longmemeval_official_eval.py -q
45 passed

python3 -m pytest tests/ spikes/python-retain/test_spike.py -q
230 passed, 1 spec-mirror failure before the same-change STATUS sync

python3 scripts/check_spec_drift.py
spec_drift=clean

python3 -m pytest tests/test_repo_contract.py -q
10 passed
```

The full Python gate is rerun after this proof and manifest land. No SOTA or
confirmation checkbox moves on development calibration alone.

# Evaluator integrity repair

Date: 2026-07-13

Plan: `docs/superpowers/plans/2026-07-12-unified-temporal-memory-sota.md`, Task 0.

## Verdict

The pre-2026-07-13 reader QA values are historical diagnostics, not promotion
evidence. Their free-form replies cannot be losslessly converted into a final
answer field, and the old scorer could accept a gold string appearing in notes
or intermediate reasoning even when the final answer was wrong. Reinterpreting
those replies with another heuristic would recreate the same ambiguity.

All promotion runs must therefore be rerun with the repaired structured
contract. The reader emits exactly `{notes, answer, abstain}`; only `answer` is
graded; abstention is exactly `abstain=true` and `answer=null`; reader, parse,
and judge errors score incorrect and make the report promotion-ineligible.

## Dataset and split

- Source: `xiaowu0162/longmemeval-cleaned`
- Revision: `98d7416c24c778c2fee6e6f3006e7a073259d48f`
- Cleaned LongMemEval-S SHA-256:
  `d6f21ea9d60a0d56f34a05b609c79c88a451d2ae03597821ea3d5a9678c3a442`
- Development: 178 historically exposed question IDs.
- Confirmation: 319 question-unseen and answer-bearing-session-disjoint IDs.
- Excluded: three question-unseen IDs linked through answer-bearing sessions.
- Strict all-haystack-session-disjoint confirmation: empty, because shared
  distractor sessions connect the cleaned benchmark.

The committed fetcher verifies downloaded bytes against the pre-existing lock
before replacement and never learns a new pin during an ordinary fetch.

## Two evaluator tracks

The upstream LongMemEval evaluator remains a separately reported comparability
track. Its canonical task-specific prompts and published `gpt-4o-2024-08-06`
judge contract are retained for leaderboard comparison. It does not carry
MemPhant promotion decisions because the upstream implementation accepts any
judge reply containing `yes` and grades the whole free-form hypothesis.

The MemPhant promotion evaluator uses the same task-specific rubric but grades
only the structured final answer and accepts only an exact normalized `yes` or
`no`. These two values must never be substituted for one another.

## Fail-closed proof

The promotion validator rejects unequal or duplicate IDs, partial runs, any
runtime/parse/judge error, non-boolean correctness, tampered evaluator hashes,
mismatched model/runtime identity, and mismatched immutable gold inputs. An
invalid baseline still leaves a complete current-run artifact marked
`HOLD/INVALID` and returns nonzero.

```text
python3 -m pytest tests/test_run_reader_contract.py tests/test_gate_compare.py -q
31 passed

python3 -m pytest tests/ spikes/python-retain/test_spike.py -q
223 passed

python3 scripts/fetch_longmemeval.py
both datasets verified against pinned sha256; split manifest regenerated

python3 -m py_compile scripts/run_reader.py scripts/gate_compare.py scripts/fetch_longmemeval.py
git diff --check
exit 0
```

No benchmark-accuracy checkbox changes on this proof alone. It repairs the
measurement instrument; Task 1 must produce fresh structured reader artifacts.

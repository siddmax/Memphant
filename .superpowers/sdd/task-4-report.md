# Corrected P1-T6 Task 4 report

- Base: `0c67c13ad50177973ca62fc7fda7da88391c6949`
- Commit: this task commit
- Paid model/database calls: none
- STATUS/ledger changes: none
- Unrelated dirty handoff preserved: `docs/handoff/NEXT-SESSION-PROMPT.md`

## Outcome

Bound aggregation to the approved build-once pair contract. An aggregate now
requires exactly 12 unique sealed construction proofs, 24 distinct clone
database identities bound to the expected case and arm, matching pair bank
seals, and only the selected Sonnet treatment. Every archived memory proof is
validated before outcome scoring, including operational failures: it must be
query-only, reference its case-bank construction hash, inherit the frozen
worker proof, and contain no arm retains or construction timing fields.

Added a controller-owned `construction_duration_ms` measurement covering
server startup, inserts, worker drain, frozen adapter proof, server cleanup,
key cleanup, redaction, and the final job-state check. The adapter construction
proof remains immutable; the duration is stored in the case-bank manifest and
therefore bound by its seal. Aggregate output reports construction duration and
zero local construction cost separately. Registered Fast/Deep query recall
latency, Deep generation cost, official scoring, wins/losses, truncation,
security, settlement, and positive-delta predicates are unchanged.

## TDD evidence

- Red: `python3 -m pytest tests/test_run_lme_v2_p1_t6.py -q` -> 58 passed, 7 failed before aggregate enforcement
- Green: `python3 -m pytest tests/test_run_lme_v2_p1_t6.py -q` -> 68 passed
- Adapter regression: `python3 -m pytest tests/test_public_benchmark_adapters.py -q` -> 15 passed, 1 intentional packaged integration skip
- `git diff --check` -> passed

Synthetic aggregate fixtures use the real Task 2/3 shapes: query-only query and
pairing fields, frozen worker evidence without arm retains, construction hashes
bound to case-bank manifests, controller construction duration, and distinct
case/arm clone identities.

## Campaign truth

The stopped `run-408363c9` diagnostic root remains immutable and ineligible.
The build-once implementation is approved through Task 3 and now has complete
aggregate contract coverage. P1-T6 remains open pending independent Task 4
review, the no-model Task 5 integration proof, and a passing immutable n=12
execution. No larger or paid run is authorized by these tests.

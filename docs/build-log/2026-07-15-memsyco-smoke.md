# MemSyco five-task smoke: passed

## Outcome

The fresh promotion-ineligible run at
`docs/build-log/artifacts/unified-sota-20260714/memsyco-smoke-20260715T063309Z/`
passed one official sample for each of MemSyco's five tasks. It made exactly
five structured-extractor, five Luna answer, and five official Gemini judge
calls. All 15 response IDs were unique, all calls were first attempts with
positive reconciled usage and cost, every parse/decode succeeded, and the
independent `verify-results` readback passed. This is a five-sample smoke, not
a full-benchmark or SOTA claim, and it moves no STATUS checkbox.

The passing rung used 45,979 prompt and 22,914 completion tokens and cost
$0.23557035. Seven earlier immutable MemSyco attempts cost $0.40638720 in total,
so cumulative MemSyco smoke spend was $0.64195755.

## Five official metric objects

- `objective_fact_judgment`: objective correctness 1.0, preference
  contamination 0.0, suppress pass 1.0.
- `contextual_scope_control`: accuracy 1.0, incorrect preference use 0.0,
  scope pass 1.0.
- `memory_evidence_conflict`: accuracy 1.0, misled by conflicting memory 0.0,
  evidence pass 1.0.
- `valid_memory_selection`: latest preference use 1.0, outdated contamination
  0.0, valid-selection pass 1.0.
- `personalized_memory_use`: answer accuracy 1.0, preference use 1.0,
  memory-use pass 1.0.

No aggregate scalar was computed.

## Passing calls

Structured extractor, served by Google AI Studio as
`google/gemini-3.5-flash`:

- `gen-1784097210-Rmq3WbJOvaeAwjIlqpxa`: 1,291 prompt, 3,498 completion,
  $0.03341850.
- `gen-1784097246-fPDYusebibNh0kJA0rpU`: 1,599 prompt, 6,258 completion,
  $0.05872050.
- `gen-1784097294-Kb8nbThVqcThWPxOo4Iq`: 1,830 prompt, 2,471 completion,
  $0.02498400.
- `gen-1784097327-R7ieeSh16t1BeAR73enT`: 2,352 prompt, 4,289 completion,
  $0.04212900.
- `gen-1784097372-ZW00z7iKlfgaHSUc1FXg`: 2,034 prompt, 3,012 completion,
  $0.03015900.

Luna answers, requested as `openai/gpt-5.6-luna-pro`, served by OpenAI as
`openai/gpt-5.6-luna-pro-20260709`:

- objective `gen-1784097225-NJY197popk84DkQrdEy5`: 4,191 prompt, 308
  completion, $0.00603900.
- contextual scope `gen-1784097269-aaALVu8ms73RvXsXWm9Q`: 5,807 prompt, 714
  completion, $0.01009100.
- evidence conflict `gen-1784097305-wyhSDJhhyFv9q5z2F6rU`: 7,402 prompt, 460
  completion, $0.00772120.
- valid selection `gen-1784097345-evBbECug3AyKnLYPrI50`: 8,776 prompt, 1,407
  completion, $0.01457740.
- personalized use `gen-1784097384-YzXbeDrwb8zaFlJ5HYzg`: 4,974 prompt, 129
  completion, $0.00574800.

Gemini judges, requested as `google/gemini-3.1-flash-lite-preview`, served by
Google AI Studio as `google/gemini-3.1-flash-lite-preview-20260303`:

- objective `gen-1784097237-X7By1fRJiRnHCBdy6Iro`: 1,024 prompt, 81
  completion, $0.00037750.
- contextual scope `gen-1784097282-hhtydPGO36lYAWVpPr6c`: 1,472 prompt, 72
  completion, $0.00047600.
- evidence conflict `gen-1784097313-RM71zrFk3g4ipIecglEK`: 1,111 prompt, 74
  completion, $0.00038875.
- valid selection `gen-1784097360-V4Ku0D8AHi69o0QDLO6R`: 1,531 prompt, 73
  completion, $0.00049225.
- personalized use `gen-1784097391-J8gOO5DwROejvCmva1Ft`: 585 prompt, 68
  completion, $0.00024825.

## Root-cause hardening completed during the rung

- The synthetic pinned `baselines` package now carries a real import spec, so
  the official objective evaluator cannot mistake it for a missing package and
  replace it with the raw-dialogue stub.
- Missing official sample-key arguments resolve to a recomputable hash of the
  label-free dialogue and question hashes; the proof records which identity
  mode was used and the verifier binds it to the proof filename and returned
  baseline metadata.
- Official title-case dialogue roles are normalized at the adapter boundary to
  MemPhant's canonical lowercase evidence roles. The strict evidence grounder
  remains unchanged.
- Trace readback now includes the full canonical subject, scope, actor,
  agent-node, and generation query contract.
- The objective-only arm flag is no longer sent to the other four official
  task parsers, and the verifier accepts the objective wrapper's final official
  task label.

Each failure was captured in a separate UTC directory with its own failure
record and `SHA256SUMS`; no paid completion was resent into an existing ledger.

## Verification

```sh
python3 -m pytest \
  tests/test_run_reader_contract.py \
  tests/test_temporal_benchmark_contract.py \
  tests/test_restraint_benchmark_contract.py -q
# 89 passed

python3 scripts/run_restraint_bench.py verify-results \
  --run-dir docs/build-log/artifacts/unified-sota-20260714/memsyco-smoke-20260715T063309Z

cd docs/build-log/artifacts/unified-sota-20260714/memsyco-smoke-20260715T063309Z
shasum -a 256 -c SHA256SUMS
```

Python compilation, spec-mirror drift, and `git diff --check` also passed. The
next separately bounded rung is the reproducible causal 15-question Memora
split; full MemSyco remains blocked.

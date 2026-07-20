# Agent-memory lever 1: evidence notes

## Verdict

Rejected on the 178-question development split. The v4 evidence-note prompt
scored 102/178 (0.573034) against the frozen v1 Sol Pro high baseline's
104/178 (0.584270): a paired delta of -2 questions (-0.011236). Both losses
were in the multi-session stratum, which fell from 18/47 to 16/47; every other
stratum was unchanged. There were zero miss-to-hit flips and two hit-to-miss
flips (`6456829e_abs`, `edced276_abs`). Both were gold abstentions whose
answer-session IDs intersected the packed evidence. Reader, parse, and judge
errors were all zero.

The candidate changed ten outputs from abstention to answer. Eight remained
wrong and two formerly correct abstentions became wrong. It therefore reduced
abstention without improving evidence use and is retired from the runtime. No
confirmation question was evaluated or inspected, and no SOTA or promotion
claim follows from this development rejection.

## Reproducibility

- Candidate artifact: `docs/build-log/artifacts/unified-sota-20260713/reader-solpro-memphant-v4-notes.json`
- Baseline artifact: `docs/build-log/artifacts/unified-sota-20260713/reader-solpro-memphant.json`
- Candidate evaluator fingerprint: `0222a4c8dee8afce3d5739d67596c4c46d85dda1fb2aa1f41cadc30a0acb4a9f`
- Candidate prompt fingerprint: `37e961e23639f4cc932d64ad89c51c48d5787cea33d80bd3e0c3165e0fc3aad8`
- Candidate calls: 294 fresh, 0 cached; complete and promotion-eligible as an evaluation artifact

## Next causal target

Do not add more global reader instructions. The remaining target is the
multi-session evidence packet: make query-relative temporal validity and
contradictory-session provenance explicit in the packed representation, then
measure that single retrieval/packing change against the unchanged frozen
reader. This directly targets the observed inability to distinguish a usable
answer from an answer-bearing but invalid distractor session.

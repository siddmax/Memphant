# Agent-memory lever 5: active-profile projection rejection

Date: 2026-07-13

## Decision

Reject and delete the relevance-gated active-profile projection. Do not spend a
reader call on it and do not retain its default-off evaluation toggle.

The implementation reused deterministic W6 preference/attribute facts, kept
real unit IDs and episode citations, filtered only facts already admitted by
the existing policy-checked fused pool, and preserved the normal packing and
budget paths. Focused tests and independent review passed before evaluation.

## Mechanism gate

The complete 178-question exposed development split ran through the packaged
Postgres runtime in an ephemeral migrated scratch database. It compared the
current fact-extraction-off baseline with fact extraction plus the proposed
profile filter under identical sample, seed, k, budget, pool, embedder, and
rerank settings. The sealed 319-question confirmation set remained unopened.

| metric | baseline | candidate | paired delta (95% CI) |
|---|---:|---:|---:|
| Recall@5 | 0.7771 | 0.6807 | -0.0964 [-0.1506,-0.0422] |
| Recall@10 | 0.7771 | 0.7410 | -0.0361 [-0.0783,0.0000] |
| preference Recall@10 | 0.3636 | 0.2727 | -0.0909 |

The candidate regressed the exact stratum it was intended to help as well as
overall retrieval. It therefore fails before reader QA. Repeating the run or
tuning thresholds would be post-hoc rescue rather than the one-lever causal
loop.

Artifacts:

- `docs/build-log/artifacts/unified-sota-20260713/development-profile-projection-retrieval.json`
- `docs/build-log/artifacts/unified-sota-20260713/reader-evidence-development-profile-projection.jsonl`

## Independent correctness fix retained

The experiment exposed a separate root-cause bug: a normal resolved
supersedence writes both `Supersedes` and `Contradicts`, while recall previously
labeled every endpoint of every contradiction as unresolved. Suppression now
requires both endpoints to be simultaneously live recall candidates. A closed
superseded predecessor no longer forces false abstention; two live conflicting
heads still do. That behavior is independent of the rejected projection and
remains protected by recall-trace regressions.

No confirmation row, SOTA checkbox, or runtime-default memory feature moved.

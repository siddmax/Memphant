# RAG retrieval admission — 2026-07-13

## Decision

Promote the release-mode `modernbert` + balanced vector/lexical admission +
Voyage `rerank-2.5` top-8 configuration as the only Task-3 development
candidate. Do **not** replace Syndai/CaaS and do **not** claim SOTA yet.

The candidate clears both the absolute synchronous screen and the final
same-binary paired retrieval screen on both exposed development sets, with
zero reranker failures, fallbacks, degraded rows, or skipped rows:

| set | R@5 | R@10 | end-to-end p95 | rerank p95 |
| --- | ---: | ---: | ---: | ---: |
| v1 | 0.267 | 0.283 | 1,053 ms | 576 ms |
| v2 | 0.317 | 0.417 | 1,027 ms | 802 ms |

The directional artifacts are under
`docs/build-log/artifacts/unified-sota-20260713/task3-rerank/voyage8-balanced-fast-release-connect500/`.
The authoritative paired artifacts are under
`docs/build-log/artifacts/unified-sota-20260713/task3-rerank/same-binary-final3/`.
They bind the corpus, golden sets, generated binaries, migration sources,
database catalog, packer, evidence, runtime configuration, and per-query
traces.

## Evaluation-integrity correction

The earlier docs gate indexed only the 3,257 sections eligible for golden
mining. That made the retrieval corpus depend on the evaluation labels. The
correct common haystack contains every pinned leaf section: 4,870 sections
from 109 tracked files, 5,134,459 characters, corpus revision
`sha256:82814a4c39ee7894fcc94d5af0192f709ecd845536e638c88966c3117aaea581`.
All partial-corpus results are historical diagnostics and cannot authorize a
replacement.

Both engines now use the same deterministic 8,192-token evidence packer after
ranking. The incumbent runner is required to exercise its production adaptive
query/rerank path and prove a rerank-complete event for every trace. Any source
skip, ingest error, silent fallback, degraded trace, incomplete pair, or
identity mismatch fails the gate.

## Causal screen

The two 60-question sets are positive-only semantic stress tests; 12/60 are
multi-hop and most questions have little lexical overlap with the answer
section. They are useful development evidence, but cannot prove restraint or
supported-answer quality.

Local cross-encoder arms improved ordering but failed latency. The strongest
release-mode local configuration reached R@10 0.267/0.200 with approximately
3.0/3.1-second p95. A fixed 32+32 quota did not materially improve recall and
also failed latency, so it was deleted. Trace analysis identified input
truncation as the dominant collapse: 29/47 answer-bearing candidates that
fell out of the top ten lost the answer span at 128 tokens. Increasing local
input length preserved spans but could not fit the latency budget.

The hosted implementation therefore stays deliberately narrow: one Voyage
model behind the existing reranker seam, raw documents, provider truncation
disabled, no retry path, strict complete/unique result validation, bounded
response bodies, and exact trace provenance. Top-16 could not reliably satisfy
the timeout budget. Top-8 preserved the latency headroom and produced the
development result above.

## What this proves and what remains

This proves a viable accuracy-first synchronous candidate and resolves the
immediate local truncation/latency tradeoff. It does not prove that MemPhant is
better than the current Syndai answer system because no same-binary,
same-corpus supported-answer comparison has run with the frozen Sol Pro reader.
The first raw no-rerank and Voyage artifacts had different source identities,
so they remain directional only. The completed `same-binary-final3` rerun
removed that ambiguity. No-rerank scored R@10 0.050 on v1 and 0.100 on v2;
Voyage scored 0.283 and 0.417. Source-document-cluster bootstrap deltas were
+0.233 [95% CI +0.133,+0.333] and +0.317 [+0.204,+0.435]. Candidate p95 was
1.015 and 0.877 seconds. Generation identity, source-tree identity, corpus
revision, binaries, migration/catalog identity, mode, k, and evidence budget
match; only the declared reranker fields differ. This promotes Voyage through
the retrieval mechanism gate and authorizes supported-answer reader scoring.

It still does not prove that MemPhant is better than the current Syndai answer
system. That requires the same Sol Pro reader and strict supported-answer judge
over both arms, including evidence support and position-swapped adjudication of
answer-quality flips.
It also does not cover a sealed version-disjoint corpus, public LongEval-RAG,
negative/abstention behavior, citations, temporal corrections, forgetting,
tenant isolation under application traffic, or code-task outcomes.

Replacement is admitted only after all those gates pass. Until then the
Syndai/CaaS implementations remain untouched.

## Supported-answer conversion gate

The first supported-answer attempt was invalid because Azure rejected the
`uniqueItems` JSON-Schema keyword. The repaired runner omits that unsupported
provider keyword while preserving the same uniqueness invariant in its local
strict parser. Focused contract coverage passes (`46 passed`).

Fresh Sol Pro `rag-supported-v1` runs now score all 60 rows per set with zero
reader, parse, or judge errors:

| set | no rerank | Voyage top-8 | paired delta (95% bootstrap CI) | adjudication |
|---|---:|---:|---:|---|
| v1 | 0.083 | 0.350 | +0.267 [+0.150,+0.400] | 15/16 improved flips resolved; one position disagreement |
| v2 | 0.117 | 0.417 | +0.300 [+0.183,+0.417] | all flips resolved |

Every resolved improvement survived position-swapped bundle adjudication. The
single v1 disagreement is a genuinely ambiguous row: the point judge accepted
the candidate answer as correct and supported, while one paired ordering chose
`neither`. The fail-closed report is therefore promotion-ineligible. Do not
retry, majority-vote, or weaken the contract to erase that uncertainty.

This proves a large supported-answer conversion gain on v2 and a strong but
not fully adjudication-clean gain on v1. It does not yet authorize promotion:
the negative slice, production hierarchy parity, independent version-disjoint
holdout, and LongEval-RAG confirmation remain required.

## Negative and exact-abstention admission contract

A hash-locked ten-case development restraint slice now covers unrelated
content, lexical decoys, plausible absence, tenant/user/project/agent
isolation, transaction snapshots, stale validity, and answerable world
knowledge unsupported by retained memory. Runtime projections exclude all
gold/forbidden/expectation labels. The lock binds the slice, both positive-set
locks, the corpus revision, provenance, creation time, and disjointness checks;
the loader also enforces kind-specific scope/time/ingest semantics so
regenerating a hash cannot bless a no-op case.

The mechanism gate grades forbidden canaries on the raw returned top ten,
before evidence packing can truncate or drop a leak. Both engines must execute
all ten cases with complete, exactly paired, error-free artifacts. MemPhant
must then achieve zero forbidden hits and exact structured abstention on all
ten. Incumbent failures remain measured replacement evidence rather than
making replacement logically impossible; the report publishes exact counts
and rates only and makes no bootstrap claim at n=10. Syndai's dated stale case
runs through its real current source/search path without pretending it has a
`valid_at` parameter.

The comparator is now fail-closed: negative provenance and both negative
reader reports are mandatory for any binding verdict, each reader is bound to
the nested negative-evidence hash and full retrieval report, and any
unsupported/runtime/parse/judge error yields `HOLD/INVALID`. Two independent
reviews found and removed earlier false-pass paths. Focused verification is
green (`122 passed` implementation suite; `83 passed` final independent
review; `124 passed` root rerun across comparator, negative runners, and reader
contracts). No live engine or paid-reader result has run yet, so this proves
the gate contract—not candidate restraint—and does not move the replacement
decision.

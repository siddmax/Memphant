# Memora causal split — 2026-07-15

The current-schema `weekly/software_engineer` causal split passed. The official
Memora revision was `a6493188efc836d6511ed5e4163fe3ba87da30ff`; the reader was
`openai/gpt-5.6-luna-pro`, served as the pinned
`openai/gpt-5.6-luna-pro-20260709`. The run covered 163 sessions, 15 questions,
and all 71 official FAMA subquestions.

The clean extraction bank contains 350 memory units, 350 embeddings, and 93
edges. It used 165 priced extractor attempts for 163 episodes, including two
bounded repairs, at $2.33492580. Its logical identity is
`ded337308e6852e4899dbb1be6f36cec469cda93be08970d54ef56609f99e87c`.
After the source schema changed during construction, the bank was deterministically
resealed against current binaries without another model call; the final archive
SHA-256 is `990599856b80dd669949c2744410c1ecf5779f08e1cd47eed53864e6185fe564`.

The no-cost paired retrieval diagnostic was eligible for all 15 questions and
five expected rollups. The final reader run made 15 first-attempt calls for
$0.20682780. The unchanged official FAMA scorer made 213 first-attempt judge
calls (71 each through Anthropic, Google, and OpenAI) for $0.15678060. All
attempts have unique response IDs, positive reconciled usage and cost, served
model/provider evidence, retry index zero, and request/result hashes.

Official FAMA improved from 53.49206349 to 61.66666667 (+8.17460317). Overall
accuracy improved from 43/71 to 56/71; current-memory presence improved from
19/44 to 29/44; forgetting absence improved from 24/27 to 27/27. Task FAMA was
40.0 Remembering, 86.66666667 Reasoning, and 58.33333333 Recommending.

The complete paid spend, including immutable failed attempts, was $3.03345640.
The failed spend was one 12-attempt extractor run ($0.16450210), one reader
call rejected by the pre-pinned snapshot validator ($0.01212940), and two
fail-closed FAMA runs ($0.09167290 and $0.06661780). The latter exposed two
root causes now covered by regressions: authoritative generation statistics
must reconcile missing immediate usage, and OpenRouter response caching must
be explicitly disabled for paid provenance runs. No paid completion was
retried in place.

The complete artifact tree and verified `SHA256SUMS` are under
`docs/build-log/artifacts/unified-sota-20260714/memora-causal-split-20260715T072809Z/current-schema-rebuild-20260715T080000Z/`.
This freezes a viable Memora causal candidate; it does not establish full-suite
SOTA and moves no STATUS checkbox.

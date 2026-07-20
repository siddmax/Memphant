# P1-T6 pre-execution amendment 5

Date: 2026-07-20

The release-built execution root `run-b3bc244d` is infrastructure-invalid and
is preserved without replay. Its Fast row compiled all 670 resources and
completed non-degraded recall in 26.556 seconds. The pinned official harness
then failed locally while counting the returned memory-context tokens: its
`Qwen/Qwen3.5-9B` `AutoProcessor` selected `Qwen3VLVideoProcessor`, which
requires PyTorch and Torchvision, but those optional runtime dependencies are
absent from the upstream `requirements.txt`. The campaign's previous
dependency proof ran the harness `--help` path and therefore did not exercise
this runtime boundary.

The Fast row produced no reader route, judge route, Deep generation, or
official score and settled no spend. The following Sonnet row was interrupted
during local memory compilation before recall or model dispatch. All campaign
processes exited and both scratch databases were dropped. The append-only
`run-b3bc244d/INVALIDATION-PROOF.json` records the exact artifact inventory,
trace hashes, zero-spend audit, and cleanup predicates.

The campaign now owns a supplemental, exact dependency contract for
`torch==2.13.0` and `torchvision==0.28.0`, which are the matching current
releases for the isolated Python 3.14 environment. Preflight requires those
exact pins in the frozen package inventory and executes the official harness's
real `count_memory_context_tokens` function with a text context. That path
instantiates the exact Qwen processor and produces a positive tensor-backed
token count. A help/import check alone can no longer authorize a paid root.

A fresh root is authorized only after the exact sanitized campaign interpreter
passes `pip check`, freezes the supplemental-requirements hash and complete
package inventory, and passes both the harness bootstrap and real processor
preflights. The invalid root is never resumed, aggregated, or used in a metric.

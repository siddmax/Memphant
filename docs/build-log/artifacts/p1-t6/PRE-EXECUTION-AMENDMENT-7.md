# P1-T6 pre-execution amendment 7

Date: 2026-07-20

The five predicates in amendment 6 are now proven. A fresh, never-before-used
execution root is authorized; none of the prior roots may be resumed,
replayed, aggregated, or used in a metric.

The release-runtime context proof at
`live-release-context-1f5b57cf/PROOF.json` (SHA-256
`ae9f3ec5618c12ed6c0e7a35e7b6302485f036621b8137937632aebeca80d794`)
compiled all 670 resources for exact first case `19367bc7` in an ephemeral
database. Release recall returned one non-empty context item. The runtime
charged 32,757 conservative tokens and the pinned official Qwen processor
measured 23,564 exact tokens against the 32,768-token budget. No truncation
occurred. The queue ended with zero pending and zero dead jobs. Reader, judge,
and Deep endpoints and credentials were absent, so paid calls were zero.

The exact route proof at `live-reader-route-05e2bf66/PROOF.json` (SHA-256
`d5b322655b870d16ebab442d8ce602db8ef12107bf0ba4669308739e28aacd77`)
made one 64-token probe through the campaign's real reader proxy. It returned
HTTP 200 in 103.788 seconds on DeepInfra and canonical model
`qwen/qwen3.5-9b-20260310`. Its asynchronous generation receipt became
complete on bounded poll 6 and settled at $0.0000116 (12 micro-dollars) against
a 19-micro-dollar pre-dispatch reservation. The request was not replayed.

The campaign now uses a 600-second reader transport timeout and performs
receipt reconciliation only after the response path. Transport ambiguity
remains an explicitly unresolved liability and never authorizes a retry. The
runtime packs model-facing context using a byte-aware conservative estimate,
hard-splits oversized UTF-8 resource spans without loss, and never drops a
document tail at the episode-only 32-chunk cap. The LongMemEval adapter keeps
1 MiB source units: the measured full-corpus fan-out is 2,296 resources at
1 MiB versus 45,743 at 32 KiB, while the runtime owns bounded evidence chunks.

The $15 campaign ceiling includes all evidence already incurred. Prior
liability is frozen at 828 settled micro-dollars plus the original unresolved
3,084-micro-dollar upper bound, or 3,912 micro-dollars total. The fresh root's
48 reservations are capped at 14,995,200 micro-dollars, for a cumulative
maximum of 14,999,112 micro-dollars. Commit `354b2c3d` enforces the prior
liability at manifest verification, every pre-dispatch reservation, resume,
and aggregation.

A fresh root may therefore run in the frozen case-major order. It must build
and fingerprint its own release binaries, use one scratch database per row,
finalize every dispatch exactly once, stop before any reservation would cross
the cumulative ceiling, and preserve any failure without replay. This is
authorization to execute the preregistered n=12 feasibility screen, not a
promotion, product-default change, or SOTA claim.

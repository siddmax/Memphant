# P1-T6 pre-execution amendment 6

Date: 2026-07-20

The release-built execution root `run-ee1575a6` is benchmark-invalid and is
preserved without replay. Its Fast row compiled all 670 resources and completed
non-degraded recall, but the official Qwen processor measured 69,393 memory
tokens against MemPhant's 30,093 whitespace-token estimate. The official
32,768-token prefix pack could not fit the first returned item and therefore
delivered zero memory tokens to the reader. This is a failed treatment, not a
reader-quality result.

The same row made one reader dispatch. Its local proxy timed out at 300 seconds
without archiving a generation ID, so the original attempt retains its
conservative 3,084-micro-USD unsettled upper bound. It is never replayed or
reclassified from later evidence. The following Sonnet row was interrupted
before recall or model dispatch; all campaign and diagnostic processes exited
and both scratch databases were dropped.

One separately authorized exact-request diagnostic settled successfully on the
frozen DeepInfra bf16 route under the frozen no-fallback, no-collection, ZDR
policy. It produced generation `gen-1784567901-LHqqVWe148iK7Ll74yRd` with 181
prompt tokens, 5,533 completion tokens, and exact cost $0.000816. Generation
took 270.740 seconds, leaving too little operational margin under the proxy's
300-second timeout. The first receipt check was incomplete; the same generation
later reconciled to DeepInfra and canonical model
`qwen/qwen3.5-9b-20260310`. The policy is valid. The transport timeout and
synchronous receipt race are campaign defects.

A fresh campaign root is unauthorized until all of these predicates are proven:

1. runtime context packing uses a conservative model-token estimate rather than
   whitespace alone and cannot return one item larger than the request budget;
2. oversized paragraphs and trajectory observations are split into bounded,
   byte-exact contextual evidence with complete source-span coverage;
3. the exact first case produces non-empty, untruncated official Qwen memory
   context in a no-model release-runtime proof;
4. the reader timeout has measured safety margin beyond observed generation
   time and transport failures remain explicitly auditable; and
5. generation metadata is reconciled after the response path with bounded,
   fail-closed polling, then a tiny exact-route probe settles completely.

The invalid root is never resumed, aggregated, or used in a metric. The
diagnostic's $0.000816 is cumulative campaign cost, not evidence that the
original reader dispatch settled.

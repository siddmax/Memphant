# P1-T6 pre-execution amendment 8

Date: 2026-07-20

## Decision

Authorize exactly one fresh P1-T6 output root from the commit containing this
amendment. Do not resume or replay `run-ee1575a6` or `run-9a0ef780`; both roots
remain immutable invalid evidence.

The v6 LongMemEval-V2 adapter uses separate transport budgets for different
operations: routine packaged REST requests retain the 120-second deadline, while
the substantially heavier official benchmark recall receives a 600-second
transport margin. This is benchmark reliability policy, not a product latency
relaxation. Every row continues to record the observed `recall_duration_ms`, and
the product-facing Fast SLO remains unchanged.

## Evidence

- `run-9a0ef780` used packaged release binaries and compiled all 670 resources
  with zero worker failures. Its Fast recall then exceeded the generic
  120-second adapter deadline under severe unrelated host contention, before
  reader, judge, or Deep dispatch. The finalized settlement is zero and the
  interrupted row contains no route or generation receipt. Invalidation proof:
  `5b1aec4960d50a8538b23d7c92fd7474c6860a8ef7b59dc18c31570ed4d2f4ae`.
- The existing no-model release proof already establishes unchanged context
  semantics for the same first case: 670/670 compiled sources, no pending or
  dead jobs, one non-empty untruncated item, exact Qwen count 23,564 within
  32,768, and release-binary provenance. Proof:
  `ae9f3ec5618c12ed6c0e7a35e7b6302485f036621b8137937632aebeca80d794`.
- The transport-only fix is commit `19650b97`. The adapter SHA-256 is
  `4a30de55e439f2a430501047788dd3f7e95a94e594bb102f8b7219c249c6ee2a`;
  the schema-v6 adapter lock SHA-256 is
  `43540494d63b0bcc08d88edd6df763fc4ebbc9f243ec70d3bd8e38b9528ec7ce`.
- Focused adapter/campaign contracts passed 57 tests with one packaged
  integration skip. The full Python gate passed 581 tests with 12 skips.
  Private spec drift was not claimed: the private mirror is absent from this
  worktree.

Repeating the no-model cold build would re-run hours of unchanged release
compilation without exercising a different semantic path. The preserved release
proof plus the endpoint-specific regression test is the narrower, stronger
precondition for a fresh root.

## Cost and replay boundary

The campaign hard ceiling remains $15 cumulative. The preexisting conservative
liability remains 3,912 micros (828 settled plus 3,084 unresolved upper bound).
`run-9a0ef780` adds zero settled or unresolved provider liability; its second-row
87,400-micro reservation is an internal ceiling only and no external dispatch
occurred. The fresh root retains the previously frozen 14,995,200-micro maximum,
for a cumulative maximum of 14,999,112 micros.

Each fresh row still permits one dispatch per authorized route, requires receipt
settlement, forbids replay after transport ambiguity, uses a fresh migrated
scratch database, and fails closed before exceeding the cumulative ceiling.

## UX call

Fast remains the automatic default. Deep remains explicit, bounded, and never
automatically escalated. Cold compilation is benchmark/background work and must
not be reported as end-user recall latency. A wider correctness-campaign socket
margin prevents infrastructure noise from destroying accuracy evidence; it does
not excuse a slow product hot path or promote Deep.

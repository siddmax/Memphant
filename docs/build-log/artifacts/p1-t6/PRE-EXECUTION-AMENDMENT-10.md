# P1-T6 pre-execution amendment 10

Date: 2026-07-20

## Decision

Authorize exactly one fresh P1-T6 output root from the commit containing this
amendment. Never resume or replay `run-ee1575a6`, `run-9a0ef780`,
`run-3ae5833f`, or `run-b8dbf0a7`; all four roots remain immutable invalid
evidence.

Keep the frozen Qwen/DeepInfra reader route for benchmark comparability. Add
at most two retries, for three dispatch attempts total, only after an explicit
HTTP 429 or 503 response with no `X-Generation-Id`. Honor a positive numeric
`Retry-After` value between one and 60 seconds; otherwise use the frozen five-
and 15-second delays. Archive every rejection and dispatch count. Never retry
an ambiguous transport failure, a response carrying a generation ID, or any
other status.

This is the smallest durable retry boundary supported by OpenRouter's current
contract. Its error documentation identifies 429 and 503 as retryable and says
to honor `Retry-After`; its streaming documentation exposes
`X-Generation-Id` on every endpoint for request correlation. It also states
that an HTTP error is returned before model processing starts, while errors
after processing begins are returned in a 200 response body or stream. Sources:

- <https://openrouter.ai/docs/api/reference/errors-and-debugging>
- <https://openrouter.ai/docs/api/reference/streaming>
- <https://openrouter.ai/docs/api/reference/overview>

## Root-cause proof

- `run-b8dbf0a7` completed all 670 resources for its Fast row with zero worker
  failures. MemPhant returned one untruncated context item, 23,564 exact Qwen
  tokens and a 32,757-token runtime estimate, in 45,279 ms of adapter-measured
  recall time. The strict Qwen reader then received one DeepInfra HTTP 429
  `engine_overloaded` rejection before any generation ID was archived.
- The campaign stopped cleanly while the Sonnet row was compiling, before its
  first external dispatch. No campaign process or scratch database remains.
  Invalidation proof SHA-256:
  `b682a55b2b550277c6b5072dda5d7f858e7f1ee22cca8d4c21023408feb81c99`.
- The 3,271.389884-second official wrapper duration includes cold compilation
  and two deliberate host-yield pauses during unrelated load spikes. It is
  preserved for audit but excluded from product latency claims. The internal
  45,279 ms measurement excludes those pauses; it is valid development
  evidence for this 500-trajectory stress case, not a Fast product SLO or a
  benchmark promotion result.
- The immutable evidence is commit `d06ddea6`. The retry and liability fix is
  commit `7ad8ab71`. The amended manifest SHA-256 is
  `50a7f7243bd59fce29439cfada93df601eed8de388658c30fca960077360eae8`.
- The P1-T6 campaign contract passed 48 tests, including explicit 429 recovery,
  bounded 503 exhaustion, generation-ID no-replay, transport-ambiguity
  no-replay, and retry delay bounds. The full Python gate passed 585 tests with
  12 skips, and formatting passed. Private spec drift remains unclaimed because
  the private mirror is absent from this worktree.

## Cost and replay boundary

The fresh 48-row reserve remains 14,995,200 micros: 10,800,000 for bounded
Deep execution and 4,195,200 for reader and judge routes. Preexisting liability
is now carried conservatively as 320,666 micros:

- 4,524 settled micros from the previously settled diagnostics and routes;
- 316,142 unresolved upper-bound micros: the prior 303,084 micros plus the
  failed Fast row's full 13,058-micro reader reservation.

The full cumulative maximum is therefore 15,315,866 micros. The hard ceiling
remains $15.50, leaving 184,134 micros of headroom. The retries do not multiply
the accepted-generation reservation: only explicit pre-processing HTTP
rejections without generation IDs are replayable, and the accepted generation
must still settle against its one frozen row reservation. Any final rejected,
invalid, transport-unknown, or unsettled reader result remains conservatively
charged at the row's full reader upper bound.

Every fresh row still requires exact-route receipt settlement, forbids model or
provider fallback, uses a fresh migrated scratch database, and fails closed
before exceeding the cumulative ceiling. A product fallback policy may be
evaluated separately only after an explicit quality-equivalence proof; it must
not contaminate this benchmark's route identity.

## Long-term UX and systems call

Fast remains the automatic product default. Deep remains explicit, bounded,
cancellable through the existing task stream, and never automatically
escalated. Background ingestion and consolidation must be measured separately
from retrieval and downstream answer generation. Fast retains a sub-second
hot-path product objective; a 500-trajectory cold stress case is not permission
to normalize 45-second interactive recall. A 100-second-class operation is
acceptable only for user-requested Deep work with progress and cancellation.

This separation agrees with the 2026 systems evidence: LongMemEval-V2 reports
that coding-agent memory improves accuracy but remains latency-heavy, while the
2026 agent-memory systems characterization recommends phase-aware accounting
for construction, retrieval, and generation, with explicit scheduling,
amortization, and freshness-latency choices:

- <https://arxiv.org/abs/2605.12493>
- <https://arxiv.org/abs/2606.06448>

The authoritative optimization order is user-visible correctness first,
bounded total cost second, and latency third for explicit Deep. On the automatic
Fast path, latency becomes a correctness property of the UX: slow work must move
to background construction or an explicit task rather than silently blocking an
interactive request.

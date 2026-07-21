# P1-T6 pre-execution amendment 12

Date: 2026-07-21

## Superseding decision

This amendment supersedes amendment 11, checkpoint 12, and the dispatch
authorization for `run-129928d8`. That root is immutable diagnostic evidence
and must never be resumed or replayed.

The next executable gate remains exactly 12 Fast/Sonnet pairs: 12 local
pre-query constructions, 24 answer rows, and at most 12 Deep dispatches. Each
case is constructed once and cloned into its Fast and Sonnet arms. Luna and Sol
remain inactive shortlist metadata. No n=100-300 confirmation or full-500 run
is authorized here.

The frozen answer-blind selection and run order are unchanged:

- selection SHA-256: `d7762dbaffff7acfe779162d4993c8c09ef0440e3c1a25e0d3408127d73e25fa`;
- expanded run-order SHA-256: `59f7ff5cc04a6ecd1b17b69565e51a71ad1b814bfa1345698a10ad766796ad1e`;
- run-order contract SHA-256: `68d101847b268a252610737bfbff2d6cb099bf460c7c90ab12ba375f3053582d`;
- campaign manifest SHA-256: `12e063eac437332655b6f4f3a098d40d18a2b89af4ef8aa395a7d258b4942a19`.

## Root-cause and model-identity contract

The one-call synthetic stream diagnostic proved that OpenRouter sends the
registered canonical alias `anthropic/claude-sonnet-5` in SSE events while the
generation receipt records the exact requested snapshot
`anthropic/claude-sonnet-5-20260630`. The production parser previously compared
both surfaces to the exact snapshot and rejected a valid first tool call.

Commit `2e5c9bcd` fixes the shared runtime contract: request and receipt identity
remain pinned to the exact snapshot, streamed response identity is separately
bound to the canonical alias, and both are included in config hash
`d521ab622efb03a0ecf5b17c8b86fdc0944c3719fceb976b0a7dbce4e2313a7c`.
The same change makes a non-operational pair exit nonzero before the parent can
open another case and lets a later exact generation receipt settle only within
the immutable runtime reservation.

Evidence:

- `docs/build-log/artifacts/p1-t6/run-129928d8/INVALIDATION-PROOF.json`;
- `docs/build-log/artifacts/p1-t6/STREAM-DIAGNOSTIC-RESULT-63be29d1.json`;
- <https://openrouter.ai/docs/api/reference/streaming>;
- <https://openrouter.ai/docs/cookbook/administration/usage-accounting>.

## Cumulative hard-cap contract

Preexisting liability is 17,420 settled micro-dollars plus 316,142 unresolved
upper-bound micro-dollars, or 333,562 micros total. Fresh reservations remain
3,600,000 micros for 12 bounded Deep dispatches and 2,097,600 micros for 24
reader/judge routes. The cumulative maximum is therefore 6,031,162 micros,
below the 6,250,000-micro hard ceiling with 218,838 micros headroom.

The manifest and runner must carry the full settled and unresolved amount
before every reservation. No historical billable row may be rerun, and no
settled or unresolved amount may be reclassified downward.

## Efficient dispatch boundary

Before a fresh output root is authorized, run only the time-sensitive,
no-treatment checks: manifest verification, materialization and pairing proof
validation, exact endpoint inventory, presence-only `syndai/dev` credential
checks, packaged-binary fingerprints, local PostgreSQL 17 create/connect/drop,
and orphan-process/database cleanup. The live stream diagnostic plus the free
production-path fixtures already cover the corrected protocol, so another paid
route probe would add cost without new information.

Execution must preserve and stop on the first failed pair, cap, infrastructure,
security, write, settlement, or proof failure. A passing n=12 result authorizes
only preparation of a separately preregistered n=100-300 confirmation.

This amendment authorizes no treatment output by itself. It is not a promotion,
ledger closure, product-default change, public claim, merge, push, or authority
for a larger run.

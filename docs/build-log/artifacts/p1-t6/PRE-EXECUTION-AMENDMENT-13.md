# P1-T6 pre-execution amendment 13

Date: 2026-07-21

## Superseding decision

This amendment supersedes amendment 12 and the dispatch authorization for
`run-e511c817`. That root is immutable diagnostic evidence and must never be
resumed or replayed. Its first pair was non-operational, and the corrected
controller exited before opening case 2.

The next executable gate remains exactly 12 Fast/Sonnet pairs: 12 local
pre-query constructions, 24 answer rows, and at most 12 Deep dispatches. Each
case is constructed once and cloned into its Fast and Sonnet arms. Luna and Sol
remain inactive shortlist metadata. No n=100-300 confirmation or full-500 run
is authorized here.

The frozen answer-blind selection and run order are unchanged:

- selection SHA-256: `d7762dbaffff7acfe779162d4993c8c09ef0440e3c1a25e0d3408127d73e25fa`;
- expanded run-order SHA-256: `59f7ff5cc04a6ecd1b17b69565e51a71ad1b814bfa1345698a10ad766796ad1e`;
- run-order contract SHA-256: `68d101847b268a252610737bfbff2d6cb099bf460c7c90ab12ba375f3053582d`;
- campaign manifest SHA-256: `09d149423ad0ec1591f34a07bcc46b106a5c2111a043c6c1d8bb384c254b74c2`.

## Root-cause and multi-tool contract

Both failed production Sonnet generations completed tool use with identical
512-prompt/22-completion-token receipts, but the runtime rejected them before
tool iteration one. A bounded production-prompt/five-tool diagnostic passed
the single-call stream parser, excluding a general transport, route identity,
usage, or fragmentation defect.

The remaining request/parser contract was internally inconsistent. The frozen
Azure endpoint does not advertise the optional `parallel_tool_calls` parameter,
so `require_parameters=true` requires MemPhant to omit it. OpenRouter documents
that the omitted parameter defaults to parallel tool calls, while the runtime
accepted only one indexed call. ZDR correctly prevents recovery of the old raw
SSE chunks; the exact historical call count is therefore unavailable and is
not recreated by weakening privacy.

Commit `69ab5a54` fixes the durable contract. Contiguous indexed calls are
assembled across SSE fragments, executed deterministically in provider order,
returned as one matching tool message per call, and counted individually
against the existing 24-tool ceiling. Non-contiguous, incomplete, over-cap,
wrong-route, malformed, or unsettled responses still fail closed. This keeps
Azure/ZDR/parameter support strict while reducing serial model round trips,
latency, and spend. The new Sonnet config hash is
`a0163962e23e5f34bd1d48e82d149b88b59f0f224f7cd171a92853bde455aedb`.

Evidence:

- `docs/build-log/artifacts/p1-t6/run-e511c817/INVALIDATION-PROOF.json`;
- `docs/build-log/artifacts/p1-t6/STREAM-DIAGNOSTIC-2-RESULT-f32fdb37.json`;
- <https://openrouter.ai/docs/api/reference/parameters>;
- <https://openrouter.ai/docs/guides/features/tool-calling>;
- <https://openrouter.ai/docs/api/reference/streaming>.

## Cumulative hard-cap contract

Preexisting liability is 28,350 settled micro-dollars plus 316,142 unresolved
upper-bound micro-dollars, or 344,492 micros total. Fresh reservations remain
3,600,000 micros for 12 bounded Deep dispatches and 2,097,600 micros for 24
reader/judge routes. The cumulative maximum is therefore 6,042,092 micros,
below the 6,250,000-micro hard ceiling with 207,908 micros headroom.

The manifest and runner carry the full settled and unresolved amount before
every reservation. No historical billable row may be rerun, and no settled or
unresolved amount may be reclassified downward.

## Secret-free verification and efficient dispatch boundary

At commit `69ab5a54`, the no-paid gates are green: 698 Python tests passed with
12 skips; all Rust all-target/all-feature tests and doc tests passed with only
explicit live-provider/live-Postgres ignores; clippy with warnings denied is
clean; all three provider lints and migration dry-run are clean. The ignored
Postgres contracts then passed against one ephemeral PostgreSQL 17 scratch
database, and the real-binary e2e probe passed against another ephemeral
database. Neither path used Doppler or provider keys.

Before a fresh output root is authorized, run only the time-sensitive,
no-treatment checks: manifest verification, materialization and pairing proof
validation, exact endpoint inventory, presence-only `syndai/dev` credential
checks, release-binary fingerprints, local PostgreSQL 17 create/connect/drop,
and orphan-process/database cleanup. The fixture-driven production provider
test covers bounded multi-tool execution, so another paid protocol probe would
add cost without new information. Context construction inputs are unchanged;
the existing exact 23,564-of-32,768-token no-model proof remains valid and no
670-resource construction should be repeated merely for authorization.

Execution must stop on the first failed pair, cap, infrastructure, security,
write, settlement, or proof failure. A passing n=12 result authorizes only
preparation of a separately preregistered n=100-300 confirmation.

This amendment authorizes no treatment output by itself. It is not a promotion,
ledger closure, product-default change, public claim, merge, push, or authority
for a larger run.

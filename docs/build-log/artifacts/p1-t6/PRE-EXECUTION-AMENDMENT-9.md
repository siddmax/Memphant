# P1-T6 pre-execution amendment 9

Date: 2026-07-20

## Decision

Authorize exactly one fresh P1-T6 output root from the commit containing this
amendment. Never resume or replay `run-ee1575a6`, `run-9a0ef780`, or
`run-3ae5833f`; all three roots remain immutable invalid evidence.

The Deep request keeps `provider.require_parameters=true` and the frozen Azure,
privacy, price, model, iteration, context, wall-time, and spend constraints. It
now omits the optional `parallel_tool_calls` parameter because the frozen Azure
endpoints do not advertise it. Safety does not depend on that provider knob:
the response parser already rejects zero, multiple, interleaved, or malformed
tool calls. The exact request key set is now a regression contract, and this
transport semantic is bound into each candidate's cross-language config hash.

## Root-cause proof

- `run-3ae5833f` completed one valid Fast row, then compiled all 670 resources
  for the Sonnet row with zero worker failures before Deep recall returned the
  public `deep_unavailable` error. It was stopped before the Luna row could
  reach any external provider. Invalidation proof SHA-256:
  `3f5c5a3700e854882d80075fc73dda983d405cd7ae29c1667ff03b171bf56ec4`.
- The isolated exact-route control sent `parallel_tool_calls=false` with
  `require_parameters=true` and received OpenRouter HTTP 404, no generation ID,
  and the explicit `No endpoints found that can handle the requested
  parameters` error. The treatment omitted only that field and received HTTP
  200 from Azure, exactly one tool call, generation
  `gen-1784599051-POODgvPPgFyX8VwfgCjj`, and a settled 1,236-micro receipt.
  Diagnostic proof SHA-256:
  `76ff822930cdd8a96bf81fb4319cc83356d3168ef2d4bee3d2218d5a0b9084c3`.
- The live OpenRouter endpoint inventory exposes the exact Sonnet, Luna, and
  Sol Azure routes with `tools`, `tool_choice`, and
  `max_completion_tokens`; it does not advertise `parallel_tool_calls`. The
  campaign validates these material predicates before its production build.
- The runtime fix is commit `2fb9d03e`. The amended manifest SHA-256 is
  `2b22410f682834226e180c1eda2f649fcabf939e3ecb5d9a2336f955d5aefe15`.
  Candidate config hashes are Sonnet
  `22730027f29f7daa15b7b8905878ce6d9f45ee49491db415960f431da72bcf75`,
  Luna `bb4174d62de4083817d5fe4741ad12552e9c857abc7bb419b55c5898c335f6a9`,
  and Sol `028fcf5f3aeac5eb32a10cc3ad0095d48585e4c13cf97b77efe7591773b1770c`.
- All 33 Deep provider unit contracts passed. The P1-T6 campaign contract
  passed 44 tests. The full Python gate passed 581 tests with 12 skips, and
  formatting passed. Private spec drift remains unclaimed because the private
  mirror is absent from this worktree.

## Cost and replay boundary

The fresh 48-row reserve remains 14,995,200 micros: 10,800,000 for bounded
Deep execution and 4,195,200 for reader and judge routes. Preexisting liability
is now carried conservatively as 307,608 micros:

- 4,524 settled micros: the earlier 828 micros, the valid Fast reader's 2,460
  micros, and the isolated Deep route diagnostic's 1,236 micros;
- 303,084 unresolved upper-bound micros: the earlier 3,084 micros plus the
  invalid Sonnet row's full 300,000-micro Deep reservation.

The full cumulative maximum is therefore 15,302,808 micros. The hard ceiling is
amended from $15.00 to $15.50, leaving 197,192 micros of headroom. This is a
truthful $0.50 ceiling increase caused by preserved failed-attempt liability;
no per-row safety reserve is weakened and no old liability is reclassified as
zero.

Every fresh row still permits one dispatch per authorized route, requires
receipt settlement, forbids replay after transport ambiguity, uses a fresh
migrated scratch database, and fails closed before exceeding the cumulative
ceiling.

## UX call

Fast remains the automatic product default. Deep remains explicit, bounded,
and never automatically escalated. Cold corpus ingestion and compilation are
background work, not user-facing recall latency. The benchmark may tolerate a
100-second-class explicit Deep operation, but Fast must retain the sub-second
hot-path product objective; neither Qwen reader time nor shared-host pauses are
MemPhant recall latency.

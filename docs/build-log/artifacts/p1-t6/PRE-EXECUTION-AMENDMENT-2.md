# P1-T6 pre-execution amendment 2

Date: 2026-07-20

The first execution root, `run-d10ee907`, is infrastructure-invalid and is
preserved unchanged except for its append-only `INVALIDATION-PROOF.json`. The
official bootstrap exited with `ModuleNotFoundError: No module named 'agents'`
because the campaign used a Python interpreter without the official artifact's
declared `openai-agents` dependency. It finalized 17 operational-failure rows
and left row 18 staged before interruption. It produced no reader route, judge
route, Deep generation receipt, memory proof, official output, or official
score artifact; settled provider spend is zero. Its fail-closed internal
reservations remain recorded as conservative liability and are not reclassified
as provider spend.

The original root is never resumed, replayed, aggregated, or used in a metric.
A fresh output root is authorized by the predeclared infrastructure-retry rule
because `run-d10ee907/INVALIDATION-PROOF.json` proves that no generation was
accepted or billed while preserving every original attempt artifact.

Before the fresh root may exist, the coordinator now:

1. runs `pip check` under the exact sanitized child environment;
2. freezes the exact interpreter, upstream requirements hash, and complete
   installed package inventory into the pre-execution proof;
3. executes the pinned official bootstrap with `--help` under that exact
   environment and fails before endpoint checks, spend reservations, scratch
   databases, or model dispatch if any import is unavailable;
4. archives official-harness stdout and stderr inside every row; and
5. starts each scratch helper in its own process group, then terminates and
   reaps the complete group on interruption so the helper's database cleanup
   trap runs.

The isolated retry environment passed this no-spend bootstrap proof with Python
3.14.2, `openai-agents==0.18.3`, upstream requirements SHA-256
`939f04fd42c639d9d14120500c7a0170939dd3e47e19b86d5586a800b319d035`,
and full package-inventory SHA-256
`c7edd4eb8a99e19f94b527a7b9248a198262f1e2cb64efedab7bea5a2e1746e5`.
No fresh-root benchmark output was observed and no billable call was made before
this amendment.

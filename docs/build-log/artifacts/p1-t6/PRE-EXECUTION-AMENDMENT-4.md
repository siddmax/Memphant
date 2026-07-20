# P1-T6 pre-execution amendment 4

Date: 2026-07-20

The first paid execution root authorized after amendment 3,
`run-480c848e`, is benchmark-invalid and is preserved without replay. Its
append-only `INVALIDATION-PROOF.json` records one finalized Fast row and one
interrupted Sonnet staging row. The Fast row compiled all 670 retained
resources on their first worker attempts, then timed out after 120 seconds
inside the local MemPhant adapter before any reader route, judge route, Deep
generation, or official score. Settled spend and unsettled provider liability
are both zero, and the scratch database was dropped.

The campaign packaged `target/debug` binaries. That is a development build,
not the optimized binary profile users run in production, so its latency is
not a valid product or benchmark measurement. The root is never resumed,
aggregated, or used in a metric.

The repair makes production fidelity part of the frozen campaign contract:

1. server, worker, and CLI binaries are built with `cargo build --release`;
2. every row launches only the corresponding `target/release` binaries;
3. the pre-execution proof freezes the `release` profile and hashes those
   binaries; and
4. resume and aggregation fail closed if the profile or binary fingerprints
   drift.

A fresh paid root remains unauthorized until a separate no-model scratch proof
runs the pinned 500-trajectory case through the release server, CLI, adapter,
and real worker; compiles all 670 resources without failure; completes an
actual Fast recall within the campaign latency ceiling; verifies empty pending
and dead-letter queues; archives release-binary hashes; and drops its scratch
database. If that production-representative Fast recall misses the ceiling,
the runtime must be profiled and repaired before another paid row is attempted.

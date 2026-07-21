# Task 6 implementation report

- Base: `560fb79f2f7e9f22725cf8d230e9354b37255074`
- Commit: `cb322595e58f56f36f43c0204e30c9da600fae9b`
- Paid model calls: none
- STATUS/ledger changes: none
- Unrelated dirty handoff preserved: `docs/handoff/NEXT-SESSION-PROMPT.md`

## Outcome

Implemented the immutable exposed LongMemEval-V2 n=12 feasibility harness without credentials or paid dispatch. The campaign now has a frozen answer-blind selection source, corrected reproducible selection digest, dated model routes, strict route/usage/cost proofs, a pre-dispatch atomic liability ledger, per-row scratch databases and atomic result finalization, restart-safe acquisition/materialization, official reader/judge execution, and immutable aggregation/advance predicates.

The packaged adapter now binds server-issued context, ingests canonical trajectory resources with semantic state-boundary fragmentation, drains the packaged worker, archives full public recall and trace responses, and proves that recall changes only the one allowed `retrieval_trace` audit row. The runtime additionally rejects a paid response whose observed model differs from the exact configured dated route and preserves that liability as unsettled.

## Immutable local evidence committed

- `docs/build-log/artifacts/p1-t6/PRE-EXECUTION-AMENDMENT.md`
- `docs/build-log/artifacts/p1-t6/MATERIALIZATION-SUMMARY.json`
- `docs/build-log/artifacts/p1-t6/PAIRING-PROOFS.json`

The pairing aggregate fingerprints all 12 full per-question proofs, which in turn contain every trajectory row hash and fragment count. It verifies 7,934 fragments across 1,338 unique trajectories. No raw 1.2 GB source or materialized corpus was committed.

No-credential preflight verified the pinned code/data hashes, exact 12-case/48-row expansion, and materialization SHA-256 `35a696fef3a2efc217ed4a3c02eb87f50b8edc6c3439fb649f7c32c07a1f2b59`. Serialized retain sizes were max 1,266,859 bytes and p95 1,199,104 bytes, below both the 1.5 MiB campaign safety limit and effective 2 MiB Axum limit. Gold fields copied to memory: none.

## Fresh verification

- `python3 -m pytest tests/test_public_benchmark_adapters.py tests/test_run_lme_v2_p1_t6.py -q` -> 23 passed, 1 opt-in packaged/Postgres test skipped
- `cargo test -p memphant-runtime deep_recall_openrouter::tests --lib` -> 32 passed
- `cargo clippy -p memphant-runtime --all-targets --all-features -- -D warnings` -> passed
- `cargo fmt --check` -> passed
- `git diff --check` -> passed
- pairing evidence re-hash -> 12 proofs and 7,934 fragments verified

The live paid 48-row campaign was deliberately not run. Its staging and aggregation paths remain blocked on explicit credentials and operator authorization.

## Execution continuation - 2026-07-20

Operator authorization was exercised after the no-credential implementation
review. The first release-built Fast row was benchmark-invalid: runtime
whitespace accounting admitted 69,393 exact Qwen tokens and official prefix
packing produced empty context; its reader dispatch also crossed the former
300-second local timeout without an archived generation ID. No score from that
root is eligible, and its 3,084-micro-dollar liability remains unresolved.

The root fixes and their regression coverage are recorded in
`.superpowers/sdd/p1-t6-task-6-fix-report.md`. A release no-model replay of the
exact case now returns one item at 23,564 exact Qwen tokens (32,757 conservative
runtime tokens) with no truncation. A separate 64-token route probe settled on
DeepInfra without replay. Amendment 7 authorizes a new root under a cumulative
14,999,112-micro-dollar maximum. The 48-row screen itself remains pending and
this report does not claim a T6 win.

## Review package

- `.superpowers/sdd/review-560fb79f..cb322595.diff`
- SHA-256: `26db3d5391d33ebaff73ff7b75c3563c9f5e1e6d16353271ae69dbe75301c7c2`

## Corrected build-once paired gate - 2026-07-20

The 48-row model screen above is protocol-superseded. The active gate is exactly
12 Fast/Sonnet pairs: 12 constructions, 24 answer rows, and at most 12 Deep
dispatches. Luna and Sol remain inactive researched candidates and require a
new answer-blind amendment and root.

The stopped diagnostic root `run-408363c9` remains immutable and ineligible;
its invalidation proof SHA-256 is
`7e360eadceead985dbc729a935cbf8d276abde27a8b016c262b49c961a210bad`.
Its completed Fast canary is never replayed.

The approved build-once implementation now constructs each case once, seals a
crash-safe case bank, restores a fresh key-free source, and runs Fast and
Sonnet from distinct query-only PostgreSQL clones. Aggregation requires 12
unique construction proofs, 24 case/arm-bound clone identities, matching pair
bank seals, and zero arm-local retain or worker construction. Controller-owned
construction duration is sealed and reported separately from query recall and
generation latency/cost.

P1-T6 remains open. No n=12 result, ledger flip, confirmation authorization, or
product claim exists until all 12 immutable pairs pass every registered
operational, score, latency, cost, truncation, security, and settlement
predicate.

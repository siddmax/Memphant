# P1-T6 pre-execution amendment 14

Date: 2026-07-21

## Superseding decision

This amendment supersedes amendment 13 and the dispatch authorization for
`run-65981e4f`. That root is immutable evidence and must never be resumed or
replayed. Its Fast row (`0001-19367bc7-fast`) settled operationally; its Sonnet
row (`0002-19367bc7-sonnet`) is an immutable operational failure at recall.
Rows 3 through 24 never opened.

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

The frozen Sonnet config hash
`a0163962e23e5f34bd1d48e82d149b88b59f0f224f7cd171a92853bde455aedb`, the frozen
adapter SHA-256 `56c2a7264112e012f9376178cc2ef3383d70705f21255d4182e92bf62f8e206c`,
and the release-binary fingerprints from amendment 13 are all unchanged and
re-verified below. This amendment introduces no runtime, adapter, or contract
change; it re-authorizes a fresh output root after an infrastructure fault.

## Root-cause

`run-65981e4f` did not fail on model, route, parser, or contract grounds. The
Fast row completed and settled. The Sonnet row booted with `store=postgres`,
served for roughly two and a half minutes, then `/v1/recall` returned
`HTTP 503 {"error":{"code":"backend_unavailable"}}`. `server.stderr` holds only
`store=postgres`; the server logged no internal fault and correctly reported its
backing store as unavailable. `row-proof.json` records `observed_deep_config_hash:
null` and `deep_config_hash_bound: false`, so no Deep dispatch ever bound and no
billable model call was made on that row.

The cause was environmental: the local PostgreSQL container did not exist at
recall time (`docker ps -a` returned zero containers; `psql :5432` refused
connection). A server that boots, serves, then loses its backing store is a
database that vanished underneath a live server — not a boot race, not a leaked
port, and not a MemPhant defect. The harness correctly fail-stopped the pair
rather than fabricating a result. This is the same failure *family* the ops
gotchas warn about (a dependency disappearing mid-run), and the standing
remediation — check process trees, ports, and container state before dispatch —
was applied here before re-authorizing.

The container was restarted from `compose.yaml` (`pgvector/pgvector:0.8.4-pg17`)
and verified healthy (`pg_isready`, `select version()` → PostgreSQL 17.10). A
scratch database create/connect/`create extension vector`/drop smoke passed.

## Liveness-probe decision (recorded, not adopted)

A constructor-level database-liveness guard in the LongMemEval-V2 adapter was
implemented and then reverted. It was the wrong design: liveness is a property
of the run, not of adapter construction; placing a live-connection probe in
`MemphantMemory.__init__` broke fixture-based unit tests (which construct the
adapter against a stub `psql` and a placeholder `postgres://fixture/...` URL)
and would have changed the frozen `adapter_sha256`, invalidating every bound
construction proof, in exchange for a brittle check. The adapter remains
byte-identical to its pinned hash. The correct home for a pre-dispatch
reachability check is the runner/preflight, which already mints the scratch
database and sets `MEMPHANT_SCRATCH_ACTIVE=1` before any billable row; adding it
there is deferred as a non-blocking carry item, because the just-in-time
create/connect/drop smoke below already fails fast at authorization time and
Postgres is re-verified live immediately before dispatch.

## Cumulative hard-cap contract

The corrected campaign liability, reconciled against the on-disk settlements of
`run-65981e4f`:

- prior settled: 28,350 micros;
- `run-65981e4f` Fast settled: 2,421 micros; Sonnet settled: 0 micros;
- corrected total settled: **30,771 micros**;
- the Sonnet row's 300,000-micro Deep reservation was never drawn (no Deep
  dispatch bound) and is **released**, not carried;
- carried unresolved upper bound (prior diagnostic roots, unchanged): 316,142
  micros;
- corrected campaign liability before fresh reservations: **346,913 micros**.

Fresh reservations for this root are unchanged from amendment 13: 3,600,000
micros for 12 bounded Deep dispatches and 2,097,600 micros for 24 reader/judge
routes, totaling 5,697,600 micros. Cumulative worst case is therefore
346,913 + 5,697,600 = **6,044,513 micros**, below the 6,250,000-micro hard
ceiling with 205,487 micros headroom.

No historical billable row may be rerun. No settled amount may be reclassified.
The only downward change recorded here is the release of a reservation that was
provably never charged, evidenced by `observed_deep_config_hash: null` in the
Sonnet row proof.

## Secret-free verification

At commit `d2f4fcb3` (the docs-only handoff-header fix on top of the frozen
runtime commit `69ab5a54`; no non-docs file differs between them), the no-paid
gates are green: 697 Python tests passed with 12 skips; all Rust
all-target/all-feature tests and doc tests passed with only explicit
live-provider/live-Postgres ignores; `cargo fmt --check` and clippy with
warnings denied are clean; all three provider lints and the migration dry-run
are clean. The ignored Postgres contracts then passed against one ephemeral
PostgreSQL 17 scratch database, and the real-binary e2e probe passed against
another ephemeral database. Neither path used Doppler or provider keys.

The three release binaries were re-fingerprinted and match amendment 13
byte-for-byte:

- server SHA-256 `3ec9e6027d3ef6e2fdb45b501d29e7bd040261705bbf302638a8e67fffa7a231`;
- worker SHA-256 `d6eaa0522f5838346bc20495a55d1ab2e03b2c75733763a14d91d66dfff0978c`;
- cli SHA-256 `a8da5a706613654cfe743a55ea93b5c99702962067052503985c1c6dd1af3c0d`.

## Efficient dispatch boundary

Before a fresh output root is authorized, run only the time-sensitive,
no-treatment checks: manifest verification, materialization and pairing proof
validation, exact endpoint inventory, presence-only `syndai/dev` credential
checks, release-binary fingerprints, **local PostgreSQL 17 create/connect/drop
plus a live reachability probe of the base server immediately before dispatch**,
and orphan-process/database cleanup. The fixture-driven production provider test
already covers bounded multi-tool execution, so no additional paid protocol
probe is authorized. Context construction inputs are unchanged; the existing
exact 23,564-of-32,768-token no-model proof remains valid and no 670-resource
construction should be repeated merely for authorization.

Execution must stop on the first failed pair, cap, infrastructure, security,
write, settlement, or proof failure. A passing n=12 result authorizes only
preparation of a separately preregistered n=100-300 confirmation.

This amendment authorizes no treatment output by itself. It is not a promotion,
ledger closure, product-default change, public claim, merge, push, or authority
for a larger run.

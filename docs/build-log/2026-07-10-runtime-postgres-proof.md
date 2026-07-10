# 2026-07-10 — Runtime Postgres proof (WS-D / WS-H exit)

## Changed
Durable, authenticated runtime landed across nine commits (`6ff62b0`..`ee2d222` + this one):
evidence reset + promotion-provenance rule; migration 002 (types↔DDL reconciliation,
tri-domain resource identity incl. code `revision`, forgotten_source tombstones,
api_key with max_trust, job_state queue reuse w/ dead-letter, body_tsv GIN); typed
jiff clock (build-time `CURRENT_VALIDITY_CUTOFF` deleted); content-hash subject keys
(auto-keys never supersede); full `MemoryStore` repository seam; `MemoryService<S>`
application layer shared by REST/MCP/CLI/worker; API-key auth with tenant binding and
trust clamping; `PgStore` on sqlx 0.9; `memphant-runtime` (AnyStore selection);
real worker loop (SKIP LOCKED, dead-letter at 5 attempts, `MEMPHANT_WORKER_ONCE`);
embedding seam (fastembed feature, stub-verified vector channel, honest
`vector: disabled` under Noop); rmcp 2.2 MCP server (persistent stdio, camelCase
schemas); CLI memory verbs; SDK resource/unit payloads; honest website; Syndai
adapter/contract sync.

## Proof
- `bash scripts/e2e_probe.sh` (this commit) against live pgvector/PG17, 2026-07-10:
  retain → degraded read-your-own-writes → worker compile → recall → **restart both
  processes → recall persists** → cross-tenant trace **404** → correct → forget with
  `post_forget_recall_probe_hits=0` → re-reflect does **not** resurrect → code
  resource (uri+revision) retained and recalled as `kind=resource` → mark accepted →
  unauthenticated request **401** → health `{"store":"postgres"}`.
  Output: `E2E PROBE: ALL CHECKS PASSED`.
- `MEMPHANT_TEST_DATABASE_URL=… cargo test -p memphant-store-postgres -- --ignored
  --test-threads=1` → 11 passed (durability across pools, isolation, SKIP LOCKED
  claims, dead-letter, tombstones, pagination, key revocation, stub-embedding vector
  channel).
- Full local gate green: fmt, clippy `-D warnings` (all features), workspace tests
  (30 suites), doc tests, pytest 61, 3× `db lint`, spec-drift clean, migration
  dry-run.

## Status deltas
- WS-D and WS-H flipped: the packaged server, worker, MCP, and CLI all run against
  the Postgres stack they start (AnyStore from `DATABASE_URL`; MCP/CLI share
  `MemoryService<AnyStore>`).
- Banner advances to `RUNTIME COMPLETE — BENCHMARK EVIDENCE PENDING`: rungs 4–15,
  WS-F full cutover, WS-G relaunch, and the launch/restraint/GateMem gates remain
  open under the promotion-provenance rule (real corpora + executed scorers on this
  Postgres-backed runtime; synthetic fixtures gate regressions only).

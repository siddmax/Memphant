# Plan: R0 recall-ordering determinism

## Problem
Re-ingesting the same corpus into a fresh tenant mints new UUIDs; at every recall
ordering point where ties are broken by insertion order, tied candidates reshuffle
across runs → ±1-question variance on the 60Q docs gate.

## Fix — add content-derived (`body`) secondary sort key at the tie points that
## currently fall back to insertion order.

`crates/memphant-store-postgres/src/store.rs`:
1. L679 FTS:      `... desc limit 200`        → `... desc, body limit 200`
2. L696 recency:  `transaction_from desc ...` → `transaction_from desc, body ...`
3. L716 subject:  `limit 200`                 → `order by body limit 200`
4. L764 vector:   `<=> $4::halfvec limit N`   → `<=> $4::halfvec, unit.body limit N`

`crates/memphant-core/src/lib.rs`:
5. L1172 in-memory vector sort: add `.then_with(|| left.0.body.cmp(&right.0.body))`

## NOT touched (already stable / out of scope)
- Core fusion/rerank/pack sorts (L2170/2240/2311/2370/2870/3163) already `body`-tie-broken.
- `memory_unit` has no `content_hash` column → tie-break on `body` (same guarantee, no hash).
- Pagination cursors `order by id` (store L1299, lib L1488) — intentional uuid cursor.

## Regression test
`crates/memphant-store-postgres/tests/pg_store_contract.rs` (DB-gated, `#[ignore]`,
matches existing convention). Ingest a tiny corpus engineered to tie on the primary
sort key into TWO fresh tenants; run identical recalls; assert identical citation
(body) orderings. Post-fix the order is fully content-determined → tenants must agree.

Rationale for DB home: the in-memory store preserves insertion order (Vec), so a
DB-free test can't reproduce the SQL physical-order nondeterminism — it would be a
vacuous guard. The postgres contract suite is the faithful home.

## Gates
cargo fmt, cargo clippy, cargo test (compiles the ignored test; runs it if
MEMPHANT_TEST_DATABASE_URL is set).

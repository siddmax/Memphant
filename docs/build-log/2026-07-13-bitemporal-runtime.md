# Bitemporal runtime proof — 2026-07-13

## Outcome

MemPhant now exposes independent transaction-time and valid-time recall through
the existing recall verb. This is a runtime correctness proof, not a benchmark
promotion or SOTA claim. The 178-question development set does not contain the
mutation trajectories needed to measure this lever causally, so no reader score
or confirmation-set claim moved.

## Contract

- `transaction_as_of` selects what MemPhant knew at an audit snapshot.
- `valid_at` selects when the represented fact was true.
- Both predicates are half-open and are applied before per-channel top-N.
- Current-state correction splits the prior validity rectangle at one
  database-assigned transaction timestamp and preserves queryable history.
- Permanent forgetting removes the selected generation and its supersedence
  lineage from every transaction snapshot.
- REST and MCP derive the exact tenant, actor, and scope principal from the API
  key. A request cannot submit an alternate scope list; both-null keys are the
  explicit tenant-admin principal.
- Direct-unit retain accepts explicit valid intervals and rejects empty or
  inverted intervals.

The public OpenAPI and MCP artifacts were regenerated from their binaries. The
public and private mirrored specifications remain byte-consistent.

## Verification

Focused root verification:

```text
cargo test -p memphant-core --test bitemporal_recall --all-features
1 passed

cargo test -p memphant-server --test auth_contract --test rest_contract --all-features
20 passed

cargo test -p memphant-mcp --test mcp_schema_contract --all-features
3 passed

python3 scripts/check_spec_drift.py
spec_drift=clean
```

An independent read-only review also verified the PostgreSQL transaction clock,
correction rectangles, edge/review temporal predicates, lineage forgetting,
principal binding on all REST handlers and all seven MCP tools, GiST exclusion
constraints, RLS/grants, generated API artifacts, provider lint, migration
contracts, CLI verbs, and Python public surfaces. It reported no blocking
finding after 40 focused checks.

## Remaining evidence

This closes implementation and regression coverage for ordered Task-2 lever 4.
It does not freeze an agent-memory candidate. Promotion still requires a pinned
real-corpus temporal mechanism evaluation that separately reports maintenance
correctness, retrieved state/role correctness, and answer-time resolution.

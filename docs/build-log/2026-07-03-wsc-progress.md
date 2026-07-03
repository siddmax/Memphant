# WS-C Progress

## Changed

- Added the WS-C execution plan: `docs/superpowers/plans/2026-07-03-memphant-wsc.md`.
- Added recall request/response, channel, drop-reason, candidate, citation, context-item, dropped-item, and retrieval-trace types to `memphant-types`.
- Added `recall` in `memphant-core` over the deterministic `InMemoryStore` proof surface.
- Added retrieval trace persistence and `retrieval_traces` inspection for the in-memory store.
- Added Stage 0 scope denial behavior that writes a retrieval trace before returning a policy error.
- Added exact, lexical, vector, temporal, and edge candidate channels with deterministic scores.
- Added weighted RRF fusion with `k_rrf = 60`, budgeted packing, citation whitelist emission, and abstention flags.
- Added memory-unit evidence fields (`source_episode_id`, `source_resource_id`) and `deletion_generation` so recall can prove citation and tombstone filtering behavior.
- Added a typed memory-edge staging seam and unresolved contradiction suppression labels for packed units with `contradicts` edges.
- Added WS-C recall golden fixtures in `examples/evals/wsc-recall-goldens.json` covering answer-bearing recall, denied recall traces, tenant isolation, L1+ scope denial, citation whitelist, small-tenant filtered vector visibility, stale/deleted/invalidated/tombstoned suppression, edge contradiction warnings, and tight-budget packing.

## Proof

- `cargo test -p memphant-core --test recall_trace_golden recall_writes_trace_for_scope_denial`
  - RED result before implementation: compile failed because `recall`, `RecallRequest`, `RecallMode`, `RecallDropReason`, `CoreError::PolicyDenied`, and `retrieval_traces` did not exist.
  - GREEN result after implementation: `1 passed; 1 filtered out`.
- `cargo test -p memphant-core --test recall_trace_golden recall_golden_fixtures_pass`
  - RED result after adding the first answer-bearing fixture: compile failed because `NewMemoryUnit` had no evidence/source fields.
  - GREEN result after initial channel/fusion/packing/citation implementation: `1 passed; 1 filtered out`.
- `cargo test -p memphant-core --test recall_trace_golden recall_golden_fixtures_pass`
  - RED result after adding WS-C exit fixtures: failed because denied-scope units were excluded but not traced as `scope` drops.
  - GREEN result after filter/drop accounting: `1 passed; 1 filtered out`.
- `cargo test -p memphant-core --test recall_trace_golden recall_golden_fixtures_pass`
  - RED result after adding the deletion-generation fixture: compile failed because `NewMemoryUnit.deletion_generation` did not exist.
  - GREEN result after tombstone filtering: `1 passed; 1 filtered out`.
- `cargo test -p memphant-core --test recall_trace_golden recall_golden_fixtures_pass`
  - RED result after adding the edge/contradiction fixture: compile failed because `NewMemoryEdge` and `stage_memory_edge` did not exist.
  - GREEN result after edge-channel and suppression-label implementation: `1 passed; 1 filtered out`.
- `cargo fmt --check`
  - Result: passed.
- `cargo test -p memphant-core --test recall_trace_golden`
  - Result: `2 passed`.
- `cargo test -p memphant-core --test store_contract`
  - Result: `6 passed`.
- `cargo test -p memphant-core --test write_compiler_golden`
  - Result: `2 passed`.
- `python3 scripts/check_spec_drift.py`
  - Result before ledger sync: `spec_drift=clean public=/Users/sidsharma/Memphant/docs/superpowers/specs/memphant private=/Users/sidsharma/Syndai/.wt/codex-memphant-cross-repo/docs/superpowers/specs/memphant`
- `python3 scripts/check_spec_drift.py`
  - Result after ledger sync: `spec_drift=clean public=/Users/sidsharma/Memphant/docs/superpowers/specs/memphant private=/Users/sidsharma/Syndai/.wt/codex-memphant-cross-repo/docs/superpowers/specs/memphant`
- `python3 -m pytest tests`
  - Result: `16 passed in 0.25s`.
- `cargo clippy --all-targets --all-features -- -D warnings`
  - Result: passed.
- `cargo test --all-targets --all-features`
  - Result: passed; includes `recall_trace_golden` (`2 passed`), `store_contract` (`6 passed`), `write_compiler_golden` (`2 passed`), and `provider_lint` (`3 passed`).
- `cargo test --doc`
  - Result: passed doc tests for `memphant-core`, `memphant-eval`, `memphant-store-postgres`, and `memphant-types`.

## Status

WS-C is checked in `STATUS.md`. Current proof covers the in-memory core/fake read path and trace spine: every tested recall writes a retrieval trace, denied recalls write traces, answer-bearing IDs appear in the candidate whitelist, citations stay inside the whitelist, small-tenant filtered vector visibility is traced, denied-scope units are dropped with a controlled reason, deleted/invalidated/deletion-generation units are suppressed, unresolved contradiction edges produce suppression labels, and tight-budget packing keeps the answer-bearing unit while dropping over-budget decoys. Postgres-backed channel SQL, REST/MCP/SDK surfaces, and `memphant verify` remain later workstreams per `29`.

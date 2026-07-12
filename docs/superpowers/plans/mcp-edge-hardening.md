# MCP edge hardening (2026-07-11)

Execution plan for the 3 findings approved after the third audit round. All three
live at the MCP/recall edge (REST edge audited clean). Pre-production, no
back-compat, KISS/DRY, long-term best practice. Ordered low-risk first; each
verified before the next. Verify: `cargo fmt --check`, `cargo clippy --workspace
--all-targets --all-features -- -D warnings`, `cargo test -p memphant-mcp -p
memphant-core`.

## 1. Recall limit/budget clamp (trivial, core)
- [ ] `service.rs` recall: clamp caller `limit` and `budget_tokens` to defensive
      ceilings (const MAX_RECALL_LIMIT / MAX_RECALL_BUDGET_TOKENS) for symmetry
      with `scope_memory_handler`'s clamp. No allocation is driven by these, so
      this only rejects absurd values.

## 2. MCP error-string hygiene (security / info-disclosure)
- [ ] Add `mcp_error(ServiceError) -> String` in memphant-mcp lib.rs mirroring the
      REST edge: `CoreError::Store(_)` -> generic "backend unavailable" (hide raw
      sqlx/backend text); validation / not-found / policy / ServiceError::Invalid
      surface their (caller-safe) messages.
- [ ] Replace all seven tool `.map_err(|error| error.to_string())` with
      `.map_err(mcp_error)`. (bind_tenant errors are already safe Strings.)
- [ ] Unit test: Store(backend) -> "backend unavailable"; Invalid -> its message.

## 3. MCP HTTP per-request auth (security)
- [ ] lib.rs: pure, testable helpers `constant_time_eq(a, b)` and
      `mcp_http_authorized(dev_mode, expected_key, auth_header)` — dev mode allows
      all (auth explicitly disabled + loud); key mode requires
      `Authorization: Bearer <token>` equal (constant-time) to the process key.
- [ ] main.rs `run_streamable_http`: wrap `/mcp` with an axum
      `from_fn_with_state` layer that reads the Authorization header and calls
      `mcp_http_authorized`, returning 401 otherwise. State = {dev_mode from the
      bound tenant, MEMPHANT_API_KEY}. Closes the gap where any client reaching a
      widened `MEMPHANT_MCP_BIND` acted as the bound tenant with no transport auth.
- [ ] Unit tests for `mcp_http_authorized` (dev allow; missing header deny; wrong
      token deny; correct Bearer allow) and `constant_time_eq`.

## Not doing (recorded, per audit)
- Crate-wide clippy panic/indexing deny: false-positive magnet on internal code;
  request path already clean. Recommended against.
- event_outbox never written / dead tables (citation, scope_block,
  belief_observation): schema ahead of implementation; mark deferred, don't wire.
- Latent footguns (enum_str .expect; reflect-verb catch_unwind asymmetry): not
  triggerable on the prod config; leave.

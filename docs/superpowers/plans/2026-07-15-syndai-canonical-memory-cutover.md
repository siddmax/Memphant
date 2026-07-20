# Syndai Canonical Memory Cutover Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Memphant the sole canonical memory substrate for Syndai while keeping Syndai's product workflows and deleting the six approved legacy memory/graph tables after proof.

**Architecture:** One Memphant tenant represents one Syndai environment and one server-only service credential derives the tenant. Syndai users map to Memphant data subjects; server-resolved scope and agent policy gate every candidate. `MemoryContextLoader` remains Syndai's orchestration chokepoint, while one public-SDK adapter owns all substrate I/O.

**Tech Stack:** Rust, Axum, SQLx/PostgreSQL, generated OpenAPI/MCP JSON, Python/httpx SDK, Litestar/SQLAlchemy/Alembic, Flutter/Dart.

## Global Constraints

- No public graph traversal API, graph database, compatibility shim, permanent dual-write, or silent legacy fallback.
- No new dependency or service; reuse SQLx, Axum, the existing Python SDK, and Syndai's existing durable jobs.
- Tenant comes only from authentication; public bodies never accept `tenant_id` or `allowed_scope_ids`.
- Memphant RLS is tenant isolation; subject and scope isolation are enforced by server-derived predicates and tested against real Postgres.
- All mutating public endpoints honor `Idempotency-Key` for 24 hours and reject same-key/different-body replay.
- Existing user WIP in both dirty worktrees must remain untouched outside the exact paths named by a task.
- The approved drop set is exactly `syndai.memory_files`, `syndai.user_facts`, `syndai.episodic_memories`, `syndai.user_behavioral_embeddings`, `syndai.memory_entities`, and `syndai.memory_fact_edges`, each after its proof gate.
- Keep persona, proposal/review workflow, `memory_references`, timeline/digest presentation, `trajectory_events`, `failure_patterns`, and mobile Drift projections in Syndai.
- Fast recall remains p50 <200 ms and p95 <500 ms; Syndai context remains within existing per-layer limits and 2,500 tokens total.

---

### Task 1: Subject-bound identity and context bindings

**Files:**
- Modify: `memphant_migrations/versions/20260703_001_wsa_bootstrap.sql`
- Modify: `crates/memphant-types/src/lib.rs`
- Modify: `crates/memphant-core/src/lib.rs`
- Modify: `crates/memphant-core/src/service.rs`
- Modify: `crates/memphant-store-postgres/src/store.rs`
- Modify: `crates/memphant-server/src/lib.rs`
- Test: `crates/memphant-server/tests/auth_contract.rs`
- Test: `crates/memphant-store-postgres/tests/role_matrix.rs`

**Interfaces:**
- Produces: `SubjectId`, `AgentNodeId`, `ContextBindingRequest`, `ContextBindingResponse`, and `PUT /v1/context-bindings/{client_ref}`.
- Produces: internal `ResolvedMemoryContext { tenant_id, data_subject_id, actor_id, agent_node_id, scopes_by_kind, subject_generation }`.

- [ ] Add failing request-contract tests proving tenant/internal UUID/trust/level/allowed-scope fields are rejected, identical binding replay is stable, immutable parent/kind changes return conflict, and a scoped key gets `403`.
- [ ] Run the focused tests and confirm they fail because the binding route/types do not exist.
- [ ] Add subject ownership to scopes, episodes, resources, units, traces, jobs, and deletion generations in the squashed bootstrap; rename semantic `subject_key` to `fact_key`.
- [ ] Implement atomic external-ref resolution for subject, actor, scope, agent node, materialized path, level, and explicit policy rows. Remove implicit placeholder `ensure_scope`/`ensure_actor` creation.
- [ ] Implement per-kind scope resolution with exact-scope default, L0 root inheritance, L1 protected-kind denial, and explicit sibling grants only.
- [ ] Run auth, role-matrix, server, core, and Postgres contract tests until green.

### Task 2: Strict authenticated public request contract

**Files:**
- Modify: `crates/memphant-types/src/lib.rs`
- Modify: `crates/memphant-server/src/lib.rs`
- Modify: `crates/memphant-core/src/service.rs`
- Modify: `crates/memphant-cli/src/main.rs`
- Modify: `crates/memphant-mcp/src/lib.rs`
- Test: `crates/memphant-server/tests/rest_contract.rs`
- Test: `crates/memphant-server/tests/auth_contract.rs`
- Test: `crates/memphant-mcp/tests/mcp_schema_contract.rs`

**Interfaces:**
- Consumes: `ResolvedMemoryContext` from Task 1.
- Produces: strict public retain/recall/reflect/correct/forget/mark DTOs without `tenant_id` or engine-control flags.

- [ ] Add failing tests for unknown-field `422`, tenant derivation from the bearer key, wrong-body legacy payload rejection, and absence of engine toggles from generated schemas.
- [ ] Confirm failures name the stale request fields rather than unrelated serialization errors.
- [ ] Add `deny_unknown_fields`, remove public tenant IDs and engine toggles, and translate authenticated requests into internal tenant-bound commands.
- [ ] Keep engine levers only in internal eval configuration; remove caller-visible edge-expansion behavior.
- [ ] Update CLI-over-HTTP and MCP adapters to the same strict types; keep admin CLI tenant/key commands separate.
- [ ] Run focused REST/auth/MCP tests until green.

### Task 3: Provenance, idempotency, listing, and erasure

**Files:**
- Modify: `memphant_migrations/versions/20260703_001_wsa_bootstrap.sql`
- Modify: `crates/memphant-types/src/lib.rs`
- Modify: `crates/memphant-core/src/lib.rs`
- Modify: `crates/memphant-core/src/service.rs`
- Modify: `crates/memphant-store-postgres/src/store.rs`
- Test: `crates/memphant-core/tests/surface_mutations.rs`
- Test: `crates/memphant-core/tests/bitemporal_recall.rs`
- Test: `crates/memphant-store-postgres/tests/pg_store_contract.rs`

**Interfaces:**
- Produces: typed `source_ref`, `observed_at`, `subject_generation`, enriched `MemoryRecord`, `exact|subtree` scope listing, replace/invalidate correction, and unit/source/scope/subject forget selectors.
- Produces: 24-hour `(tenant, verb, key)` replay ledger with request hash and committed response.

- [ ] Add failing tests for retain replay/conflict, crash atomicity, source provenance, subtree listing, stale-generation rejection, replace/invalidate, and complete subject erasure across units, episodes, resources, edges, embeddings, jobs, citations, and traces.
- [ ] Run each focused test and verify the intended missing behavior fails.
- [ ] Implement one transaction per mutation plus idempotency record; return stored responses on identical replay.
- [ ] Increment subject generation during subject erase and reject any older write generation.
- [ ] Extend listing and recall records with the approved typed fields and immediate lineage only.
- [ ] Run mutation, bitemporal, store, and real scratch-Postgres tests until green.

### Task 4: Provider and migration truthfulness

**Files:**
- Modify: `scripts/apply_memphant_migrations.py`
- Modify: `scripts/check_memphant_live_catalog.py`
- Modify: `deploy/provider-profiles/supabase.env.example`
- Modify: `crates/memphant-cli/src/main.rs`
- Test: `tests/test_wsa_migration_contract.py`
- Test: `crates/memphant-store-postgres/tests/provider_lint.rs`

**Interfaces:**
- Produces: atomic migration execution, direct/session Supabase validation, exact-mode pgvector 0.8.x support, and HNSW floor >=0.8.4.

- [ ] Add failing tests proving a deliberately broken migration leaves no objects/ledger row, `:6543` is rejected for persistent SQLx runtime, exact mode accepts compatible 0.8.x, and HNSW rejects versions below 0.8.4.
- [ ] Implement `ON_ERROR_STOP` plus `--single-transaction` and capability-based vector checks.
- [ ] Change the Supabase example to direct/session `:5432`; keep runtime roles non-owner and non-`BYPASSRLS`.
- [ ] Run provider lint, bootstrap-check, migration dry-run, and scratch-Postgres role tests.

### Task 5: Generated surfaces and Python SDK

**Files:**
- Modify: `bindings/python/memphant/__init__.py`
- Generate: `openapi/memphant.v1.json`
- Generate: `mcp/memphant.tools.v1.json`
- Test: `tests/test_wsd_public_surfaces.py`
- Test: `crates/memphant-server/tests/rest_contract.rs`

**Interfaces:**
- Consumes: Tasks 1-4 public types.
- Produces: Python methods for context binding and all strict memory verbs; MCP remains limited to memory verbs and excludes context provisioning.

- [ ] Add failing SDK/OpenAPI drift tests for the context-binding route, strict request shapes, typed memory records, and missing graph/engine controls.
- [ ] Implement the smallest SDK wrapper over the generated contract and regenerate both JSON artifacts through server/MCP binaries.
- [ ] Run SDK, OpenAPI, MCP, and spec-drift checks until green.

### Task 6: Canonical Syndai adapter and real dogfood path

**Files:**
- Replace: `backend/src/features/memory/memphant_dogfood_adapter.py`
- Modify: `backend/src/features/memory/context_loader.py`
- Modify: `backend/src/config.py`
- Modify: `backend/src/features/memory/provenance_validator.py`
- Test: `backend/tests/contracts/test_memphant_openapi_contract.py`
- Test: `backend/tests/unit/features/memory/test_memphant_dogfood_adapter.py`
- Test: `backend/tests/unit/features/memory/test_context_loader.py`

**Interfaces:**
- Produces: one `MemphantMemoryAdapter` using the public Python SDK and namespaced context bindings.
- Produces: explicit `memory_degraded` context behavior and Memphant trace/unit citation authority.

- [ ] Add failing tests removing `tenant_id=user_id` and `allowed_scope_ids`, proving two users share one service credential without leakage, and proving outage behavior emits no citations or legacy read.
- [ ] Implement context binding, process-local resolved-ID caching, strict SDK calls, and trace-backed citation mapping.
- [ ] Replace file-specific flag/fallback behavior with required production config and explicit degraded runtime behavior.
- [ ] Add a real server + scratch-Postgres contract for bind -> retain resource -> recall -> trace -> correct -> forget and L1 protected-memory denial.
- [ ] Run the focused Syndai contract/unit suite and real dogfood contract until green.

### Task 7: Family cutovers and approved Syndai table drops

**Files:**
- Modify: `backend/src/features/memory/models.py`
- Modify: `backend/src/features/memory/memory_experience_controller.py`
- Modify: `backend/src/features/memory/fact_review.py`
- Modify: `backend/src/features/memory/episodic_service.py`
- Modify: `backend/src/features/memory/correction_service.py`
- Create: `backend/migrations/versions/2026_07_15_001_memphant_canonical_memory.py`
- Create: `backend/scripts/export_memphant_memory.py`
- Test: affected `backend/tests/unit/features/memory/` suites and migration contract tests.

**Interfaces:**
- Consumes: `MemphantMemoryAdapter` from Task 6.
- Produces: deterministic idempotent export manifests and a proposal-only Syndai fact-review model referencing Memphant unit IDs.

- [ ] Add failing family tests for agent resources, confirmed facts, direct imported episodes without re-extraction, behavioral belief/procedure mapping, correction, archive/invalidate, forget, digest/timeline reads, and 2,500-token context packing.
- [ ] Implement one family at a time: files, confirmed facts, episodes, then behavioral memory. Reuse existing durable jobs for agent-derived retries; explicit writes return `503` on dependency failure.
- [ ] Keep proposed fact content only in a proposal workflow table; on confirmation retain/correct Memphant and store the returned unit ID.
- [ ] Export by stable legacy source reference and content hash, compare counts/checksums/traces, then switch reads and writes.
- [ ] In the approved forward migration, drop exactly the four canonical memory tables only after the exporter proof marker is present; retain trajectory/failure operational tables and product projections.
- [ ] Run affected backend checks, migration DB contract, memory evals, and readback scripts.

### Task 8: Delete the legacy graph boundary and preserve product UX

**Files:**
- Delete: `backend/src/features/memory/graph_traversal.py`
- Modify: `backend/src/features/memory/memory_experience_controller.py`
- Modify/delete: registered Syndai graph tools and their tests after caller inventory.
- Modify: `mobile/lib/features/memory/data/repositories/memory_experience_repository.dart`
- Modify: `mobile/lib/features/memory/screens/memory_knowledge_tab.dart`
- Modify: `mobile/NAVIGATION.md`
- Modify: `mobile/TESTS.md`
- Test: affected backend graph/controller and mobile memory tests.

**Interfaces:**
- Keeps: immediate Memphant lineage in recall/list/trace.
- Removes: graph traversal endpoint, graph service/tools, `graphTraverse`, entity-only graph affordances, and `syndai.memory_entities`/`syndai.memory_fact_edges`.

- [ ] Add failing negative contract tests proving the graph route/tool/client method and public Memphant edge-expansion flag are absent while immediate lineage remains available.
- [ ] Remove every verified caller and update navigation/test contracts for the removed mobile surface.
- [ ] Repeat the live zero-row/no-caller proof, then drop the two approved graph tables in the Task 7 forward migration.
- [ ] Mark Memphant rung 6 `RETIRED`, not complete, with the real zero-benefit evidence.
- [ ] Run focused backend/mobile tests and repository guards for retired symbol names.

### Task 9: Integrated replacement proof and ledger reconciliation

**Files:**
- Modify: `docs/superpowers/specs/memphant/STATUS.md`
- Create: `docs/build-log/2026-07-15-syndai-canonical-memory-cutover.md`
- Create: proof artifacts under `docs/build-log/artifacts/` only from real command output.

**Interfaces:**
- Produces: the evidence required to close dogfood and each family cutover; it does not close unrelated launch/SOTA gates.

- [ ] Run two-user real-Postgres auth/policy/erasure tests, all family export/readback checks, trace/citation validation, restraint negatives, temporal fixtures, and hot-path SLO.
- [ ] Run the full Memphant gate from `AGENTS.md`, Syndai `make check`, affected mobile/web gates, and full Syndai preflight.
- [ ] Record exact commands, revisions, counts, latency, and failures; update only checkboxes supported by those artifacts.
- [ ] Run whole-diff engineering/security/code review and a Ponytail deletion pass; fix and re-run until clean.

## GSTACK REVIEW REPORT

### Engineering Review

- **Architecture:** Cleared after replacing the broken per-user-tenant mapping with one tenant-wide service credential, subject-bound context bindings, server-derived scope policy, and one canonical Syndai adapter.
- **Code quality:** Cleared with deletion-first constraints: no public graph API, no second graph store, no per-family adapters, no permanent fallback, and no new dependency.
- **Tests:** Cleared with explicit real-Postgres auth/policy/erasure, contract-drift, family export/readback, product behavior, and latency/restraint gates in Tasks 1-9.
- **Performance:** Cleared conditionally on the existing p50 <200 ms / p95 <500 ms fast-recall gate; subject/scope filtering must occur before candidate admission.
- **Destructive scope:** The user explicitly approved the six named table drops after proof. Operational and product-projection tables remain.

### NOT in scope

- Public or generic N-hop traversal — no consumer or measured value.
- A graph database — relational lineage is sufficient.
- Per-user API keys or OAuth delegation — the trusted Syndai backend is the current auth boundary.
- Direct web/mobile Memphant clients — clients continue through Syndai.
- Persona storage migration — persona remains product behavior in Syndai.
- Permanent dual-write or compatibility shims — pre-production cutover is family-by-family and destructive after proof.

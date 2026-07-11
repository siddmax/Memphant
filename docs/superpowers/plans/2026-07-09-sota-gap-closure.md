# MemPhant SOTA Gap-Closure Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn MemPhant from an in-memory prototype with fabricated benchmark evidence into a durable, authenticated, tri-domain (agents/documents/code) Postgres-backed memory service whose every public surface (REST/MCP/CLI/SDK/web) is honest and validated end-to-end.

**Architecture:** Expand the `MemoryStore` trait into a full repository seam; add a `MemoryService<S>` application layer used by REST, MCP, CLI, and worker (deletes the duplicated orchestration); implement `PgStore` with SQLx 0.9 runtime queries against the reconciled schema; keep deterministic ranking/policy pure in core over trait-fetched candidates. A new small crate **`memphant-runtime`** (depends on core + store-postgres) owns `AnyStore` (enum store selection from `DATABASE_URL`), embedding-provider construction, and `MemoryService` wiring — server/worker/mcp/cli depend on it (AnyStore cannot live in core: store-postgres already depends on core, and AFIT traits are not dyn-safe). Auth = API-key → tenant binding at the edge; caller-declared `source_trust` is CAPPED at the key's `max_trust` tier (trust is provenance-derived, not forgeable). Evidence ledger reset without deleting built rung machinery — and the reset executes FIRST (reopening fabricated promotions precedes feature work; the ledger must not stay falsely green while we build).

**Tech Stack:** Rust 1.96 / edition 2024, axum 0.8, sqlx 0.9 (runtime queries, postgres + runtime-tokio-rustls), rmcp 2.2 (MCP 2025-11-25), schemars 1, fsrs 6.6.1 (already pinned), pgvector 0.8.x (`halfvec`), fastembed 5.x behind an optional feature, Python 3 SDK (urllib), vanilla-JS web.

## Complaint-Driven Product Decisions (2026 field research)

Top user complaints across Claude Code memory, Mem0, Zep, Letta, ChatGPT, Copilot, Cursor (ranked by frequency×severity) and where this plan answers each:
1. Memory ignored at use time → budgeted, salient recall packs with citations (existing `budget_tokens` cap), never preamble dumps.
2. Extraction junk/duplicates (Mem0: 97.8% junk audit) → dedup + content-hash subject keys (T2), quality-gated writes stay (admission policy).
3. Memory silently lost (Cursor reload wipes, ChatGPT Nov-2025 incident) → durable transactional PgStore (T5), retain is all-or-error.
4. Deleted memories resurfacing (ChatGPT verbatim recall of year-deleted facts) → forget tombstones block re-derivation (T3/T4), episode-level forget, read-back verification in e2e (T12).
5. No provenance ("why does it remember this?") → every recall item carries citations to episode/resource spans; trace inspection endpoint stays.
6. Stale facts confidently reused → bitemporal generations + injected clock (T2); supersede-not-append for explicit subjects.
7. Token bloat (11–16k preambles) → hard token budget per recall; zero-cost when unused.
8. No control/kill switch → full CRUD verbs + tenant-bound inspection surfaces; API-key revocation (T4).
9. Memory poisoning (MINJA >95% injection) → trust tiers + quarantine already in core; retained as-is.
10. Ingestion lag / cold recall (Zep hours-later availability) → **read-your-own-writes: recall falls back to raw un-reflected episodes with `degraded: true` + `consolidation_lag` (spec 08 §4), implemented in T4** — a fact stored this turn is retrievable next turn.
11. Cross-user/project leakage (Copilot cross-session memories) → key-derived tenancy + scope binding on every query (T4/T5).
12. Lock-in/no export → `GET /v1/scopes/{id}/memory` paginated JSON is the programmatic export; CLI `verify --export` stays.

## Global Constraints

- Pre-production: **no backwards compatibility required**; delete/rewrite freely.
- Six public verbs stay: `retain, recall, reflect, correct, forget, mark` + `trace`/`scopes/{id}/memory` inspection (spec 08 §1; do not collapse to four).
- Retain is payload-dispatched: `episode | unit | resource` (spec 08 §209). Resource carries `uri, mime_type, content_hash, revision?` (revision = code commit identity).
- All tenant-scoped reads/writes MUST be tenant-bound server-side from the API key, never from client-declared body fields (body tenant_id allowed only if it matches the key's tenant).
- No hardcoded clock: `CURRENT_VALIDITY_CUTOFF` is deleted; a `Clock` trait is injected (SystemClock prod, FixedClock tests).
- The token-overlap channel is renamed `lexical`; the `vector` channel only reports scores when a real embedding provider is configured (default Noop = channel traced as `disabled`).
- Synthetic fixtures may gate regressions, never promotions (new ledger rule; STATUS rungs 4–15 + dogfood/restraint/GateMem/public-launch gates reopen).
- No new external datastores (no graph DB, no vector DB). Postgres only. No competitor code in the dependency tree.
- Workspace gate must stay green: `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test --all-targets --all-features`, `cargo test --doc`, 3× `memphant-cli db lint`, `python3 -m pytest tests/ -q`, `python3 scripts/check_spec_drift.py`, migration dry-run.
- Commits: small, per task, message prefix `feat(memphant):`/`fix(memphant):`/`docs(memphant):`; end with `Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>`.

---

### Task 0: Evidence reset FIRST (was Task 9 — execute before any feature work)

Run the full Task 9 content (ledger reopen, scorecard invalidation, provenance rule, meta-test updates, AGENTS.md fixes) as the FIRST commit. Rationale (outside voice, accepted): if promotions are known-fabricated, leaving STATUS falsely green through eight tasks violates the repo's own live-ledger rule (AGENTS.md line 3).

### Task 1: Reconcile types ↔ DDL and add tri-domain identity (migration 002)

**Files:**
- Create: `memphant_migrations/versions/20260709_002_runtime_reconciliation.sql`
- Modify: `crates/memphant-types/src/lib.rs` (StoredResource, StoredMemoryUnit, ReviewEvent, ForgetSelector, RetainRequest)
- Modify: `crates/memphant-store-postgres/src/lib.rs` (lint list includes new migration)
- Test: `crates/memphant-store-postgres/tests/provider_lint.rs`, `tests/test_wsa_migration_contract.py`

**Interfaces (Produces):**
- `StoredResource { id, tenant_id, scope_id, actor_id, uri, kind: ResourceKind, mime_type, content_hash, revision: Option<String>, source_trust, acl: serde_json::Value, created_at }` with `enum ResourceKind { Document, Code, Conversation, Other }` (serde lowercase).
- `ForgetSelector { memory_unit_id: Option<Uuid>, episode_id: Option<Uuid>, resource_id: Option<Uuid>, scope_id: Uuid }` — exactly one of the three ids required (validated in service).
- `ReviewEvent` keeps Rust shape `(trace_id, caller_id, used_ids, outcome)`; DDL replaced to match (one row per (trace_id, caller, unit)).
- Migration 002 (additive/rewrite, pre-production):
```sql
-- NOTE (verified against 001): episode/resource/memory_unit have COMPOSITE PKs (tenant_id, id).
-- Every FK below is therefore a composite (tenant_id, <col>) pair — bare `references t(id)` fails.
alter table memphant.resource
  add column actor_id uuid,
  add column mime_type text,
  add column revision text,
  add column body text,                       -- durable resource content; worker compiles from this (object storage later)
  add column source_trust text not null default 'untrusted';
create table memphant.forgotten_source (      -- tombstones: forget blocks re-derivation durably
  tenant_id uuid not null references memphant.tenant(id),
  source_kind text not null check (source_kind in ('episode','resource','memory_unit')),
  source_id uuid not null,
  forgotten_at timestamptz not null default now(),
  primary key (tenant_id, source_kind, source_id)
);
alter table memphant.forgotten_source enable row level security;
alter table memphant.memory_unit
  add column actor_id uuid,
  add column source_kind text,
  add column source_episode_id uuid,
  add column source_resource_id uuid,
  add column churn_class text,
  add constraint memory_unit_source_episode_fk
    foreign key (tenant_id, source_episode_id) references memphant.episode(tenant_id, id),
  add constraint memory_unit_source_resource_fk
    foreign key (tenant_id, source_resource_id) references memphant.resource(tenant_id, id);
-- (no freshness_due bool — 001 already has freshness_due_at timestamptz; Rust field becomes
--  freshness_due_at: Option<String> instead of freshness_due: bool)
-- Drop the REAL existing index by its REAL name (implementer: `grep -n subject memphant_migrations/versions/*001*.sql`
-- and use the exact name found, expected memphant_memory_unit_tenant_open_subject_idx). NOT `if exists` on a guessed name.
drop index memphant.memphant_memory_unit_tenant_open_subject_idx;
create unique index memphant_memory_unit_scope_subject_idx
  on memphant.memory_unit (tenant_id, scope_id, subject_key)
  where transaction_to is null and kind = 'semantic';
drop table memphant.review_event;  -- declared migration_kind: rewrite (see boundary-checker change below)
create table memphant.review_event (
  id uuid primary key default gen_random_uuid(),
  tenant_id uuid not null references memphant.tenant(id),
  trace_id uuid not null,
  caller_id text not null,
  outcome text not null check (outcome in ('success','failure','corrected','ignored')),
  created_at timestamptz not null default now(),
  unique (trace_id, caller_id)                -- idempotency preserved: one event per (trace, caller)
);
create table memphant.review_event_unit (     -- join table keeps empty-used_ids marks AND per-unit rows
  review_event_id uuid not null references memphant.review_event(id) on delete cascade,
  tenant_id uuid not null,
  memory_unit_id uuid not null,
  primary key (review_event_id, memory_unit_id),
  foreign key (tenant_id, memory_unit_id) references memphant.memory_unit(tenant_id, id)
);
alter table memphant.review_event enable row level security;
alter table memphant.review_event_unit enable row level security;
create table memphant.api_key (
  id uuid primary key default gen_random_uuid(),
  tenant_id uuid not null references memphant.tenant(id),
  key_hash text not null unique,
  label text not null default '',
  max_trust text not null default 'trusted_user',  -- server caps caller-declared source_trust at this tier
  created_at timestamptz not null default now(),
  revoked_at timestamptz
);
alter table memphant.api_key enable row level security;
-- Replicate 001's grant/policy pattern for EVERY new table (001's `grant on all tables` predates them):
-- grants to memphant_app / memphant_cron / memphant_readonly per the 001 §grants block, tenant indexes included.
alter table memphant.memory_unit
  add column body_tsv tsvector generated always as (to_tsvector('english', coalesce(body,''))) stored;
create index memory_unit_body_tsv_idx on memphant.memory_unit using gin (body_tsv);
```
Job queue: **reuse the existing `job_state` table from 001** (it already has state/run_after + index) — do NOT create a parallel `reflect_job` table. If `job_state` lacks `attempts int`/`claimed_at timestamptz`, add them in 002 via `alter table`. Dead-letter rule: `attempts >= 5 → state='dead'` (never re-claimed; count surfaced by `/v1/health`).
Contract test additionally asserts the OLD tenant-open-subject index is GONE (query pg_indexes in the live-catalog checker) — a silent `if exists` no-op must fail the test.
  (Belief/candidate kinds are dropped from the unique index — beliefs may hold multiple same-subject generations; supersedence for them is compiler-policy, not a DB constraint.)

**Steps:**
- [ ] Update Rust types as above; fix all compile errors across crates (mechanical field additions; default `ResourceKind::Other` where unknown). `StoredResource` gains `body: Option<String>`; `freshness_due: bool` → `freshness_due_at: Option<String>`.
- [ ] Extend `scripts/check_memphant_migration_boundary.py`: allow `drop table`/`drop index` ONLY when the migration file declares `-- migration_kind: rewrite` in its header (check_memphant_migration_class.py already classifies rewrite kinds); update its tests.
- [ ] Extend `scripts/apply_memphant_migrations.py`: record applied versions in `memphant.schema_migrations` (create if absent) and SKIP already-applied files — 002 is non-idempotent and the runner currently reapplies everything.
- [ ] Write migration 002 exactly as above (header: `-- migration_kind: rewrite`); register in `apply_memphant_migrations.py` ordering (directory glob—verify it picks up new file) and in store-postgres lint include list.
- [ ] Extend `tests/test_wsa_migration_contract.py` to assert: `memory_unit_active_subject_idx` contains `scope_id`; `review_event` has `trace_id`+`memory_unit_id`; `api_key`/`reflect_job` exist with RLS.
- [ ] Run: `python3 -m pytest tests/test_wsa_migration_contract.py -q` → PASS; `cargo test -p memphant-store-postgres` → PASS.
- [ ] Commit `feat(memphant): reconcile types/DDL, tri-domain resource identity, api_key + reflect_job tables`.

### Task 2: Clock injection + honest channel naming + subject derivation fix (core)

**Files:**
- Modify: `crates/memphant-core/src/lib.rs`
- Test: `crates/memphant-core/tests/surface_mutations.rs`, `crates/memphant-core/tests/write_compiler_golden.rs`, `crates/memphant-core/tests/recall_trace_golden.rs`

**Interfaces (Produces):**
- `pub trait Clock: Send + Sync { fn now_rfc3339(&self) -> String; }`, `pub struct SystemClock;` (uses `time`/`chrono` — pick whichever is already in the tree; if neither, add `jiff 0.2`), `pub struct FixedClock(pub &'static str);`
- **Typed time at the core:** `Clock` returns a typed instant (`fn now(&self) -> jiff::Timestamp`, or `time::OffsetDateTime` if `time` is already in-tree); serialization to the ONE canonical UTC string (`…Z`) happens only at the DTO edge via `fmt_rfc3339()`; ALL temporal comparisons on existing string fields go through `cmp_rfc3339(a,b)` which PARSES (never lexical) — PgStore reads `timestamptz` natively and formats at the edge. (Full typed DTOs deferred — NOT-in-scope.)
- **Auto-keys never supersede:** content-hash subject keys participate only in exact-duplicate dedup (`observation_count++`), never in the supersede path; subject-based supersedence requires an explicit subject/predicate. (Prevents both cross-content collisions and correction orphaning the chain.)
- All functions that stamped `CURRENT_VALIDITY_CUTOFF` take `&dyn Clock` (threaded via `MemoryService` in Task 4; free functions gain a `clock: &dyn Clock` param now).
- `RecallChannel::Lexical` replaces `RecallChannel::Vector` for the token-overlap scorer; `RecallChannel::Vector` is only emitted by the embedding path (Task 6) and otherwise appears in traces as `{channel: "vector", state: "disabled"}`.
- `derive_subject_key(scope_id, subject, predicate, body) -> String`: explicit subject/predicate → `"{scope_id}:{subject}:{predicate}"`; when absent → `"{scope_id}:auto:{blake3_or_sha256(body)[..16]}"` so distinct content never collides and identical content dedups. Handlers stop hardcoding `"retained episode"/"body"`; `RetainRequest` gains `subject: Option<String>, predicate: Option<String>` passed through to the reflect candidate.

**Steps:**
- [ ] Write failing tests first: (a) `two_trusted_retains_with_distinct_content_do_not_supersede` (retain two episodes w/o subject via the compiler path; assert both units Active); (b) `explicit_subject_updates_supersede_prior_generation` (same subject/predicate twice; assert one Active + one Superseded); (c) `unit_transaction_from_uses_injected_clock` (FixedClock("2031-01-01T00:00:00Z"); assert stamp); (d) rename Vector→Lexical assertions in `recall_trace_golden.rs`.
- [ ] Run `cargo test -p memphant-core` → new tests FAIL.
- [ ] Implement: delete `CURRENT_VALIDITY_CUTOFF`; add Clock; change `valid_for_query` to compare against `clock.now_rfc3339()`; update subject derivation + retain plumbing; rename channel enum variant.
- [ ] `cargo test -p memphant-core` → PASS (update pinned 2026-07-03 literals to FixedClock values — the tests keep determinism via FixedClock, not via a build-time constant).
- [ ] Commit `fix(memphant): injected clock, content-hash subject keys, honest lexical channel`.

### Task 3: Full repository seam — expand `MemoryStore`, port InMemoryStore

**Files:**
- Modify: `crates/memphant-core/src/lib.rs` (trait + InMemoryStore + rewire recall/correct/forget/mark/reflect to the trait)
- Test: `crates/memphant-core/tests/store_contract.rs`

**Interfaces (Produces):** async trait via native AFIT (Rust 1.96/edition 2024). AFIT traits are not object-safe — do NOT use `Arc<dyn MemoryStore>`. Dispatch statically: `MemoryService<S: MemoryStore>` stays generic, and the binaries select the store with an enum that itself implements the trait:
```rust
pub enum AnyStore { Mem(InMemoryStore), Pg(PgStore) }  // impl MemoryStore by delegation match
```
(server/worker/mcp construct `MemoryService<AnyStore>`; tests use `MemoryService<InMemoryStore>` directly.)
```rust
pub trait MemoryStore: Send + Sync {
    // existing staged-write API stays
    async fn fetch_recall_candidates(&self, tenant: Uuid, scopes: &[Uuid], kinds: &[MemoryKind], query_terms: &[String], query_vec: Option<&[f32]>, limit: usize) -> Result<Vec<StoredMemoryUnit>, StoreError>;
    // ^ candidate set is the UNION of: FTS top-N (via body_tsv/GIN), most-recent-M per scope, vector top-K
    //   (when query_vec given), and exact-subject matches — deduped by id. FTS-only prefiltering would starve
    //   the exact/temporal/edge/vector channels; the deterministic ranking in core needs all four families.
    async fn fetch_units_by_ids(&self, tenant: Uuid, ids: &[Uuid]) -> Result<Vec<StoredMemoryUnit>, StoreError>;   // edge expansion
    async fn fetch_edges(&self, tenant: Uuid, unit_ids: &[Uuid]) -> Result<Vec<StoredMemoryEdge>, StoreError>;
    async fn fetch_review_events(&self, tenant: Uuid, unit_ids: &[Uuid]) -> Result<Vec<ReviewEventRow>, StoreError>; // decay fold
    async fn fetch_episodes_for_scope(&self, tenant: Uuid, scope: Uuid, limit: usize) -> Result<Vec<StoredEpisode>, StoreError>; // L4/exhaustive + degraded fallback
    async fn pending_job_count(&self, tenant: Uuid, scope: Uuid) -> Result<usize, StoreError>; // non-mutating read-your-own-writes check
    async fn fetch_episode(&self, tenant: Uuid, id: Uuid) -> Result<Option<StoredEpisode>, StoreError>;
    async fn fetch_resource(&self, tenant: Uuid, id: Uuid) -> Result<Option<StoredResource>, StoreError>;
    async fn apply_correction(&self, tenant: Uuid, c: CorrectionWrite) -> Result<CorrectOutcome, StoreError>;
    async fn apply_forget(&self, tenant: Uuid, f: ForgetWrite) -> Result<ForgetOutcome, StoreError>;
    async fn record_review_events(&self, tenant: Uuid, ev: Vec<ReviewEventRow>) -> Result<(), StoreError>;
    async fn store_trace(&self, tenant: Uuid, trace: RetrievalTrace) -> Result<(), StoreError>;
    async fn trace_by_id(&self, tenant: Uuid, id: Uuid) -> Result<Option<RetrievalTrace>, StoreError>; // TENANT-BOUND
    async fn scope_memory_page(&self, tenant: Uuid, scope: Uuid, cursor: Option<Uuid>, limit: usize) -> Result<ScopePage, StoreError>;
    async fn claim_reflect_jobs(&self, filter: JobFilter, limit: usize) -> Result<Vec<ReflectJobRow>, StoreError>; // SKIP LOCKED in PG; JobFilter{tenant: Option<Uuid>, scope: Option<Uuid>} — service.reflect claims scope-filtered, worker claims unfiltered
    async fn complete_reflect_job(&self, id: Uuid) -> Result<(), StoreError>;
    async fn persist_compiled_units(&self, tenant: Uuid, w: CompiledWrite) -> Result<(), StoreError>;
    async fn upsert_embeddings(&self, tenant: Uuid, rows: Vec<EmbeddingRow>) -> Result<(), StoreError>;
    async fn lookup_api_key(&self, key_hash: &str) -> Result<Option<ApiKeyRow>, StoreError>;
}
```
`recall`, `correct_memory`, `forget_memory`, `record_mark`, `reflect_recorded` become free functions/service methods **generic over `S: MemoryStore`** (no more `store.inner.lock()` outside InMemoryStore's own impl). Ranking/fusion/packing stay pure functions over the fetched candidate vec (in-memory fetch returns all in-scope units; PG prefilters by FTS/kind).

**Steps:**
- [ ] Extend `store_contract.rs` with trait-level scenarios executed against `InMemoryStore` (candidates fetch respects tenant+scope; trace_by_id with wrong tenant → None; forget by episode_id hides derived units AND blocks re-derivation via a `forgotten` tombstone checked by `persist_compiled_units`).
- [ ] Run → FAIL; implement trait + port the five bypassing functions; keep public function signatures used by server/mcp until Task 4 swaps them.
- [ ] `cargo test -p memphant-core --all-targets` → PASS. Commit `feat(memphant): full repository seam; recall/correct/forget/mark/reflect go through MemoryStore`.

### Task 4: `MemoryService` + auth middleware + tenant-bound REST

**Files:**
- Create: `crates/memphant-core/src/service.rs` (`pub mod service`)
- Modify: `crates/memphant-server/src/lib.rs`, `crates/memphant-server/src/main.rs`
- Test: `crates/memphant-server/tests/rest_contract.rs` (+ new `crates/memphant-server/tests/auth_contract.rs`)

**Interfaces (Produces):**
```rust
pub struct MemoryService<S: MemoryStore> { store: Arc<S>, clock: Arc<dyn Clock>, embedder: Arc<dyn EmbeddingProvider> }
impl<S: MemoryStore> MemoryService<S> {
  pub async fn retain(&self, tenant: Uuid, req: RetainRequest) -> Result<RetainResult, ServiceError>;   // dispatches episode|unit|resource payloads
  pub async fn recall(&self, tenant: Uuid, req: RecallRequest) -> Result<RecallResult, ServiceError>;
  pub async fn reflect(&self, tenant: Uuid, scope: Uuid) -> Result<ReflectResult, ServiceError>;
  pub async fn correct(&self, tenant: Uuid, req: CorrectRequest) -> Result<CorrectResult, ServiceError>;
  pub async fn forget(&self, tenant: Uuid, sel: ForgetSelector) -> Result<ForgetResult, ServiceError>;   // real verification: re-runs recall probe, returns counts
  pub async fn mark(&self, tenant: Uuid, req: MarkRequest) -> Result<MarkResult, ServiceError>;
  pub async fn trace(&self, tenant: Uuid, id: Uuid) -> Result<Option<RetrievalTrace>, ServiceError>;
  pub async fn run_worker_tick(&self, batch: usize) -> Result<usize, ServiceError>;  // claims + compiles reflect jobs
}
```
- **Compile-order note:** Task 4 wires `MemoryService<InMemoryStore>` only (server stays in-memory at the end of T4); `AnyStore`/PgStore selection lands in T5's runtime crate; the `EmbeddingProvider` TRAIT + `NoopEmbedding` are defined in T3 (core) so T4 can hold the field — the fastembed impl is T6. Each task compiles green on its own.
- **Reflect races through one path:** `service.reflect(tenant, scope)` claims that scope's pending jobs via `claim_reflect_jobs` (same claim/complete path the worker uses) — the public endpoint and the worker can never double-compile a job; idempotency key `(job_id, compiler_version)` remains the backstop.
- Auth: axum extractor `AuthedTenant { tenant: Uuid, max_trust: TrustTier }` from `Authorization: Bearer mk_...` → sha256 → `lookup_api_key` (revoked_at set → 401). Missing/invalid → 401 envelope. Body `tenant_id` present and ≠ key tenant → 403 `tenant_mismatch`. Caller-declared `source_trust` is clamped to `min(declared, key.max_trust)` on every write — an authenticated caller can no longer forge `trusted_system`. Scope model this phase: the tenant is the trust boundary; `allowed_scope_ids`/`actor_id` are caller-asserted WITHIN the tenant (per-scope key grants are a stated follow-up, not silently trusted-forever). Dev bypass: env `MEMPHANT_DEV_TENANT=<uuid>` (logged loudly `AUTH DISABLED (dev)`) binds ALL requests to that tenant — body tenant_id is ignored in dev mode too, never honored.
- `GET /v1/traces/{id}` and `GET /v1/scopes/{id}/memory` now require auth and are tenant-bound (wrong tenant → 404).
- Retain accepts all THREE spec payload shapes (08 §209): `episode` (default), `resource` (`{uri, mime_type, content_hash, revision?, body}` → `enqueued: ["reflect_resource"]`), and `unit` (direct pre-compiled unit write for trusted callers: requires explicit subject/predicate + kind; `source_kind:"direct"`; admission trust policy still applies — untrusted keys get candidate tier, never semantic).
- **Provisioning:** retain auto-upserts `scope` and `actor` rows inside the store write transaction (`insert … on conflict do nothing`) — client-supplied scope/actor UUIDs must not require pre-registration. Tenants + keys are created via new CLI admin commands `memphant admin create-tenant --name X --database-url …` and `memphant admin create-key --tenant <uuid> …` (direct DB writes, prints the plaintext key once); the e2e probe uses these instead of raw psql.
- **Read-your-own-writes:** when recall finds no matching units AND the scope has pending reflect jobs, it falls back to matching raw episode bodies and returns them with `degraded: true` + `consolidation_lag_ms` (spec 08 §4 contract). Test: retain → recall immediately (no reflect) → episode body returned, `degraded: true`.
- server main: `DATABASE_URL` set → `PgStore::connect` (Task 5) else InMemoryStore with `warn!("EPHEMERAL in-memory store — set DATABASE_URL for durability")`.

**Steps:**
- [ ] Failing tests: `auth_contract.rs` — no key → 401; wrong-tenant body → 403; tenant-b key fetching tenant-a trace → 404; **revoked key (revoked_at set) → 401**; dev-mode env honors body tenant. `rest_contract.rs` — resource retain → reflect → recall returns `kind=="resource"` item; forget by episode_id → recall empty AND second reflect does not resurrect; **`GET /v1/scopes/{id}/memory` cursor pagination: page 1 + cursor → page 2, no overlap, `has_more` correct**.
- [ ] Implement service (move server/mcp duplicated orchestration into it — delete the copied ~65-line reflect blocks), auth extractor, route rewiring.
- [ ] `cargo test -p memphant-server -p memphant-core` → PASS. Regenerate OpenAPI: `cargo run -p memphant-server -- --openapi-json > openapi/memphant.v1.json` (now includes auth security scheme + resource payload). Update `tests/test_wsd_public_surfaces.py` expectations.
- [ ] Commit `feat(memphant): MemoryService application layer, API-key auth, tenant-bound traces, resource ingest`.

### Task 5: `PgStore` (SQLx 0.9) + durable worker

**Files:**
- Create: `crates/memphant-runtime/` (AnyStore enum, store selection from env, MemoryService construction, embedding provider seam — used by server/worker/mcp/cli).
- Modify: `crates/memphant-store-postgres/src/lib.rs` (+ `Cargo.toml`: `sqlx = { version = "0.9", features = ["postgres","runtime-tokio","tls-rustls","uuid","time","json"] }` — VERIFY exact feature names against docs.rs/sqlx 0.9 before committing; 0.8's combined `runtime-tokio-rustls` was split), keep `lint` module.
- Modify: `crates/memphant-worker/src/main.rs` (loop: connect store → `service.run_worker_tick(16)` every 500ms; SIGTERM graceful; `MEMPHANT_WORKER_ONCE=1` runs one tick and exits for tests).
- Modify: `compose.yaml` (server/worker now really consume DATABASE_URL; healthcheck on `/v1/health` which pings DB).
- Test: `crates/memphant-store-postgres/tests/pg_store_contract.rs` — gated `#[ignore]`-by-default unless `MEMPHANT_TEST_DATABASE_URL` set; CI/local runs it against dockerized pgvector.

**Interfaces (Consumes):** the Task 3 trait verbatim. FTS prefilter: `where tenant_id=$1 and scope_id = any($2) and kind = any($3) and transaction_to is null and (to_tsvector('english', body) @@ websearch_to_tsquery('english', $4) or $4 = '')` ordered by `ts_rank_cd` desc limit `$5`; when query terms empty, recency order. `claim_reflect_jobs`: `update memphant.reflect_job set claimed_at = now(), attempts = attempts+1 where id in (select id from memphant.reflect_job where completed_at is null and (claimed_at is null or claimed_at < now() - interval '5 minutes') order by created_at for update skip locked limit $1) returning *`.

**Steps:**
- [ ] Write `pg_store_contract.rs` mirroring the in-memory `store_contract.rs` scenarios (shared macro or duplicated file — duplicate is fine) + durability scenario: write via one pool, read via a fresh pool.
- [ ] Implement PgStore for every trait method; map rows ↔ Task-1 types; wrap writes in transactions; forget = `update ... set transaction_to = now(), state='deleted'` (column is `state`, not `status`) + delete embeddings + insert into `memphant.forgotten_source` (the durable tombstone `persist_compiled_units` consults).
- [ ] Add `memphant admin create-tenant|create-key|revoke-key` CLI commands (direct DB writes via the runtime crate; plaintext key printed once).
- [ ] Local validation: `docker compose up -d postgres && python3 scripts/apply_memphant_migrations.py --database-url postgres://memphant:memphant@localhost:5432/memphant && MEMPHANT_TEST_DATABASE_URL=... cargo test -p memphant-store-postgres -- --ignored` → PASS.
- [ ] Commit `feat(memphant): durable PgStore on sqlx 0.9 + SKIP LOCKED reflect worker`.

### Task 6: Embedding provider (optional real vectors)

**Files:**
- Modify: `crates/memphant-core/src/lib.rs` (`pub trait EmbeddingProvider: Send + Sync { fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbedError>; fn dimensions(&self) -> usize; fn id(&self) -> &str; }`, `NoopEmbedding`)
- Create: `crates/memphant-runtime/src/embeddings.rs` behind `feature = "fastembed"` (`FastEmbedProvider` bge-small-en-v1.5, 384d) — lives in the runtime crate so BOTH server and worker use it (compilation/embedding happens in the worker; a server-private module would be unreachable from it)
- Test: unit test with a `StubEmbedding` (deterministic hash-vectors) proving: embeddings persisted on compile; recall vector channel scores by cosine via `upsert_embeddings`/pgvector `<=>` in PgStore, in-memory cosine in InMemoryStore; Noop → trace `vector: disabled`.

**Steps:**
- [ ] Failing tests with StubEmbedding; implement trait + wiring in `persist_compiled_units` path + recall vector channel; PG side `order by embedding <=> $1::halfvec limit k` (cosine, matching `halfvec_cosine_ops` index and `embedding_profile.distance='cosine'`) merged into candidates. **Seed row:** when a non-Noop provider is configured, upsert its `embedding_profile` row (provider id, model, dims) on service startup — the `embedding` table FKs `(tenant_id, embedding_profile_id)` and Task 6 is unimplementable without it.
- [ ] Default build stays Noop (no network, no model download in CI). `cargo test --all-targets` → PASS. Commit `feat(memphant): embedding provider seam, real vector channel when configured`.

### Task 7: MCP rewrite on rmcp 2.2

**Files:**
- Rewrite: `crates/memphant-mcp/src/main.rs`, `crates/memphant-mcp/src/lib.rs` (+ Cargo.toml `rmcp = { version = "2.2", features = ["server","transport-io","transport-streamable-http-server"] }`)
- Test: `crates/memphant-mcp/tests/mcp_schema_contract.rs`
- Regenerate: `mcp/memphant.tools.v1.json`

**Interfaces:** 7 tools (`retain` incl. resource payload, `recall`, `reflect`, `correct`, `forget`, `trace`, `mark`) as `#[tool]` methods on a `MemphantMcp` struct holding `MemoryService` + fixed tenant from `MEMPHANT_API_KEY`/`MEMPHANT_TENANT` env (stdio is a per-principal transport). Persistent stdio session via `rmcp::serve_server`. Schemas camelCase (`inputSchema`) — assert in contract test. Delete `McpToolSpec` hand-rolled envelope; keep `--list-tools-json` for the committed artifact.

**Steps:**
- [ ] Contract test asserts: artifact JSON has `inputSchema` (camelCase) for all 7 tools; initialize→tools/list→tools/call retain→recall round-trip over an in-process rmcp client (or duplex stdio pipes) works WITHOUT closing stdin first; **missing `MEMPHANT_API_KEY`/`MEMPHANT_TENANT` (non-dev mode) → server refuses to start with a clear error, not an unauthenticated session**.
- [ ] Implement; regenerate artifact; `cargo test -p memphant-mcp` → PASS. Commit `feat(memphant): rmcp 2.2 MCP server, persistent stdio, camelCase schemas`.

### Task 8: CLI memory verbs + Python SDK update

**Files:**
- Modify: `crates/memphant-cli/src/main.rs` (add `retain|recall|reflect|correct|forget|mark|trace` as thin HTTP commands — all six verbs plus trace, matching the frozen contract; env `MEMPHANT_URL`, `MEMPHANT_API_KEY`; `retain --resource --uri --revision` supported), keep existing `lock/verify/compile/db` + Task-5 `admin` commands (incl. `admin revoke-key`).
- Modify: `bindings/python/memphant/__init__.py` (auth header actually meaningful now; `retain_resource(...)` helper; `trace(trace_id)` unchanged signature but server now enforces tenant).
- Test: `crates/memphant-cli/tests/http_verbs.rs` (spins the axum app in-process on a random port with dev-mode env; runs the binary via `assert_cmd`-style `std::process::Command`), `tests/test_wsd_public_surfaces.py` update.

**Steps:**
- [ ] Failing test: CLI `retain` then `recall` against in-process server returns the retained body; `forget` then `recall` empty.
- [ ] Implement with `ureq = { version = "3", features = ["json"] }` (cli-only dep; smaller than reqwest, blocking by design — right shape for a CLI). PASS → commit `feat(memphant): real CLI memory verbs; SDK resource support`.

### Task 9: Evidence reset (ledger, scorecards, meta-tests, provenance rule)

**Files:**
- Modify: `docs/superpowers/specs/memphant/STATUS.md` (reopen rungs 4–15, WS-F, WS-G, dogfood gate, restraint gate, GateMem gate; banner stays RUNTIME INCOMPLETE; add the provenance rule to the header), `docs/superpowers/specs/memphant/27-sota-ladder-and-validation.md` (§1 gains the rule: *"Promotion evidence must be produced by the packaged Postgres-backed runtime against pinned real corpora with recorded hashes and an executed reader/scorer. Synthetic fixtures gate regressions, never promotions."*)
- Modify: `docs/launch/public-launch-scorecard.json`, `docs/launch/restraint-launch-scorecard.json`, `docs/launch/gatemem-conditional-scorecard.json` → `"status": "invalid_synthetic_fixture"`, `"source_status": "fabricated_fixture_20260703"`, keep files as audit trail.
- Modify: `scripts/ingest_public_bench.py` — module docstring + emitted `source_status: "synthetic_contract_fixture"`; delete the hardcoded `passed: True`/`utility: 1.0` GateMem scorecard writer and the `ci: [1.0,1.0]` launch axes (the script now only emits fixture corpora, never scorecards).
- Modify: `tests/test_launch_evidence_contract.py`, `tests/test_public_launch_gate.py`, `tests/test_restraint_launch_gate.py`, `tests/test_gatemem_conditional_gate.py`, `tests/test_standing_quality_bars.py` — assert the *reset* state (gates open, scorecards marked invalid, no `passed: true` without `runtime: postgres` field).
- Modify: root `AGENTS.md` (fix line 16 tenant claim → describe key-derived tenancy; "SDKs"→"the Python SDK"; add: generated artifacts `openapi/memphant.v1.json` + `mcp/memphant.tools.v1.json` are regenerated, never hand-edited).

**Steps:** edit → `python3 -m pytest tests/ -q` PASS → commit `docs(memphant): evidence reset — reopen synthetic promotions, promotion-provenance rule`.

### Task 10: Honest website + wired actions

**Files:**
- Modify: `web/public/app.js`, `web/public/styles.css` (if present), `web/tests/launch-surface.spec.js`
**Changes:** quickstart shows only real commands (`docker compose up`, `curl -H "Authorization: Bearer $MEMPHANT_API_KEY" .../v1/recall`, real `memphant recall` CLI verb from Task 8); every page rendered from fixture is labeled "Demo data"; Correct/Forget buttons: if `window.MEMPHANT_API_BASE` is set → real fetch calls with key prompt, else disabled with tooltip "Connect an API base to enable"; fix ≤390px overflow (tables → `overflow-x:auto` wrappers); copy-trace handles clipboard rejection. Playwright: assertions updated (demo-data label present; buttons disabled-not-dead; 390px viewport project added asserting `document.documentElement.scrollWidth <= 390`).
**Steps:** failing playwright specs → implement → `cd web && npx playwright test` PASS → commit `fix(memphant): honest quickstart, wired/disabled actions, mobile overflow`.

### Task 11: Syndai side — mirror sync, phantom-mode fix, cross-repo contract test

**Files (in /Users/sidsharma/Syndai):**
- Sync: `docs/superpowers/specs/memphant/**` (byte-copy from MemPhant after Tasks 9 edits; run both repos' drift checks)
- Modify: `docs/superpowers/specs/memphant/07-syndai-integration-spec.md` **in MemPhant first** (remove "Python native binding" and "MCP dogfood" modes or mark `status: not-built`), then mirror.
- Modify: `backend/src/features/memory/memphant_dogfood_adapter.py` — point payload builders at the real contract (retain resource payload shape from Task 4; correct/forget selectors incl. `resource_id`).
- Create: `backend/tests/contracts/test_memphant_openapi_contract.py` — resolves the MemPhant repo via `MEMPHANT_REPO` env var, falling back to `<syndai_root>/../Memphant` (sibling-checkout convention from porting.md); if neither exists, the test FAILS with a skip-reason marker only under `MEMPHANT_CONTRACT_OPTIONAL=1` (CI without the sibling sets that var; local dev runs it for real). Validates adapter request/response fixtures against the schema (jsonschema already in Syndai deps — verify, else stdlib checks on required fields).
**Steps:** edit → `python3 -m pytest backend/tests/ -k memphant -q` PASS → `python3 scripts/check_spec_drift.py` green in both repos → commit in Syndai `fix(syndai): memphant adapter matches real contract; spec mirror synced`.

### Task 12: End-to-end validation gauntlet (the "API ready" proof)

**Files:**
- Create: `scripts/e2e_probe.sh` (repo-committed, used by CI later): boots compose (postgres+server+worker), applies migrations, creates 2 tenants + keys via `memphant admin create-tenant`/`create-key`, then curls: retain episode (A) → **deterministic worker step: run `MEMPHANT_WORKER_ONCE=1 memphant-worker` (single tick) instead of sleeping** → recall (A) hit → retain resource w/ revision (A) → recall kind=resource hit → trace fetch with B's key → expect 404 → restart server+worker → recall (A) still hit → correct → recall returns corrected → forget episode → recall empty → reflect again → still empty (no resurrection) → mark → `/v1/health` shows db ok. Exits non-zero on any failure, prints transcript.
**Steps:** run full workspace gate (Global Constraints list) + `MEMPHANT_TEST_DATABASE_URL=… cargo test -p memphant-store-postgres -- --ignored` + `bash scripts/e2e_probe.sh` → all green → commit `test(memphant): end-to-end durability/auth/tri-domain probe`. The Postgres-gated tests and e2e probe are part of THIS plan's done-definition (run in-session against dockerized pgvector), not deferred to future CI — "Postgres readiness" may not be claimed from the in-memory gate alone.

---

## Self-Review Notes

- Spec coverage: six-verb + resource dispatch (T4), tenant floor (T4/T5), durable kernel (T5), honest channels/flags (T2/T6), evidence reset + provenance rule (T9), honest web (T10), Syndai contract (T11), validation (T12). Deferred per critique: specialist code/doc rankers, learned rerank, graph DB, FSRS fitting (fields kept), adapters R79 pair (post-kernel), STATE-Bench runs (need real corpora + answer model budget — out of this session's scope, ledger stays open).
- Type consistency: `ForgetSelector`/`ResourceKind`/`MemoryService` signatures defined once in T1/T3/T4 and consumed verbatim in T5–T8, T11.
- Execution order: T0 (evidence reset) → T1→(T2+T3 as ONE branch phase — the Clock threading of T2 lands directly in T3's trait-generic signatures, no double churn)→T4→T5 strictly sequential (same crates); T6–T8 sequential after T5 (touch server/core lightly); T9, T10 parallel-safe with T6–T8 (disjoint files); T11 after T4 (needs OpenAPI) and T9 (mirror); T12 last. `memphant admin create-tenant/create-key` lands in T5 (PgStore needs it for its own gated tests; T8 only adds the memory verbs).
- NOT in scope (explicit deferrals with rationale): RLS policies + non-owner runtime role (tenancy is app-layer-enforced this phase; RLS-with-policies is the hosted-hardening follow-up — current RLS enablement is decorative and this is now stated, not hidden); full `OffsetDateTime` typing of DTOs (canonical-format + parsed-compare helpers close the misordering bug now); STATE-Bench/LME-V2 real-corpora runs (need answer-model budget + pinned corpora; ledger rows stay OPEN — this plan is the durable honest kernel those runs require, it does not claim them); learned rerank/FSRS fitting/graph engines (unchanged from critique); R79 adapters (post-kernel).

## GSTACK REVIEW REPORT

| Review | Trigger | Why | Runs | Status | Findings |
|--------|---------|-----|------|--------|----------|
| CEO Review | `/plan-ceo-review` | Scope & strategy | 0 | — | — |
| Codex Review | `codex exec` (outside voice) | Independent 2nd opinion | 1 | ISSUES_FOLDED | 23 findings (16 P1, 7 P2/P3) — all accepted ones folded into tasks |
| Eng Review | `/plan-eng-review` | Architecture & tests (required) | 1 | CLEAR (PLAN, auto-decide mode) | 8 issues (AFIT dispatch, FTS index, reflect race, revocation/pagination/MCP-key tests, ureq, complaints-driven RYOW) — all folded |
| Design Review | `/plan-design-review` | UI/UX gaps | 0 | — | Task 10 scope is corrective, not new design |
| DX Review | `/plan-devex-review` | Developer experience gaps | 0 | — | — |

- **CODEX:** cross-model pass found the composite-PK FK blocker, boundary-checker `drop table` conflict, crate cycle in AnyStore placement, FTS-only candidate starvation, unstored resource body, forgeable trust tier, migration-runner non-idempotency, review_event semantics change, and evidence-reset ordering — every accepted finding is folded into Tasks 0–12; rejected: none (finding 10's "defer MCP rewrite" softened to keeping T7 with the simplified env-key tenant model, per the user's explicit API-ready-everything mandate).
- **CROSS-MODEL:** Claude outside voice and Codex independently agreed on composite-PK FKs, wrong index name, missing tenant/scope/actor bootstrap, string-clock hazard, job-queue duplication, and FTS index absence — treated as settled, all folded.
- **VERDICT:** ENG + OUTSIDE VOICES CLEARED (auto-decide mode under explicit user delegation: "Make the decisions for what is best") — ready to implement.

NO UNRESOLVED DECISIONS

---

# 2026-07-10 Addendum — Accuracy Wave (evidence-synthesized one-shot plan)

Phase 1-2 above (runtime completion + evidence reset) are DONE. This addendum is the
next canonical wave, synthesized from an 11-lens research fleet (7 research lenses + 4
Syndai surface analyses; full reports in the session scratchpad, conclusions and
citations absorbed here). Authoritative; supersedes the handoff's step-2 lever list.

## Ground truth after the fleet

- Our LME-S 0.60 (n=100, k=10, terra reader) equals the paper's full-context GPT-4o
  baseline; the paper's optimized-retrieval condition reaches ~0.70-0.73; oracle 0.87.
  Vendor "90%+" claims are recall-metric or self-run harnesses; the independently
  reproducible band is 0.58-0.72. Target: ≥0.70 on our harness, then ONE
  published-protocol run (full 500q, canonical LongMemEval judge prompt) before the
  word "SOTA" is ever used. Until then the word is banned (devil's-advocate REAL-3).
- Failure modes under the promoted config (re-classified per question): preference =
  retrieval-miss of specific facts inside long sessions; temporal = composition with
  operands already packed; counting = split reader-undercount / dropped siblings;
  multi-session = cross-session assembly.
- Intent-vs-implementation debt found in core (all with file:line in the fleet
  reports): shipped server hardcodes NoopEmbedding (vector channel dead in prod);
  vector SQL missing embedding_profile_id predicate + app-side cosine recompute;
  fusion is dedup-truncate with hardcoded query-substring weight hacks; valid_from
  never read at recall; subject_key mostly an opaque hash; the falsified "rerank" was
  a hand heuristic, never a real cross-encoder.
- Syndai reality: knowledge tables are EMPTY (0 rows; schema complete: HNSW,
  BM25+RRF, Jina reranker option, text-embedding-3-small@1536); episodic memories
  have 100% NULL embeddings; the only substantial real corpus is 63.6k
  coding_execution_attempt_events. Mobile ships a rich Memory Hub consuming
  /api/v1/memory/* + /api/v1/facts*; web has NO functional memory UI at all.
  MemPhant /v1/recall dogfood path exists with pre-built, uncalled write adapters.

## Answers to the open questions (binding)

1. **Evaluate existing memory mechanisms?** Yes — as an engine-vs-engine gate, not a
   production-data mining exercise (no data exists to mine). Seed one real document
   corpus through BOTH Syndai's knowledge stack and MemPhant; mine a golden set from
   the corpus (cross-model generation, span-level grading, version-pinned, test-tenant
   exclusions per the backend report's recipe); MemPhant must beat Syndai's stack on
   it before any replacement. Episodic/behavioral/persona consolidation follows the
   docs gate; the coding-continuity lane (63.6k events) is first-mover — build after
   the gate harness exists.
2. **Rerank?** Re-test with a REAL cross-encoder (fastembed TextRerank) over a widened
   pool (64-128 → k) as a measured arm. The prior falsification indicted the heuristic.
3. **Embedding model?** bge-base-en-v1.5 arm now (one enum change + re-embed; profiles
   coexist). Qwen3-class embedders later only if the bge-base arm shows the channel is
   binding.
4. **Knowledge graphs?** No. Mem0's own ablation: +1.6pp at 2x tokens/3x latency and
   worse multi-hop. Our edges stay for supersedence/contradiction only. The
   chat-domain replacement for edge expansion is an entity-anchor boost — deferred
   until the levers below are measured.
5. **LLM at ingest?** Not in v1. Zero-cost deterministic writes are a differentiator
   (mem0/zep pay LLM per write). Extraction v1 is deterministic (preference/attribute
   pattern mining); an LLM extractor is a later measured experiment behind the same
   trait.
6. **w4 / 8192 / rerank-off / runtime-chunks defaults?** Confirmed; runtime-chunks
   default additionally subject to the held-out seed-20260711 confirmation + codex
   replication now running. If held-out fails to confirm B2, fall back to
   rendering-only (B1 shape) and re-measure.
7. **Chunks on non-chat kinds?** Unmeasured risk (REAL-2): the ≤32-window cap
   truncates >~128-line bodies. Fix: adaptive window growth (window size scales so 32
   blocks always cover the whole body); tri-domain ablation before any tri-domain
   accuracy claim.

## The wave (single SDD execution, sequential implementers, main branch)

- W1 Gate hardening: AGENTS.md pytest → directory discovery (`pytest tests/`), add
  pg `--ignored` line + e2e_probe to the gate; add the 4 smallest-closing tests
  (pg chunk round-trip, multi-tenant job-claim starvation, urlopen-mocked retry
  loop, worker-binary smoke).
- W2 Vector-channel honesty: shipped server gets real fastembed embeddings (kill
  NoopEmbedding default), embedding_profile_id predicate in vector SQL, fusion
  consumes SQL `<=>` scores (drop the second app-side fetch+recompute).
- W3 Fusion cleanup: remove query-substring weight hacks; clean weighted RRF over
  the four families; widened candidate pool (configurable, default preserved until
  measured).
- W4 Packing: sibling-gather (session-complete once any window of an episode packs)
  + per-session diversity quota keyed on source_episode_id.
- W5 Temporal grounding: populate validity from episode observation dates at
  reflect; extract_query_date + real valid_from<=q<=valid_to windowing at recall;
  date-stamp packed items with absolute episode dates (properly threaded
  first_observed_at — not the compile clock).
- W6 Extraction v1 (deterministic): compile_job emits extra ReflectCandidates for
  preference/attribute facts via pattern mining; honest subject_keys for them.
- W7 Reader routing: per-question-type prompt routing in run_reader (CoT to
  temporal/counting only) — harness-level, prompt_version=3.
- W8 Measured arms: bge-base embedding profile; real cross-encoder rerank flag.
- W9 Chunk safety: adaptive window growth replacing the truncating cap.
- W10 Syndai gate harness: corpus seeder + golden-set miner + Syndai-knowledge
  search adapter as a bench engine (engine-vs-engine runner).

Then ONE measurement campaign: all arms paired on dev seed 20260710 vs current
champion; winners confirmed on held-out 20260711 before promotion (two-seed rule is
now binding for promotions — forking-paths defense); promotions + STATUS + build-log
+ mirror in the same wave. Priorities: Accuracy/UX > cost > perf/latency > security
(RLS/roles stay queued, not in this wave).

## 2026-07-11 Round 2 — prosumer-reframed (canonical pointer)

The accuracy wave measured NO promotions (two-seed rule; `docs/build-log/2026-07-11-accuracy-wave.md`)
and the Syndai doc gate returned HOLD. Round 2 is pre-registered in
`docs/reports/2026-07-11-prosumer-memory-campaign-report.md` §6: dev n=300 (seed
20260712) / confirm full-500 with virgin-200 subset; levers R1 Chain-of-Note reader,
R2 preference profile block, R3 voyage-context-3 contextualized embedder (doc lane
first, re-run the gate), R4 temporal re-measure, R5 HyDE A/B, R6 months-scale
hygiene. Verbatim-is-the-memory rule adopted; prosumer scale retires the ANN/graph
work items.

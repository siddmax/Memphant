# MemPhant - Engineering Spec

## 0. Engineering Posture

Build the smallest Rust core that preserves the hard seams, but implement every SOTA-critical memory lever as a traceable mode from the first public architecture. The hot path is cheap; the benchmark/Deep path proves the ceiling.

### 0.1 What Rust actually buys (honest rationale)

Decision #2 is correct but must be justified for the *right* reasons, or it reads as premature optimization. The dominant cost on every target benchmark (LME-V2, BEAM, STATE) is **write-time LLM extraction and embedding network calls** — and **Rust buys nothing there**; a `tokio` task awaiting a provider is not faster than `asyncio` awaiting the same provider. The memory *model* and *extraction quality* — the part that actually moves accuracy — are language-agnostic. (A 2026 diagnostic, arXiv:2603.02473, also found *retrieval* method drives a ~20pt accuracy swing vs 3–8pt for *write* strategy on LoCoMo — so the cheap durable write path is the accuracy-correct call too, not only the latency one; spend the optimization budget on the retrieval-stage levers in `05` §1.4.)

Where Rust genuinely pays, and therefore why it is the core language:

1. **Single static-binary deployment** for self-host/BYOC (no runtime, no dependency hell).
2. **Eval/trace replay** over archived corpora — CPU-bound, embarrassingly parallel, run constantly; this is the real throughput win.
3. **Memory-safety at the multi-tenant boundary** — isolation bugs are the worst class of failure here.
4. **Deterministic, GIL-free parallel candidate generation + fusion** on the read path.

Rust does **not** make the memory model SOTA (invariant #12). The frozen interfaces are in Rust because retrofitting the static-binary/eval-replay/isolation properties later is the expensive migration — not because extraction latency is Rust-bound (it is provider-bound). Keep the core small precisely so the velocity tax lands only where Rust earns it.

**Preconditions on Decision #2 (R83 — Round-9 relitigation retained Rust with these made explicit):**

1. **The iteration-loop rule.** No accuracy-critical iteration may require a Rust recompile: extraction prompts, contradiction-judge policies, fusion weights, relevance/confidence thresholds, and eval fixtures are ALL versioned data/config (behind `compiler_version` / the checked-in weight tables / YAML), hot-loadable by the running binary. SOTA is won in the prompt-and-threshold loop; if that loop ever acquires an edit-compile-link cycle, Decision #2's cost has silently exceeded its price. Review-blocking rule, not a preference.
2. **The WS-0 two-language spike** (`29` §2) is the crux experiment: implement `retain` + the golden-runner in Rust and Python with the actual team; measure wall-clock to change an extraction policy end-to-end. <1.5× = Rust proceeds; ≥3× = re-open Decision #2 before WS-A freezes the workspace.
3. **Team Rust fluency is a named assumption** (`26`); a staffing change that invalidates it re-opens the decision rather than silently eating the velocity tax.

## 1. Repo Layout

```text
memphant/
  Cargo.toml
  LICENSE
  README.md
  crates/
    memphant-core/
    memphant-store-postgres/
    memphant-server/
    memphant-mcp/
    memphant-cli/
    memphant-eval/
    memphant-types/
  bindings/
    python/
  sdks/
    typescript/
  docs/
  examples/
```

## 2. Language Split

Rust:

- core types and policy checks
- retrieval fusion
- trace/event model
- SQL store adapter
- HTTP and MCP servers
- CLI
- eval replay runner

Python:

- ergonomic SDK
- notebooks and benchmark scripts
- integration examples with Python agent frameworks

TypeScript:

- HTTP SDK
- MCP/web examples

### 2.1 Package Matrix

| Artifact | Registry | Source |
|---|---|---|
| `memphant-core` | crates.io | Rust library types/policies/retrieval |
| `memphant-server` | Docker/GitHub Releases | Axum HTTP service |
| `memphant-cli` | crates.io/GitHub Releases | import/export/eval/debug |
| `memphant` Python package | PyPI | ergonomic pure HTTP client |
| `memphant._native` | deferred | PyO3 binding only after a real embedded/local API exists |
| `@memphant/sdk` | npm | generated TypeScript HTTP client |
| `memphant-mcp` | Docker/GitHub Releases | MCP stdio/Streamable HTTP server |
| harness provider adapters (`memphant-provider-hermes` first) | per-harness registry | activation-gated thin adapters mapping a harness memory-provider SPI onto the seven public verbs (`08` §5.1b, R87); never a parallel API |

HTTP is the primary integration path. Native Python is deferred until there is a real embedded/local API to expose; do not ship placeholder native packaging. The per-scope stats/block surfaces (`08` §2) ride the existing `memphant-server` — no new crate.

### 2.2 Python Layout

Required `pyproject.toml` shape:

```toml
[build-system]
requires = ["setuptools>=68"]
build-backend = "setuptools.build_meta"

[project]
name = "memphant"

[tool.setuptools.packages.find]
where = ["."]
include = ["memphant*"]
```

The Python package exports a normal HTTP client and must not reference `memphant._native` until native/local APIs exist.

PyO3/maturin remains an allowed future path, but the first public package is pure Python metadata plus the HTTP client.

Native Python entrypoints that run CPU-bound Rust retrieval, fusion, ranking, trace replay, or eval work must release the Python GIL around the Rust section. The Python binding is allowed to be ergonomic; it is not allowed to erase Rust's parallelism advantage.

### 2.2a Native Binding Contract (what is native, the GIL rule, the wheel)

- **Status:** deferred. Do not add native packaging until at least one native/local API is implemented and tested.
- **Exposed natively** (CPU-bound, local-embedding): `recall` fusion/rerank over fetched candidates, `eval`/trace-replay, decay recompute, local-text embedding. **HTTP-only:** anything needing hosted policy/auth, `reflect` (owns provider LLM calls), admin/forget. Importing `memphant` must succeed with no compiled extension and route those to HTTP.
- **GIL rule, concrete:** each `#[pyfunction]` entering a Rust hot loop wraps it in `py.detach(|| …)` (PyO3's current spelling of `allow_threads`); inputs convert to owned Rust types *before* detach, results *after* — no `Py<…>` touched inside. Pure-marshalling calls don't detach.
- **Test:** `tests/test_native_parallelism.py` spawns N threads each calling a detached native op and asserts **wall-clock ≈ max(call), not Σ(call)** — proving the GIL was released. A dropped `detach` fails this gate, not a subtle perf drift.
- **Wheel:** when native APIs exist, maturin builds an **abi3** wheel (`pyo3/abi3-py311`) — one wheel per platform spans 3.11+; manylinux + macOS arm64/x86 + Windows in CI. Until then, the package remains pure HTTP SDK metadata.

### 2.3 `memphant-core` Module Map

The one crate whose internal seams matter (every invariant lands here). Modules, not a plugin framework:

```text
memphant-core/
  policy/      # Stage-0 gate: resolve_policy(...) -> Policy; the 28 §2 chokepoint
  write/       # retain pipeline: dedup_key, trust prior, transactional enqueue
  reflect/     # consolidation stages (04 §9): extract, contradict, corroborate, promote, decay, trust
  retrieval/
    stages/    # one module per read stage (exact|lexical|vector|temporal|fusion|rerank|assemble)
    fusion.rs  # deterministic weighted RRF (05 §1.2), no provider call
    rerank.rs  # Reranker trait: deterministic default in-core; provider impls behind balanced/deep
  decay/       # thin wrapper over fsrs-rs: DSR fields <-> MemoryState; never reimplements the curve (04 §8)
  trace/       # RetrievalTrace builder; every stage appends, never the caller
  subject_key/ # the post-LLM canonicalizer (04 §3.3) — one code path for write and probe
  error.rs     # thiserror CoreError; converted once at the surface (§3.1a)
```

Trait seams are **only** `MemoryStore` (§4), `Reranker` (deterministic + provider), `EmbeddingProvider` (real + fake). `policy`/`subject_key`/`fusion` are pure/deterministic and own no I/O; only `write`/`reflect`/`retrieval/stages` touch `MemoryStore`. The decay wrapper exists so a FSRS weight-count bump (04 §8) is an `fsrs-rs` upgrade, not a refactor.

## 3. Rust Dependency Defaults

Use boring crates:

- `tokio`
- `axum`
- `serde`
- `sqlx` (with the `pgvector` type integration for `vector`/`halfvec` columns)
- `rmcp` (the official MCP Rust SDK — `02` §7; pin `features = ["server"]`)
- `schemars` (JSON Schema derivation for API + MCP `inputSchema`)
- `fsrs-rs` (DSR/FSRS decay kernel — do not reimplement the forgetting curve; `04` §8)
- `tracing`
- `opentelemetry`
- `uuid`
- `time`
- `thiserror`
- `clap`

No custom plugin framework in v1. Traits only where there are at least two implementations or a test fake is genuinely useful. `rmcp`'s `#[tool]` macro derives `inputSchema` but **not** `outputSchema` from the macro alone — attach each output schema explicitly from the canonical response type with `Tool::with_output_schema<T>()` where `T: JsonSchema` (`02` §7).

### 3.1 Error and Type Policy

- Public errors use stable machine codes.
- Internal Rust errors use `thiserror` and convert once at the surface.
- API structs derive `serde` and JSON Schema from the same canonical types where possible.
- Time uses `time::OffsetDateTime`; do not mix chrono unless a dependency forces it.
- IDs are UUIDv7 for public IDs and primary keys.
- Secrets never implement `Debug` with values.
- Any model/provider-specific enum includes an `other` escape hatch only at the API edge, not in core policy.

### 3.1a Error Taxonomy (the "convert once" boundary, concrete)

Core is one `thiserror` enum; it becomes a public `08` §2.1 code *only* at the surface (`memphant-server` handler, `memphant-mcp` tool, PyO3 boundary). A storage/policy module never constructs an HTTP status.

```rust
#[derive(thiserror::Error, Debug)]
enum CoreError {
  #[error("policy denied: {0:?}")] PolicyDenied(DenialReason),     // scope|tenant|policy
  #[error("not found: {0}")]       NotFound(EntityRef),
  #[error("conflict: {0}")]        Conflict(ConflictKind),         // already-deleted | idempotency
  #[error("invalid: {0}")]         Invalid(ValidationErrors),      // carries the field list
  #[error("consolidation lagged")] ConsolidationLagged{lag_ms:u64}, // NOT an error to the client
  #[error(transparent)]            Store(#[from] StoreError),
}
```

| `CoreError` | `08` code | HTTP |
|---|---|---|
| `PolicyDenied(Tenant\|Scope\|Policy)` | `tenant_denied`/`scope_denied`/`policy_denied` | 403 |
| `NotFound` | `not_found` | 404 |
| `Conflict(AlreadyDeleted\|Idempotency)` | `conflict`/`idempotency_conflict` | 409 |
| `Invalid` | `invalid_request` (`details.fields`) | 422 |
| `ConsolidationLagged` | `consolidation_lagged` | **200 + `degraded:true`** |
| `Store(Backend/Transport)` | `backend_unavailable` | 503 |

`ConsolidationLagged` is load-bearing: an `Err` *inside core* (the degraded branch, `02` §3.1) that converts to a **200** with `degraded:true` + `consolidation_lag_ms` at the surface. A `policy_denied` recall still writes a Stage-0-denial trace *before* the error converts — denial is auditable, not silent. `StoreError` never leaks a raw SQL string or memory snippet across the boundary.

## 4. Store Adapter Contract

The core talks to storage through one trait (shapes in `memphant-types`; `Result<_, StoreError>` elided):

```rust
#[async_trait]
trait MemoryStore: Send + Sync {
  // unit-of-work — opaque handle wrapping sqlx::Transaction adapter-side; core threads it
  // and commits, but CANNOT run SQL through it (the SQL-vs-core line holds).
  type Txn<'t>: Send where Self: 't;
  async fn begin(&self) -> Self::Txn<'_>;
  async fn commit(&self, tx: Self::Txn<'_>);                              // consumes; rollback on Drop
  // write — the atomic retain trio takes the unit-of-work, never self-commits (02 §3.0)
  async fn stage_episode(&self, tx: &mut Self::Txn<'_>, e: NewEpisode) -> RetainOutcome; // {episode_id, dedup:{matched, observation_count}}
  async fn stage_memory_unit(&self, tx: &mut Self::Txn<'_>, u: NewMemoryUnit) -> UnitId; // idempotent on (tenant, scope, dedup)
  async fn enqueue_reflect(&self, tx: &mut Self::Txn<'_>, j: ReflectJob) -> JobId;       // pgmq send IN-tx (14 §3.2)
  async fn write_edge(&self, tx: &mut Self::Txn<'_>, edge: NewEdge) -> EdgeId;           // ON CONFLICT (tenant,src,dst,kind) DO NOTHING
  // read — ONE call per channel; core fuses
  async fn recall_candidates(&self, q: ChannelQuery) -> Vec<Candidate>;
  //   ChannelQuery = Exact{subject_key} | Lexical{tsquery,k} | Vector{profile_id,qvec,k,index_strategy} | Edge{seeds,kinds,depth}
  //   Candidate { unit_id, channel, raw_score, rank_in_channel, kind, trust, subject_key, filter_selectivity: Option<f32> }
  // forget — the deletion_generation bump committed in this tx IS the saga's durable commit point (06 §6.2)
  async fn begin_forget(&self, sel: ForgetSelector) -> (Self::Txn<'_>, DeletionGeneration);
  async fn bump_deletion_generation(&self, tx: &mut Self::Txn<'_>, gen: DeletionGeneration) -> ForgetReport; // {units,embeddings,edges,resources,blobs}; blob deletes run AFTER commit as saga steps, reconciled by GC
  // blob GC seam — reference set + ledger, both Postgres-sourced (never object_store.list()), 02 §2.3
  async fn list_referenced_hashes(&self, tenant: TenantId) -> Vec<ContentHash>;          // live rows only
  async fn collectible_ledger_blobs(&self, tenant: TenantId, min_age: Duration) -> Vec<LedgerBlob>; // present & older than MIN_AGE
  async fn mark_blob_collected(&self, tenant: TenantId, h: ContentHash);
  // trace + profiles
  async fn write_retrieval_trace(&self, t: &RetrievalTrace) -> TraceId;   // always, even on a denied recall
  async fn list_embedding_profiles(&self, tenant: TenantId) -> Vec<EmbeddingProfile>;
}
```

**The SQL-vs-core rule (the line every adapter method honors):** the adapter owns *set retrieval and persistence* — the `WHERE tenant_id`/`scope_id` prefilter, `ts_rank`, the `<=>` distance, the per-`index_strategy` query path, `SET LOCAL hnsw.iterative_scan`, the migration mechanics. The adapter **never** ranks across channels, applies trust eligibility, assembles context, or decides promotion — those are core. `recall_candidates` returns a per-channel `rank_in_channel` + `raw_score`; **RRF fusion, the citation whitelist, and the eligibility label are computed in core, never in SQL** (a method returning a *fused* or *trust-filtered* list is a layering bug). The `Txn` handle is **opaque** — core holds it, threads it through the staging calls, and calls `commit`, but cannot run SQL through it (it wraps `sqlx::Transaction` adapter-side), so the unit-of-work seam does not leak SQL across the boundary; the in-memory fake implements `Txn` as a staged-op buffer applied on `commit`. The trait earns its single-impl existence as the **test-fake seam** (a deterministic in-memory `MemoryStore` lets the isolation/deletion/corroboration lanes assert policy without a live Postgres), not as a backend-portability layer.

**`sqlx` discipline:** adapter queries are compile-time-checked `query_as!`/`query!` with a committed `.sqlx/` cache so CI builds offline (`SQLX_OFFLINE=true`; `cargo sqlx prepare` in WS-A's exit packet). The vector stage's index-path selection is the **one** sanctioned runtime `query()` site (the SQL text varies by `index_strategy`), behind a builder that re-applies the tenant prefilter + the `embedding_profile_id` predicate so a dynamic path can never drop the isolation `WHERE` or the partial-index match. **Forward-compat exception (`25` §11c):** reads against the *frozen-interface* tables use runtime `query_as` with **explicit column lists** (never `SELECT *`, never the `query_as!` macro — which fails to compile against a self-hoster's newer additive column set) and read evolvable enum-like columns as `text` with a fallback variant, so a pinned binary tolerates a newer additive schema. The compile-time macro stays for internal, same-version, non-frozen queries.

First public implementation: Postgres only.

SQLite/PGLite is rejected for the public architecture. Local development uses Docker/plain Postgres so storage semantics, extensions, and query plans match production/eval behavior.

## 5. Schema Migration Rules

MemPhant has its own schema or database. If sharing Postgres physically, use `memphant`, not `public`, not `syndai`.

Rules:

- Every tenant table has `tenant_id`.
- No cross-FKs to Syndai.
- No app objects in `public`.
- Vector dimensions are tied to `embedding_profile_id`.
- Function `search_path` is pinned.
- FK indexes are explicit.
- Fresh bootstrap is tested from zero.
- Separate migrator, runtime, and read-only roles.
- Schema migrations live in `memphant.schema_migrations`, not `public`.
- All tenant hot-path indexes lead with `tenant_id` or a tenant-derived partition key.
- `vector`, `pg_trgm`, `ltree` (scope ancestor walk, `04` §11.0), and related extensions have an explicit provider strategy.
- Grants and default privileges are tested.
- **RLS is the default for hosted multi-tenant**, not a BYOC-only conditional. Tenant isolation is non-negotiable (sec-invariant #1), so defense-in-depth at the DB cannot be opt-in — application-layer `WHERE tenant_id` is the first line, RLS is the backstop. RLS may only be relaxed for a deployment that is provably single-tenant.

### 5.0a Migration Discipline (mirrors Syndai's verified mechanics)

The Syndai backend already enforces these and CI catches violations; MemPhant adopts the same mechanics so its schema is correct from the first migration (see `25` for the scaffolding plan):

- **Constraint/index names ≤ 63 bytes** (Postgres identifier limit; silently truncated otherwise → drift). A name-length check gates CI.
- **`CHECK` on a populated table is two-step**: `ADD CONSTRAINT … NOT VALID` then `VALIDATE CONSTRAINT` (avoids a full-table lock). Always `DROP … IF EXISTS` first for idempotency.
- **Index on a populated table is concurrent**: `CREATE INDEX CONCURRENTLY … IF NOT EXISTS` inside an autocommit block (not a transaction).
- **Every table gets `created_at DEFAULT now()` + an `updated_at` column with a shared `set_updated_at` trigger.** A validator catches missing triggers (Alembic-style autogen does not see triggers/CHECKs/partial-indexes/enums).
- **Descriptive long revision IDs** (text version column, not `String(32)`) so the migration ledger is human-readable.
- **Enum changes ship as CHECK-constraint edits**, ordered `drop old CHECK → UPDATE rows → add new CHECK`.
- **Apply-to-env-DB-before-push.** A green local build does not apply migrations; the live-DB contract check (`db_revision == expected_revision`) is what catches an unapplied migration before it becomes red CI.

### 5.1 Conceptual DDL

The **full typed DDL** for the load-bearing tables (`episode`, `memory_unit`, `memory_edge`, `embedding`) — column types, CHECKs, partial indexes, the `retention_tier`/`dedup_key`/`subject_key` columns — is owned by `04` §7. This section lists the complete entity set and the columns unique to the engineering view:

```text
memphant.tenant(id, slug, plan, region, created_at, updated_at)  -- region set at creation, IMMUTABLE (cell home; migration = export→import, never live copy; 25 §7b)
memphant.subject(id, tenant_id, external_ref, kind, privacy_policy, created_at, updated_at)
memphant.scope(id, tenant_id, parent_scope_id, kind, external_ref, materialized_path ltree, scope_depth, created_at, updated_at)  -- adjacency + cached ltree path (GiST @> ancestor walk); UNPARTITIONED tree (04 §11.0)
memphant.scope_policy(id, tenant_id, scope_id, kind, direction, min_level, grantee_scope_id, admit, …)  -- inheritance-policy object; deny-by-default; grant is an explicit row, never a memory_edge (full DDL 04 §11.0)
memphant.actor(id, tenant_id, kind, external_ref, trust_level, created_at, updated_at)  -- SOURCE/provenance identity (user/agent/tool/web/system); carries trust_level; drives source_kind + the 04 §5 corroboration-independence gate (distinct actor_id AND source_kind). NOT a tree node — most actors have no agent_node.
memphant.agent_node(id, tenant_id, scope_id, parent_agent_node_id, level, external_ref)  -- ACCESS-tree node (agents only); carries parent + level; drives 04 §11 inheritance/L0-L1+ gating. episode.agent_node_id is NULLABLE (not every actor is an agent); the read/recall path carries agent_node, the write/retain path carries actor.
memphant.episode(…full DDL in 04 §7: + retention_tier, dedup_key, observation_count, first/last_observed_at)
memphant.resource(id, tenant_id, scope_id, kind, uri, content_hash, acl, extractor_state)
memphant.memory_unit(…full DDL in 04 §7: + subject_key, stability_days, difficulty, reinforcement_count)
memphant.memory_edge(…full DDL in 04 §7: edge_kind ∈ supersedes|contradicts|derived_from|cites|same_subject|depends_on)
memphant.embedding_profile(id, tenant_id, provider, model, dimensions, distance, version,
                           index_strategy)  -- hnsw_full|hnsw_subvector|hnsw_binary|exact (02 §2.1a)
memphant.embedding(memory_unit_id, embedding_profile_id, tenant_id, vec halfvec)  -- one row per (unit, profile); resource chunks are kind='resource' memory_units (04 §6.1) → NO chunk dimension/table. vec DIMENSIONLESS (per-profile dim, partial index per embedding_profile_id; 02 §2.1a)
memphant.blob_ledger(tenant_id, content_hash, state, created_at, byte_len)  -- physical-blob enumeration for GC (02 §2.3); PK (tenant_id, content_hash); state ∈ present|collected; PARTITION BY HASH(tenant_id)
memphant.citation(id, tenant_id, memory_unit_id, episode_id, resource_id, span, quote_hash)
memphant.trust_event(id, tenant_id, target_kind, target_id, decision, reason_code, policy_version)
memphant.retrieval_trace(…full schema in 05 §3.1: + filter_selectivity, consolidation_lag, config_hash)
memphant.deletion_generation(id, tenant_id, scope_id, requested_by, state, completed_at)
```

`embedding_profile.index_strategy` is **not optional** — it is the column that selects the per-profile partial-index DDL (and prevents a >4,000-dim `halfvec` model from silently failing HNSW index creation; `02` §2.1a).

Invariant: a derived row that can affect recall must link back to an episode/resource or a typed deletion/privacy generation. A **resource chunk is a `kind='resource'` `memory_unit`** (`04` §6.1) — there is no separate chunk table or chunk key, so `embedding` keys on `memory_unit_id` only (the earlier `resource_chunk_id` was a ghost; deleting it avoids a frozen-PK re-key).

### 5.2 Worked `recall()` Algorithm

The staged read path (`02` §4), with the SQL each stage emits, so the implementation has one canonical reference:

```text
recall(query, scope, constraints, mode) -> RecallResult:
  # Stage 0 — gates (policy; cheap; fail-closed)
  policy = resolve_policy(tenant, scope, actor, level)        # 28 §2 chokepoint; resolves policy.scopes via
                                                              #   the 04 §11.0 ltree ancestor walk + scope_policy
                                                              #   (inherit rows + explicit grants), NOT a pre-materialized list
  assert policy.allows(scope, constraints)                     # else 403 + trace denied selectors

  # Stage 1 — exact/entity
  exact = SELECT id FROM memory_unit
          WHERE tenant_id=$t AND scope_id=ANY(policy.scopes)
            AND state='active' AND subject_key = $resolved_subject

  # Stage 2 — lexical (FTS, tenant-prefiltered GIN)
  lex = SELECT id, ts_rank(tsv, q) FROM memory_unit
        WHERE tenant_id=$t AND scope_id=ANY(...) AND tsv @@ plainto_tsquery($query)
        ORDER BY ts_rank DESC LIMIT $k

  # Stage 3 — vector (per index_strategy; set iterative_scan; capture filter_selectivity)
  SET LOCAL hnsw.iterative_scan = 'relaxed_order';            # 02 §2.1a — else silent under-recall
  vec = SELECT e.memory_unit_id, e.vec <=> $qvec AS dist
        FROM embedding e JOIN memory_unit mu ON mu.tenant_id=e.tenant_id AND mu.id=e.memory_unit_id
        WHERE e.tenant_id=$t AND e.embedding_profile_id=$pid   # profile predicate REQUIRED, else partial index skipped → silent seq scan (02 §2.1a)
          AND mu.scope_id = ANY(policy.scopes)                 # scope + resource.acl gate IN-stage, NEVER post-ANN (a post-filter is the RAG authz anti-pattern + re-opens the leak; 04 §6.1)
          AND mu.state='active' AND mu.transaction_to IS NULL  # current generation only (04 §7.3a)
        ORDER BY dist LIMIT $k
  # hnsw_binary is two-phase (binary prefilter → halfvec rerank):
  #   SELECT memory_unit_id FROM embedding WHERE tenant_id=$t AND embedding_profile_id=$pid
  #   ORDER BY binary_quantize(vec)::bit(D) <~> binary_quantize($qvec)::bit(D) LIMIT 200   → rerank by vec <=> $qvec LIMIT $k
  # hnsw_subvector reranks the subvector(vec,1,2000) prefix the same way (MRL models only)

  # Stage 4 — temporal/edge expansion (1-hop default)
  edges = expand(contradicts|supersedes|same_subject, depth = mode.edge_depth)

  # Stage 5 — RRF fusion across channels (deterministic)
  fused = rrf(exact, lex, vec, edges, weights = mode.weights)

  # Stage 6 — bounded rerank (deterministic default; ML only in balanced/deep)
  ranked = rerank(fused[:rerank_cap], mode.reranker)

  # Stage 7 — context assembly (budgeted; citeable units first; warnings)
  pack = assemble(ranked, budget = mode.budget,
                  flags = {stale, contradicted, consolidation_lag})

  # Stage 8 — durable trace (always)
  write_retrieval_trace(query_hash, config_hash, channels, candidates,
                        filter_selectivity, consolidation_lag, latency, cost)
  return pack
```

The live default path runs **no generative LLM** — Stage 6's default reranker is deterministic; provider rerankers are `balanced`/`deep`-only and trace-labeled.

## 6. Test Gates

Minimum local gates:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo nextest run --all-features
cargo test --doc
```

Python binding gate:

```bash
cd bindings/python && python -m pytest
```

Native performance gate:

```bash
cd bindings/python && python -m pytest tests/test_native_parallelism.py
```

This gate proves CPU-bound native calls can run in parallel from Python callers and do not hold the GIL across the Rust hot loop.

Eval gate:

```bash
cargo run -p memphant-eval -- run examples/evals/golden.yaml
```

DB gate:

```bash
cargo run -p memphant-cli -- db lint --url "$DATABASE_URL"
cargo run -p memphant-cli -- db bootstrap-check --provider plain-postgres
cargo run -p memphant-cli -- db bootstrap-check --provider neon
cargo run -p memphant-cli -- db bootstrap-check --provider supabase-byoc
```

Release gate:

```bash
cargo run -p memphant-eval -- run benchmarks/release.yaml --archive-traces
# the whole 9-axis SOTA profile in one command (12 §2.0a) — orchestrates run/ablate/compare/security:
cargo run -p memphant-eval -- profile --config memphant.lock --compare-to baselines/release.yaml --archive-traces
```

### 6.1 Named Test Lanes

Each lane maps to an invariant and runs at a defined cadence. Naming them makes coverage auditable (mirrors Syndai's named-eval discipline) — the lanes are *declared here as a spec contract*; the executable suites themselves are build-time work.

| Lane | Asserts (invariant) | Cadence |
|---|---|---|
| `memphant-tenant-isolation` | no recall returns another tenant's memory; RLS backstop holds (sec #1) | every PR |
| `memphant-scope-inheritance` | child agent_node never receives parent-only memory (inv #10) | every PR |
| `memphant-deletion-completeness` | forgotten content unreachable via lexical/vector/cache/resource (inv #7) | every PR |
| `memphant-citation-forgery` | model cannot cite IDs outside the candidate whitelist (inv #5) | every PR |
| `memphant-contradiction-detection` | conflicts are *derived* (not pre-annotated) → edge written (inv #6, `04` §3.1) | every PR |
| `memphant-corroboration-farming` | K reinforcing low-trust obs from one origin never promote (inv #3, `04` §5) | every PR |
| `memphant-stale-fact` | newer valid evidence outranks old (inv #6) | every PR |
| `memphant-poisoning` | low-trust web/tool output stays quarantined/labeled (inv #3/#4) | every PR |
| `memphant-small-tenant-recall` | recall@k holds for a small tenant in a large shared corpus (`02` §2.1b) | nightly |
| `memphant-deletion-generation` | concurrent forget + write race stays isolated | nightly |
| `memphant-scope-grant-admit` | the POSITIVE grant path: every explicit `scope_policy` grant row `(grantee, kind, ≥level)` is actually reachable — a grant that silently admits nothing fails (dual of `-scope-inheritance`, which only proves the deny side; R84) | every PR |
| `memphant-blob-gc-fence` | a blob whose `blob_ledger` row lands after a sweep's mark-begin is uncollectible (the mid-sweep race, `02` §2.3) — concurrency test, not prose | nightly |
| `memphant-stage-resume` | `reflect` killed after stage 4 resumes at stage 5: extraction-call count unchanged, accumulators applied once (`04` §9.4) — crash-injection | nightly |
| `memphant-cold-tier-roundtrip` | demote→recall→re-promote: cold recall ships `evidence_cold` (no silent miss), re-promotion re-derives embeddings and the vector channel works again (`04` §2.4) | nightly |
| `memphant-restore-order` | simulated PITR skew: live-row/blob-absent ⇒ hard quarantine never served; GC resume before the integrity gate ⇒ FAIL (`14` §4.2) | release |
| `memphant-backpressure-shed` | extraction saturation sheds by demand tier, recall declares `consolidation_lag` + falls back, admission sheds `429`+`Retry-After` (`02` §1.1a/§3.1) | release |

### 6.2 Determinism & How the Lanes Are Built

The default read path must be **bit-reproducible** given (corpus, query, `config_hash`) — what makes the `05` §9 ablations meaningful. The three nondeterminism sources and their pins:

- **Fusion** is integer-rank RRF; ties break on `unit_id` (UUIDv7 total order), never on float score or hash-map iteration order. Snapshot-tested.
- **Rerank** default is a pure scoring function; provider rerankers (balanced/deep) are excluded from the deterministic snapshot lane (asserted only "ran + labeled").
- **Decay** is `fsrs-rs` with a pinned weight vector and elapsed time passed in via a `Clock` seam (never `now()` inside the kernel), so a replay at fixed `t` reproduces.

**Property tests (proptest) for the two unforgeable invariants** (generative, not example-based): `memphant-tenant-isolation`/`-scope-inheritance` generate a random multi-tenant corpus + recall and assert no returned `unit_id` belongs to a non-admitted tenant/scope, for all inputs; `memphant-citation-forgery` generates a recall + a random cited-ID set and asserts any ID outside the candidate whitelist is rejected.

**Golden YAML execution:** `memphant-eval` is the only runner — it seeds episodes into a throwaway `memphant` schema, runs `reflect` where the case requires *derivation* (contradiction/corroboration goldens must not be pre-annotated, `05` §4.2), runs `recall`, scores deterministically against `answer_bearing_ids` + citations + forbidden leaks (no LLM judge), and for `verify-golden` re-runs with `answer_bearing_ids` **masked** and fails the fixture if the masked run still satisfies every remaining `expect` assertion (the mechanical load-bearing check, `05` §4.0). The manifest↔file orphan guard is a `cargo nextest` test (fails the PR lane, not a nightly).

**Symbolic-ID resolution (R81):** derived units carry runtime UUIDv7 ids, so every `mem_*` name in a fixture's `expect` MUST be declared in its `expect_units` binding block (`05` §4.2) — the runner resolves each binding by `(subject, predicate, value_contains)` match against post-reflect units of the case's throwaway schema. Zero or multiple matches = **fixture error** (exit distinct from a recall miss), so an authoring bug can never masquerade as a regression.

**Record-replay extraction on the PR lane (R81):** PR-cadence derivation goldens replay recorded extraction outputs keyed `(fixture_version, compiler_version)` with a pinned local embedder — the PR gate stays ~$0 AND deterministic (a live-LLM PR gate can flip on model variance with no code change, breaking `22` §3.0's fail-closed rule). A `compiler_version` bump invalidates recordings and forces one live re-record, archived with the fixture. Live-derivation runs are the nightly lane.

## 7. Trace-First Development

Every non-trivial retrieval path must leave a trace. If a benchmark fails and the trace cannot tell whether the failure was candidate generation, fusion, rerank, context assembly, trust filtering, or answer synthesis, the implementation is not done.

## 8. Performance Rule

Rust is not the excuse for complexity. Measure first.

Optimize in this order:

1. SQL/index shape.
2. Candidate counts.
3. Avoiding external calls.
4. Parallel candidate generation.
5. Rust hot loop optimization.
6. New backend.

External graph DBs and cache clusters come after traces show they are the bottleneck. L4 deliberate recall ships as an explicit benchmark/Deep mode, never the default path.

## 9. Generated vs Handwritten Clients

OpenAPI is canonical for HTTP.

Toolchain (2026): **`@hey-api/openapi-ts`** for the TypeScript client (typed SDK + validators), **`openapi-generator-cli -g python`** (or `datamodel-code-generator`) for Python request/response types. Both consume the single OpenAPI document the Axum server emits.

Rules:

- Python and TypeScript clients are generated for request/response types.
- Ergonomic wrappers are handwritten but thin.
- MCP schemas reference the same JSON Schema definitions where possible.
- CLI output schemas are snapshot-tested.
- Any public field removal is a major version change.
- Any new nullable field must explain whether absence means unknown, unset, or unauthorized.

### 9.1 One Canonical Type → serde + JSON Schema + MCP (single-source)

Every public request/response is **one struct in `memphant-types`** deriving `serde` + `schemars::JsonSchema`. From it: **OpenAPI** components (consumed by the TS/Python codegen, §9), and **MCP `inputSchema`** (rmcp's `#[tool]` macro derives it from the same param struct, so MCP input == REST body by construction). **MCP `outputSchema` is explicitly attached, not hand-authored**: MemPhant calls `Tool::with_output_schema<T>()` on the same response type that backs REST. A `cargo nextest` test asserts per tool that `outputSchema == schema_for_output::<T>()` and that `structuredContent` validates against it — so four artifacts (OpenAPI, both SDKs, MCP in, MCP out) derive from one struct and only the attach is non-derived.

## 10. Feature Flags

Every research lever is independently switchable:

```text
lexical_enabled
vector_enabled
temporal_enabled
edge_expansion_enabled
rerank_enabled
query_decomposition_enabled
trust_filter_enabled
decay_enabled
proactive_recall_enabled
l4_deliberate_recall_enabled
procedure_promotion_enabled
```

Feature flags are recorded in retrieval traces and benchmark archives. A SOTA run without flag state is invalid.

### 10.1 Flag Plumbing (config → trace → archive)

- **One config struct (`RetrievalConfig`, serde + schemars), validated at the run boundary — not at process start.** Mirroring Syndai's frontier-monitor pattern (validate worker-scoped, not in global `Settings`), an invalid flag combination fails *that recall/eval call*, never blocks server boot. Validation returns the *full* `Vec<ConfigError>` (a 422 with `details.fields`), not the first error.
- **The config is hashed into `config_hash`** and that hash + the resolved flag vector ride into every `retrieval_trace` and benchmark archive row — making "a SOTA run without flag state is invalid" enforceable because the trace writer owns it.
- **Resolution order:** request `mode` floor → per-tenant overlay → request overrides; the *executed* mode + any auto-escalation (`05` §1.3) are trace fields, so an escalation-overridden flag is visible.
- Fails closed: an unknown flag, or `decay_enabled` with no fitted weights (`04` §8 data-gate), is a `ConfigError`, not a best-effort run.

HyDE is rejected for v1 because generated pseudo-documents blur provenance. Query decomposition must produce traceable subqueries, not synthetic evidence.

## 11. Pre-Production Rewrite Rule

Delete or rewrite code when it removes coupling or improves the frozen interfaces. Do not delete tests, contracts, or migration evidence. The project is pre-production, but correctness evidence is not optional.

## 12. Engineering Workstreams

Implementation order and exit packets are owned by `29-implementation-plan.md`. This doc owns engineering invariants, crate boundaries, dependency choices, and local gate mechanics.

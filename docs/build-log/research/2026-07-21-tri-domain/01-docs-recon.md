# 01 — Library/Docs Recon (2026-07-21)

Scope: current (2026) capabilities of the stack MemPhant sits on, with emphasis on the
"can we get real BM25 in Postgres on Supabase + Neon + self-host?" decision.
Read-only recon; no repo files touched.

---

## 1. pgvector 0.8.x

Source: pgvector README (github.com/pgvector/pgvector, master).

**Types and limits**
- `vector` (fp32): index up to **2,000 dims**.
- `halfvec` (fp16): 2×dims+8 bytes storage, up to 16,000 dims stored, **index up to 4,000 dims**. Scalar quantization = expression index cast: `CREATE INDEX ... USING hnsw ((embedding::halfvec(n)) halfvec_l2_ops)`; query must use the same cast. Halves index size for small recall cost.
- `bit` + `binary_quantize()`: binary quantization, index up to **64,000 dims**, Hamming `<~>` and Jaccard `<%>` ops. Canonical two-stage pattern: inner query orders by `binary_quantize(embedding)::bit(n) <~> binary_quantize($q)` LIMIT ~4×K, outer re-ranks by full-precision `<=>` LIMIT K. ~32x smaller index, "keeps indexes in-memory at scale".
- `sparsevec`: `{i:v,...}/dims`, up to 16,000 non-zero elements, L2/IP/cosine/L1 ops — usable for SPLADE/BGE-M3 sparse lanes.

**Iterative index scans (0.8.0+) — the filtered-recall fix**
- Filtering is applied **after** the ANN index scan. With default `hnsw.ef_search = 40` and a 10%-selective predicate, only ~4 rows survive on average.
- `SET hnsw.iterative_scan = strict_order | relaxed_order` (default off) makes the scan continue until K results or limits hit. Knobs: `hnsw.max_scan_tuples` (default 20,000), `hnsw.scan_mem_multiplier` (× work_mem). IVFFlat: `ivfflat.iterative_scan = relaxed_order`, `ivfflat.max_probes`.
- `relaxed_order` gives better recall, possibly out-of-order results (re-sort via materialized CTE); `strict_order` preserves exact distance order.

**Filtering/multi-tenancy guidance (verbatim from README)**
- Partial indexes for low-cardinality filters (`WHERE category_id = 123`).
- LIST-partition by tenant; "sharing an approximate index between tenants means vectors from one tenant can affect recall (and speed) for other tenants".

**Scale**
- Non-partitioned table limit 32 TB (Postgres default); HNSW build wants the graph in `maintenance_work_mem`. No hard row limit documented; practical ceiling per un-partitioned HNSW table is memory-bound (tens of millions of 384-d vectors is comfortable; the billion-vector class is what Neon's `lakebase_ann` (IVF+RaBitQ, ~32x compression, "1B+ vectors per index") exists for — see §2.
- Hybrid search: README shows tsvector + `ts_rank_cd` alongside vector search and points to Reciprocal Rank Fusion and cross-encoder re-ranking as the fusion patterns.

**Verdict:** pgvector 0.8.x natively covers every quantization tier MemPhant plausibly needs at 384-d/1024-d (fp32 → halfvec → binary+rerank) and fixed its historical filtered-recall weakness via iterative scans. No pgvectorscale/VectorChord needed at our scale.

---

## 2. Postgres-native lexical search, 2026 (KEY DECISION INPUT)

### pg_search (ParadeDB)
- Real BM25 (Tantivy-based), mature (0.22.x on PGXN), SQL+JSON query syntax, hybrid BM25+pgvector, faceting.
- **License: AGPL-3.0** (commercial license available). (github.com/paradedb/paradedb)
- **Neon: withdrawn.** "Deprecated. Not available for new Neon projects" as of **2026-03-19**; existing installs must migrate **before 2026-06-01** (neon.com/docs/extensions/pg_search, neon.com/docs/extensions/pg-extensions). Replacement is Neon/Databricks' in-house `lakebase_text`.
- **Supabase: never on the managed allowlist.** The "ParadeDB × Supabase" partner page is a *separate ParadeDB deployment fed by logical replication*, not an extension (supabase.com/partners/integrations/paradedb; open request: github.com/orgs/supabase/discussions/18061).

### Neon Lakebase extensions (proprietary, Neon-only)
- `lakebase_text` 0.1.0-dev: `lakebase_bm25` index type, **real BM25 with corpus statistics**, Block-Max WAND top-K pushdown, works over standard `tsvector`/`tsquery`, score operator `<@>`. PG16+, experimental, "requires enablement". (neon.com/blog/lakebase-search-on-neon)
- `lakebase_vector` 1.0.0-dev: `lakebase_ann` index, "100% pgvector-compatible" types/operators, IVF+RaBitQ ~32x compression, 1B+ vectors/index claim.
- Because it consumes plain `tsvector`, a tsvector-based schema upgrades to lakebase BM25 on Neon *without schema change* — only the CREATE INDEX and ORDER BY differ.

### PGroonga
- On the **Supabase** allowlist (dashboard-enable); **absent from Neon's** list.
- Scoring is currently **TF only** — "the score is based on how many keywords are included"; Groonga itself offers TF-IDF (`scorer_tf_idf`) but **no BM25 scorer**, and PGroonga doesn't expose scorer customization (pgroonga.github.io/v1/reference/functions/pgroonga-score.html, groonga.org/docs/reference/scorer.html). So PGroonga is a *multilingual/CJK FTS* answer, **not** a BM25 answer.

### VectorChord-bm25 (TensorChord)
- Native BM25 index with Block-WeakAnd; needs companion `pg_tokenizer.rs`; v0.2.x — young. **Dual-licensed AGPL-3.0 / Elastic License v2.** Not on Supabase or Neon allowlists (EDB and Pigsty package it; self-host only).

### Everywhere-available baseline
- `tsvector`/`tsquery` + `ts_rank`/`ts_rank_cd` and `pg_trgm` 1.6 are on **all three** targets (vanilla PG17, Supabase, Neon). Caveat: `ts_rank` has no IDF or BM25-style length normalization — it is measurably weaker than BM25 on keyword-heavy queries; RRF fusion with the dense lane plus a cross-encoder rerank recovers most of the gap.

### Conclusion (the decision)
**There is no BM25 extension available on both Supabase and Neon in 2026.** The intersection of managed allowlists for lexical search is exactly {tsvector, pg_trgm, pgroonga-on-Supabase-only}. Portable lexical must therefore be **tsvector-based**, with BM25 as a *provider-specific accelerator* (lakebase_text on Neon; pg_search/VectorChord-bm25 on self-host where AGPL is acceptable — note MemPhant is Apache-2.0, so shipping a hard dependency on an AGPL extension is also a licensing smell; keeping it an optional backend avoids both problems).

---

## 3. sqlx 0.9 + Postgres 17 (ingestion-relevant)

Source: transact-rs/sqlx CHANGELOG (0.9.0 released **2026-05-06**), docs.rs.

- Project moved to the `transact-rs` GitHub org. MSRV 1.94. New `sqlx.toml` per-crate config (rename DATABASE_URL per crate, macro type overrides, migrations-table location).
- Breaking: all `query*()` now take `impl SqlSafeStr` (`&'static str` or explicit `AssertSqlSafe`) — affects any dynamic SQL builders. `Migrate` trait redesigned. Postgres macros now use forced generic plans (better nullability inference; may change some `query!()` types).
- **COPY is the bulk-ingest path and is fully supported**: `PgConnection::copy_in_raw("COPY t (…) FROM STDIN (FORMAT csv|binary)") -> PgCopyIn` with `send`/`finish`/`abort` (abort or finish is mandatory or the connection is poisoned), and `copy_out_raw` streaming `Bytes` for export. (docs.rs/sqlx/latest/sqlx/postgres/struct.PgConnection.html)
- **Protocol pipelining still not shipped** (launchbadge/sqlx#408 open). For batch inserts short of COPY: multi-row `INSERT ... UNNEST($1::uuid[], ...)` remains the idiom.
- Pooler interaction (matters on Supabase): transaction-mode poolers (Supavisor port 6543) break sqlx's prepared-statement cache; use session mode (5432)/direct connections, or `PgConnectOptions::statement_cache_capacity(0)` (supabase.com/docs/guides/troubleshooting/disabling-prepared-statements-qL8lEL).
- PG17 declarative partitioning (LIST by tenant/space, RANGE by time for the event ledger) works transparently with sqlx; nothing version-specific needed.

---

## 4. fastembed-rs (anush008/fastembed-rs)

**Text embedding catalog (44+ models; `Q` = quantized ONNX):**
- BGE family incl. **BGESmallENV15 (default, 384-d)** + Q, BGE-M3 (joint dense+sparse+ColBERT in one pass).
- **ModernBertEmbedLarge** (docs lane, present in catalog), **JinaEmbeddingsV2BaseCode** (a *code-specific* embedder — relevant to the code lane), EmbeddingGemma300M (+Q, +Q4), Nomic v1.5 (+Q), Snowflake Arctic XS→L (+Q), GTE, MxbaiEmbedLargeV1 (+Q), multilingual E5.
- Sparse: SPLADE-PP v1, BGE-M3 sparse — pairs with pgvector `sparsevec` for a learned-sparse lexical lane that *is* portable (it's just a vector column).

**Rerankers (cross-encoder, `TextRerank::rerank`):** BGERerankerBase (default), **BGERerankerV2M3**, JinaRerankerV1TurboEN, JinaRerankerV2BaseMultilingual. No quantized reranker variants listed.

**Config/latency:** `TextInitOptions` — `with_max_length` (default 512), `with_intra_threads`, `with_execution_providers`, cache dir; default embed batch 256. Docs give memory/speed trade-off tables per batch size but no absolute CPU latencies; expect low-single-digit ms/passage amortized for bge-small-Q on modern CPUs, ~10x that for large/ModernBERT-class (estimate, not doc-cited).

**Matryoshka:** no output-dimension truncation knob is exposed in `TextInitOptions` per current docs — MRL-capable models (Nomic v1.5, EmbeddingGemma) require manual truncate+renormalize on the returned vectors. Fine, but it lives in MemPhant code.

---

## 5. Supabase (docs.supabase.com, July 2026)

**Storage**
- S3-compatible endpoint: buckets/objects CRUD, ListObjectsV2, CopyObject, **full multipart upload**, SigV4 presigned URLs. **No versioning ("deleted objects are permanently removed"), no object lock/tagging/ACLs.**
- TUS resumable uploads: fixed **6 MB chunks**, upload URLs valid 24 h, 409 on concurrent same-path uploads.
- Egress billing: uncached **$0.09/GB**, Smart-CDN-cached **$0.03/GB**; Pro/Team include 250 GB + 250 GB. A doc-plane that re-reads large files through Storage has a real egress cost; CDN-cacheable reads are 3x cheaper.

**Database constraints that hit a memory workload**
- Default `statement_timeout`: **anon 3 s, authenticated 8 s**, `postgres` capped at 2 min; REST/dashboard hard-capped 60 s. Long maintenance (HNSW builds, consolidation jobs) must run over Supavisor **session mode (5432) or direct**, with role-level timeout overrides (`ALTER ROLE ... SET statement_timeout`).
- Supavisor transaction mode (6543) breaks prepared statements → sqlx needs session mode/direct or `statement_cache_capacity(0)`.
- RLS + pgvector is a documented, supported pattern (rag-with-permissions guide) but "RLS is latency-sensitive"; custom-session-variable (`current_setting('app.current_user_id')`) pattern for non-Supabase-auth callers.

**Realtime (memory-update push to UX)**
- Three primitives: Broadcast, Presence, Postgres Changes. Official guidance: **Broadcast is the recommended, scalable path**; `postgres_changes` "does not scale as well". Server-side push = `realtime.broadcast_changes()` in a trigger onto topic-scoped private channels with RLS-based Realtime Authorization. This gives MemPhant push-on-memory-write with zero extra infra.

---

## 6. MCP spec + rmcp (July 2026)

**Stable spec = 2025-11-25** (modelcontextprotocol.io/specification/2025-11-25): tools, resources, prompts, roots, logging, sampling (+`tools`/`toolChoice`), **elicitation incl. URL-mode and enum/default schemas**, experimental **tasks**, icons metadata, OAuth CIMD registration, JSON Schema 2020-12 default.

**2026-07-28 release candidate — finalizes in ~1 week** (blog.modelcontextprotocol.io/posts/2026-07-28-release-candidate):
- **Stateless core**: `initialize` handshake and `Mcp-Session-Id` removed; version/client-info ride in `_meta`; plain round-robin LB works. Application state via explicit handles the model passes between tool calls.
- **Extensions framework** (reverse-DNS IDs, independent versioning); Tasks becomes an extension; MCP Apps (sandboxed HTML UI) debuts.
- **Deprecated: Sampling, Roots, Logging** (≥12-month sunset). Elicitation morphs into **MRTR** (`InputRequiredResult` → client re-issues call with answers; no held SSE stream).
- Resources gain `ttlMs`/`cacheScope` cache metadata — directly useful for exposing the file-plane (.md memory files) as cacheable MCP resources.

**rmcp (official Rust SDK)**: v0.16.0, supports **both 2025-11-25 and 2026-07-28**; `#[tool]`/`#[tool_router]`/`#[prompt]`/`#[task_handler]` macros; stdio + streamable-HTTP transports; OAuth support documented; sampling/logging already flagged deprecated (SEP-2577) in the SDK. MemPhant's rmcp surface should be: **tools primary, resources (with cache metadata) for file-plane, prompts for retrieval presets — and nothing built on sampling.**

---

## 7. fsrs-rs

Crate `fsrs` **6.6.1** (2026-06-09), BSD-3-Clause, repo open-spaced-repetition/fsrs-rs.

- Core types: `FSRS`, `MemoryState { stability, difficulty }`, `NextStates { again|hard|good|easy: ItemState { memory, interval } }`, `FSRSItem { reviews: Vec<FSRSReview> }` (chronological), `TrainingConfig`.
- API shape over an event ledger: `fsrs.next_states(prev: Option<MemoryState>, desired_retention, elapsed_days)`; `current_retrievability(state, elapsed)` gives the decay score at query time; `compute_parameters()` trains per-corpus parameters from review logs (Burn-based — heavyweight dep even though inference uses the NdArray backend); `simulate()`, `optimal_retention()`; `DEFAULT_PARAMETERS`, `FSRS6_DEFAULT_DECAY`.
- Mapping for MemPhant: each recall/confirmation/contradiction event = an `FSRSReview` with a grade; store `MemoryState` per memory item; rank/decay by `current_retrievability`; parameter training is optional and only worthwhile once there is a graded feedback signal (the approval-lifecycle signal noted in the npcsh audit is exactly such a grade source).

---

## Implications for MemPhant storage/substrate design (ranked)

1. **BM25-in-Postgres is NOT portable across Supabase+Neon in 2026** — pg_search was pulled from new Neon projects (2026-03-19, migrate-by 2026-06-01) and was never on Supabase; PGroonga is Supabase-only *and* TF-scored (no BM25); VectorChord-bm25 is on neither allowlist and AGPL/ELv2. → **Canonical lexical lane must be `tsvector` + `ts_rank_cd` + `pg_trgm`** (present on vanilla PG, Supabase, Neon), behind a small `LexicalBackend` trait.
2. **Store `tsvector` as the schema-level lexical representation regardless of backend** — Neon's `lakebase_text` BM25 index consumes standard `tsvector`, so a tsvector schema gets real BM25 on Neon (and pg_search on self-host) by swapping only the index + ORDER BY, feature-detected at startup via `pg_available_extensions`. No portable-schema sacrifice.
3. **Close the tsvector-vs-BM25 quality gap in the ranker, not the storage**: RRF fusion of dense (bge-small) + lexical + optional SPLADE/BGE-M3 **sparse lane in pgvector `sparsevec`** (a learned-sparse "BM25 substitute" that IS portable because it's just a vector column), then fastembed cross-encoder rerank (BGERerankerV2M3) on the fused top-K. This matches accuracy>cost>speed priorities without any non-allowlisted extension.
4. **Turn on pgvector iterative scans for every filtered recall path** (`hnsw.iterative_scan = relaxed_order` + re-sort, tune `hnsw.max_scan_tuples`): MemPhant queries are always tenant/space/kind-filtered, which is exactly the post-filter underfill case (default ef_search 40 → ~4 rows at 10% selectivity). Partition or partial-index per tenant/space rather than sharing one HNSW graph across tenants.
5. **Quantization ladder is already in pgvector — plan it, don't add extensions**: fp32 `vector(384)` now; `halfvec` expression index when index size hurts; `binary_quantize` + Hamming + full-precision rerank at large scale (32x, README-endorsed two-stage). Dimension caps (2000 vector / 4000 halfvec) comfortably fit bge-small 384 and modernbert 1024.
6. **MCP surface: tools + resources only; nothing on sampling/roots/logging** — the 2026-07-28 spec (final in days) deprecates all three and goes stateless. Design the rmcp server (rmcp 0.16 already supports the RC) with explicit `memory_space`/cursor handles in tool params, and expose the flat-file plane as MCP **resources with `ttlMs`/`cacheScope`** — that is the sanctioned way to give any agent read access to .md memory files.
7. **Bulk ingestion = sqlx `copy_in_raw` (binary COPY) + UNNEST batches; no pipelining exists** in sqlx 0.9 (issue #408 open), so COPY is the only high-throughput path for doc/code ingestion. Budget for the 0.9 `SqlSafeStr` breaking change in any dynamic SQL builder.
8. **On Supabase, run MemPhant's worker/API over session-mode or direct connections (port 5432)**: transaction-mode Supavisor breaks sqlx prepared statements, and default statement timeouts (anon 3 s / authenticated 8 s / REST 60 s) will kill index builds and consolidation jobs unless role-level `statement_timeout` overrides are set.
9. **Memory-update push to UX: use Supabase Realtime Broadcast via `realtime.broadcast_changes()` triggers** (private channels + RLS auth), not `postgres_changes` — Broadcast is Supabase's own scalability recommendation, and a NOTIFY/LISTEN fallback covers non-Supabase deployments.
10. **Cloud file plane: Supabase Storage is adequate but has NO versioning** — full S3 multipart + SigV4 presigned + TUS (6 MB chunks) means standard S3 client code works, but version history for memory files must live in MemPhant's own ledger (or a git repo plane); deletes are unrecoverable. Egress: cached $0.03/GB vs uncached $0.09/GB → serve doc reads through the CDN path.
11. **FSRS decay is a drop-in over the event ledger**: crate `fsrs` 6.6.1 (BSD-3) — persist `MemoryState{stability,difficulty}` per item, feed retrieval/confirm/contradict events as `FSRSReview`s, rank by `current_retrievability`; skip `compute_parameters` training until a graded feedback signal exists (Burn dep is the heavy part; defaults are fine to start).
12. **License hygiene**: the only real-BM25 extensions are AGPL (pg_search) or AGPL/ELv2 (VectorChord-bm25) — keep them strictly optional backends so Apache-2.0 MemPhant never hard-depends on copyleft server code; fastembed-rs models used (BGE, ModernBERT-embed, Jina-code) are Apache/MIT-family and safe to bundle-by-download.

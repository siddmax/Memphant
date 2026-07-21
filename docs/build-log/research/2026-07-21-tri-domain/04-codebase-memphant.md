# MemPhant Codebase Deep-Read (worktree: /Users/sidsharma/.codex/worktrees/Memphant/p1-deep-mode @ ab1f52e3, branch codex/memphant-p1-deep-mode)

All paths relative to the worktree root unless absolute. READ-ONLY audit; nothing modified.

---

## (a) Architecture map (file:line anchors)

### Crates
| Crate | Role | Size anchor |
|---|---|---|
| `crates/memphant-types` | Entire wire/contract surface, serde-strict | `src/lib.rs` 1,884 lines |
| `crates/memphant-core` | Recall pipeline, write compiler, reflect, packing, deep-recall seam, InMemoryStore | `src/lib.rs` 12,823 lines + `service.rs` 4,091 + `structured_state.rs` 683 + `deep_recall.rs` 48 |
| `crates/memphant-store-postgres` | sqlx Postgres store, RLS, roles | `src/store.rs` 4,813 |
| `crates/memphant-runtime` | Embedder/reranker/deep/structured-state provider factories, env wiring | `src/lib.rs` 1,364; `deep_recall_openrouter.rs` 2,989; `structured_state_openrouter.rs` 4,944; `api_embeddings.rs` 1,035 |
| `crates/memphant-server` | Axum REST | `src/lib.rs` 1,144 |
| `crates/memphant-mcp` | rmcp MCP server, 7 tools | `src/lib.rs` 813 |
| `crates/memphant-worker` | Thin reflect loop over `run_worker_tick` | `src/main.rs` 138 |
| `crates/memphant-eval` | YAML golden/profile/security lanes + bench-lme LongMemEval lane | `src/lib.rs` 2,998; `bench_lme.rs` 1,544; `main.rs` 704 |
| `crates/memphant-cli` | verbs + bootstrap/BYOC preflight (object-store residency checks at `main.rs:865-875`) | 1,044 |

### Types contract (`crates/memphant-types/src/lib.rs`)
- `MemoryKind` (5 kinds) :854-870; `UnitState` (11 states, Captured→…→Retired) :872-886; `TrustLevel` (8 tiers) :888-899; `actor_kind_trust` :276-284; `agent_level_allows_memory_kind` (L0 user boundary vs L1+ agent-local) :266-274.
- `RecallMode` Fast/Balanced/Deep :371-388 (contract test: "deep is the only explicit deliberate mode"; `exhaustive` rejected).
- `RecallRequest` :463-490 (per-request lever toggles default true — but see server defaults below); `RetrievalTrace` :726-823 (full trace spine incl. `recall_pool_depth`, `cross_rerank_ms`, `l4_*` deep identity fields, bitemporal `recall_time`).
- `ResourceKind` Document/Code/Conversation/Other :948-956; `ResourceAcl` + `ResourceProtectedCategory` (6 protected categories) :958-990 — `is_deep_eligible()` = only empty ACL; comment admits "Ordinary recall enforcement is intentionally separate and remains pending".
- Deep types: `DeepRecallLimits/Usage/Status/StopReason` :663-705, `DeepSnapshotEntry`/`DeepWorkspace` :1126-1167 (bound_units carried to avoid a second racy store read).
- Retain payloads (tagged union episode|resource|unit, deny_unknown_fields) :1567-1628; edges `MemoryEdgeKind` (supersedes/contradicts/derived_from/cites/same_subject/depends_on) :1263-1272; `ReflectJobKind` episode/resource/scope :1306-1312; `MarkOutcome` :1796-1803.
- Version lock `MemphantLock` (engine/compiler/trace/schema/methodology/export versions) :1450-1550.

### Recall pipeline (`crates/memphant-core/src/lib.rs`)
- ONE pool knob `DEFAULT_RECALL_POOL_DEPTH=64` :398-420 (R1.5-T0: internal fan-out never derives from k). `PackLevers` (sibling-gather + session quota, both default OFF) :432-454.
- Entry points: plain `recall` :5252 → `recall_with_pool` :5801 → `recall_with_pool_and_selection_and_deep` :5872 → `..._impl` :6030. Stage-0 admission predicate `recall_scope_admitted` :5862.
- Deep branch :6130-6190: validate provider identity → `store.fetch_deep_snapshot` (pg: `store.rs:1939`) → policy re-filter of bound units → `build_deep_workspace_owned` :127 → `provider.gather()` → `validate_deep_provider_result` :5959 → ranked units merged into candidate pool as `RecallChannel::Deep` with rank-1.0 scores :6369-6417, and deep-ranked candidates lead packing order :7620-7624 and the authoritative partition :7688-7698.
- Channels :6287-6297 (Exact, Lexical, Semantic-token-overlap, Temporal, Edge, + Vector only when a real embedder is present). Fusion = RRF-style `weight/(60+rank)` :6320-6342.
- W-stages: W3 fusion weights `channel_weight` :9238; W4 pack levers; W5 temporal grounding (`extract_query_date` :8996, dated packs :7750-7761); W6 fact extraction (ingest-time, service flag); W8 cross-encoder rerank `cross_rerank_candidates` :7135 invoked :6562-6595; retired W-heuristic rerank `rerank_candidates` :7216 (opt-in only).
- Query decomposition :6419-6515 (deterministic structural conjuncts :7015; decomposition RETAINS only subquery-tagged candidates).
- Packing: `recall_pack_scan_limit` :8023 (Deep scans pool×25; Fast/Balanced scan pool floor), `pack_recall_context` :7589, `admit_or_drop` :7784 (coarse gate; measured-permanent per memory note), sibling gather :7980, chunk mask selection :8203, abstention on unresolved contradiction :7763-7769.
- Bitemporal: `resolve_recall_time` :199, `bitemporally_recallable` :8548, `deep_unit_is_snapshot_eligible` :8572, correction rectangles :722-858.
- Deterministic projections: `quantity_rollups` :5377 (aggregation_window), `artifact_bundle` :5648 (quoted/colon artifact anchors → rank-1 authoritative bundle), goal companion :7543.
- Write compiler/reflect: `reflect_recorded`/`reflect_recorded_claimed` :9504/9516, `minted_unit` :10223, `derive_fact_key` :10275, belief composition `compose_inferred_beliefs` :10332 (guardrailed, `derived_by=composition`), composed-dependent expiry :10534.
- DSR decay fold: `decay_score_for` :9275, review-grade replay :9372 (fixed-prior FSRS-ish; fitter dormant).
- High-risk suppression: `high_risk_recall_drop_reason` :8587, trust floor :8625.

### Service (`crates/memphant-core/src/service.rs`)
- `MemoryService` :786 with construction-time levers (builders :900-1011): contextual chunks (default ON), resource chunks (OFF), pool depth, sibling/session/temporal/fact-extraction (OFF), cross-reranker, structured-state provider, deep provider (`with_deep_recall_provider` :1008).
- PUBLIC recall defaults :1336-1373: k=8, budget_tokens=512(!), mode=Fast, `edge_expansion_enabled: false` (rung-6 off on public path), `rerank_enabled: false` (heuristic reranker retired). `recall_internal` :1377 is the bench/eval entry that accepts a full `RecallRequest`.
- Degraded read-your-own-writes fallback (pending reflection → raw episode surfacing, trace `degradation`) :1441+.
- Trust clamping `clamp_trust`/`trust_rank` :764-784; retain :1023; worker tick :1788.

### Postgres store (`crates/memphant-store-postgres`, `memphant_migrations/versions/20260703_001_wsa_bootstrap.sql` — single squashed bootstrap, 1,368 lines)
- 27 tables. Highlights: `memory_unit` :331-386 (bitemporal cols, DSR cols difficulty/stability/desired_retention, `body_tsv` generated, `payload jsonb`), semantic/belief validity-overlap EXCLUSION constraint :817-826, edge transaction-overlap exclusion :836-847, `embedding` (halfvec) :432-449, `embedding_profile` with `index_strategy in (hnsw_full|hnsw_subvector|hnsw_binary|exact)` :425 — **but NO vector index is ever created; `<=>` runs as exact scan** (`store.rs:2214-2224` `order by embedding.vec <=> $9::halfvec limit N`). FTS via `websearch_to_tsquery` + `ts_rank_cd` top-200 (`store.rs:1849-1871`).
- Ledgers: `mutation_ledger` (24h idempotency replay) :621-640, `blob_ledger` :642, `trust_event` :479, `event_outbox` :502 (6 event types, no consumer), `belief_observation` :659, `review_event(_unit)` :681-731, `retrieval_trace` :532, `deletion_generation` :565, `forgotten_source` :760, `scope_block` (versioned 300-token scope summary) :775-795.
- `episode.retention_tier hot|warm|cold` :278 + partial index :810 — **zero Rust references; no tier_episode job exists**.
- Security: 6 NOLOGIN roles :8-29, FORCE RLS on all tenant tables :893-948, transaction-local `bind_tenant` :102, SECURITY DEFINER `authenticate_api_key` :1039 and `claim_reflect_jobs` :1066. API key principal binding (all-or-nothing subject/actor/scope/agent columns) :733-758.

### Runtime (`crates/memphant-runtime`)
- Embedder grammar `embedder_from_id` `src/lib.rs:149-173`: off/noop, fastembed bge-small (default), base, bge-m3, modernbert, gemma (feature `fastembed`), qwen3 (feature `qwen3`), + always-compiled API arms voyage-4/-lite/-large/code-3/context-4, gemini-embedding-001, openai-text-embedding-3-small.
- Cross-reranker factory :236-255 (`MEMPHANT_RERANKER` = fastembed BAAI/bge-reranker-base | voyage-rerank-2.5), shared by server env wiring AND bench `--cross-rerank`.
- `build_service` :336-355 wires cross-rerank (`MEMPHANT_CROSS_RERANK`) and Deep (`MEMPHANT_DEEP=on`) into the served MemoryService; `build_worker_service` :359 wires structured-state instead (workers never recall).
- Deep provider (`deep_recall_openrouter.rs`): ceilings :27-31 — wall 120s, 24 tool iterations, 96k context tokens, 300,000 micros ($0.30) per deep recall. Env contract `build_deep_recall_provider` :1138-1182 (`OPENROUTER_API_KEY`, `MEMPHANT_DEEP_MODEL`, `MEMPHANT_DEEP_RESPONSE_MODEL`, `MEMPHANT_DEEP_PROVIDERS`, `MEMPHANT_DEEP_PROMPT_PATH`, input/output price micros, optional base URL). Read-only workspace tool loop `WorkspaceTools` :1209+ — `list_files`/`search_files`/`read_file`/`record_evidence(finish)` over the materialized episode/resource snapshot; SSE `parse_turn` :924; spend reservation/settlement :666-867 with unsettled upper bounds; generation-id audit trail.
- Structured state (`structured_state_openrouter.rs`, gated `MEMPHANT_STRUCTURED_STATE=on` :36): exact-unit-bound create/replace/delete against the live snapshot, fail-closed zero-target, versioned prompt at `config/structured-state-v1.txt` hashed into compiler identity.

### Public surfaces
- REST (`crates/memphant-server/src/lib.rs:261-271`): /health, /openapi, retain, recall, reflect, correct, forget, mark, trace, scope memory, PUT context-binding. API-key auth with principal binding + trust clamp (`assigned_trust` in retain response). Dev mode `MEMPHANT_DEV_TENANT` :108.
- MCP (`crates/memphant-mcp/src/lib.rs`): 7 `#[tool]`s at :223,269,306,347,388,434,474 (retain/recall/reflect/correct/forget/mark/trace), same principal binding; tool schemas exported to `mcp/memphant.tools.v1.json`.

### Eval machinery
- `memphant-eval` subcommands (`main.rs:16-31`): run (YAML goldens with per-lever `--disable-*` and `--l4-runtime-provider`), bench-lme, verify-golden, security, ops, syndai-trace-compare, schema, ablate, profile, compare.
- bench-lme (`bench_lme.rs`): in-process Postgres-backed LongMemEval lane. Per-question fresh tenant :803, chronological session/turn ingestion :850-901, worker drain :905-910, `recall_internal` with `--mode fast|balanced|deep` :912-929 (`main.rs:497-501`), retrieval metrics + `--emit-qa` reader-evidence JSONL, stratified sampling, paired bootstrap CI `bootstrap_ci` :494, `--baseline` paired comparison. **The bench service never installs a deep provider** (:701-753) — `--mode deep` currently returns DeepUnavailable.
- YAML lane deep: `EvalDeepProvider` (deterministic local stub, `eval-deep-query-overlap-v1`) `lib.rs:35-102`; `--l4-runtime-provider` swaps in the REAL OpenRouter provider `lib.rs:2031-2076`.
- T6 harness `scripts/run_lme_v2_p1_t6.py` (6,065 lines): sealed n=12 LongMemEval-V2 screen. Arms = `fast` (mode=fast) vs `sonnet` (mode=deep, `anthropic/claude-sonnet-5` via azure :3865). Ceremony: immutable manifest+selection hashes :35-49, per-case DB clones + content-addressed case banks + seals + leases + quiescence waits, `verify-no-model` adapter-fidelity fixtures (paid_calls=0), hash-repair authorization machinery :88-117. Claim boundary: "A pass authorizes only a separately preregistered exposed confirmation" (`benchmarks/manifests/longmemeval_v2.p1_t6.json`). Adapter `benchmarks/longmemeval_v2/memphant_memory.py` drives the packaged REST server; `memphant.memory.json` pins top_k=20, budget 32768, mode=deep.
- Support: `scripts/with_scratch_db.sh` (ephemeral migrated scratch DB, fixes job-debris starvation), `materialize_longmemeval_v2_runtime.py`, `run_reader.py`, gate_* docs-lane scripts, code_lane_* scripts, `run_swe_explore.py` (pinned but fails closed — upstream bundle not executable yet).

---

## (b) Dormant-mechanism inventory (mechanism → where coded → flag/rung → activation cost)

| Mechanism | Where coded | Gate/flag | Status & what activation takes |
|---|---|---|---|
| Deep recall (L4, rung 12) | core :6130-6190 + `deep_recall_openrouter.rs` + server `build_service` | `MEMPHANT_DEEP=on` + 7 env vars | BUILT, contract-tested, default OFF. Needs completed paired Deep-vs-Fast evidence (P1-T6 open). Flip env on a served instance = live today. |
| Cross-encoder rerank (rung 8) | core :6562-6595, runtime :236 | `MEMPHANT_CROSS_RERANK` / `MEMPHANT_RERANKER` | Accuracy-validated (+0.158 QA) but latency-RETIRED at 12.9-13.6s CPU; voyage-rerank-2.5 API arm passed docs gate. Activation = flag + latency fix (truncated input / top-32 pool / GPU or API arm). |
| Edge expansion (rung 6) | core channel :6292, `edge_score` :8480 | `RecallRequest.edge_expansion_enabled` — **hardcoded FALSE on public path** (service.rs:1357) | Coded+traced; zero measured delta because chat episodes mint no relational edges. Activation = a write path that mints edges (docs/code lanes), then flip the service default. |
| Heuristic deterministic rerank | `rerank_candidates` :7216 | `rerank_enabled` (public FALSE, service.rs:1362) | Measured harmful (-0.143 R@5). Retirement candidate, kept for ablation. |
| Learned rerank profile (rung 13) | `LearnedRerankProfile` types :534-543, `validate_learned_rerank_profile` core :7398 | request field, no producer | Plumbing only; training floor (archived traces) does not exist. |
| Learned DSR/FSRS fitter | decay fold coded :9275-9391 (fixed priors) | rung 11/13 | Fold BUILT; fitter DORMANT pending many-card review-history floor + longitudinal suite. |
| Query decomposition (rung 9) | :6419-6515, :6978-7065 | `query_decomposition_enabled` (public TRUE) | On but zero measured delta; needs a real composite-query corpus win or demotion. |
| Temporal grounding (W5) | :8773-9096, dated packs :7750 | `with_temporal_grounding_enabled` (default OFF; bench `--temporal-grounding`) | Coded; rejected as default in lever-2 round (ns/regression). |
| Pack levers W4 (sibling gather, session quota) | :441-454, :7712-7748, :7980 | builders + bench flags, default OFF | Coded; unpromoted (ns). |
| Fact extraction (W6) | service `with_fact_extraction_enabled` :967 | default OFF | Retrieval ΔR@10 +0.074 real but QA-blocked by pack displacement; named follow-up. |
| Resource chunks (docs lane) | service `with_resource_chunks_write_enabled` :912 | `MEMPHANT_RESOURCE_CHUNKS` (default OFF) | Third consecutive ns → retirement candidate. |
| Structured state (Memora state maintenance) | core `structured_state.rs`, runtime `structured_state_openrouter.rs` | `MEMPHANT_STRUCTURED_STATE=on` + model/prompt env | BUILT + screened (exact-unit mutations, fail-closed); default OFF; strict restored-bank compat + Task-4 open. |
| Retention tiers hot/warm/cold | migration :278,:810 + spec 04 §2.4 | none | Schema-only. No `tier_episode` job, no object store, zero Rust refs. Activation = object-store integration + demotion job + re-promotion on recall. |
| HNSW / index_strategy | `embedding_profile.index_strategy` :425; spec 27 §3 vector row | none | No vector index created; exact scan is deliberate at ≤100k units. Activation = one `create index using hnsw` migration when scale demands. |
| L0/L1/L2 multi-resolution resource summaries | spec 04 §6.1 (:292) only; `memory_unit.payload` reserved | ablation-gated | NOT CODED. |
| Event outbox consumers | `event_outbox` schema :502 | ledger row: DORMANT | Taxonomy + shape only; no delivery loop. |
| Trust events / corroboration ledger | `trust_event` :479, `belief_observation` :659 | partial | belief_observation drives confidence recompute; trust_event table has no writer in core paths (erasure-list only, store-postgres/src/lib.rs:31). |
| Scope block (pinned scope summary) | schema :775-795 | no public verb | Store CRUD only; no REST/MCP surface, no injection into recall. |
| 3-tier DEK envelope encryption | spec only (`key_custody` frozen) | DORMANT | No BYOC KEK demand. |
| Ablation-voting recall (SMSR), delta recall, miss-repair re-extraction, retrievability probe | spec/ledger rows | DORMANT | Not worth multiplied read cost per WS-I profile. |
| Hermes adapter, TS SDK, cache cluster, Helm, SQLite, CRDT, multi-region, billing | ledger §5 | DORMANT | Demand-gated. |
| External graph/vector engine | — | RETIRED (rung 14) | Do not rebuild. |
| Deep ACL enforcement in ordinary recall | `ResourceAcl` types :985-989 | pending | Deep fails closed on non-empty ACL; ordinary-recall ACL enforcement is an admitted gap in the type's own doc comment. |

---

## (c) Recall pipeline stage diagram (text)

```
RecallHttpRequest (REST/MCP; k=8, budget=512, mode=fast, edges OFF, heuristic-rerank OFF)
  └─ service.recall → recall_internal
       ├─ Stage 0  policy admission (recall_scope_admitted; denied → traced abstention + PolicyDenied)
       ├─ Stage 0b query embedding (real embedder → VectorQuery under embedding_profile id)
       ├─ [Deep only] fetch_deep_snapshot → policy re-filter bound units → DeepWorkspace
       │     → OpenRouter tool loop (list/search/read/record_evidence; caps 120s/24 iters/96k tok/$0.30)
       │     → validated ranked_units (Deep channel)
       ├─ Stage 1  candidate pool: fetch_recall_candidates (usize::MAX) ∪ vector KNN top-pool(64)
       │     ∪ quantity rollups (aggregation_window) ∪ artifact bundles ∪ deep units
       ├─ Stage 2  channel passes: Exact | Lexical | Semantic(token-overlap) | Temporal | Edge? | Vector?
       │     each ranked, fused RRF-style weight/(60+rank); DSR decay computed per unit
       ├─ Stage 3  W9 query decomposition (deterministic conjuncts) → per-subquery channel passes
       │     (cap = pool depth, never k); active decomposition retains only tagged+deep candidates
       ├─ Stage 4  fusion sort (fused_score desc, body tie-break) → fused ranks traced
       ├─ Stage 5  heuristic rerank (retired; opt-in rerank_enabled)
       ├─ Stage 6  W8 cross-encoder rerank over top pool (opt-in; FusedHead or VectorLexicalBalanced
       │     candidate selection; cross_rerank_ms traced)
       ├─ Stage 7  packing (pack_recall_context): order = deep_rank > cross_rerank > decomposition >
       │     heuristic > fused; authoritative-projection partition (deep→projection→goal-companion→rest);
       │     greedy admit_or_drop w/ subject dedup + budget + replacement; scan_limit = pool floor
       │     (Deep: pool×25); optional session quota (work-conserving) + sibling gather + dated packs;
       │     chunk-mask rendering for contextual chunks; abstention on empty or unresolved contradiction
       ├─ Stage 8  trace assembly (RetrievalTrace: channels, candidates, drops, citations, deep summary,
       │     pool depth, latency, cost_micros) → store_trace (durable, tenant-bound)
       └─ Stage 9  degraded read-your-own-writes fallback (pending reflect jobs → raw episode items,
             consolidation_lag_ms + degradation diagnostic)
```

Write path: retain (episode|resource|unit, idempotency ledger, trust clamp) → job_state queue → worker claim (`claim_reflect_jobs`, SECURITY DEFINER, dead-letter at 5 attempts) → reflect compile (dedup, admission policy, contextual chunks ≤32, fact-key derivation, contradiction/supersession via bitemporal exclusion constraint, belief composition, structured-state ops when enabled) → units + edges + citations + embeddings.

---

## (d) Cheapest credible path to a FUNCTIONAL T6 Deep-vs-Fast proof (no sealed ceremony)

The blocker is not machinery — Deep is fully wired end-to-end through the served runtime; the sealed P1-T6 campaign keeps aborting on ceremony (immutable roots, case-bank seals, provider-defect invalidations; see `docs/build-log/artifacts/p1-t6/run-e511c817/INVALIDATION-PROOF.json`). Three escalating options, cheapest first:

1. **Zero-Rust-change, zero-DB smoke (~$1, minutes):** `memphant-eval run benchmarks/rung12-l4-exhaustive-sampled.yaml --l4-runtime-provider` with the strict Deep env set (`MEMPHANT_DEEP=on`, OPENROUTER_API_KEY, model/response-model/providers/prompt-path/prices — `deep_recall_openrouter.rs:1138`). This exercises the REAL OpenRouter tool loop (workspace, SSE, settlement, citations) against the synthetic rung-12 suite. Proves the loop functions; proves nothing about accuracy (synthetic fixtures are regression-only per the provenance rule) — use as the $1 preflight before any paid comparison.

2. **~10-line Rust change, the actual cheap paired proof:** bench-lme already has `--mode deep` (`main.rs:497-501`) and full paired-CI scoring (`--baseline`, `bootstrap_ci`, `--emit-qa`), but its recall service never installs a deep provider (`bench_lme.rs:701-753`) → DeepUnavailable. Mirror the existing cross-rerank wiring (:749-753) with `memphant_runtime::deep_recall_openrouter::build_deep_recall_provider()` → `.with_deep_recall_provider(...)` when the env is set. Then:
   `bench-lme --database-url <scratch> --data <LME> --sample 30 --seed S --mode fast --emit-qa fast.jsonl --out fast.json` and the same with `--mode deep --baseline fast.json`. Cost ceiling is hard-capped at $0.30/query (300k micros, `deep_recall_openrouter.rs:31`) → n=30 deep ≤ $9 worst case, realistically a few dollars; reader QA via the existing `scripts/run_reader.py` lattice adds a few dollars more. This yields exactly the "does Deep recover Fast misses" paired delta the rung-12 row demands, on real LongMemEval through the real Postgres path — just without the sealed-confirmation packaging. Ingest once per question per arm (fresh tenants); the fast arm's QA rows identify Fast-miss strata so a targeted Deep run can even be restricted to Fast-miss questions for maximum information per dollar.

3. **Zero-Rust-change REST variant (matches T6 semantics exactly):** run the packaged server under `scripts/with_scratch_db.sh` with `MEMPHANT_DEEP=on`, ingest via the official adapter `benchmarks/longmemeval_v2/memphant_memory.py` (its pinned config is already top_k=20 / budget 32768 / mode deep), and issue each question twice with `mode: fast` vs `mode: deep` — skipping run_lme_v2_p1_t6.py's clone/seal/lease machinery entirely. More moving parts than (2) but byte-identical to the campaign's serving path, so a positive result de-risks the sealed rerun rather than replacing it.

Recommendation: (1) as preflight, then (2). Frame the output as *functional/diagnostic evidence* (mechanism works, direction of effect) feeding the still-required sealed confirmation — this respects the claim-boundary rule in `benchmarks/manifests/longmemeval_v2.p1_t6.json` and STATUS's promotion-provenance rule while breaking the current ceremony deadlock.

---

## (e) Real gaps vs the tri-domain goal (genuinely NOT built)

1. **Codebase domain has no ingestion front-end.** `ResourceKind::Code` + `revision` (commit hash) exist on the wire, and `code_lane_*.py` scripts mine/run a 40-question sample — but there is no repo indexer: no file walker, no language-aware/AST chunking, no incremental re-index on commit, no symbol/def-ref edges. Code enters only as manually retained resource bodies. This is the largest tri-domain gap.
2. **Docs domain: no document-structure pipeline.** Resource chunking is byte/paragraph-heuristic (`service.rs` RESOURCE_CHUNK_* consts) and default OFF (ns three times); no heading-path context (the measured residual vs Syndai's stack was diagnosed as "heading-path context + fusion, NOT dims", STATUS R0 row), no hierarchy parity, no L0/L1/L2 multi-resolution summaries (spec 04 §6.1 — not coded).
3. **No file-plane projection / object store.** Retention tiers, blob offload, `blob_ledger` GC, and warm/cold demotion are schema+spec only; `MEMPHANT_OBJECT_STORE*` env vars are checked by the CLI BYOC preflight but nothing reads/writes an object store. Supabase Storage integration: absent (STATUS confirms no deployed memphant objects in Supabase projects). Deep's workspace is the only file-plane view, and it is ephemeral and Deep-only.
4. **Edge minting is starved.** The whole rung-6 apparatus (edge channel, one-hop expansion, contradiction/supersession edges) exists, but ordinary chat episodes mint no relational edges, and the public recall path has edges hardcoded OFF. Docs/code corpora — where derived_from/cites/depends_on edges naturally exist — never got a minting write path.
5. **Deep ACL enforcement asymmetry.** Non-empty `ResourceAcl` blocks Deep export, but ordinary recall ACL enforcement is "intentionally separate and remains pending" (`types/src/lib.rs:985-989`).
6. **No trace-training loop.** Rung-13 learned rerank/DSR fitter blocked on an archived-trace training floor that no lane produces yet (bench emits real traces; nothing accumulates/curates them).
7. **No event consumers.** `event_outbox` has producers' schema but no delivery loop — memory.promoted/superseded/contradiction events never reach a subscriber (needed for agent-facing "memory changed" UX and Syndai integration).
8. **No hot-path SLO proof** (STATUS §6): fast p50<200ms/p95<500ms unproven on the packaged runtime; vector search is an exact scan (fine ≤100k units, unbenchmarked beyond).
9. **Benchmark instruments**: SWE-Explore adapter fails closed (upstream bundle incomplete); MemBench/SWE-ContextBench adapters absent — the empty-leaderboard first-mover slots (memory note) have no runnable adapters yet except LME-V2 and Memora-FAMA.
10. **Scope blocks** (pinned per-scope summaries — the "always-in-context" primitive every competitor ships) have storage but no public verb and no recall injection.

## (f) KISS violations / dead code worth deleting

1. **Spike-era stub `pub fn retain(input: RetainInput)`** (`core/src/lib.rs:958-968`) + `RetainInput`/`RetainResult` types — WS-0 leftovers with zero callers outside the spike; delete.
2. **Retired heuristic rerank stage** (`rerank_candidates` :7216, `rerank_intent_anchor_score` :7313, `rerank_enabled`, trace fields `reranker_id`/`rerank_input_count`/`rerank_overfetch_ratio`) — measured harmful, public-path OFF, superseded by the cross-encoder seam. Keep one ablation flag at most; the intent-token lexicon (:7334) is exactly the query-substring special-casing W3 was supposed to purge.
3. **`RecallMode::Balanced`** — behaviorally identical to Fast everywhere except a rerank-cap constant (:7278); nothing public defaults to it, benches don't use it. Two modes (fast/deep) tell the true story.
4. **T6 controller bloat**: `run_lme_v2_p1_t6.py` is 6,065 lines of single-use ceremony — hash-repair authorization machinery (`repair-no-model-proof-hashes`, NO_MODEL_HASH_REPAIR_TARGET :88-117), duplicated bank/clone/lease/seal logic — for an n=12 screen. The campaign has invalidated more runs on ceremony than on substance; fold the reusable parts (scratch DB, adapter, reader) back onto memphant-eval and delete the rest after the rung-12 decision.
5. **Legacy `l4_*` naming** across trace/types/eval ("L4 exhaustive" → Deep) — dual-name compatibility shims in eval (`legacy_l4_match` lib.rs:1057-1060, :1652) for a pre-production product with no external consumers; rename once, delete shims.
6. **`subject_hint`/`subject`/`predicate` on `RetainRequest`** (types :339-343) — the internal type still carries fields the strict HTTP contract explicitly rejects (contract test :140 rejects `subject_hint`); prune the internal type to match.
7. **Schema-only tables with no writer**: `trust_event` (no producer), `event_outbox` (no consumer), `scope_block` (no surface), `retention_tier` (no job). Fine to keep as frozen contracts per the ledger, but each is a standing store-divergence risk (memory note: write paths vs bounded reads); at minimum they belong on an explicit dormant list in-repo rather than looking live in the migration.
8. **Rung-profile YAML fleet** (`benchmarks/rung4..15-*.yaml`) — synthetic-fixture suites that can no longer promote anything under the provenance rule; keep the handful used as regression gates, archive the rest.
9. **Duplicate spike dirs** (`spikes/python-retain`, `spikes/rust-retain`, `examples/spike`) — WS-0 artifacts, decision recorded; delete.
10. **In-memory vs Postgres dual-store surface** (`InMemoryStore` ~1,000+ lines in core, `memphant-store-testkit` 2,424 lines) — a known divergence-hiding anti-pattern (memory note). Not deletable (tests depend on it), but every new store method now costs 3 implementations; worth a deliberate contraction toward scratch-Postgres-only tests.

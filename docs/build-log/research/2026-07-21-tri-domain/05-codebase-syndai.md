# Syndai Cutover Surface — verified inventory (2026-07-21)

Read-only recon of /Users/sidsharma/Syndai (branch state as-is) + live Supabase (project `wmnzjmrysnzjthldgffh` "Finn", read-only queries). All numbers below are measured, not quoted from the plan.

## Verified inventory deltas vs. brief

| Claim | Verified |
|---|---|
| knowledge/ ~12.7k LOC | **12,696 LOC** (`backend/src/features/knowledge/`, 40+ modules) |
| episodic memory ~21k LOC | **21,790 LOC** (`backend/src/features/memory/` — includes facts, behavioral, persona, graph, privacy, onboarding) |
| coding_execution_attempt_events ~62k rows | **64,159 rows live** (72 MB total, ~50 MB JSONB payload), 111 attempts, 60 runs. NOTE: the MemPhant mining corpus (`benchmarks/data/coding_events_corpus.stats.json`) was mined from a **local dev DB with 359 attempts**, not prod. |
| evalrank feature | **25,119 LOC** (`backend/src/features/evalrank/`) + separate `evalrank-web/` Next.js app + `evalrank-public/` pinned catalog; truth-kernel authority runbook at `docs/runbooks/evalrank-truth-kernel-authority.md` |
| Cutover plan tasks 1–3 done | Tasks 1–3 landed **in MemPhant** (commit f1a1c6d9 "canonical context-binding cutover"). **Nothing from Tasks 6–8 has landed in Syndai**: no `2026_07_15_001_memphant_canonical_memory.py` migration, no `export_memphant_memory.py`, adapter unreplaced. |

### Critical live-data fact: production memory corpora are TINY

Live Supabase (`syndai` schema), exact counts:

| table | rows | size |
|---|---|---|
| coding_execution_attempt_events | **64,159** | 72 MB |
| episodic_memories | **252** | 3.9 MB |
| user_behavioral_embeddings | 54 | 432 kB |
| trajectory_events | 19 | — |
| agent_personas | 8 | — |
| user_facts | **2** | — |
| failure_patterns | 2 | — |
| memory_files | **0** | — |
| knowledge_sources / chunks / sections / versions / agent_knowledge_sources | **0 / 0 / 0 / 0 / 0** | — |
| memory_entities / memory_fact_edges | **0 / 0** | — |
| user_fact_review_events | 0 | — |

The knowledge lane and the graph tables are **empty in production**. The only at-scale corpus is coding execution events. Data-migration risk for the entire cutover is near zero *today*; the window closes as usage grows.

**RLS check (live):** `relrowsecurity = false` on every memory/knowledge table above. Isolation is app-enforced (`user_id` predicates in SQLAlchemy) only.

---

## (a) Cutover-surface table

Consumer → current backend → MemPhant contract that must serve it → parity bar → UX surface affected.

### 1. Knowledge (docs/RAG lane) — prod-empty, strongest gate evidence

Pipeline config (verified in code): ingest = scraper/firecrawl/file_parsing → sectionizer (691 LOC) → chunker **500 tok / 75 overlap / min 100** (`chunking.py`) → `text-embedding-3-small@1536` (`missions/model_catalog.py:KNOWLEDGE_EMBEDDING_MODEL`) → `halfvec(1536)` HNSW `halfvec_ip_ops` (max_inner_product), ef_search=100, iterative scan `relaxed_order`, max 20k tuples. Retrieval = hybrid vector + language-aware BM25 (per-tsconfig GIN, doc text = source name + heading hierarchy + body, `search.py:_bm25_document_text`) → **RRF K=60**, first-stage 20–80 candidates (4× multiplier), top_k ≤ 20 → **Jina reranker v2** (`jina-reranker-v2-base-multilingual`, api.jina.ai, enabled iff `jina_api_key` set — `search_detached.py:76`) with rerank floor + RRF merge fallback → quality tiering + adaptive retry (`retrieval_quality.py`). 25 credits/search (`TOOL_CREDITS["memory_read"]`).

| Consumer | Current backend | MemPhant contract | Parity bar | UX surface |
|---|---|---|---|---|
| `POST /api/v1/agents/{id}/knowledge/search` (`knowledge/controller.py`) | `search_knowledge_detached` (hybrid+RRF+Jina) | `/v1/recall` (resource kind), scope = agent binding | **k=10 comparable-volume CI-clean win — NOT yet won** (R1.5 best +0.083 [floor 0.000]); deep-recall flip +0.142 has 14× evidence-volume asterisk | Mobile agent knowledge search; AttachSheet flows |
| `knowledge_search` canonical tool (`tools/canonical/knowledge_search.py`) → mission chats | same, + `citation_validator.py` provenance (chunk/source/version IDs) | `/v1/recall` + trace/citation IDs (`citation_resource_id`) | same + citation IDs must round-trip so `validate_knowledge_citations` still rejects fabricated cites | **Chat citation chips** (`mission_detail_screen_message_widgets.dart:_CitationsChips`, kind=="knowledge" book icon) |
| `knowledge_save` canonical tool | `KnowledgeService.create_text_source` | `/v1/episodes` retain with `resource` payload (uri/mime/content_hash) | idempotent by content hash; source listing parity | Agent-created knowledge appears in library |
| Source CRUD: `/api/v1/knowledge/sources` (text/url/file, refresh, retry, delete) | knowledge_sources/versions/sections/chunks + Supabase Storage + refresh jobs | retain resource + `/v1/forget` (resource selector); **versioning/refresh/scrape stays Syndai product logic** | delete must erase from MemPhant too (no orphaned recall) | Mobile Memory Hub **Knowledge tab**; snapshot_at delegation-time stable reads must be preserved (bitemporal recall covers this) |

### 2. Episodic + facts + behavioral (the ~22k LOC memory feature) — real live usage

Write paths: `EXTRACT_EPISODIC` job every **10 messages** + on completion/compaction (`memory_job_handlers.py`), source kinds with importance weights (user_correction 1.5 … system_generated 0.3); same job best-effort proposes `user_facts` candidates (`fact_candidate_extractor.propose_facts_from_transcript`, provenance-grounded). Compaction, rollup (threshold 10), dedup loop, async re-embed (`MEMORY_EMBEDDING_MODEL = text-embedding-3-small`). Behavioral: `behavioral_analysis.py` → `user_behavioral_embeddings`. Retention: trust audit (tainted+trust<40 hard-delete after 7d; 40–60 archive; >60 LLM re-eval), archive trust<30 @90d, hard-delete archived @180d, L1 row-cap prune for trajectory/failure tables (those tables STAY in Syndai per plan).

Read path: `MemoryContextLoader` (chokepoint, `context_loader.py`) — hot/full context, per-layer token budgets (`context_loader_types.py`): user_facts 400, persona 300, behavioral 700, **episodic 1200**, trajectory 300, file_memory 800, failure_hints 200, **total_max 2500**. Episodic retrieval is its own hybrid RRF (semantic 1.0 / BM25 0.5, K=60).

| Consumer | Current backend | MemPhant contract | Parity bar | UX surface |
|---|---|---|---|---|
| Prompt context injection (every mission) | `MemoryContextLoader.load_hot_context/_load_full_context` | `/v1/recall` fast mode, budget-packed, per-layer mapping | **p50 <200 ms / p95 <500 ms**, ≤2,500 tokens, no citation regressions | Invisible but highest-stakes: every agent reply |
| `recall` + `correct_memory` canonical tools | loader + `correction_service.py` | `/v1/recall`, `/v1/correct` (replace/invalidate) | correction visible on next recall; trace authority | Agent self-serve memory |
| `GET /api/v1/memory/search` (user-facing, same loader path) | loader across facts/episodic/behavioral | same recall | same ranked hits | Memory Hub search |
| Fact review: `/api/v1/facts` confirm/dismiss/pending, `user_fact_review_events` | proposal rows in `user_facts` | **proposal workflow stays Syndai**; on confirm → retain/correct, store returned unit ID (plan Task 7) | confirmed-fact readback identical | Memory Hub **Facts tab**, fact_dialog, pending review |
| Digest/timeline: `GET /api/v1/memory/digest`, `/timeline` | Syndai projections | stay Syndai (presentation), backed by MemPhant units | render-identical | Memory Hub **Timeline/Conversations tabs** |
| Reinforce/archive/forget: `/api/v1/memory/{type}/{id}/reinforce|archive`, `/forget/project/{id}`, `/forget/mission/{id}` | `scoped_forget_service.py` etc. | `/v1/mark`, `/v1/forget` (unit/scope selectors) | forget = gone from recall + traces | Memory management UX |
| Privacy: `POST /api/v1/memory/export`, `DELETE /api/v1/memory/all` | `privacy_controller.py` (770 LOC), delete-pending gates on all jobs | `/v1/forget` subject selector + **subject_generation** stale-write rejection (landed in MemPhant Task 3); export must include MemPhant units | complete erasure proof across units/episodes/resources/embeddings/traces | Settings privacy screens |
| Graph: `GET /api/v1/memory/graph/traverse`, `retrieve_context_graph` tool | `graph_traversal.py` over memory_entities/fact_edges (**0 rows prod**) | **none — deleted** (plan Task 8; immediate lineage only) | negative contract tests | Mobile `memory_knowledge_tab.dart` entity affordances removed |
| Persona | `agent_personas`, persona_service/evolution | **stays Syndai** | n/a | persona trait UX |
| File memory (dogfood surface) | `memory_files` (**0 rows prod**) + `file_service.py` | retain resource / recall / correct / forget (adapter exists) | trace-compare fixture (`Memphant/examples/syndai/file-memory-trace-compare.yaml`) | Memory Hub **Files tab** |

### 3. Coding-continuity corpus

`coding_execution_attempt_events` — schema (verified `coding/execution_models.py`): `(coding_run_id FK, attempt_id FK, sequence ≥0 unique per attempt, event_type ≤80 chars, subtype, payload JSONB, occurred_at)`. **Single writer**: `execution_attempts.py` (`_events_for_result` — attempt started/completed payloads from Claude Code stream-json). No product read path found outside coding feature internals — it is an operational transcript, i.e. a pure ingestion corpus for MemPhant.

MemPhant ingest design: (1) **one-shot backfill** — 64,159 rows / ~50 MB grouped by attempt (111) → retain as episodes with `source_ref = coding_execution_attempt_events:{attempt_id}:{sequence}` and content hash (idempotent, replayable); (2) **streaming** — hook after event persist in `execution_attempts.py` via Syndai's existing durable jobs (plan constraint: reuse durable jobs, no new service). Table itself is NOT in the approved drop set — it stays as operational ground truth.

### 4. MemPhant adapter boundary in Syndai — wired vs stubbed (verified)

- **Wired**: config (`MEMPHANT_FILE_MEMORY_DOGFOOD_ENABLED` default false, `MEMPHANT_API_BASE_URL/KEY/REQUEST_TIMEOUT_SECONDS`, `config.py:561-571`); `context_loader.py:_load_memphant_agent_file_memory` active-read with legacy fallback on None/error; shared httpx client; recall→file-memory row mapping with `memphant_trace_id`/`memphant_unit_id`.
- **Stubbed/stale**: `memphant_dogfood_adapter.py` payloads use the **pre-cutover contract** — `tenant_id`, `allowed_scope_ids`, `scope_id`, `actor_id` in public bodies. MemPhant's landed Tasks 1–3 (`deny_unknown_fields`, tenant-from-bearer, context bindings) now **reject exactly these fields with 422**. Flipping the flag on today would 4xx every call and silently fall back to legacy — the precise "silent legacy fallback" the plan bans. correct/forget/retain payload builders exist but have no production callers. Plan Task 6 (SDK-based `MemphantMemoryAdapter` + context bindings + `memory_degraded`) is the unbuilt bridge.

### 5. Web/mobile UX inventory

- **Web** (`/web`): marketing only — no product memory/knowledge UI. Product client is the Flutter app.
- **Mobile** (`mobile/lib/features/memory/`): `memory_hub_screen.dart` — **5 tabs: Facts | Conversations | Knowledge | Files | Timeline** (Drift reactive streams; offline projections in `memory_tables.dart`/`memory_expanded_tables.dart` fed by sync outbox events — cutover must keep publishing sync events); `fact_dialog.dart`; entity chip widgets (graph — slated for removal); `memory_experience_repository.dart` (digest/timeline/search/reinforce/archive/correct); mission chat citation chips (`missions/screens/mission_detail_screen_message_widgets.dart:222-311`); settings privacy screens (export / delete-all).
- **evalrank-web**: separate public leaderboard app (not a memory consumer).

---

## (b) Minimal-parity set for a first real cutover slice

Ranked by (user value shipped) ÷ (surface area), given the live-data reality:

**Slice 0 — contract proof, ~zero user risk (do first, this week-scale):** Rebuild the adapter per plan Task 6 against the NEW strict contract (context bindings + SDK), flip dogfood for **agent file memory + confirmed facts**. Prod has 0 files and 2 facts — backfill is trivial, blast radius ~nil, but it proves bind → retain → recall → trace → correct → forget end-to-end against real Postgres with two-user isolation. Ships no visible value; ships the rails.

**Slice 1 — first real user value: episodic conversation memory.** Cut `MemoryContextLoader`'s episodic layer + `recall`/`correct_memory` tools + reinforce/archive/forget endpoints to MemPhant, keeping Syndai's extraction jobs, fact-review workflow, digest/timeline projections, and retention *presentation* untouched. Needs exactly: retain(episode, source_ref, observed_at), recall(fast, budget 1200), correct(replace/invalidate), forget(unit + mission/project scope selectors), trace IDs for citations. Backfill = 252 rows (minutes). Parity bar: memory-eval no-regression + hot-path SLO (p50<200/p95<500) + identical Conversations tab + two-user isolation on real Postgres. This is the smallest slice a user can actually feel (agent remembers across missions), and it exercises the bitemporal/correction machinery MemPhant is differentiated on.

**Slice 2 — knowledge lane, gated on the k=10 bar.** Prod knowledge tables are empty, so there is no migration and no incumbent user data to regress — but the comparable-volume parity gate (k=10) is not yet won (+0.083, CI floor exactly 0.000; deep-recall +0.142 carries the 14× evidence-volume asterisk). Do not cut the sync in-chat `knowledge_search` path until the rank-compression lever (truncated-input rerank / top-32 / smaller model — R1.5 follow-ups) lands inside the 1.5 s ceiling. An acceptable interim: deep-recall config for async surfaces only.

**Slice 3 — coding-continuity ingestion (net-new value, not a cutover).** Backfill 64k events + streaming retain hook. No incumbent to beat — this is MemPhant-only capability (40Q code-lane golden exists; full mined set deferred to R4 per R0 verdict).

## (c) Data-migration sketch

| Source table | Rows (live) | MemPhant call | Backfill est. |
|---|---|---|---|
| memory_files | 0 | retain resource | nil |
| user_facts (confirmed only) | 2 | retain fact (fact_key) | seconds |
| episodic_memories | 252 | retain episode, direct import, **no re-extraction** | minutes |
| user_behavioral_embeddings | 54 | retain belief/procedure; **re-embed** (1536 OpenAI vectors are not portable to modernbert@1024 — embeddings never migrate, only text) | minutes |
| memory_entities / memory_fact_edges | 0 / 0 | none — drop after zero-row proof | nil |
| knowledge_* | all 0 | none — users re-ingest; functional cutover only | nil |
| coding_execution_attempt_events | 64,159 (~50 MB) | retain episodes grouped by attempt (111), source_ref = table:attempt:sequence, content-hashed idempotent | tens of minutes incl. embedding; embedding cost trivial (~12–13M tokens → <$0.30 API, $0 local) |

Export by stable legacy source reference + content hash, compare counts/checksums/traces, then switch reads and writes (plan Task 7 mechanics). Total backfill wall-clock: **under an hour, dominated by coding events**. The approved drop set (6 tables) currently holds ~310 live rows total — the destructive step is small *now*; every week of delay grows it.

## (d) Risks

1. **Stale adapter contract (active landmine).** Syndai's only wired MemPhant path sends fields the canonical MemPhant now 422-rejects; the failure mode is silent legacy fallback. Task 6 must land before any dogfood flip; add a contract-drift test pinned to `openapi/memphant.v1.json`.
2. **Isolation model swap.** Today: no Postgres RLS anywhere, app-level user_id filters. Target: MemPhant tenant-RLS + server-derived subject/scope predicates. The cutover must be proven with two-user real-Postgres leakage tests (plan Tasks 6/9); any gap is a data-exposure incident, not a bug. Supabase specifics: runtime role non-owner/non-BYPASSRLS, direct `:5432` only (pgbouncer `:6543` rejected for persistent SQLx).
3. **Latency budgets per surface.** Context injection: p50<200 ms/p95<500 ms, ≤2,500 tokens — hot path of every reply. `knowledge_search`: synchronous in-chat tool; cross-encoder rerank measured **12.9–13.6 s/query** is retired against a pre-registered 1.5 s ceiling; the k=10 accuracy bar must be won *within* that ceiling. Deep-recall config trades 14× reader-token volume (cost) for accuracy — fine for async panels, wrong for chat.
4. **Cost wins are real but conditional.** Cutover eliminates: Jina rerank API (per-search cost + external egress of user content — a privacy win too), and can eliminate OpenAI embedding dependency (text-embedding-3-small → local modernbert@1024). The embedder swap is free ONLY while corpora are tiny/empty — re-embedding cost grows with usage; do it in the first slice or price a re-embed later.
5. **Evidence-volume asterisk is the honesty risk.** The docs-gate flip (+0.142 pooled) is real engine-vs-engine at each side's chosen operating point but buys accuracy with reader-context volume; comparable-volume (k=10) parity is NOT won (best +0.083, floor 0.000). Shipping on the flip alone and calling it "beats Syndai" would not survive scrutiny — R6 replacement wiring stays locked per the pre-registered rule.
6. **UX preservation debt.** 5-tab Memory Hub, citation chips (validator rejects unknown chunk IDs — MemPhant trace/citation IDs must be authoritative), fact-review proposal flow, privacy export/delete-all (delete-pending gates every memory job), mobile Drift offline projections + sync outbox events. Any missed event emission breaks offline UX silently.
7. **Golden-set provenance gaps.** Coding corpus mined from local dev DB (359 attempts) vs prod (111) — distribution drift; code-lane parity never run on full mined set (R0 n=40 sample: no CI-clean winner, revisit R4). Docs goldens are self-mined from Syndai's corpus — fine for internal gates, insufficient for public claims (see e).
8. **Concurrent-worktree hazard (operational).** `/Users/sidsharma/Syndai/.claude/worktrees` has an active unrelated review; any cutover execution must respect the plan's "existing WIP untouched" constraint.

## (e) What evalrank needs to publish third-party-credible MemPhant results

Evalrank already has the right skeleton for credibility (verified): truth-kernel authority with `schema_generation`/`manifest_sha256`/`public_contract_sha` fencing, pinned catalog manifests, decision receipts, deterministic public read projections with ETags, benchmark provenance modules. To publish MemPhant results that a third party would accept:

1. **Neutral instruments, not self-mined sets.** Publish on LME-V2 / MemBench / SWE-ContextBench / SWE-Explore (per July-2026 landscape: all vendor numbers are self-run; empty neutral leaderboards are first-mover slots). Syndai-mined goldens (docs v1/v2, 40Q code) stay as *internal* gates; publishing them as the headline is circular.
2. **Conflict-of-interest disclosure as a first-class artifact.** Same owner operates the leaderboard and a listed engine; the listing must carry a machine-readable COI flag and identical pipeline treatment (same runner, same receipts) for competitor engines.
3. **Pre-registration in the public record.** Evalrank should surface the frozen decision rules (the `.superpowers/sdd/*-plan.md` discipline already practiced internally) as published methodology versions before runs, with the receipt chain proving rules predate results.
4. **Reproducibility bundle per row**: pinned corpus lock (sha256 manifests already exist, e.g. `syndai_docs_golden.lock.json`), exact engine revision, container/runner digest, judge+reader lattice config (same-lattice pairing rule), raw evidence JSONL + per-question provenance, paired-bootstrap CIs — downloadable behind the existing decision-receipt endpoint.
5. **Volume/cost axis on the scoreboard.** The R1 lesson institutionalized: report accuracy *at declared evidence-volume and latency operating points* (k, budget tokens, reader-chars, p50/p95), so a +0.142-style win cannot hide a 14× context-volume subsidy.
6. **Third-party re-run path**: a public runner (MemPhant is Apache-2.0 and self-hostable — the incumbent-vs-MemPhant harness pattern in `Memphant/scripts/gate_run_syndai.py`/`gate_run_memphant.py` generalizes) so outsiders can reproduce a row from lock files without Syndai credentials.

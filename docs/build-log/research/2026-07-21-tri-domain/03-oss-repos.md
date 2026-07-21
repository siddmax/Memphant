# OSS Repo Study — Competitor Architecture Patterns (2026-07-21)

Method: GitHub API (`gh api`), raw.githubusercontent.com reads, WebFetch on docs. No clones into worktree; no code copied. All observations from CURRENT main-branch state as of 2026-07-21.

---

## 1. mem0ai/mem0 — 61.4k stars, pushed 2026-07-21

**Storage layout**
- Vector-store abstraction with 27 backends (pgvector, qdrant, elasticsearch, turbopuffer, s3_vectors, …) — `mem0/vector_stores/`.
- SQLite sidecar (`memory/storage.py` SQLiteManager) for the audit history: `history(id, memory_id, old_memory, new_memory, event[ADD|UPDATE|DELETE], created_at, updated_at, is_deleted, actor_id, role)` + a raw `messages` table.
- NEW: a second vector collection, the **entity store**, holding extracted entities with `linked_memory_ids` back-references to the memories that mention them.
- `mem0/graphs/` is **gone**. Removed in the "v3 pipeline" cutover (commit 2026-04-14, PR #4805: "port v3 pipeline with hybrid search, entity extraction, and additive scoring"). Graph memory is dead in OSS mem0.

**Taxonomy**: flat fact strings + metadata (`user_id`/`agent_id`/`run_id`, actor, custom metadata). Procedural memory exists only as an alternate extraction prompt. No episodic/semantic/belief distinction.

**Write path**
- V2 (legacy, still in tree): LLM fact extraction (FACT_RETRIEVAL_PROMPT — "Personal Information Organizer", few-shot, user-messages-only variant) → embed each fact → search similar existing → second LLM call decides ADD/UPDATE/DELETE/NONE (DEFAULT_UPDATE_MEMORY_PROMPT) → destructive apply.
- V3 (current default direction): **ADD-only additive extraction**. "Your sole operation is ADD: identify every piece of memorable information and produce self-contained, contextually rich factual statements." Related existing memories are referenced via `linked_memory_ids` (UUIDs) instead of being mutated. Entities extracted and upserted into entity store. This is a public retreat from LLM-arbitrated destructive updates.

**Read path**: top-k vector search, threshold 0.1; hybrid BM25 (`lemmatize_for_bm25`, `normalize_bm25`) fused with semantic when the backend implements `keyword_search` (qdrant/es/pgvector); optional reranker stage (RerankerFactory); `expiration_date` filter hides expired memories unless `show_expired`.

**Consolidation/decay**: none in OSS. `decay=True` **raises an error** — decay and "temporal" are stub notices (`notices.py`: `decay_stub`, `temporal_stub`, upsell events) pointing at the paid platform. Only `expiration_date` (hard date) exists in OSS.

**Multi-tenant/auth**: filter-scoping only (user_id/agent_id/run_id); no authz in OSS; platform holds the real tenancy.

**Distribution**: OpenAI-compatible proxy dir, CLIs, plugins for OpenCode/OpenClaw/Pi-agent, Vercel AI SDK provider — plugin-per-agent-runtime strategy, ~biweekly releases.

**Pain (top-commented open issues)**
- #5245 Silent memory loss when batch embedding partially fails in V3 add pipeline (no write-path durability).
- #4892 Concurrent AsyncMemory writes corrupt Qdrant HNSW index.
- #4988 Full table scan on every delete/update in `_remove_memory_from_entity_store` (back-reference maintenance is O(N)).
- #4884 BM25 + entity extraction hardcoded to English.
- #3918 Unterminated JSON from extraction LLM kills the write.

---

## 2. letta-ai/letta — 23.9k stars, pushed 2026-07-03

**Storage layout**: Postgres via SQLAlchemy ORM + alembic. Core memory = **blocks** (`label`, `value`, char `limit`, `read_only`, template lineage); archival memory = **archives** containing **passages** (text + embedding + tags + source/file provenance), each archive pinning its own `vector_db_provider` + embedding config. Files/folders/sources are first-class (`files_agents_manager` tracks which files are "open" in context, `max_files_open`).

**File-based memory (2026 direction)**: `Memory.compile()` renders blocks as (a) standard sections, (b) **line-numbered** blocks, (c) **git-style tree with frontmatter** (`_render_memory_blocks_git`, `block_manager_git.py`), or (d) a **filesystem view** (`_render_memory_filesystem`) including a skills tree. They are converging on "memory looks like a repo/filesystem to the agent" — same direction as the Anthropic memory tool.

**Write path**: agent-driven tools (`core_memory_append/replace`, archival insert); no background extraction on the hot path.

**Read path**: agentic — agent calls archival search tools; passages vector search; conversations API ("agent-direct mode", 0.16.6).

**Consolidation**: sleep-time compute = `SleeptimeMultiAgentV4`: a background agent group fires every `sleeptime_agent_frequency` turns (or every turn if unset), issues background tasks to reorganize memory; variants for doc-ingest and voice. Separate summarizer service for context compaction.

**2026 retirements/changes**: 0.16.7 (Mar 2026, 173 commits) — **block char limits no longer enforced** (breaking); context default 32k→128k; compaction runaway-loop fixes; duplicate skills-block fixes; conversation forking; request-scoped system prompt overrides. 0.16.8 (May) — pickle→JSON for sandbox transport (security). Release cadence slowed markedly in 2026 (0.16.x maintenance; energy visibly on cloud + Letta Code).

**MCP surface**: MCP servers are first-class managed objects (`mcp_manager`, `mcp_server_manager`) whose tools agents attach; 2026 security work blocked internal MCP targets (SSRF class).

**Multi-tenant**: organizations + users pervasive in schemas; real server-side tenancy.

**Pain**: memory-sync fragility (duplicate blocks), compaction loops, context preservation on model override — the block-sync machinery between DB state and prompt state is their recurring bug farm.

---

## 3. getzep/zep + getzep/graphiti

**zep** (4.8k): repo is now explicitly *not the product* — examples/integrations for **Zep Cloud**; Community Edition moved to `legacy/` marked deprecated/unsupported. The operable temporal-KG memory server is closed. OSS engine = Graphiti. They ship `benchmarks/` (LoCoMo, LongMemEval) + `zep-eval-harness` in the examples repo — benchmarks as marketing surface.

**graphiti** (29k, pushed 2026-07-20)
- **Storage**: temporal "context graph": episodes (raw provenance) → entity nodes with evolving summaries → fact edges (triplets) with `valid_at`/`invalid_at` validity windows; community nodes; `group_id` namespacing; drivers for neo4j/falkordb/kuzu/neptune; pluggable embedder + cross_encoder; hybrid retrieval (semantic + BM25 + graph traversal).
- **2026 reality check (releases)**: v0.29 "Major efficiency and Internal Architecture changes"; a continuous run of "efficiency gains" releases (0.27.x pre-releases about ingestion cost) — **LLM-in-the-write-path cost is their tax**; 0.28.2 SECURITY: Cypher injection via search filters; 0.28.1 removed diskcache; 0.27.1 "fix duplicate info appearing in summaries" (dedup pain).
- **Open-issue pain**: label propagation can loop forever on non-converging graphs (#402); pydantic validation errors on `ExtractedEntities` (#912); local models (Ollama) can't reliably produce their structured outputs (#868); **API returns 202 but episodes never persist** (#566 — async ingestion opacity); FalkorDB driver bugs; node types not set.
- **Verdict**: temporal-KG claims are real but the maintenance reality is: per-driver bug matrix, injection surface in generated Cypher, LLM extraction fragility, silent async ingestion failures, and permanent cost-optimization treadmill. Zep keeps the version that survives production closed.

---

## 4. mastra-ai/mastra — Observational Memory (26.4k, pushed 2026-07-21)

Mechanics (from `packages/memory/src/processors/observational-memory/` + docs):
- **Actor/Observer/Reflector** triad. Observer watches the thread; when unobserved message tokens exceed `messageTokens` (default **30,000**; can be a `{min,max}` range keyed to how full the observation space is), it emits observations. Reflector condenses when observations exceed `observationTokens` (default **40,000**).
- Defaults: both agents `google/gemini-2.5-flash`; Observer temp 0.3 (thinkingBudget 215), Reflector temp 0 (thinkingBudget 1024); `maxOutputTokens` 100k; token-tiered model routing (`ModelByInputTokens`) and retry/fallback arrays supported.
- **Async buffering**: observations pre-compute in background every `bufferTokens` = 20% of threshold; `bufferOnIdle` end-of-turn buffering; `blockAfter` forces a synchronous observation only as last resort; `activateAfterIdle: 'auto'` uses **provider-aware prompt-cache TTLs (5min–24h)**; `activateOnProviderChange` re-anchors when the actor model changes.
- **Retention**: after activation keep ~20% of raw message tokens (`bufferActivation: 0.8`) so the tail of the conversation stays verbatim.
- **Record format**: append-only date-headed bullet hierarchy with HH:MM timestamps and priority emojis (🔴 critical / 🟡 notable), e.g. `Date: 2026-01-15` / `- 🔴 12:10 User is building…`. Append-only + stable prefix = **prompt-cache alignment** (explicit design goal).
- **Robustness details worth stealing**: degenerate-output detection on Reflector (repetition-loop detect → discard/retry); attachment forwarding policy by observer-model capability; extractors for typed side-channel fields; observation markers embed a config snapshot for debugging.
- Storage via Mastra storage providers (pg/libsql/mongodb/convex), thread or resource scope. Claimed compression 5–40x (no benchmark cited in docs).

---

## 5. cognee / memobase / supermemory

**topoteretes/cognee** (29k, v1.4.0 Jul 2026)
- Substrate: kitchen-sink — `infrastructure/databases/{relational, vector, graph, hybrid, unified, provenance, cache}` over many backends; ECL (extract→cognify→load) pipelines; `memify_pipelines`; in-repo eval_framework; alembic; datasets with per-dataset DB handling and permissions (multi-tenant via users/datasets).
- 2026 state: growth via **paid hackathon backlog** (top issues are all "Hackathon:" — DLT ingestion scaling, LLM-mocked tests, PageRank/centrality retrieval, VS Code extension, deployment e2e). Notably: issue #3516 "**Cognee-rs**: Perf: optimize LLM API call usage in cognify" — a Rust port is in motion. Pain mirrors graphiti: cognify LLM cost (concurrency/batch/prompt-caching optimization pleas).

**memodb-io/memobase** (2.8k, last push 2026-01-11 — stalled; team pivoted to Acontext "context data platform")
- Pattern: **user-profile-as-materialized-view**: per-user profile (topic/subtopic JSON slots) + event timeline; per-user write buffer batches chats before LLM processing (cost amortization); explicit "no agents in the pipeline" cost stance; reads are plain SQL, <100ms, zero retrieval machinery for the common case.
- Lesson: the profile read-path idea is good; the company outcome (pivot away) says profile-only memory is not a durable product.

**supermemoryai/supermemory** (28.5k, pushed 2026-07-21)
- OSS repo is **distribution shell, not engine**: apps (web, MCP, browser/raycast extensions, memory-graph playground) + SDK packages (ai-sdk, openai/cartesia/pipecat python SDKs, agent-framework) + connectors. Engine is closed: cloud API + "supermemory local" one-binary curl-install.
- Claims: "#1 on LongMemEval, LoCoMo, ConvoMem", "95% Recall@15 with 99.4% context reduction, ~50ms user profiles" — **self-reported**, README-marketed. Multi-modal extractors (OCR, transcription, AST-aware code chunking), hybrid RAG+memory single query, auto-forgetting of expired info (claimed).
- Lesson: benchmark-first README marketing works (28k stars); nothing verifiable in the repo.

---

## 6. Anthropic memory-tool contract + Claude Code (distribution target)

**API memory tool** (`{"type": "memory_20250818", "name": "memory"}`, GA, no beta header; all Claude 4+ models):
- Client-side: Claude emits commands, *our handler* executes against any storage and returns strings. `/memories` is a virtual path prefix the handler maps to real storage (per-user dir, DB keys, …). This is exactly the seam where MemPhant can back any agent's file memory.
- Commands + exact reference semantics:
  - `view {path, view_range?}` — dirs: listing **up to 2 levels deep**, `{size}\t{path}` lines, human sizes, hidden files + node_modules excluded. Files: `"Here's the content of {path} with line numbers:"` then **6-char right-aligned, 1-indexed, tab-separated** line numbers; >999,999 lines → error; text views truncate at 16k chars (Claude expects to page with `view_range [start, end|-1]`); image files (.jpg/.jpeg/.png) viewable.
  - `create {path, file_text}` — tool description says create-or-overwrite; reference behavior errors `"Error: File {path} already exists"` (either is valid).
  - `str_replace {path, old_str, new_str?}` — old_str must appear exactly once; duplicate → error naming the line numbers; success returns edit confirmation + snippet; omitted new_str = deletion.
  - `insert {path, insert_line, insert_text}` — after line N, 0 = top.
  - `delete {path}` — recursive on dirs; must reject deleting `/memories` root.
  - `rename {old_path, new_path}` — no overwrite; must reject renaming root.
  - Errors via `tool_result` with `is_error: true`.
- API auto-injects a system-prompt protocol: "ALWAYS VIEW YOUR MEMORY DIRECTORY BEFORE DOING ANYTHING ELSE… ASSUME INTERRUPTION: your context window might be reset at any moment." So agents *will* issue `view /memories` at task start — first-call latency matters.
- Security is delegated to the handler: path-traversal rejection, size caps, sensitive-data stripping, expiry ("periodically delete memory files that haven't been accessed in a long time" — decay is *recommended to the implementer*).
- SDK seams: Python/C# `BetaAbstractMemoryTool` subclass, TS `betaMemoryTool(handlers)`, Java `BetaMemoryToolHandler` — a MemPhant-backed handler drops in with zero agent changes.
- Pairs with context editing + server-side compaction; documented multisession pattern = progress log + feature checklist files (memory as recovery mechanism).

**Claude Code product memory**: CLAUDE.md hierarchy (managed policy → `~/.claude/CLAUDE.md` → project → `CLAUDE.local.md`), `@path` imports (4-hop), `.claude/rules/*.md` with `paths:` frontmatter for lazy load; **auto memory**: `~/.claude/projects/<project>/memory/` with `MEMORY.md` index loaded first **200 lines / 25KB** + on-demand topic files; `modified` ISO-8601 frontmatter stamped on writes; machine-local, git-repo-scoped (shared across worktrees). The index-file + topic-files pattern with hard load budget is the de facto standard MemPhant's file view should be able to serve.

---

## 7. LongMemEval + LongMemEval-V2 (submission mechanics)

**V1** (958 stars): 500 questions; abilities: IE, multi-session reasoning, knowledge updates, temporal reasoning, abstention (`_abs` ids); datasets `longmemeval_s` (~115k tok), `_m` (~500 sessions), `_oracle`; use the **cleaned** HF dataset (2025/09). Submission = JSONL `{question_id, hypothesis}`; official scorer `src/evaluation/evaluate_qa.py` (GPT-4o judge → `autoeval_label`); `has_answer` turn labels + `answer_session_ids` enable turn/session-level recall accuracy reporting.

**V2** (separate repo `xiaowu0162/LongMemEval-V2`, May 2026, 96 stars, leaderboard live and near-empty — first-mover window per benchmark-landscape memory):
- 451 curated questions over multimodal **web-agent trajectory** haystacks (up to 500 trajectories / 115M tokens), web + enterprise domains, small/medium leaderboard tiers. Abilities: static state recall, dynamic state tracking, workflow knowledge, environment gotchas, premise awareness.
- **Harness contract we must implement** (`memory_modules/memory.py`): subclass `Memory` with `insert(trajectory: dict)` and `query(query, query_image?) -> list[MemoryContextItem{type: "text"|"image", value}]`; `memory_config` for persistence/reload reconciliation (`reconcile_loaded_memory_config`); optional `configure_runtime`, `post_query_hook` (online-learning hook); harness token-budget-truncates returned context and measures **answer accuracy + query latency** (LAFS leaderboard scoring in `leaderboard/`).
- **AgentRunbook-C reference impl** (`agentrunbook_c.py`, extends `codex.py`): ingestion renders per-trajectory `TRAJECTORY_SUMMARY_CONCISE.md` / `TRAJECTORY_SUMMARY_FULL.md`; query time spins a **Codex coding agent in a sandbox dir** (INSTRUCTION.md + question.json + trajectories/ + inspection scripts), agent explores files and writes `memory_module_output.json`. Default codex model gpt-5.4-mini @ xhigh reasoning.
- **Evidence-status protocol** (in codex baseline): answer policy keyed to evidence status — `directly_supported → answer_normally`, `contradicts_premise → state_premise_false`, `near_match_only → say_exact_target_not_found`, `insufficient → abstain_unknown`. A clean, benchmark-blessed formalization of calibrated recall.

## 8. SWE-bench / SWE-ContextBench

- SWE-bench (5.5k): harness = predictions `{instance_id, model_name_or_path, model_patch}` + dockerized eval (`sb-cli` for remote). Memory integrates at the *agent* layer, not the harness layer. SWE-smith (710★, active) generates tasks/environments at scale — useful for building memory-training corpora.
- **SWE-ContextBench** (arXiv 2602.08316, Feb 2026): SWE-bench Lite + 99 related tasks linked by real issue/PR dependencies → **task sequences with shared context**, 51 repos, 9 languages; metrics: resolution accuracy, time, token cost; finding: summarized+retrieved prior experience improves accuracy and cuts cost on hard tasks. **No public harness repo discoverable on GitHub yet** (gh search empty; paper page lacks code link) — running it means implementing the sequence protocol from the paper; publishing a reference memory adapter is an open first-mover slot.

## 9. 2026 newcomer triage (>1k stars, previously unmapped)

- **volcengine/OpenViking — 27k stars, created 2026-01** ("Self-evolving Context Database. Unify Agent Memory, Knowledge RAG and Skills"). Filesystem-paradigm context DB: memories/resources/skills as one directory tree; **L0/L1/L2 tiered context loading**; directory-recursive retrieval (directory positioning + semantic search); **visualized retrieval trajectories** (observability as a feature); auto session compression to long-term memory; desktop Helper that integrates Claude Code/Codex/Cursor/Trae session traces. **This is the closest strategic collision with MemPhant's tri-domain one-substrate thesis**, with ByteDance/Volcengine weight and hosted Studio. Benchmarks self-run (User Memory / Agent Memory / KB-QA, May 2026 update).
- **vectorize-io/hindsight — 18.7k stars, created 2025-10**: "agent memory that learns"; 2-line LLM-wrapper integration; single Docker image with embedded Postgres (`.pg0`); paper (arXiv 2512.12818); LongMemEval SOTA claim **independently reproduced by Virginia Tech Sanghani Center and The Washington Post** — the only vendor with third-party reproduction; explicitly frames all other vendors' scores as self-reported. Raises the evidence bar MemPhant must clear.
- **TencentCloud/TencentDB-Agent-Memory — 9.2k stars, created 2026-04**: fully local, zero external APIs; **symbolic short-term memory** (offloads tool logs, condenses to compact Mermaid symbols) + **layered long-term memory** (personas/scenes, "reject flat vector piles" and "reject irreversible lossy summarization"); eval methodology worth noting: **continuous long-horizon sessions** (50 consecutive SWE-bench tasks per session) rather than isolated turns; claims −61% tokens / +51% relative pass on WideSearch via OpenClaw plugin.
- Also seen: Shichun-Liu/Agent-Memory-Paper-List (2.3k), IAAR-Shanghai/Awesome-AI-Memory (1.1k) — survey/link lists, no engines. No other missed >1k-star engines surfaced.

---

## Full pattern table

| System | Storage | Taxonomy | Write path | Read path | Consolidation | Decay/forget | Multi-tenant | Retired/changed in 2026 | Top pain |
|---|---|---|---|---|---|---|---|---|---|
| mem0 | 27 vector backends + SQLite history + entity vector collection | flat facts + metadata; procedural via prompt | V3: LLM ADD-only extraction + `linked_memory_ids`; V2 legacy ADD/UPDATE/DELETE | top-k + BM25 hybrid + reranker, threshold 0.1 | none (OSS) | `expiration_date` only; decay = paid stub | filter-scoping only | **graph memory removed**; destructive updates → additive V3 | silent loss on partial embed failure; concurrent-write index corruption; O(N) entity cleanup; English-only |
| letta | Postgres ORM; blocks + archives/passages; files/folders | core blocks / archival passages / files / skills | agent tool calls (append/replace/insert) | agentic tool search; conversations API | sleep-time agent every N turns; summarizer | none built-in | orgs + users, real | block limits unenforced; git/filesystem memory rendering; slowed OSS cadence | block-sync dupes; compaction loops |
| zep/graphiti | temporal KG (neo4j/falkor/kuzu/neptune): episodes→entities→edges w/ valid_at/invalid_at, communities, group_id | episodes/entities/facts/communities | LLM extraction+resolution per episode (costly) | hybrid semantic+BM25+traversal, cross-encoder | entity summary updates, community detection | edge invalidation (supersede), no decay | group_id namespaces | Zep CE killed (legacy/); diskcache removed; injection hardening; perpetual efficiency releases | ingestion cost; dedup; 202-but-not-persisted; local-LLM structured output; per-driver bugs |
| mastra OM | app storage providers (pg/libsql/mongo/convex) | append-only observation log + working memory | Observer @30k tok (async buffer @20%); Reflector @40k tok | none — observations ride in prompt; append-only | Reflector condensation, degenerate-detect | reflection compression only | thread/resource scope | new subsystem (2026) | (young; explorations/prod-readiness docs in-tree) |
| cognee | relational+vector+graph+hybrid+provenance, many backends | datasets→graph entities | ECL/cognify (LLM-heavy) | graph+vector search, adding PageRank | memify pipelines | ad hoc | users/datasets/permissions | v1.x; hackathon-driven; **Cognee-rs Rust port starting** | cognify LLM cost; complexity sprawl |
| memobase | SQL profile slots + event timeline | profile topics/subtopics + events | buffered batch LLM profile updates | direct SQL profile read <100ms | buffer flush | profile slot overwrite | per-user | stalled (Jan 2026); team pivoted to Acontext | product, not tech |
| supermemory | closed engine (cloud + local binary); OSS = SDK/app shell | facts+profiles+docs (claimed) | auto extraction (claimed) | hybrid RAG+memory single query (claimed) | claimed | claimed auto-forget | API keys | — | unverifiable claims, self-run benchmarks |
| Anthropic memory tool | client-implemented; `/memories` virtual FS | files | agent-driven file edits (6 commands) | agent-driven `view` | agent-driven reorganization (prompted) | recommended to implementer | handler's problem | GA'd, no beta header; SDK helpers | traversal/size/security pushed to handler |
| OpenViking | context DB, filesystem paradigm, L0/L1/L2 tiers | memories/resources/skills unified | session auto-compression + commits | directory-recursive + semantic, visualized trajectories | self-iteration | tiered demotion | hosted Studio + local | new (Jan 2026) | — (young, hype-heavy) |
| hindsight | embedded Postgres, single binary/Docker | (paper: learn-over-recall framing) | LLM-wrapper auto capture | auto recall in wrapper | "learns" | — | cloud + local | new | — |
| TencentDB-AM | local 4-tier pipeline | symbolic ST (Mermaid) + layered LT (personas/scenes) | symbolization + distillation | layered lookup | layer promotion | explicit anti-lossy stance | local | new (Apr 2026) | — |

## Cross-cutting lessons

1. **The destructive-update write path is being abandoned by the market leader.** mem0's V3 is ADD-only with linking; TencentDB explicitly "rejects irreversible lossy summarization"; Graphiti supersedes via `invalid_at` instead of deleting. MemPhant's retain/correct split (corrections as first-class, no destructive rewrite at write time) is now the mainstream-validated design — our store-divergence anti-pattern memory note is directly confirmed by mem0 #5245/#4892 (write-path durability + concurrency is where they bleed).
2. **Graph-shaped memory keeps losing to cost/maintenance**: mem0 dropped it; Zep closed the operable version; Graphiti's release history is a cost-reduction treadmill with an injection CVE-class fix; cognee's backlog is cognify-cost triage. Temporal *validity metadata* survives; general-purpose graph infrastructure in the hot path does not.
3. **Everyone is converging on file/filesystem-shaped memory surfaces**: Anthropic memory tool, Claude Code MEMORY.md, Letta git/filesystem block rendering, OpenViking's whole thesis. The winning interface for agent memory in 2026 is "a small, well-organized directory the agent edits" — the winning backend is whoever serves that interface with real storage, recall, and audit underneath.
4. **Background-cadence consolidation is standardized**: Letta sleep-time (every N turns), Mastra observer/reflector (token thresholds + async buffering), memobase buffers, OpenViking session compression. Nobody consolidates on the hot path anymore.
5. **Prompt-cache alignment is a first-class design constraint** (Mastra): append-only memory blocks with stable prefixes, provider-aware TTLs, re-anchor on model change. Retrieval-per-turn breaks caching; append-only observation logs don't.
6. **Evidence bars are rising**: hindsight bought third-party reproduction (Virginia Tech, WaPo) and weaponizes "everyone else is self-reported". Empty LME-V2/SWE-ContextBench leaderboards remain the cheapest credibility land-grab.
7. **English-hardcoding, LLM-JSON fragility, and async-ingestion opacity** are the three recurring OSS failure modes across mem0/graphiti — all three are avoidable by construction in a Rust substrate with typed extraction outputs and synchronous ack-or-error writes.
8. **Rust moat is eroding**: cognee-rs is underway; hindsight ships a single static binary. "Fast local binary, bring any model" is table stakes by 2027.

---

# Adopt / Avoid / Adapt for MemPhant (ranked)

## ADOPT (highest leverage first)

1. **Anthropic memory-tool backend as the distribution wedge.** The contract is small, exact, and GA: six commands (`view/create/str_replace/insert/delete/rename`) over a `/memories` virtual path, with specified listing format (2-deep, `{size}\t{path}`), 6-char right-aligned 1-indexed line numbers, str_replace uniqueness errors, 16k-char view truncation + `view_range` paging, and `is_error` tool results. Implement a MemPhant handler (SDK seams: `BetaAbstractMemoryTool`/`betaMemoryTool`/`BetaMemoryToolHandler`) that maps the file view onto retain/recall/correct with full audit — any Claude-4+ agent gets MemPhant with zero agent changes. Also serve Claude Code's auto-memory shape: `MEMORY.md` index under 200 lines/25KB + topic files + `modified` timestamps.
2. **LongMemEval-V2 submission adapter, now.** Contract is one ABC: `insert(trajectory)` + `query(q, image?) -> [{type, value}]` with config-reconciled persistence, latency measured, LAFS-scored leaderboard that is effectively empty (repo has 96 stars). Map insert→retain(episodic), query→recall(deep). Same sprint: V1 cleaned-dataset run with the official GPT-4o scorer.
3. **Evidence-status → answer-policy protocol from the LME-V2 codex baseline**: `directly_supported / contradicts_premise / near_match_only / insufficient` mapped to `answer_normally / state_premise_false / say_exact_target_not_found / abstain_unknown`. Fold this into deep recall's output contract — it is a benchmark-blessed formalization of calibrated recall and directly scores abstention/premise questions in both LME versions.
4. **Mastra's cache-alignment discipline** for anything MemPhant injects into prompts: append-only rendering with stable prefixes, activation tied to provider cache TTLs, re-anchor on model change, background (async-buffered) consolidation that never blocks the hot path, degenerate-output detection on any LLM condensation step. Their threshold shape (observe ~30k, reflect ~40k, buffer at 20%, retain ~20% raw tail) is a sane starting point for reflect cadence defaults.
5. **Sleep-time consolidation cadence (Letta)**: run reflect as a background job every N interactions/token-budget, never inline. MemPhant's reflect verb should have a first-class scheduled/idle trigger, not only explicit calls.
6. **AgentRunbook-C's agentic-exploration recall as the deep-mode benchmark reference**: materialize concise+full summaries at ingest; at query time let an agent explore raw + summarized layers with cheap inspection helpers. This validates MemPhant's `recall deep` design — keep verbatim episodes reachable beneath summaries.

## AVOID

1. **LLM-arbitrated destructive updates at write time** (mem0 V2's ADD/UPDATE/DELETE). The market leader retreated from it; corrections belong in a separate verb (we have `correct`) over an additive log. Never let a bounded LLM decision delete source-of-truth rows.
2. **General-purpose knowledge-graph substrate in the hot path** (graphiti/cognee): entity resolution + summary maintenance + per-driver query generation = permanent cost treadmill, dedup bugs, injection surface, and local-model fragility. Keep temporal validity as *columns* (valid_at/invalid_at/superseded_by on Postgres rows), not as a graph engine dependency.
3. **Async fire-and-forget ingestion** (graphiti's "202 but nothing persisted", mem0's silent partial-batch loss). Every retain must ack-or-error transactionally; partial embedding failure must never silently drop memories. Also: no O(N) scans for back-reference cleanup (mem0 #4988) — index the link column.
4. **English-hardcoded lexical machinery** (mem0's BM25 lemmatizer): use language-agnostic tokenization (Postgres `simple`/ICU config) from day one; it's a rewrite later otherwise.
5. **Unverifiable benchmark claims in the README** (supermemory pattern). hindsight has already weaponized "self-reported"; our own memory notes say the same. Only publish numbers a third party can rerun from our repo.
6. **Profile-only or chat-only scoping** (memobase — technically fine, commercially stalled, team pivoted). Confirms tri-domain scope; don't ship a "user profile product" subset as the identity.
7. **Char-limit-enforced memory blocks** (Letta removed enforcement in 0.16.7 as breaking change): budget by tokens at render time, don't hard-cap stored values.

## ADAPT

1. **mem0's entity store, minus its bugs**: a lightweight entities table (name, kind, embedding, linked memory ids) as an *index* over memories — gives cross-memory linking without a graph engine. Do it relationally in Postgres with proper indexes and cascade-safe cleanup; treat as recall-boost metadata, never a write dependency.
2. **Graphiti's fact validity windows on rows, not graphs**: `valid_at`/`invalid_at`/`superseded_by` columns on semantic/belief rows, set by `correct` and by contradiction detection at retain; recall filters by as-of time. This captures the one durable idea in temporal-KG systems with zero graph infrastructure.
3. **memobase's materialized profile view**: a `recall fast` path that reads a pre-consolidated per-scope summary (profile/current-state doc maintained by reflect) with plain SQL — sub-100ms, no vector query — for the "who am I talking to" call every agent makes at session start. Also its write-buffer batching to amortize extraction cost.
4. **Mastra's observation record format** (date-headed, timestamped, priority-tagged bullet hierarchy) as MemPhant's canonical *rendering* of episodic clusters into file views and prompt blocks — human-auditable, diff-friendly, cache-stable. Skip the emoji-as-schema; use explicit priority tags.
5. **OpenViking's L0/L1/L2 tiered loading + retrieval-trajectory observability**: adapt as (a) tiered recall responses (index line → summary → full body) matching our fast/balanced/deep, and (b) an explain-recall debug surface (we already have `explain` hooks) — observability of *why* a memory surfaced is becoming a differentiator. Watch this project closely: 27k stars in 6 months on our exact tri-domain thesis, filesystem paradigm, ByteDance backing.
6. **TencentDB's long-horizon eval methodology**: evaluate memory under *continuous sessions* (e.g. 50 consecutive SWE tasks with accumulating context), not isolated turns — adopt for our SWE-ContextBench/AgentRunbook harness runs; their symbolic tool-log compression is a decent pattern for episodic retain of tool-heavy trajectories (store condensed structural summary + keep verbatim reachable).
7. **hindsight's credibility play**: pursue independent reproduction (university lab or neutral org) of our first headline number instead of another self-run table; single-binary local deploy (embedded Postgres) as a packaging goal — they proved both moves convert.
8. **SWE-ContextBench**: no public harness repo exists yet — implement the paper's sequence protocol (300 base + 99 dependency-linked tasks) around SWE-bench Lite's docker eval and publish the first open memory-adapter reference; pair with SWE-smith for scaled task generation.

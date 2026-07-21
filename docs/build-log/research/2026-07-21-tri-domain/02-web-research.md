# MemPhant 2026 Research Sweep — Web Evidence Report

Date of sweep: 2026-07-21. All claims tagged: [self-run] = vendor/author-conducted, [third-party] = independent, [academic] = peer-track paper (usually author-run experiments), [unverified] = could not confirm against a primary source. Baseline for "what is NEW": prior verified landscape of 2026-07-19.

---

## 1. Benchmark deltas since 2026-07-19

### LongMemEval-V2 (LME-V2)
- **Leaderboard is STILL EMPTY as of 2026-07-21.** Both LME-V2-Small and LME-V2-Medium tables display "Leaderboard entries coming soon." First-mover slot remains open.
  Source: https://xiaowu0162.github.io/longmemeval-v2/ (fetched 2026-07-21) [authors' page, primary]
- **Baseline number delta**: the project page now shows **AgentRunbook-C at 74.9% accuracy / 108.3s avg query latency** (our prior record said 72.5% — either a paper-revision update or our record captured an earlier variant; treat 74.9%/108.3s as current). Other baselines: AgentRunbook-R 58.6% @ 26.9s; **Codex baseline 69.9% @ 177.2s** (new datapoint — a second agentic-exploration baseline); RAG variants (query-to-slice, +notes) 42.8–51.0%.
  Source: https://xiaowu0162.github.io/longmemeval-v2/ (fetched 2026-07-21) [authors, self-run baselines]
- **Scoring mechanics confirmed**: leaderboard metric is **LAFS Gain** — how much a submission improves the accuracy–latency **frontier** formed by released baselines + AgentRunbook. Inputs: `overall_full_set * 100` (accuracy) and `memory_query_avg_seconds` (latency); web+enterprise example-count-weighted averages; two tiers (small/medium); submission via Google Form (https://forms.gle/rxUpiuRKDERqpqSi9); two-step packaging scripts in repo. Implication: **a fast operating point can score LAFS gain without beating 74.9% absolute** — any point outside the current frontier counts.
  Sources: https://github.com/xiaowu0162/LongMemEval-V2/blob/main/leaderboard/README.md (fetched 2026-07-21); repo https://github.com/xiaowu0162/LongMemEval-V2 (Apache-2.0, 8 commits, no errata/news section) [authors, primary]
- Benchmark stats reconfirmed: 451 questions, up to 500 trajectories/haystack, up to 115M tokens, web+enterprise domains, five abilities (static state recall, dynamic state tracking, workflow knowledge, environment gotchas, premise awareness). Paper: arXiv:2605.12493.

### SWE-ContextBench — naming collision discovered (important)
Two distinct instruments now share the name:
1. **Academic "SWE Context Bench"** — arXiv:2602.08316 (v1 2026-02-09, v3 2026-05-06). 1,100 base tasks + 376 related tasks from real GitHub issue/PR dependency links; 51 repos, 9 languages. Findings: "accurately summarized and retrieved previous experience can significantly improve resolution accuracy and reduce runtime and token cost, particularly on harder tasks"; **unfiltered context provides limited or negative benefit**. No memory-provider (Supermemory/Mem0) eval inside the paper.
  Source: https://arxiv.org/abs/2602.08316 (fetched 2026-07-21) [academic]
2. **Third-party memory-provider eval (n=99 related tasks)** — the one in our landscape: Supermemory best (30.30% resolve, 55.95% FAIL_TO_PASS pass rate, 5.04 min, $0.58/task); Mem0 lower accuracy at higher cost ($0.62). Covered by Markus Sandelin, "The First Controlled Benchmark of AI Memory in Coding Agents."
  Source: https://medium.com/@mrsandelin/the-first-controlled-benchmark-of-ai-memory-in-coding-agents-8e0bb776d39e (2026) [third-party]
  **No update found to the n=99 numbers since 2026-07-19.** When citing, disambiguate which "SWE-ContextBench" is meant.

### SWE-Explore — now fully characterized; NO memory system has entered
- arXiv:2606.07297, submitted 2026-06-05 (SJTU, Xinjiang U, UIUC, et al.). Task: given issue+repo, return ranked list of up to K=5 relevant code regions (file+line range) under a fixed line budget; **line-level ground truth distilled from independent successful agent trajectories**. 848 issues, 10 languages, 203 repos.
- Metrics: Precision, nDCG@500, HitFile, Context Efficiency (+Recall, F1, HitRegion, First Useful Hit, noise rates). **Context Efficiency correlates r=0.950 (Pearson) with downstream repair success** — the strongest known proxy metric.
- Results (K=5): Oracle HitFile 0.923 / nDCG@500 0.858. Claude Code HitFile 0.667, nDCG@500 0.938, Recall 0.154. Mini-SWE-Agent HitFile 0.640, nDCG@500 0.885. CoSIL (academic localizer) Recall 0.788, F1 0.602, HitFile 0.544. **BM25 HitFile 0.079** — sparse retrieval collapses. "Agentic exploration is a clear step above non-agentic retrieval." Missing core evidence hurts more than redundant context.
- **No memory or experience-replay systems were tested.** Open instrument for MemPhant's code domain.
  Source: https://arxiv.org/html/2606.07297v1 (fetched 2026-07-21) [academic]

---

## 2. Agentic memory architecture patterns with credible evidence

### Mastra Observational Memory (OM) — mechanics + numbers
- Mechanics (docs): context window split into two blocks — (1) observation log, (2) raw uncompressed messages. New messages append to block 2; at **30k message tokens** (configurable) a separate **observer agent** compresses messages into observations appended to block 1; at **40k observation tokens** a **reflector agent** garbage-collects/restructures observations. Background observer runs off the hot path producing buffered observation "chunks" that activate at threshold. Prefix (system prompt + observations) is **append-only and stable → full prompt-cache hits every turn**, claimed 4–10x cost reduction.
  Sources: https://mastra.ai/docs/memory/observational-memory ; https://mastra.ai/blog/observational-memory [vendor docs]
- Evidence: **self-run** LongMemEval (`longmemeval_s`, 500 Q, ~57M tokens): gpt-5-mini **94.87%** ("highest ever" per Mastra), gemini-3-pro-preview 93.27%, gpt-4o 84.23%. Per-category (gpt-5-mini): knowledge-update 96.2%, temporal-reasoning 95.5%, multi-session 87.2% (acknowledged weakness). Claims: beats oracle baseline (82.4%), beats Supermemory-on-gpt-4o (81.60%), +3.5pp over Hindsight's best. Observer used gemini-2.5-flash; compression 3–6x text, 5–40x tool-heavy.
  Source: https://mastra.ai/research/observational-memory (fetched 2026-07-21) [self-run]; media echo: https://venturebeat.com/data/observational-memory-cuts-ai-agent-costs-10x-and-outscores-rag-on-long [third-party coverage of self-run numbers]
- Note: the 94.87% is on **LongMemEval v1 (_s)**, not LME-V2. Nobody has posted OM-style results on LME-V2's 25–115M-token agentic haystacks.

### Letta — sleep-time compute + skill learning
- Blog "Skill Learning: Bringing Continual Learning to CLI Agents," published 2025-12-02. Terminal Bench 2.0 (89 tasks): trajectory-only skills **+9.0pp absolute (21.1% relative)**; trajectory+feedback skills **+15.7pp absolute (36.8% relative)** over no-skill baseline. Skills stored as **`.md` files, "modular and can be managed by git."** Two stages: reflection (evaluate success, identify abstractions) → creation (learning agent writes skill). Sleep-time compute can scale reflection depth but was not fully exploited in the reported runs. [self-run]
  Sources: https://www.letta.com/blog/skill-learning ; repo https://github.com/letta-ai/letta-code ; sleep-time paper materials https://github.com/letta-ai/sleep-time-compute
  Caveat: fetch reported the model as "Claude Sonnet 3.5 with extended thinking" — likely a mis-OCR of Sonnet 4.5 [unverified detail]. Letta separately claims #1 open-source terminal agent, 42.5% Terminal-Bench overall, 4th overall [self-run].
- Signal: skill learning = ReasoningBank-style write-back applied to CLI agents, with **markdown files as the storage substrate** — converges with the file-based memory surface (Section 3).

### ReasoningBank (Google Research) — outcome write-back canon
- arXiv:2509.25140 (OpenReview under review). Distills reusable strategies from **both successful and failed** trajectories; LLM-as-judge labels outcome; retrieved at test time; new lessons appended after each task. Up to **+8% success rate** and fewer interaction steps; introduces memory-aware test-time scaling. [academic/self-run]
  Sources: https://arxiv.org/abs/2509.25140 ; https://openreview.net/forum?id=jL7fwchScm
- 2026 follow-on: **SWE-MeM** (arXiv:2606.28434, 2026-06-26, Shuzheng Gao, Michael R. Lyu et al.): memory management as a **learned policy** — agent decides "when, what, and how to compress based on trajectory state, task progress, and remaining context budget," trained with Memory-aware GRPO (memory-aware trajectory splitting + step-level credit assignment). SWE-Bench Verified: **43.4% resolve (4B model), 60.2% (30B)**, beating static-compression baselines on accuracy AND token efficiency. [academic] Signal: hand-tuned compression thresholds (Mastra-style) are being overtaken by learned policies at the research frontier.
  Source: https://arxiv.org/abs/2606.28434

### Hindsight (Vectorize) — direct naming overlap with MemPhant verbs
- arXiv:2512.12818 (2025-12-14; Chris Latimer et al., Vectorize + Virginia Tech + Washington Post collaborators). Open source. Architecture: **retain / recall / reflect** verbs over four networks — world facts, agent experiences, entity summaries, evolving beliefs; temporal, entity-aware layer; reflection layer produces traceable updates.
- Numbers: LongMemEval **91.4%** (Gemini-3 Pro backbone), 89.0% (OSS-120B), 83.6% (OSS-20B vs 39.0% full-context baseline); LoCoMo 89.61%; **BEAM 10M-token tier 64.1% vs next-best published 40.6%** (claimed #1, 2026-04-02). [self-run with academic collaborators]
  Sources: https://arxiv.org/abs/2512.12818 ; https://github.com/vectorize-io/hindsight-benchmarks ; https://hindsight.vectorize.io/blog/2026/04/02/beam-sota ; https://www.prnewswire.com/news-releases/vectorize-breaks-90-on-longmemeval-with-open-source-ai-agent-memory-system-302643146.html
- **Positioning threat**: an open-source system already markets "retain, recall, reflect" with strong self-run numbers. MemPhant's six-verb surface is a superset (correct/forget/mark are the differentiators — see Section 6).

### Codebase memory / beating agentic exploration
- **"Code Isn't Memory: A Structural Codebase Index Inside a Coding Agent"** (arXiv:2606.22417, 2026-06-21; Bhola, Krishnan, Kurmala, Mukunda NS). Structural index inside a fixed harness (Claude Opus 4.7), leak-audited sandboxes, SWE-PolyBench Verified + SWE-bench Pro: **large localization gain; no regression vs agentic-grep on resolve; lower cost per solve**; value concentrated in **multi-file changes**. [academic/industry preprint]
  Source: https://arxiv.org/abs/2606.22417
- Codebase-Memory: tree-sitter-based knowledge graphs served over MCP (arXiv:2603.27277) [academic]. Note: graph-shaped, which MemPhant has rejected 4x — but the tree-sitter *structural index* (non-graph ranked regions) from 2606.22417 is compatible with our stance.
- **MemGym** (arXiv:2605.20833, 2026-05-20, Wujiang Xu, Yu Wang et al.): unifies agent gyms behind one memory-reasoning interface (tool-use dialogue, research search, coding, web navigation); introduces **memory-isolated scores** decoupling memory from reasoning/retrieval/tool-use; critique: "existing memory benchmarks... overlook the dynamic memory formation that occurs during extended agent execution." [academic]
  Source: https://arxiv.org/abs/2605.20833
- **"Are We Ready For An Agent-Native Memory System?"** (arXiv:2606.24775, 2026-06-23; Zhou, Zhou, Li et al.): audits 12 memory systems + 2 baselines over 5 workloads / 11 datasets across four modules (representation/storage, extraction, retrieval/routing, maintenance). Findings: **no single architecture wins universally — "effectiveness depends on how well the memory structure aligns with the workload bottleneck"; "localized maintenance is more cost-efficient than global reorganization."** [academic] Supports MemPhant's per-domain recall modes over a one-true-structure design, and supports incremental (not global) consolidation.
  Source: https://arxiv.org/abs/2606.24775
- Nothing verified yet **beats** the agentic-file-exploration baselines on LME-V2 (leaderboard empty; AgentRunbook-C 74.9% stands).

---

## 3. File-based memory as a product surface

- **Anthropic memory tool contract**: tool type `memory_20250818`, name "memory"; Claude does CRUD on files under a `/memory` directory persisted across sessions by the harness. Anthropic internal benchmark: **84% token savings + 39% performance improvement** on a 100-turn web-search task with memory tool + context editing. [self-run]
  Sources: https://platform.claude.com/docs/en/agents-and-tools/tool-use/memory-tool ; https://www.anthropic.com/news/memory ; practitioner walkthroughs https://www.leoniemonigatti.com/blog/claude-memory-tool.html
- 2026-03-02: Anthropic extended memory + an **import tool** (migrate ChatGPT memories into Claude) to free users. [third-party coverage] https://www.macrumors.com/2026/03/02/anthropic-memory-import-tool/
- **AGENTS.md**: formalized Aug 2025 (OpenAI, Google, Cursor, Factory, Sourcegraph); **donated to Linux Foundation's Agentic AI Foundation Dec 2025**; read natively by 30+ tools (Claude Code, Copilot, Cursor, Codex, Gemini CLI, Windsurf, Devin, Aider, Amazon Q); adoption figures cited 20k → 60k+ repos (blog-sourced, exact count [unverified]). Plain markdown, no frontmatter, no schema.
  Sources: https://www.morphllm.com/agents-md-guide ; https://codersera.com/blog/agents-md-complete-guide-2026/ ; https://www.iuriio.com/blog/posts/2026/05/agents-md-field-guide-2026 [third-party]
- **Convention landscape**: "configuration-as-markdown" is the 2026 paradigm — CLAUDE.md (conventions), SKILL.md (procedures), MEMORY.md (cross-session memory), Memory-Bank-style directories (product-context.md, active-context.md, progress.md, decision-log.md, system-patterns.md). 120+ memory MCP servers listed on mcpservers.org; PulseMCP indexes 22,300+ MCP servers total. No single memory.md spec has won; markdown-dir + MCP verbs is the de facto shape.
  Sources: https://mcpservers.org/category/memory ; https://www.pulsemcp.com/servers?q=memory ; https://mcp.directory/servers/memory-bank ; https://mcp.directory/blog/claude-code-memory-mcp-servers-2026 [third-party]
- Practitioner verdict worth internalizing: memory MCP servers are "real and improving in mid-2026, but the bar to beat a well-kept CLAUDE.md is higher than the launch hype implies." (mcp.directory, 2026) [third-party]
- **MemPalace cautionary tale**: celebrity-fronted (Milla Jovovich) local-first memory MCP launched 2026-04-05/06; 19–24 MCP tools (count varies by audit); claims 96.6% R@5 on LongMemEval with zero API calls; star count contested (42,497 recorded in an April 2026 audit vs a widely shared "purchased stars" exposé; 7k stars in 48h). Multiple third-party teardowns dispute the benchmark claims; there is even an arXiv critique (arXiv:2604.21284, "Spatial Metaphors for LLM Memory: A Critical Analysis of the MemPalace Architecture"). Signal: enormous consumer appetite for file/local-first memory + very low trust in claims = **evidence-first positioning is a real moat**.
  Sources: https://github.com/mempalace/mempalace ; https://gist.github.com/roman-rr/0569fc487cc620f54a70c90ab50d32e3 ; https://github.com/lhl/agentic-memory/blob/main/ANALYSIS-mempalace.md ; https://arxiv.org/pdf/2604.21284 [third-party, claims contested — treat all MemPalace numbers as unverified]

---

## 4. Retrieval accuracy levers 2026, with latency budgets

### Cross-encoder rerankers
- Production reality: cross-encoder on K=100, batch=8 lands at **300–400ms = 40–60% of request p95** (practitioner playbook). Typical well-tuned deployments: **<200ms added latency for +5–15 nDCG@10 points**. Standard truncation: 512-token max (~64 query / ~448 doc).
  Sources: https://zeroentropy.dev/playbooks/reranker-on-the-request-path/ ; https://bigdataboutique.com/blog/rag-reranking-improving-retrieval-quality-with-cross-encoders [third-party practitioner]
- 2026 compression research: **layer-wise token compression** (1D adaptive pooling of token embeddings before upper transformer layers, preserving early query/doc interaction) — arXiv:2605.20683; **ResRank** residual passage compression — each passage compressed to ~1 embedding for LLM listwise reranking, linear complexity — arXiv:2604.22180; LLM→efficient cross-encoder distillation — arXiv:2607.11933 (July 2026). [academic]

### Late interaction (ColBERT family) on CPU
- PLAID: **150–300ms on CPU at 140M passages** (9–45x over vanilla ColBERTv2). Reranking 100 docs: tens of ms on GPU; production case study: top-1 accuracy 52% → 68% by layering ColBERT rerank on top-100, +~25ms p95. Field is active: first Late Interaction & Multi-Vector Retrieval workshop (LIR) at **ECIR 2026** (arXiv:2511.00444).
  Sources: https://www.emergentmind.com/topics/colbert-style-late-interaction-mechanism ; https://arxiv.org/pdf/2511.00444 ; https://medium.com/@2nick2patel2/colbert-and-friends-re-ranking-that-feels-instant-6c09102b7526 [mixed academic/practitioner]
- Verdict for MemPhant: CPU-only late-interaction **rerank of a bounded candidate set** (≤100) is production-viable inside a ~100–300ms budget; full-corpus late-interaction indexing is not needed.

### Matryoshka embeddings (MRL)
- Two-stage funnel is now a documented memory-system pattern: **Stage 1 shortlist on 256D index, ~5–10ms for 200 candidates; Stage 2 rescore shortlist with full 768D, ~10–20ms** — from Cognis, a context-aware memory system for conversational agents (arXiv:2604.19771). Milvus ships funnel search natively. Practitioner consensus: "use Matryoshka + quantization in production — yes, when measured."
  Sources: https://arxiv.org/pdf/2604.19771 [academic] ; https://milvus.io/docs/funnel_search_with_matryoshka.md ; https://futureagi.com/blog/evaluating-embedding-models-2026/ [third-party]

### Contextual chunk headers
- Anthropic contextual retrieval (Sept 2024) remains the anchor evidence: contextual embeddings alone cut top-20 retrieval failure **-35%**; + contextual BM25 **-49%**; + reranking **-67%** [self-run]. Independent replications (AWS et al.) report **5–15% precision gains** across datasets [third-party]. 2026 academic work continues on chunking (Adaptive Chunking arXiv:2603.25333; cross-document topic-aligned chunking arXiv:2601.05265).
  Sources: https://www.freecodecamp.org/news/how-contextual-embeddings-and-hybrid-search-fix-retrieval-failures/ ; https://medium.com/coinmonks/contextual-retrieval-anthropics-method-for-cutting-rag-failures-b28d98d57c48

### HyDE — our rejection holds
- 2026 consensus: not dead, but **demoted to a low-confidence fallback** inside hybrid policies. Measured costs: +43–60% latency and elevated hallucination on personal queries vs plain RAG; recommendation is confidence-gated fallback + cross-encoder post-validation, not default-on. Our rejection is consistent with the field.
  Sources: https://www.emergentmind.com/topics/hypothetical-document-embeddings-hyde ; https://arxiv.org/pdf/2412.17558 [survey]

### Query fan-out / depth
- AI search engines decompose a prompt into ~8–12 sub-queries (SEO-industry telemetry) [third-party]. Research: **iterative decomposition retrieving ~20 chunks matches ~200-chunk retrieval** (EfficientRAG line, arXiv:2408.04259); the open knob is adaptive budget allocation across sub-queries — exploration/exploitation formulation in arXiv:2510.18633; POQD trains the decomposer against downstream performance (arXiv:2505.19189). Combined with SWE Context Bench's "unfiltered context is negative," the lever is **bounded, adaptive fan-out with per-sub-query depth control**, not wider fan-out.
  Sources: https://searchengineland.com/guide/query-fan-out ; https://arxiv.org/pdf/2510.18633 ; https://arxiv.org/pdf/2505.19189 [academic]

---

## 5. Storage tiering, demotion vs deletion, CaaS pricing

### Tiering / demotion
- **AMV-L** (arXiv:2603.04443, "Lifecycle-Managed Agent Memory for Tail-Latency Control"): value-scored items map to hot/warm/cold lifecycle tiers; **promotion/demotion run asynchronously off the request path**; demotion shrinks the high-cost retrieval footprint while retaining items; **eviction only from cold tier below a value threshold**. [academic]
- 2026 practitioner consensus: **demote, don't delete** — periodic deletion loses information; keep-everything inflates cost and degrades retrieval SNR; tiered demotion is the middle path. This matches MemPhant's "evidence reset without machinery deletion" doctrine and gives `forget` a natural semantics (demote → evict) distinct from `correct` (supersede).
  Sources: https://arxiv.org/pdf/2603.04443 ; https://atlan.com/know/agent-memory-architectures/ ; https://docs.bswen.com/blog/2026-03-21-ai-agent-memory-architecture/ [third-party]
  (Excluded: clawrxiv.io/abs/2603.00037 "Memory Tiering: HOT/WARM/COLD" — clawRxiv is an agent-authored preprint mill, not credible evidence.)

### Cloud pricing (for CaaS positioning) — nobody prices "per-1k memories"
- **Mem0** (https://mem0.ai/pricing, fetched 2026-07-21) [vendor]: Hobby free (10k add req/mo, 1k retrieval req/mo); Starter **$19/mo** (50k adds, 5k retrievals); Pro **$249/mo** (500k adds, 50k retrievals, graph memory, unlimited projects); Enterprise custom. Metered per **add request / retrieval request**; overage rates undisclosed.
- **Supermemory** (https://supermemory.ai/pricing, fetched 2026-07-21) [vendor]: Free ($0, ~$5 usage included); Pro **$19/mo** (~$20 usage); Max **$100/mo** (~$130 usage); Scale **$399/mo** (~$600 usage, SOC 2/HIPAA, self-host option); Enterprise custom. Usage meter: storage **$0.005 / 1k "SM tokens"** plain ($0.010 rich); extraction $0.001/1k ($0.002 rich); search $0.005 per 1k queries; operations $0.10 per 1k. **SM tokens are deduplicated — repeated content is free.** Most UX-honest metering seen.
- **Zep**: graph memory from **$25/mo Flex** tier; Flex $125/mo = 50k credits, 1 credit per "Episode" (any data object ingested). [third-party blog figures — verify against getzep.com before quoting publicly] Source: https://dev.to/varun_pratapbhardwaj_b13/5-ai-agent-memory-systems-compared-mem0-zep-letta-supermemory-superlocalmemory-2026-benchmark-59p3
- **Letta Cloud**: Pro tier ~**$20/mo** bundling hosting + agent runtime. [third-party blog, unverified against letta.com pricing page]
- Positioning read: the market meters (a) requests (Mem0), (b) deduplicated ingested tokens + queries (Supermemory), (c) ingestion events/credits (Zep). Graph memory is consistently an **upsell** (Mem0 $249 tier; Zep $25+) — a substrate that doesn't need a graph can undercut structurally.

---

## 6. Long-horizon consistency: bitemporal, supersession, forgetting

- **Zep/Graphiti bitemporal model**: every edge carries event time (when true in world) + ingestion time (when observed) → supersession/invalidation without information loss; retroactive corrections distinguishable. Zep temporal sub-task 63.8% vs Mem0 49% on LongMemEval temporal reasoning (numbers circulate via vendor + secondary analyses [self-run origin]).
  Sources: https://arxiv.org/abs/2501.13956 ; https://www.emergentmind.com/topics/zep-a-temporal-knowledge-graph-architecture ; https://vectorize.io/articles/mem0-vs-zep [third-party summary]
- **Biggest find of the sweep — "Control-Plane Placement Shapes Forgetting"** (arXiv:2606.15903, 2026-06-14, Dongxu Yang): studies WHERE the LLM sits between recall and the mutation control plane (supersede / release / purge) across 13 configurations. Results: deterministic primitives alone fail canonicalization (5% identifier-obfuscation, 0% cross-lingual); inscribe-time LLM fixes canonicalization (100%) but scores **0% on intent-aware deletion** (prefix-collision cases); **mutation-time LLM hooks recover intent-aware deletion (78–85%) and reach 91.7–93.2% overall**. Thesis: "production systems experience primarily forgetting failures rather than recall failures, yet existing benchmarks measure only recall." Releases **ForgetEval: 1,385-case forgetting benchmark, MIT license.** [academic]
  Source: https://arxiv.org/abs/2606.15903
  Direct hit on MemPhant's correct/forget/mark verbs: run the LLM at **mutation time** (when correct/forget fires), not only at inscribe time. ForgetEval has no published system leaderboard — another first-mover instrument.
- **Memory-T1** (arXiv:2512.20092): RL for temporal reasoning in multi-session agents — temporal consistency is becoming a trained capability. [academic]
- **BEAM** (ICLR 2026, https://github.com/mohammadtavakoli78/BEAM): 100 conversations up to 10M tokens, 2,000 probing questions, 10 ability categories incl. knowledge update, contradiction resolution, abstention, event ordering. Hindsight claims 64.1% at the 10M tier vs 40.6% next best [self-run]. Neutral instrument worth tracking alongside LME-V2.
- **MemSyco-Bench** (arXiv:2607.01071, July 2026): benchmarks **sycophancy in agent memory** (memory systems overwriting truth with user-pleasing updates) — brand-new failure axis adjacent to supersession quality. [academic]
- Counter-evidence to "you need a graph for temporal": Mastra OM scores temporal-reasoning 95.5% and knowledge-update 96.2% on LongMemEval v1 with **no graph** — compression + observation logs suffice at chat scale [self-run]. Consistent with our 4x graph rejection; bitemporal **columns** (valid-time + observed-time on rows) capture the Graphiti benefit without graph machinery.

---

## Plan implications (ranked)

1. **Submit to LongMemEval-V2 now — the leaderboard is still empty (verified 2026-07-21) and LAFS Gain rewards any point outside the baseline accuracy–latency frontier.** Enter TWO operating points: a fast/balanced-mode point (beat RAG's 42.8–51.0% at seconds-scale latency, far under AgentRunbook-R's 58.6%@26.9s frontier point) and a deep-mode point targeting AgentRunbook-C's updated 74.9%@108.3s. First entry on an empty official leaderboard is the single highest-leverage credibility move available, and it expires the moment someone else submits.
2. **Make forgetting/supersession the measured differentiator: run the LLM at mutation time and adopt ForgetEval (1,385 cases, MIT) as a reported instrument.** arXiv:2606.15903 shows inscribe-time-only LLM placement scores 0% on intent-aware deletion while mutation-time hooks reach 78–85% (91.7–93.2% overall), and argues production failures are forgetting failures that no mainstream benchmark measures. This lands exactly on correct/forget/mark — verbs Hindsight (which now owns "retain/recall/reflect" branding at 91.4% LongMemEval) does not have. Second empty-instrument first-mover slot, and it's our home turf.
3. **Ship an observation/consolidation block layer (append-only observation prefix + token-threshold observer/reflector) as the agent-domain surface for reflect.** Three independent lines converge: Mastra OM 94.87% self-run LongMemEval with 4–10x prompt-cache savings; Hindsight's reflection layer at 91.4%; Letta skill write-back +15.7pp absolute on Terminal Bench 2.0 with skills as git-managed .md files. Emit consolidated observations/skills as markdown files compatible with the AGENTS.md/CLAUDE.md/Anthropic memory-tool ecosystem (Linux Foundation-governed, 30+ tools) — that is the product surface users already trust, and "Are We Ready" (arXiv:2606.24775) confirms localized maintenance beats global reorganization.
4. **Harden the retrieval stack with the proven cheap levers and keep the rejected ones rejected:** matryoshka two-stage funnel (256D shortlist ~5–10ms → full-D rescore ~10–20ms, per Cognis arXiv:2604.19771), CPU late-interaction or token-compressed cross-encoder rerank of ≤100 candidates inside a 100–300ms budget, contextual chunk headers (-49 to -67% retrieval failures, replicated 5–15% precision gains), and bounded adaptive query fan-out (~20 well-chosen chunks ≈ 200; unfiltered context is measurably negative per SWE Context Bench). HyDE stays rejected — 2026 consensus has demoted it to a confidence-gated fallback with +43–60% latency cost.
5. **Claim the code domain on SWE-Explore (848 issues, no memory system has ever entered) and fold in a structural-index experiment for deep mode.** SWE-Explore's Context Efficiency metric correlates r=0.950 with repair success and agentic explorers dominate (BM25 HitFile 0.079 vs Claude Code 0.667) — MemPhant's deep agentic recall plus a tree-sitter structural region index ("Code Isn't Memory," arXiv:2606.22417: no regression vs agentic-grep, lower cost per solve, wins on multi-file changes) is a credible first entrant. On the business side, meter deduplicated ingested tokens + retrievals (Supermemory's model, the market's most honest) and implement forget as demote-then-evict tiering (AMV-L) — competitors paywall graph memory at $25–249/mo, a tax a graph-free substrate never has to charge.

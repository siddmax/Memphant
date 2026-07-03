# MemPhant - Product Spec

## 0. Product Definition

MemPhant is the memory layer an agent calls when context is no longer enough. It stores long-running experience, retrieves compact evidence, and keeps memory scoped, cited, and poison-resistant.

It is not:

- A full agent runtime.
- A workflow engine.
- A vector database wrapper.
- A Syndai-only backend module.
- A benchmark leaderboard company.

### 0.1 When You Do Not Need MemPhant

Memory answers one failure: the demo works because the session fits one context window, and production breaks when users return across sessions, days, and topics. If you are not hitting that wall, MemPhant is overhead. Do **not** adopt it for:

| You have | Use instead | Why MemPhant is wrong here |
|---|---|---|
| A single-session agent (state is disposable) | the context window | nothing needs to persist; durable storage is pure latency/ops cost |
| A small static-corpus RAG app | a vector index | you retrieve documents you already have; you don't *write/update/correct* experience over time |
| A team happy with context-window-only design | nothing | MemPhant earns its place only once you need a returning user recognized, a decision from last week recalled, or a now-wrong fact corrected |
| A throwaway prototype where wrong/poisoned memory costs nothing | a dict / JSON file | trust gates, citations, ablations are cost you don't yet need |

Adopt when an agent must live longer than one session **AND** a wrong memory has a real cost (a bad action, a leaked fact, an un-auditable answer). If either is false, you're early — come back when it's true.

### 0.2 Edge Cases — New Users & Cold Start

A memory substrate is judged hardest in its first sessions, when there is almost nothing to recall. The empty/thin-corpus regime has its own failure modes, each handled by a named mechanism:

| Cold-start edge case | Risk | Handling |
|---|---|---|
| **Empty memory (session 1)** | recall over an empty store must return *nothing* and the agent must not confabulate | recall abstains (returns `[]` + a trace, never a fabricated unit); the citation whitelist (`04` §7.4) makes a cite-from-nothing structurally impossible |
| **Single-source first facts** | the ≥2-independent-source gate (`04` §5) would strand every early fact at `belief` → the assistant feels amnesiac by session 2 | a **direct first-party user assertion promotes on one `trusted_user` source** (`04` §5 cold-start exception) — the user *is* the authority on themselves |
| **Thin corpus over-retrieval** | with 3 memories, one irrelevant memory dominates the pack ("memory hijacking" is *worse* at low N) | the relevance gate (`05` §1.5) prunes off-query memory — it matters *most* at cold start, exactly when recall is weakest |
| **Small/new tenant, large shared index** | a new tenant inside a big shared HNSW index gets the *worst* recall (filtered-search collapse) — the day-one customer's worst case | partial per-tenant indexes + `filter_selectivity` trace + the small-tenant-recall benchmark (`02` §2.1b) |
| **No reinforcement history yet** | DSR decay / learned fitting have no signal | fixed priors decay gently toward the trust-class prior; learned fitting is data-gated off until traces exist (`04` §8) |
| **Time-to-first-useful-recall** | the activation metric that decides whether a new user trusts memory at all | the first recall that beats a dict (a cited, scoped, correctable fact) is the first-value moment (§4.1); the activation drip is keyed to it (`15`) |
| **Interactive formation (not just imported logs)** | a new user's memory forms *during* interaction, not from a backfilled transcript | the write path captures each turn as a durable episode (`02` §3), so recall improves within the first session; the interactive-formation regime is an eval gap tracked via EMemBench (`12`) |

The through-line: **cold start is where restraint matters more than recall.** A new user is better served by an honest "I don't know that yet" than by a confident answer drawn from one barely-relevant memory.

### 0.3 Edge Cases — Mature Users (years of memory)

The opposite end of the lifecycle is the user with thousands of sessions, who must still feel the memory **consolidates, cleans itself, stays temporally correct, and forgets on request** — not a landfill that grew confidently wrong. The dominant maturity failure is **memory rot**: a real Mem0 production audit (GitHub #4573, 32-day run) found **97.8% of 10,134 stored entries were junk** (37.6% near-duplicates, one fact re-extracted 200+ times, one hallucination copied 808× via a feedback loop), and the Harvard/D³ study (arXiv:2505.16067) found indiscriminate "add-all" storage performs *worse than no memory at all*. Each rot mode has a named defense:

| Maturity edge case | Risk | Handling |
|---|---|---|
| **Memory rot / junk accumulation** | dups + re-extraction + contradictory facts pile up; recall degrades into noise | the corroboration + dedup + contradiction gates run as a **write-time quality gate at ingest** (`04` §9), not a later sweep — admission control, since "add-all" loses to no-memory |
| **Stale facts going quietly wrong** | a fact (employer/role/location) is right until it isn't, with no contradiction to trigger invalidation | bitemporal validity is *reactive*; pair it with **active freshness** — periodic re-confirm / confidence-decay for high-churn fact types (`04` §8) |
| **Silent recall decay at 100K+ vectors** | HNSW recall drops ~10pt as the corpus grows at fixed `ef` — *latency stays flat so it's invisible* | corpus-size-aware retrieval (raise `ef_search` as the corpus grows) + **continuous recall monitoring** as an SLI (`02` §2.1b, `22`) |
| **Consolidation cost at scale** | reflect could become O(total corpus) and fall hopelessly behind | consolidation is **linear in *new* memories, not total** (bounded batches + bisection retry + hierarchical retrieval, no full-scan, async) — `04` §9 |
| **"Forget everything about me" at scale** | HNSW tombstones don't truly delete; cross-store orphans accumulate (Mem0 #3245); GDPR must reach indexes + backups | cross-store saga deletion + read-back + tombstone compaction + **crypto-shredding** a per-user key (`06` §6.2) |
| **Embedding-model upgrade over the years** | a model swap invalidates every vector; a naive re-embed is downtime + cost | second-profile vector cutover + **model-version tag on every vector** + a Drift-Adapter bridge (`14` §10) |
| **"It feels creepy / obsessed"** | wrong/over-eager recall, project bleed, fabricated "memories" | scope-tree isolation (no project bleed) + the relevance gate + **user-visible provenance and one-click per-fact correction**; never surface fabricated/low-confidence recall as fact (`19`, `06`) |

The through-line: **for the mature user, cleaning and honesty beat accumulation.** A memory system that quietly hoards and goes stale feels worse at year three than one that holds less but is correct, current, and inspectable.

## 1. Primary Users

| User | Job |
|---|---|
| Agent runtime builders | Add durable memory without inventing storage, retrieval, and poisoning controls. |
| App teams with long-running assistants | Remember user/project/workflow state safely across sessions. |
| Research and eval teams | Compare memory strategies with reproducible traces and ablations. |
| Syndai | Dogfood MemPhant for user/project/agent memory through the same public surface. |

## 2. Core Jobs To Be Done

1. `retain`: store an episode, resource pointer, fact, belief, or procedure with provenance.
2. `recall`: return compact cited evidence under tenant/scope/trust constraints.
3. `reflect`: consolidate raw episodes into semantic/procedural/belief memory offline.
4. `correct`: revise a selected memory through an auditable supersession/invalidation event.
5. `forget`: remove or invalidate memory and derived indexes according to policy.
6. `trace`: explain why a retrieval returned what it returned.
7. `eval`: replay a corpus through several retrieval configurations and compare quality/cost/latency.

## 2.1 Use-Case Modes

| Mode | Example | Dominant kinds / trust floor | Required product behavior |
|---|---|---|---|
| Personal assistant memory | preferences, recurring tasks, contacts | semantic + belief; `trusted_user` floor | high privacy, explicit correction/forget |
| Project memory | decisions, files, constraints, open issues | semantic + resource; `trusted_user`/`verified_tool` | scope-aware recall and resource citations |
| Coding-agent memory | repo facts, failed commands, procedures | episodic + procedural + resource; tool output starts low-trust | file/resource pointers, procedural candidates, branch/run provenance |
| Enterprise agent memory | policies, workflow state, customer facts | semantic + procedural; strict tenant isolation | tenant isolation, audit, deletion, access policy |
| Research/eval memory | benchmark corpora and traces | episodic + resource; fixture-trust | reproducible configs, ablations, archived traces |

The same substrate serves all modes through policy and adapters. Do not create product-specific memory models.

### 2.2 Worked Journey (retain → recall → correct)

The contract made concrete — one assistant-memory flow end to end (full JSON in `08`):

1. **retain** — a tool reports `"refund window is 14 days"`. `POST /v1/episodes` stores the raw episode (`source_kind: tool`, `verified_tool`), dedups, enqueues `extract_episode`. Returns `202` with `episode_id`.
2. **reflect** (background) — extracts a candidate semantic unit; contradiction detection (`04` §3.1) finds an existing active `"30 days"` fact on the same `subject_key` with overlapping validity; corroboration is independent → promotes the new fact and writes a `supersedes` edge.
3. **recall** — `POST /v1/recall` "what is our refund window?" returns the `14-day` fact first, cites `episode_id`, surfaces a `contradiction` warning for the superseded `30-day` fact, and emits a trace (`filter_selectivity`, `consolidation_lag: 0`).
4. **correct** — a human says it is actually 21 days. `POST /v1/correct` with the `memory_unit_id` + new value supersedes the `14-day` fact (never silent overwrite), returns the `supersedes` edge and a trace. The old fact stays citable for history.

Every step leaves a durable trace; no step lets recall text flow into a tool argument without the integrating runtime's policy gate (invariant #4).

#### 2.3 Worked Use Case: Coding-Agent Memory (Syndai's own #1)

The mode made concrete — a coding agent working a repo across many runs:

1. **retain (episode, low trust)** — a run reports `npm test` failed with a flaky timeout; stored as an episode (`source_kind: tool`, low-trust), scoped to `repo:acme/api @ branch:main`, with run/commit provenance.
2. **reflect** — after the third corroborating run the "raise the timeout" fix promotes to a *procedural candidate* with `validation_status`; one success does not promote it, independent corroboration does (invariant #3), and adversarial replay asserts the steps carry no destructive action (`04` §4.2).
3. **recall (scoped)** — on a new run "how do I make the suite green?" returns the validated procedure first, cited to the runs that proved it, and **excludes** a stale procedure for the old build system (the classic coding-agent failure: a true memory that no longer applies). Scope isolation keeps a *different* repo's convention out of the answer.
4. **correct** — the build tool changes; the procedure is now wrong. `correct` supersedes it; the old one stays citable for the runs it explains.

Branch/run provenance, low-trust tool output, and validated procedural promotion are the three things a dict, a vector index, and a single-bank memory each cannot do. This is the Syndai dogfood path (`07`/`28`).

## 3. Wedge

The wedge is not "memory SDK." The wedge is:

```text
ground-truth episodes
+ brain-inspired memory policies
+ tree-scoped sharing
+ poisoning defense
+ citation-first retrieval
+ ablation-first evals
```

Competitors can match one or two of these. MemPhant wins if the combined contract is simple enough to adopt and rigorous enough to trust.

Two market shifts sharpen where this wedge sits. First, the field is bifurcating into **learned memory managers** (Letta's stated direction: memory models trained with memory-native RL) and **governed substrates**; MemPhant is the substrate a learned manager reads and writes through, not its competitor. Second, memory moved INSIDE the mega-harnesses and is climbing the wedge (R92): the free baseline is no longer flat files — OpenClaw's builtin (381k★, on by default) ships hybrid search, active recall, and a wiki compiler producing structured claims with freshness and contradiction tracking; Claude Code's auto-memory is GA and on by default. What stays unbundled is the **governed tier for app teams embedding agents** — multi-tenant isolation, poisoning defense, citations, auditable deletion — sold where a plug point exists: the Claude/OpenAI file-tool handler (`08` §5.1a, the R79 adapter), the Hermes provider SPI (`08` §5.1b, specced at an activation gate), and NOT OpenClaw (no plug point; its builtin users are a recorded non-target, `13` §1.4). **One governed memory ledger, many surfaces: agents, RAG, coding, file-memory.** Harness users were never the market; app teams are — and the compressing window (Hindsight's weekly cadence, cognee's rise) makes the `29` §2a envelope a hard constraint, not prudence.

## 4. Success Criteria

MemPhant is working when:

- A developer can add it through MCP in under 10 minutes.
- A Python app can use it with `pip install memphant`.
- A hosted service can run the Rust server and expose REST/MCP.
- Syndai can replace its memory path incrementally without private hooks.
- Eval traces can explain a failed benchmark case without rerunning the full benchmark.
- Public claims always pair accuracy with latency and cost.
- Poisoned memories are quarantined, down-weighted, or excluded by default.

## 5. V1 Scope

V1 ships:

- Rust core library and server.
- Postgres + pgvector store.
- Object-store backed resource/episode blob pointers.
- REST API.
- MCP server with `retain`, `recall`, `reflect`, `correct`, `forget`, `trace`, `mark`.
- Python SDK backed by HTTP plus native Rust binding wheels for local/embedded use.
- TypeScript SDK backed by HTTP: client contract frozen against the v1 OpenAPI surface; built at its activation gate, not in the first public build (`29-implementation-plan.md` owns the cut line).
- Golden eval pack, sampled benchmark runner, and release scorecard runner.
- Trust/provenance/poisoning controls.
- Temporal/edge expansion, bounded rerank, query decomposition, contextual chunks, DSR decay fields, and L4 exhaustive recall mode: interfaces frozen in v1 (schema, trace fields, explicit flags); each behavior is built at its `27` §2 ladder rung, not in the first public build. The capability claims stand — only the build timing follows the ladder.

Out of core by decision:

- Managed billing.
- External graph database backend.
- Large framework adapter set.
- Vendor benchmark leaderboard.
- CRDT/Yjs skill editor.
- Agent runtime or workflow engine.

Dashboard scope is fixed: trace explorer, memory inspector, API keys, usage, and eval runs. It is not a generic SaaS analytics dashboard.

## 6. Product Risk Register

| Risk | Mitigation |
|---|---|
| Benchmarks are expensive | Use sampled evals and retrieval-only oracle checks before full runs. |
| Rust slows iteration | Keep core small; SDKs and examples can be Python/TS. |
| Brain model becomes branding fluff | Tie each memory kind to concrete policies and tests. |
| Syndai coupling leaks in | No Syndai FKs, no Syndai-only API paths, neutral `scope_ref` values only. |
| SOTA claim fails | Publish honest scorecard, use traces to pull the next lever, narrow the wedge if needed. |

## 7. User-Facing Promise

The product promise is not "your agent remembers everything." That is unsafe and expensive.

The promise:

```text
Your agent can remember the right evidence, for the right scope, with citations, deletion, and measurable quality.
```

This wording prevents the two common traps:

- stuffing everything into context
- hiding memory decisions behind opaque vector search

### 7.1 Why Switch, Why Start Here

Most readers already store memory somehow (a dict, a vector index, Mem0/Zep). The honest calculus: **the migration users actually notice is memory** — rewriting a memory layer means migrating user data, there is no universal export standard, and picking wrong means early users lose continuity. So the decision is not "is MemPhant marginally better" but "is this an architecture I will not have to flee." A Mem0/Zep user gains: **ground-truth episodes** they can recover (not just lossy extractions), provenance + poisoning defense as core, **no second datastore** (bitemporal-graph on Postgres, no Neo4j), and a trace per recall instead of opaque vector search. What MemPhant promises so the *next* switch is never trapping: Apache-2.0 core + a **mandatory export** of episodes/units/resources/citations/traces/deletion-logs (`09` §5). The anti-lock-in is part of the product — start here because leaving is always possible, by contract.

### 7.2 "Not Yet" vs "Never"

The out-of-scope list (§5) is two different promises — don't confuse them. **Never** (category boundaries you can design against permanently): agent runtime / workflow engine, vendor benchmark leaderboard. **Not yet** (data-gated, announced on the roadmap, never a surprise dependency): external graph DB backend (reopens only if traces show relational edges can't reach the target, `26` §7), learned FSRS/DSR fitter (schema + DSR fields freeze now; fixed-prior decay builds at its `27` §2 rung, the learned fitter only after review traces exist), managed billing/compliance pack (hosted-lane, the core stays free + self-hostable regardless).

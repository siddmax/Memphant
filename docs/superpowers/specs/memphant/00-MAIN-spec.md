# MemPhant - Long-Term Agent Memory Substrate

> Status: SPEC, pre-build.
> Codename: MemPhant, "memory like an elephant."
> License posture: Apache-2.0 public core repo from day one.
> Relationship to Syndai: standalone product; Syndai is customer #1 through the public API/SDK/MCP surface.

---

## 0. One Sentence

MemPhant is an Apache-2.0, Rust-first memory substrate for long-running agent trees: it preserves raw experience, derives scoped memories, defends against poisoning, retrieves compact cited evidence, and proves quality through reproducible evals.

---

## 1. Why This Exists

Flat vector memory is not enough. Long-running agents need:

1. Ground-truth episodes that are never replaced by lossy extraction.
2. Multiple memory kinds with different trust, decay, and retrieval policies.
3. Tree-scoped sharing rules so child agents do not inherit unsafe parent context.
4. Provenance and poisoning defense before memory affects behavior.
5. A cheap ablation loop so benchmark failures point to a specific lever.

Current benchmark pressure supports this direction:

- LongMemEval-V2 tests agent memory over up to 500 trajectories and 115M tokens, with accuracy and latency both relevant: <https://arxiv.org/abs/2605.12493>. (The classic ~115K-token `LongMemEval-S` is the separate, near-saturated comparability split; do not conflate the two.)
- BEAM ("Beyond a Million Tokens") tests memory at 100K to 10M-token scale and reports accuracy, speed, and context-token tradeoffs. Cite the primary paper, <https://arxiv.org/abs/2510.27246>; the `agentmemorybenchmark.ai` board is a vendor-operated (Vectorize) leaderboard, so its numbers ingest as `source_status: vendor_reported`, never as an anchor (`12` §8).
- OWASP Agentic Top 10 includes memory and context poisoning as ASI06: <https://genai.owasp.org/resource/owasp-top-10-for-agentic-applications-for-2026/>.
- pgvector supports the core Postgres vector primitives MemPhant needs: `vector`, `halfvec`, `sparsevec`, `bit`, HNSW, IVFFlat, and rerank patterns: <https://github.com/pgvector/pgvector>.

---

## 2. The Five Decisions

| # | Decision | Why |
|---|---|---|
| 1 | Apache-2.0 standalone repo | Real open source adoption; no source-available ambiguity. |
| 2 | Rust core, thin SDKs | Fast deterministic kernels, static binaries, safer concurrency; Python remains an integration surface. |
| 3 | Brain-inspired memory kinds | Different memories need different policies; one `memory` table with vibes will not be SOTA. |
| 4 | Ablation-first eval system | Do not build every speculative subsystem; preserve levers and test cheaply. |
| 5 | Syndai consumes public surface only | If Syndai gets private shortcuts, MemPhant is not really standalone. |

---

## 3. Subdocument Index

| Doc | Scope |
|---|---|
| `00-relations-graph.md` | Doc ownership graph, frozen interfaces, build DAG, and drift-prevention rules. |
| `01-product-spec.md` | Product positioning, users, use cases, out-of-scope, success criteria. |
| `02-architecture-spec.md` | Rust service topology, storage, memory pipeline, MCP/API/SDK surfaces. |
| `03-engineering-spec.md` | Repo layout, Rust/Python packaging, build gates, implementation rules. |
| `04-memory-model-spec.md` | Brain-inspired memory kinds, schema concepts, trust/decay policies. |
| `05-retrieval-and-eval-spec.md` | Retrieval stages, ablation flags, benchmark ladder, failure-to-lever map. |
| `06-trust-security-spec.md` | Poisoning defense, tenant isolation, provenance, deletion, audit. |
| `07-syndai-integration-spec.md` | How Syndai consumes MemPhant without coupling or leaking app concepts. |
| `08-api-sdk-mcp-spec.md` | REST, SDK, CLI, MCP tool contracts. |
| `09-open-source-governance-spec.md` | Apache-2.0 repo posture, contribution boundaries, public/private split. |
| `10-build-plan.md` | Dependency order, gates, what not to build yet. |
| `11-business-launch-spec.md` | Market wedge, pricing posture, public-launch gates, risks. |
| `12-data-methodology-and-benchmark-inventory.md` | Benchmark inventory, dataset policy, evidence tiers, scorecard rules. |
| `13-prior-art-and-competitive-spec.md` | Competitor teardown and what to copy/avoid. |
| `14-ingestion-seeding-and-ops-spec.md` | Importers, seed data, background jobs, ops contracts. |
| `15-auth-onboarding-and-tiering.md` | Auth, local/hosted onboarding, API keys, tier ladder. |
| `16-growth-gtm-playbook.md` | Launch channels, launch artifacts, content wedge, adoption loops. |
| `16a-gtm-agent-buildbook.md` | Concrete GTM automations and ownership. |
| `17-legal-compliance.md` | Open-source, privacy, benchmark, export, and security legal posture. |
| `18-governance-and-neutrality-charter.md` | Public trust charter, conflict disclosure, benchmark honesty. |
| `19-ui-ux-design-spec.md` | Website, docs, dashboard, trace explorer, memory inspector UX. |
| `20-metrics-kpi-and-events.md` | Product metrics, eval metrics, event taxonomy. |
| `21-financial-model-and-open-source.md` | Business model, COGS levers, public/private repo split. |
| `22-observability-telemetry-and-self-improvement-spec.md` | Runtime telemetry, quality facts, regression loop. |
| `23-design-system-and-i18n-spec.md` | Brand, visual system, accessibility, internationalization. |
| `24-methodology-hardening-refinements.md` | Known hardening refinements and when to implement them. |
| `25-db-provider-byoc-and-app-surface-spec.md` | Database provider posture, BYOC rules, Supabase/Neon/plain Postgres gates, web/mobile boundaries. |
| `26-decision-register.md` | Final build decisions so no launch-critical choice is left implicit. |
| `27-sota-ladder-and-validation.md` | Exact increment -> test -> increment ladder for reaching SOTA without rearchitecture. |
| `28-syndai-code-contract.md` | Checked Syndai backend memory invariants MemPhant must preserve through the adapter. |
| `29-implementation-plan.md` | EvalRank-style implementation spine: workstreams, gates, activation timing, status/log rules. |
| `frame.md` | One-page launch framing and promise. |
| `STATUS.md` | Live checkbox ledger: workstreams, rungs, gates, activation-gated items — MemPhant is DONE when its banner flips to COMPLETE. |

---

## 4. Non-Negotiable Invariants

1. Raw episodes are *recoverable* ground truth. Extraction creates indexes, never the only record. Cold-tiering may drop a stale episode's derived indexes/embeddings (re-derivable on demand), but never the recoverable raw episode itself — so the lossless guarantee survives while Postgres-resident derived-index cost stays bounded (`04` §2.4 retention tiers).
2. Every tenant-scoped row carries `tenant_id`; every critical query is tenant and scope constrained.
3. Untrusted input enters low-trust memory or quarantine first. It does not become high-trust semantic memory without corroboration.
4. Retrieved memory is evidence, not instructions. MemPhant supplies the *primitives* for data/control separation — trust labels, content delimiting, high-risk-action suppression flags — but the guarantee is a **shared contract**: the integrating runtime must not place raw recall text into a tool-argument or control-flow position. MemPhant cannot enforce this alone; the Syndai adapter's obligation is pinned in `28` §3.
5. Every answer-bearing memory can produce a citation path back to episode/resource/provenance.
6. `correct` must supersede or invalidate selected memory through auditable revision events, not silent overwrite.
7. `forget` must invalidate derived indexes, citations, embeddings, cache entries, and cold blobs according to policy.
8. The live hot path does not run expensive L4 deliberate recall by default.
9. All retrieval changes are measured by ablation traces before they are marketed as quality improvements.
10. Syndai concepts (`mission`, `project`, `L0`) map into neutral MemPhant scopes at the adapter boundary only.
11. Public API contracts are the dogfood contracts. Syndai does not call hidden internals.
12. Rust improves deterministic hot paths; it does not make the memory model SOTA by itself.
13. Benchmark claims require frozen configs, cost/latency/token reporting, confidence intervals, and archived traces.
14. Memory benchmarks are not long-context benchmarks. They test write/update/retrieve over time, not one giant prompt.
15. Brain-inspired design is an engineering policy map, not a biology proof. Benchmarks decide.

---

## 5. Build Philosophy

Freeze the interfaces that would be painful to retrofit. Every SOTA-critical method exists in the first public architecture as a **contract** — schema columns, feature flag, retrieval mode, trace fields — and its expensive *behavior* is built when its `27` ladder rung activates, never speculatively. The hot path remains cheap; benchmark/exhaustive mode contracts exist from day one. (This resolves an earlier internal contradiction — "ship all the methods in the first build" vs `29` §1's "store the fields, activate behind gates" — in favor of the ladder; R73.)

**Frozen in the first public architecture (schema/flags/contracts — cheap now, migrations later):**

- Memory kind enum and policy table.
- Episode, memory unit, resource, edge, embedding profile, citation, trust event, retrieval trace.
- Write-path consolidation contract columns: `episode.retention_tier`, `episode.dedup_key`/`observation_count`, the `contradicts`/`supersedes` edge kinds, `subject_key` (`04` §2.4/§3/§5).
- DSR fields (`stability_days`, `difficulty`) + the append-only `review_event` ledger (rows captured from day one — outcome/reinforcement labels cannot be backfilled; the fold engine is rung-11 work, `04` §8.2).
- Procedural kind enum value + payload schema (the replay-validation harness is rung-10 work, `04` §4.2).
- L4 deliberate recall as an explicit `exhaustive`/benchmark mode, not the default hot path — the mode/flag/trace contract from day one; the agentic behavior at rung 12.
- All `05` §2 feature flags, retrieval-mode enum, and stage trace fields (a flag-disabled stage passes through and traces as such).
- The `mark` outcome-feedback verb and consolidation event taxonomy shapes (`08`, R77/R78).

**Built as behavior in v1 (the rungs 0–3 spine plus the alpha set — the cut line is owned by `29` §2a):**

- Raw episode/resource capture with citations, dedup, retention tiers.
- Write-path consolidation: traced `reflect`, contradiction detection (embedding-proximity + subject-key + valid-time overlap), source-*independent* corroboration for belief→semantic promotion, episodic near-dedup.
- Hybrid FTS + vector + RRF retrieval with budgeted context packing and abstention.
- Temporal validity (bitemporal generations, `correct`, `forget`).
- Rust HTTP + MCP surfaces; Python client; CLI.
- Golden and sampled eval harness with the retrieval-only oracle.

**Built at rung activation (interface frozen above; behavior lands when its `27` rung fires):**

- Relational edge expansion (rung 6, against the no-edges + filesystem controls), bounded/provider rerank (rung 8), query decomposition (rung 9), contextual-chunk enrichment (rung 4), procedure promotion/replay (rung 10), DSR decay fold (rung 11 — v1 ranks by plain recency/exponential, the field-standard baseline), L4 exhaustive behavior (rung 12), learned levers (rung 13).
- TypeScript client (generated from OpenAPI; first external TS consumer or launch window, whichever first).
- Public scorecard runner for LME-V2, BEAM, STATE-Bench, and poisoning suites (with the eval harness spine in place, at the sampled-public rung).

Rejected from the first public architecture:

- Separate graph database as a default dependency. Relational edges are the core graph.
- CRDT/Yjs procedural memory. Versioned procedure rows are enough for v1.
- Agent-native billing. Hosted billing is outside memory core.
- Large framework adapter matrix. Public API/MCP/SDKs are the adapter surface.
- SQLite/PGLite local store. Local mode uses Docker/plain Postgres.
- Vendor leaderboard business. Benchmarks prove the memory system; MemPhant is not a ranking company.

Feature flags are required eval controls and production safety switches.

## 6. Launch-Grade Definition

MemPhant is launch-grade only when these are all true:

| Gate | Requirement |
|---|---|
| Public boundary | Syndai can switch from internal memory to a separately deployed MemPhant service with config and adapter changes only. |
| Traceability | Every recall emits candidate-channel, trust-filter, fusion, rerank, context-budget, citation, latency, token, and cost facts. |
| Golden evals | Golden cases include seed corpus, query, expected memory IDs, expected citations, forbidden leaks, and trace assertions. |
| Benchmark ladder | PR, nightly, weekly, and release gates exist before any SOTA claim. |
| DB exposure | No direct browser/API-role access to memory tables unless deliberately protected by RLS policies and tests. |
| Deletion | Forget/delete invalidates raw blobs, derived units, embeddings, edges, traces, caches, and exports according to policy. |
| Public repo | Apache-2.0 repo includes license, notice, security, contribution, release, API, SDK, MCP, and eval docs. |
| Dogfood | Syndai consumes the same HTTP/SDK/MCP contracts an external customer would use. |

## 7. The Long-Term Bet

The product does not win by having the most memory features. It wins by making memory inspectable, measurable, and safe enough for long-running agents to trust.

The hard things to freeze now are:

- raw episode preservation, with a retention-tier lifecycle (hot/warm/cold) that bounds derived-index cost
- memory-kind policy separation
- scope tree and inheritance rules
- the write-path consolidation contract: contradiction detection, source-independent corroboration, episodic near-dedup
- evidence/citation ledger
- retrieval trace schema
- benchmark/eval harness contract
- DB/provider isolation contract
- API/MCP schemas
- extension points for decay, graph expansion, rerank, and skill promotion

The hard things explicitly rejected until evidence overrides the decision are:

- external graph database
- CRDT skill editing
- wide framework adapter matrix
- hosted enterprise control plane beyond the memory service itself

The full FSRS/DSR parameter fitter is not rejected; it is data-gated. The schema and update events ship now, fixed priors ship now, and learned fitting turns on when enough reinforcement traces exist to estimate parameters without cargo-culting Anki review data.

Pre-production freedom should be used to remove coupling and freeze long-term interfaces now. Anything that does not strengthen the frozen interfaces, benchmark loop, or Syndai dogfood cutover is rejected, not left vague.

# EXPERIMENTAL-THOUGHTS — 12 concrete proposals (2026-07-21)

Grounding repo: `/Users/sidsharma/.codex/worktrees/Memphant/p1-deep-mode` (read-only).
Key anchors used throughout:
- Deep machinery: `crates/memphant-core/src/deep_recall.rs` (DeepRecallProvider trait), `crates/memphant-core/src/lib.rs:120` (`build_deep_workspace` from `DeepSnapshotEntry` → files + `manifest_sha256`), `crates/memphant-runtime/src/deep_recall_openrouter.rs`, types `DeepRecallLimits/Status::Partial/Usage` incl. unsettled-spend upper bounds (`memphant-types/src/lib.rs:663-724`).
- Dormant file projection: `memphant compile` → per-entry `.md` + `index.md` + `memphant-export.json` (lock + source hash), verified by `verify --lock --export` (`crates/memphant-cli/src/main.rs:639-717,971-1003`).
- Outcome ledger: `mark` verb + `ReviewEvent` (`memphant-types/src/lib.rs:1796-1844`); procedural candidate→validated gate (spec `04` §4.2).
- Cold plane contract already spec'd: `object_store` crate, content-addressed `tenant_id/<hash[:2]>/<hash>` (spec `25` §5/132, §7a).
- Trace spine: `RetrievalTrace` carries `cost_micros`, `latency_ms`, `cross_rerank` I/O stats, `l4_*` deep fields, `consolidation_lag_ms` — the metering and diagnosis substrate is already computed per call.
- Binding campaign order from `docs/handoff/NEXT-SESSION-PROMPT.md`: T6 → T1 → P3.2 (SWE-CB Lite) → P3.1 (LME-S) → P3.3 (LME-V2); n=12 gate mandatory; no new architecture in P1–P3.

All proposals below are activations of existing machinery unless flagged WILDCARD. Every falsification test is same-lattice, scratch-DB, paired vs control, per standing rules.

---

## P1 — (d) Observation-block hot plane via the existing reflect worker, published to three surfaces at once

**Mechanism.** Exactly the T1 shape the handoff authorizes — a versioned derived projection owned by the `reflect` worker, not a sixth kind: dated observations append to the open generation until a token threshold, reflection atomically compacts and swaps generations, recall always serves the last complete generation as a stable prefix (stable-before-dynamic ordering so provider prompt caches hit). The sharpening this proposal adds: the block is ONE artifact published to THREE surfaces — (1) the chat hot path (prefix in the recall pack), (2) file 0 of every Deep workspace (`build_deep_workspace` just prepends a `DeepWorkspaceFile{path:"00-observations.md"}` entry — the agentic explorer gets orientation for free), and (3) the header of the compiled `.md` projection (P4). DRY: one reflect job, three consumers, one generation counter.

**Expected win.** Usecase 1 (chat) QA on the reader-bound lane — the flip analysis attributes 21/44 misses to reader-with-adequate-pack and 16/44 to pack displacement; a dense date-annotated block attacks both. Highest-external-evidence chat lever (Mastra OM pattern; re-measure, never import). Secondary: Deep orientation iterations drop (see P10).

**Cost/latency budget.** Background LLM compaction on the reflect lane only (construction cost charged separately per the handoff's cost policy); zero added per-turn latency; prompt-cache savings are net-negative cost at scale.

**Falsification test (n=12).** 12 LME-S dev questions from the 178-development split whose labeled failure class is pack displacement or reader-with-adequate-pack (labels exist in the 2026-07-13 flip-analysis artifacts, `docs/build-log/2026-07-13-agent-memory-lever-*.md`). Block+hybrid vs frozen v1 control, Sol Pro reader, reader cache reused. Promote iff ≥+2 net flips and no new abstention breaks; else delete code, keep the negative artifact.

---

## P2 — (f) The SOTA slice we can lead FIRST: LME-V2 dual operating point (fast + deep), 50Q pilot as the kill-switch

**Mechanism.** LME-V2's official leaderboard is EMPTY and ranks by LAFS Gain — an accuracy-latency frontier, not a single number. Nobody occupies the fast end: AgentRunbook-C's 72.5% costs 108–140 s/query; strongest RAG is 48.5%. MemPhant is the only system in the verified landscape with BOTH a proven sub-second hot path (recall p95 70.61 ms from the PMU campaign) and a T6 agentic-file deep path on the same substrate — two operating points is literally the shape LAFS rewards. Minimal run: Small-tier ~50Q dev subset at both operating points → full paired web+enterprise runs per operating point (every question covered; submission mechanics verified: Google Form + tarball + `submission_overview.json`) → first slot on the board. STATE-Bench (also empty, Microsoft, memory-agnostic) is the named backup slice, but it needs a full agent scaffold plus paired ablation to attribute the delta — strictly more spend for a weaker claim; LME-V2 first.

**Expected win.** The P3.3 claim already gated in the handoff: "first published result on the official LongMemEval-V2 leaderboard," plus a defensible Pareto-frontier sentence for the fast point even if deep doesn't beat 72.5%. This is the highest-mindshare slot reachable with machinery that already exists.

**Cost/latency budget.** Fast point near-free (local embeddings + Postgres). Deep point bounded by the existing per-run spend ceiling: at ~$0.05–0.30/query, 451Q × 2 corpora × 1 run ≈ $45–270 worst case; the 50Q pilot ≈ $15–30.

**Falsification test.** The 50Q dev pilot IS the cheap kill-switch, pre-registered: if fast <45% (below the strongest-RAG band) AND deep <60% on the pilot, the slice is not leadable yet — stop before any full run, assign misses to trace fields per spec `27` §0. Fixture: LME-V2 official repo dev split (arXiv:2605.12493).

---

## P3 — (c) Deep search as a product UX: one streamed request, cited partials, no job subsystem

**Mechanism.** The contract is already decided (P1 decision of record: Deep is an explicit action, never auto-selected, returns the same cited evidence contract, caps return best cited partial). What's missing is the surface. Extend the `DeepRecallProvider` loop so each accepted tool iteration emits a progress event — `{iteration, wall_ms, spend_micros, files_touched, evidence_so_far:[source_ids]}` — over SSE on `POST /v1/recall` with `mode=deep, stream=true`, and as MCP progress notifications on the existing rmcp server. The fields all exist per-iteration inside the provider loop (`DeepRecallUsage` is already accumulated; `source_ids` are already collected); streaming them is plumbing, not new state. Client cancel mid-stream maps to the existing `DeepRecallStatus::Partial` + unsettled-spend upper bounds — the types were built for exactly this. Final SSE event is the ordinary `RecallResponse` with `deep` summary, so non-streaming callers are unchanged.

**Expected win.** Usecase 1 UX (owner: best UX above all) — a "Deep search" button that visibly works for 100 s is a feature; a spinner for 100 s is a bug. Same instrumentation feeds the LAFS latency accounting P2 needs, and Syndai's existing task stream (per the decision of record) consumes the same events.

**Cost/latency budget.** Zero added model spend; SSE keeps one Axum connection open; progress events are byproducts of the loop. First event target <2 s (workspace materialization + first tool call).

**Falsification test (n=12).** 12 questions from the LME-V2 dev subset (share P2's pilot). Assert: first progress event <2 s on ≥11/12; first cited partial before 30% of total wall time on ≥9/12; cancel-at-50%-wall returns ≥1 valid citation on ≥10/12 with `Partial` status and settled-vs-unsettled spend recorded. Compare partial-answer QA vs completed-answer QA to calibrate the "is a partial worth showing" copy. Fixture: LME-V2 dev subset + existing deep trace schema.

---

## P4 — (a) File plane verdict: flat .md is a PROJECTION (compiled artifact), edits re-enter as retains — bidirectional without merge conflicts

**Mechanism.** The lockfile analogy is already half-built: `memphant compile <scope>` renders entries to `.md` + `index.md` + `memphant-export.json` carrying `MemphantLock` + source hash, and `verify --lock --export` detects drift (`memphant-cli/src/main.rs`). Promote it to the canonical answer: a deterministic scope→directory projection (`memory/` per scope: observation block header from P1, `semantic.md`, `procedures/*.md`, `index.md`), each entry footed with an HTML comment `<!-- memphant unit_id=… body_sha256=… generation=… -->`. Files are NEVER the source of truth; the write path is always the retain verb. Human/agent edits are detected by per-entry hash mismatch on `memphant sync` (the npcsh audit's hash-gated-ingestion takeaway) and re-ingested as ordinary episodes (`source_kind=file_edit`, trust=trusted_user), flowing through the existing admission control — supersession, contradiction detection, dedup — then the scope is recompiled. CLAUDE.md/agent-memory-dir conflicts dissolve structurally: compile is a pure function of canonical state, and concurrent edits race through admission (which already has single-apply supersession, `04` §3.4) instead of through three-way file merges. SOURCE-only and true-bidirectional are rejected: SOURCE-only forfeits provenance/bitemporality; bidirectional-at-the-file demands a CRDT/merge machinery that admission control already IS.

**Expected win.** Usecases 1+3 distribution — the handoff's "any-agent distribution (MCP + file adapters)" end state. Any Claude Code/Codex/Cursor agent gets MemPhant memory as a plain directory with zero integration. Also the offline/exportability story for CaaS (P8).

**Cost/latency budget.** Compile = one scope read + fs writes, O(units in scope), <1 s/scope; sync = ordinary retain path on changed entries only. No hot-path involvement.

**Falsification test (n=12).** Round-trip pack on 12 scopes seeded from the spec-28 fixture families + Syndai golden set: compile → apply 4 edit classes across files (mutate fact, append fact, delete entry, write contradicting fact) → sync → assert canonical store shows supersede / new unit / forget-candidate / contradiction-edge respectively → recompile → assert fixed point (compile∘sync∘compile byte-identical) and `verify --export` clean. Fails if any edit class corrupts state or the projection doesn't converge.

---

## P5 — (e) Codebase memory shape: BOTH — repo-index resources first, outcome write-back second, with a contamination firewall between the experiments

**Mechanism.** Repo indexing needs no new machinery: files ingest as `ResourceKind::Code` resources with `revision` = commit hash (fields exist on `NewResource`), the extractor state machine chunks/embeds them, and Deep mode materializes them as a workspace — that is the read-path baseline for SWE-ContextBench. Outcome write-back (T8) activates the dormant half: the `mark` verb + `ReviewEvent` ledger already records `used_ids` + Success/Failure/Corrected per trace; a reflect job compiles completed-task trajectories into candidate procedural units (`04` §4.1 payload, keyed repo+revision) that pass the candidate→validated gate before retrieval eligibility, with `validation_state` visible in `ProcedureTraceFact`. Firewall (handoff rule): the read-path experiment runs with `procedure_recall_enabled=false` (a first-class `RecallRequest` flag) until the repo-index baseline is frozen — write-back may never contaminate the retrieval measurement.

**Expected win.** Usecase 3; SWE-ContextBench resolved rate (floor: no-context 26.26%; target: Supermemory 30.3% → the P3.2 claim). Write-back direction externally supported (Letta +9/+15.7 Terminal Bench, vendor-reported — re-measure).

**Cost/latency budget.** Indexing one-time per revision (embedding cost only, local fastembed); write-back on the reflect lane; retrieval unchanged.

**Falsification test (two sequential n=12s).** (i) 12-task SWE-ContextBench-lite subset matching the paper's Claude Sonnet 4.5 setup: repo-index recall vs no-context; continue iff resolved ≥ no-context. (ii) Only after (i) freezes: same 12 tasks with procedural units minted from trajectories of 12 DISJOINT previously-solved tasks; promote write-back iff ≥+2 resolved and zero unsafe-procedure reuse (rung-10 disable-when). Fixture: SWE-ContextBench 99-task related set.

---

## P6 — Comparable-volume docs win: kill the 14x asterisk with a budget-matched arm (claim-integrity lever, nearly free)

**Mechanism.** The docs-gate win (+0.142 pooled) carries the 14x reader-evidence-volume asterisk — the one thing that would not survive the post-Mem0/Zep evidence bar we ourselves enforce. `budget_tokens` is a first-class `RecallRequest` field: re-run the existing exposed sets with MemPhant's pack capped at Syndai RAG's median evidence tokens (measure it from the archived runs first). If the pooled delta shrinks but the CI still excludes zero, the comparable-volume win is earned and the R6 unlock fires clean; if the delta dies, the honest conclusion is that the win is volume, and the next lever is packing quality (`05` §1.2 fusion/assembly), not retrieval — either way the asterisk is resolved before the P2 cutover leans on it.

**Expected win.** Usecase 2 claim integrity; unblocks the Syndai RAG cutover with a defensible sentence. Cheapest true-information-per-dollar item on this list.

**Cost/latency budget.** One re-run of existing exposed sets at capped budget; reader cache (`docs/build-log/artifacts/r0-embedder/reader-cache`) makes unchanged-evidence re-scores free; hours, not days.

**Falsification test (n=12 first).** 12 questions MemPhant won in the k=10/b8192 runs, re-run at matched budget: if ≥8/12 hold, run the full exposed sets; if <8, skip the full run and file the packing-quality lever instead. Fixture: `docs/build-log/2026-07-13-rag-retrieval-admission.md` artifacts + the corrected 4,870-section corpus.

---

## P7 — (g) Rank-compression cross-rerank: top-16 × 256-token chunk-truncated pairs, <1.5 s, flag already exists

**Mechanism.** The +0.158 QA cross-encoder was latency-retired at 13 s/query CPU because it scored the full pool at full body length. Compress both axes using machinery already in the trace: (i) rerank only the top-16 by fused score — rank-sensitive flips concentrate at the top; the retired run's `CrossRerankTrace`/candidate records (fused_rank vs rerank_rank) tell us exactly how many of its flips occurred inside top-16 BEFORE we build anything; (ii) truncate each pair input to header + best `ContextualChunk` (~256 tokens) instead of full bodies — `input_chars_p50/p95/max` fields exist to verify the compression landed; (iii) execute via either the already-validated Voyage rerank-2.5 API (docs lane measured p95 ≈ 0.9–1.0 s) or a local ONNX cross-encoder at the 16×256 budget (~0.5 s CPU). Balanced mode only; fast stays rerank-free; the `CrossReranker` seam and flag are already in the service.

**Expected win.** Usecases 1+2: recover a measured fraction of the +0.158 QA delta inside product latency; docs lane R@10 secondarily.

**Cost/latency budget.** ≤1.5 s p95 added in balanced mode; ~$0.02/1k queries (Voyage) or $0 (local ONNX).

**Falsification test (n=12 + one free pre-check).** Pre-check (zero cost): from the retired run's sampled traces, count what fraction of its beneficial rank flips are reproducible within top-16 — if <60%, stop. Then 12 rank-sensitive cases (identified from those same traces): compressed rerank vs no-rerank, same lattice, packaged runtime; promote iff ≥8/12 reproduce the full-rerank ordering AND measured p95 <1.5 s. Fixture: rung-8 sampled-trace artifacts from the retired 13 s arm.

---

## P8 — (h) CaaS gap analysis: metering, per-tenant ceilings, fair reflect scheduling, tenant offboarding — pricing is trace rollups, not new plumbing

**Mechanism.** Isolation exists (TenantId on every row, `04` §7.0 partitioning, API-key trust clamps, tenant-bound traces). What external customers need and we lack, each a thin activation: (1) **Metering** — every `RetrievalTrace` already computes `cost_micros`, `token_estimate`, `latency_ms`, mode, and Deep settled+unsettled spend; every `ReflectTrace` has `cost_units`. Add a per-tenant daily rollup (one table + one worker sweep) and `/v1/usage` — the pricing axes fall out: retains, storage bytes, fast/balanced recalls, deep spend pass-through with margin. (2) **Per-tenant policy** — `DeepRecallLimits` exists per-run; add per-tenant monthly deep-spend ceiling + rate limits as API-key policy rows, fail-closed to `Capped` (status exists). (3) **Fair reflect scheduling** — per-tenant weighted dequeue on the job queue so one tenant's ingest burst can't inflate everyone's `consolidation_lag_ms` (the SLO metric is already on every trace and health has `dead_letter_jobs`). (4) **Offboarding** — tenant-wide export = P4 compile over all scopes + resource tarball + traces; tenant delete = existing forget machinery swept per-partition. Items 1–2 are the pricing blockers; 3–4 are the enterprise-trust blockers.

**Expected win.** CaaS launch-ready unit economics (usecase 1 external customers); deep pass-through pricing is uniquely honest because unsettled-spend upper bounds are already tracked — competitors can't bill caps correctly.

**Cost/latency budget.** Zero hot-path cost (fields already computed); one rollup table + sweep; queue weighting is a dequeue-order change.

**Falsification test (n=12 tenant-sim).** 12 synthetic tenants (4 × 3 traffic profiles) replaying mixed-verb traffic via the e2e probe against one scratch DB: assert (a) Σ per-tenant rollups == Σ global trace values exactly (no leakage, no loss); (b) with one tenant at 10× ingest, others' `consolidation_lag_ms` p95 rises <2× with fairness on, vs unbounded with it off; (c) tenant-delete leaves zero rows for that tenant_id across all tables. Fixture: `scripts/e2e_probe.sh` + spec-28 fixture families.

---

## P9 — (b) Supabase Storage / S3 as the cold/artifact plane: content-addressed bodies out of Postgres, hot rows stay

**Mechanism.** Activate spec `25`'s already-decided contract (object_store crate, customer-brings-the-bucket, content-addressed `tenant_id/<hash[:2]>/<hash>`; Supabase's role is Postgres + Storage, never compute — R93). Division of labor: **Postgres keeps** everything queried on the hot path — units, embeddings, contextual chunks, resource metadata + `content_hash`, small bodies (<~64 KB). **Bucket holds** — large resource raw bodies (`StoredResource.body` becomes a pointer past the threshold), Deep workspace snapshots keyed by `manifest_sha256` (the audit artifact for "what did the agent see"), archived `RetrievalTrace`s past N days, and tenant export tarballs (P8). Retention tiers (`04` §2.4) get their physical form: hot=PG rows, warm=chunks in PG, cold=bucket. Fetch is lazy and only on Deep materialization or citation-quote verification — the fast/balanced path never touches the bucket.

**Expected win.** Usecases 2+3 at scale: repo and doc corpora (and LME-V2's 25M–115M-token ingest) without Postgres bloat; CaaS COGS (object storage ~10× cheaper than managed-PG storage); Deep audit artifacts become durable and cheap.

**Cost/latency budget.** One config switch per spec 25; +50–150 ms per cold body fetch on Deep materialization only (amortized by P10's cache); hot-path p95 unchanged by construction.

**Falsification test (n=12).** Ingest the 12 largest resources from the 4,870-section docs corpus twice — bucket-backed vs PG-backed control, same lattice: assert byte-identical Deep `manifest_sha256` and identical recall traces (proves the plane split is invisible to retrieval); measure PG size delta and deep-materialization p95 delta. Fixture: corrected docs corpus + `object_store` local-FS backend (no cloud account needed for the test).

---

## P10 — WILDCARD: Deep orientation cache keyed by `manifest_sha256` — sleep-time-lite that amortizes at query #2, not #10

**Mechanism.** Every Deep run pays a query-independent orientation prefix: materialize workspace, list files, read manifest/observation block. `manifest_sha256` already uniquely identifies workspace content. Cache two things keyed by (scope, manifest_sha256): the materialized workspace itself (ties into P9's snapshot storage), and the orientation transcript — the first K tool iterations before the first query-conditioned search — replayed as a prefix for the next deep query against unchanged memory. This deliberately respects the T9 boundary the handoff draws (no anticipatory ANSWER precompute; that requires ≥~10 queries/context to pay): materialization + orientation are deterministic per workspace state, so the cache pays from the second query and invalidates itself for free on any memory change (hash miss). It also stacks with provider prompt caching when the orientation prefix is byte-stable.

**Expected win.** Deep operating point latency + spend (helps P2's LAFS frontier and P3's UX): if orientation is 20–40% of iterations (verify in OUR traces first — `generation_ids` let us count), that fraction of spend/wall time disappears for repeat deep queries, which is exactly the CaaS deep-usage pattern (a user drilling into one corpus).

**Cost/latency budget.** Storage of one workspace snapshot + a few KB of transcript per (scope, hash); zero cost when memory changed; no correctness risk because cache key = content hash of everything the agent can see.

**Falsification test (n=12).** 12 LME-V2 dev questions run as 6 pairs of sequential deep queries against unchanged scopes, cache on vs off, same lattice: promote iff median spend −25% and wall −20% on the second query of each pair with byte-identical citation sets. Pre-check (free): measure orientation fraction in existing deep traces; if <15%, don't build. Fixture: LME-V2 dev subset + archived deep traces.

---

## P11 — WILDCARD: "Receipts mode" — citation spans + quote hashes surfaced end-to-end as the trust UX

**Mechanism.** `StoredCitation` already carries `span: Option<CitationSpan>` and `quote_hash`; the citation ledger passed the rung-1 ≥99% validity gate. Activate the last mile: recall responses (all modes, incl. Deep partials in P3) resolve each citation to an exact source span; a verifier recomputes `quote_hash` against the canonical episode/resource body at read time and flags drift (catches store corruption, extractor regressions, or tampering fail-closed); product surfaces render a per-claim "show receipt" that opens the quoted span with provenance (source_ref, observed_at, trust level, bitemporal window). No competitor in the verified landscape surfaces verifiable quotes — and post-Mem0/Zep-scandal, "every remembered fact has a hash-verified receipt" is our evidence-discipline turned into a visible product feature rather than an internal virtue.

**Expected win.** Best-UX mandate (usecase 1) + docs-lane "supported answers" metric directly (usecase 2); differentiation copy for CaaS; also hardens the P4 file projection (footers carry the same hashes).

**Cost/latency budget.** Hash recompute is microseconds per item; span resolution is one indexed read of an already-fetched body; UI is client-side.

**Falsification test (n=12).** 12 items from the docs-lane supported-answers set: assert 12/12 recall items resolve to spans whose quote_hash verifies against canonical bodies; then mutate one stored body out-of-band and assert the verifier flags exactly that citation (fail-closed, not silent). Fixture: docs corpus + citation ledger fixtures from rung 1.

---

## P12 — WILDCARD: MCP resources adapter — mount the P4 projection as live MCP resources for any-client distribution

**Mechanism.** The rmcp MCP server already exists (WS-D proven). Serve the P4 compiled projection through MCP `resources/list` + `resources/read` + subscriptions: a client like Claude Code mounts `memphant://scope/<x>/observations.md` etc. as readable context files and receives update notifications on each generation swap (P1's atomic swap gives a clean subscription edge). Writes stay on the existing retain/mark tools. This is the same artifact as P4 delivered without filesystem access — the "npm-install memory for any MCP agent" wedge, and the CaaS acquisition funnel (free tier = mounted memory resources; paid = deep mode + tenancy).

**Expected win.** Usecases 1+3 distribution; makes the file-plane answer (projection, P4) portable to clients we don't control. Cheap because it composes P1's block, P4's compiler, and the existing MCP surface.

**Cost/latency budget.** Resource endpoints over the existing compile output; notification on generation swap only; near-zero runtime cost.

**Falsification test (n=12).** 12 tasks from the spec-28 fixture families (arch-decision honored, compaction rehydrate, cross-agent transfer) run in a real MCP client session: projection-mounted-resources arm vs recall-tool-only arm; measure decision-honored rate and tokens consumed. Promote iff honored-rate ≥ control with ≤1.5× token cost. Fixture: spec-28 acceptance fixture families (already the P2-CaaS gate).

---

## Coverage map

(a) P4 (+P12 transport) — projection verdict with hash-gated re-ingest.
(b) P9 — Supabase/S3 cold plane split.
(c) P3 — Deep as streamed product UX.
(d) P1 — observation block via reflect, tri-surface.
(e) P5 — both, sequenced with firewall.
(f) P2 — LME-V2 dual operating point, 50Q kill-switch.
(g) P7 — top-16 × 256-token rank compression.
(h) P8 — metering/ceilings/fairness/offboarding.
Wildcards: P6 (claim integrity), P10 (orientation cache), P11 (receipts), P12 (MCP resources).

## Ranked order (leverage × cheapness, respecting the binding T6→T1→P3.2→P3.1→P3.3 order)

1. P1 observation block — active-second lever, highest external evidence, feeds three surfaces.
2. P2 LME-V2 dual operating point — the first credibly leadable public slot; pilot is a $30 kill-switch.
3. P3 Deep UX streaming — UX mandate + shared instrumentation with P2; near-zero spend.
4. P6 comparable-volume docs re-run — hours of work, removes the one asterisk on our best result.
5. P4 file-plane projection — distribution wedge; machinery half-built in the CLI.
6. P5 codebase both-with-firewall — P3.2 path; two n=12s.
7. P7 rank compression — free pre-check from retired traces before any build.
8. P8 CaaS metering — pricing from existing trace fields.
9. P9 S3/Supabase cold plane — scale + COGS; invisible-to-retrieval by test.
10. P10 orientation cache — build only if the free trace pre-check shows ≥15% orientation share.
11. P11 receipts mode — small, differentiating, hardens P4.
12. P12 MCP resources — composes P1+P4; do after both exist.

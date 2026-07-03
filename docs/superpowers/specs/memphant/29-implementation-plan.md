# MemPhant - Implementation Plan

> Status: SPEC, pre-build.
> Owner: implementation order and status tracking.
> Rule: build the hard-to-retrofit interfaces now; activate expensive or risky behavior only behind gates that prove it helps.

This is the operational plan. `10-build-plan.md` is the short dependency summary; this file is the EvalRank-style build spine that tracks order, exit packets, and when "do not build yet" items become buildable.

## 0. Current Status

**Live status lives in ONE place: `STATUS.md`** (the checkbox ledger with the deterministic DONE definition). This doc owns the *order and gate contracts*; STATUS.md owns *state* — do not maintain a second status table here. Snapshot at the time of the STATUS.md cutover: spec corpus complete (16 passes, gates green); no repo, runtime, adapter, DB, or tests exist yet; advanced levers are interface-specified with §8 activation gates.

## 1. Execution Doctrine

1. **One public product boundary.** MemPhant is standalone; Syndai consumes the public API/SDK/MCP surface.
2. **No private Syndai shortcut.** A feature is not done if only Syndai can use it.
3. **Build contracts before knobs.** Schema, API, trace, policy, and eval contracts land before dashboards, adapters, hosted packaging, or expensive research features.
4. **Advanced is not deferred if the interface is hard to add later.** Store the fields, traces, flags, and extension seams now. Turn on the behavior only when its gate passes.
5. **Every build slice has an exit packet.** A slice is complete only when it leaves a runnable command, fixture, trace archive, schema snapshot, or cutover proof.

## 2. Workstream Order

### 2a. V1 Cut Line and Calendar Envelope (R73 — this section owns the cut)

The suite froze every interface; this section pins what v1 **builds**. The doctrine (§1 item 4) always said "store the fields, turn on the behavior at its gate" — R73 resolved the older build-everything phrasing in `00-MAIN` §5 / `05` §1 / `27` §2 to that doctrine.

- **V1 built behavior = the §3 slices 1–13 PLUS the §5 alpha-gate completion set** (`correct`/`forget` through append-only generations, the full MCP verb set, Python SDK examples, DB exposure gate). Slices 1–13 alone stop at export-compare and are NOT a shippable v1.
- **Rung-gated behavior is NOT built in v1** (interfaces frozen, engines at their `27` rung): Stage-4 edge expansion (rung 6), provider rerank (rung 8), query decomposition (rung 9), contextual-chunk enrichment (rung 4), procedural replay-validation harness (rung 10), DSR fold + fsrs integration (rung 11 — the `review_event` ledger CAPTURE is v1; only the fold engine waits), L4 exhaustive behavior (rung 12), learned levers (rung 13).
- **Soft calendar envelope:** alpha gate (§5) targets **~8–10 weeks of build effort** from WS-0 exit. This is a forcing function, not a promise: if the envelope is clearly unreachable, cut in this order before extending it — TypeScript SDK → contextual-chunk job → adaptive cascade → provider-rerank plumbing → `hnsw_binary`/scale levers (modulus-1 posture is fine for alpha). Never cut: tenant isolation, citations, deletion completeness, the trace spine, the golden oracle.
- **Launch is NOT hostage to full Syndai cutover** (see WS-F stop-rule below): the public-launch dogfood proof is one low-risk surface exported + trace-compared; further surfaces migrate post-launch on their own gates.

### WS-0 - Spec and Repo Freeze

Purpose: remove stale claims before code exists.

Build:

- Standalone repo skeleton: Rust workspace, Apache-2.0, `SECURITY.md`, `CONTRIBUTING.md`, DCO, README.
- Spec patches from the 2026 review:
  - `sparsevec` HNSW cap is 1,000 non-zero elements, while storage supports 16,000.
  - MCP launch contract is selected at WS-D start from the stable spec then available; `2025-11-25` is stable today, and the `2026-07-28` stateless redesign remains a material RC to re-evaluate before implementation.
  - `rmcp` output-schema guidance uses `Tool::with_output_schema<T>()` where canonical types derive `JsonSchema`.
  - `28-syndai-code-contract.md` refreshes the checked file list and records that Syndai has no canonical agent `forget` tool today.
- `memphant.lock` format stub: `engine_version`, `compiler_version`, `trace_schema_version`, `schema_compat_revision`, `methodology_version`, export schema version.
- **Two-language spike (R83, the Decision #2 crux experiment):** implement `retain` + a minimal golden-runner in BOTH Rust and Python with the actual build team; measure wall-clock to change an extraction policy end-to-end. <1.5× (Rust vs Python) = Rust proceeds; ≥3× = re-open Decision #2 in `26` before WS-A freezes the workspace. The spike result is recorded in the build log either way.

Exit packet:

- `cargo metadata` works in the new repo.
- `memphant.lock` schema snapshot exists.
- Spec drift checklist passes.
- Two-language spike result recorded (Rust-vs-Python iteration wall-clock + the go/re-open call).

### WS-A - Schema, Core Types, and Store Seam

Purpose: freeze every table/key/trace shape that is painful to retrofit.

Build:

- `tenant`, `subject`, `actor`, `agent_node`, `scope`, `scope_policy`.
- `episode`, `resource`, `memory_unit`, `memory_edge`, `embedding_profile`, `embedding`.
- `citation`, `trust_event`, `retrieval_trace`, `deletion_generation`, `job_state`, `blob_ledger`.
- `belief_observation`, `review_event` (the day-one append-only ledgers — capture is v1 even though the folds are rung-gated, `04` §5.1a/§8.2), `scope_block` (`04` §12).
- Append-only bitemporal generation model.
- `MemoryStore` transaction seam and in-memory fake.
- Provider bootstrap/lint command over plain Postgres, Supabase, and Neon.

Exit packet:

- Fresh DB bootstrap succeeds.
- Provider lint checks tenant columns, RLS/grants, indexes, extensions, `search_path`, FK indexes, and migration ledger.
- Unit tests prove tenant/scope IDs are required on every tenant-scoped table.

### WS-B - Write Path and Memory Compiler

Purpose: durable capture first, derived memory second.

Build:

- `retain` stores raw episode/resource before extraction.
- Transactional enqueue for `reflect`.
- Dedup key + observation count.
- Candidate extraction into memory units.
- Write-time admission action: `reject`, `append`, `merge`, `supersede`, `invalidate`, `quarantine`.
- Contradiction detection: subject key + embedding proximity + valid-time overlap.
- Source-independent corroboration for belief -> semantic promotion.
- Active freshness fields and due scan for volatile facts.

Exit packet:

- Golden fixtures pass for noisy-write rejection, duplicate collapse, contradiction detection, corroboration-farming resistance, and stale fact handling.
- `reflect` emits stage/cost/trace facts and is idempotent under duplicate job delivery.

### WS-C - Read Path and Trace Spine

Purpose: answer-bearing recall with explainable misses.

Build:

- Stage 0 policy gates.
- Exact/entity, FTS, vector, temporal, and edge candidate channels.
- Weighted RRF fusion.
- Budgeted context packing with abstention and contradiction warnings.
- Candidate whitelist and citation ledger.
- Retrieval trace write/read.
- Agent memory capsule output: admitted scopes, candidate whitelist, budget, citations, suppression labels, trace ID.

Exit packet:

- Every recall writes a complete trace, including denied recalls.
- Golden oracle proves answer-bearing IDs appear or explains miss category.
- Tenant isolation, L1+ denied memory, citation whitelist, small-tenant filtered vector recall, stale suppression, and deletion-generation filters pass.

### WS-D - Public Surfaces

Purpose: make every integration use the same contract.

Build:

- Axum REST API for `retain`, `recall`, `reflect`, `correct`, `forget`, `trace`, `mark`, scope memory listing, health.
- OpenAPI and JSON Schema snapshots.
- MCP stdio and Streamable HTTP server over the same verbs.
- Python HTTP SDK plus optional native wheel shell.
- TypeScript SDK.
- CLI: `retain`, `recall`, `reflect`, `correct`, `forget`, `trace`, `mark`, `db lint`, `verify`.

Exit packet:

- REST examples round-trip.
- SDK examples round-trip.
- MCP tool `inputSchema` and `outputSchema` validate.
- `memphant verify` detects schema/trace/export drift.

### WS-E - Eval, Security, and Ops

Purpose: create the cheap loop before public benchmark claims.

Build:

- Executable YAML golden oracle.
- Manifest/orphan guard for golden cases.
- Trace schema snapshot test.
- Security fixture suite: poisoning, query/filter injection, high-risk action suppression, tenant leakage, deletion completeness.
- Sampled public benchmark runner.
- Release benchmark runner.
- Blob GC, deletion saga, reindex/compaction SLA checks.
- Compiled Markdown export and eval trace notebooks.

Exit packet:

- PR gate runs unit/property/golden/security-smoke locally.
- Nightly sampled runner archives traces.
- Deletion completeness attack lane passes.
- `memphant compile --scope ... --out wiki/` exports a read-only Markdown view and `memphant verify` can report it stale.

### WS-F - Syndai Dogfood Cutover

Purpose: prove MemPhant against real Syndai memory contracts, then delete replaced paths.

**Stop-rule (R73).** WS-F is phased and evidence-gated per surface; it can stall without holding launch hostage. If the FIRST low-risk surface's trace-compare cannot converge within its gate (persistent unexplained mismatches after fixture triage), STOP the cutover: public launch proceeds with the export + trace-compare proof for that surface only, the mismatch analysis becomes golden fixtures, and further surfaces wait for MemPhant fixes — Syndai's working memory system is never destabilized to make a launch date. This honors the standing "surgical fixes, don't re-baseline" posture: each surface migrates only when its gate proves parity-or-better, and replaced Syndai paths are deleted only after that proof.

Build:

- Export one low-risk memory surface from Syndai to MemPhant.
- Trace-compare adapter against `MemoryContextLoader`.
- Golden cases for every mismatch.
- Active-read cutover for that surface.
- Correction cutover for that surface.
- Backend/UI forget cutover for that surface.
- Repeat for L0 user recall, project/resource memory, then remaining memory surfaces.

Exit packet:

- Focused Syndai memory tests pass.
- Trace compare passes.
- Mobile Memory Hub/citation/correction behavior remains covered.
- Replaced Syndai-specific memory code is deleted after the surface passes gates.

### WS-G - Public UI, Docs, and Launch Surface

Purpose: expose inspectability after the trace substrate exists.

Build:

- Docs site.
- Trace explorer.
- Memory inspector.
- API keys and usage pages.
- Eval run viewer.
- Compiled memory export viewer.

Exit packet:

- Public UI never reads MemPhant DB directly.
- Accessibility, route, and Playwright gates pass.
- Every visible memory/citation item links to a trace or citation path.

### WS-H - BYOC, Hosted Packaging, and Deployment

Purpose: make self-host and hosted use boring after core behavior is proven.

Build:

- Docker and Compose.
- Provider bootstrap profiles for plain Postgres, Supabase, and Neon.
- Hosted closed control-plane hooks: billing, region routing, tenant provisioning.
- Supabase BYOC preflight.
- Backup/restore/PITR reconciliation runbook.

Exit packet:

- Plain Postgres self-host path is documented and green.
- Supabase BYOC bootstrap/lint passes without touching `public`.
- Hosted service uses the same core API behavior as self-host.

### WS-I - Advanced Lever Activation

Purpose: turn on the expensive SOTA levers only when traces say which lever matters.

Build only through the Section 8 gates.

Exit packet:

- Each lever has archived before/after traces, paired deltas, cost/latency, security/deletion result, and default/exhaustive-mode decision.

## 3. First Implementation Slices

These are the first commits, in order:

1. Repo skeleton + lock/schema snapshots.
2. Core type crate with IDs/time/errors/config and schema snapshots.
3. Postgres migration scaffold + provider bootstrap lint.
4. In-memory `MemoryStore` fake + transaction seam tests.
5. `retain` raw episode path.
6. `reflect` no-op job with durable trace and idempotency.
7. Executable golden oracle harness.
8. `recall` exact/FTS-only path with citation whitelist.
9. Vector channel and filtered-recall tests.
10. RRF/context pack/abstention.
11. REST `retain`/`recall`/`trace`.
12. MCP `recall` over local stdio.
13. Syndai export-only trace compare for one low-risk surface.

## 4. Gates

Slice-local PR gate:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo nextest run --all-features
cargo test --doc
```

Add the commands below as soon as the slice creates their inputs:

```bash
memphant db lint --provider plain-postgres
memphant-eval verify-golden examples/evals/golden.yaml
memphant-eval run examples/evals/golden.yaml
memphant-eval security examples/evals/security-smoke.yaml
```

The steady-state PR gate is all commands in both blocks. No command is waived after its fixture exists.

Syndai cutover slices also run focused backend/mobile gates named in the target surface plan, then `make check` if backend contracts changed.

## 5. Alpha Gate

Alpha is ready only when:

- `retain` stores recoverable raw episodes/resources.
- `reflect` derives at least one citeable memory unit.
- `recall` returns cited context through exact, FTS, vector, and edge channels.
- every recall has a complete trace.
- tenant/scope/L0-L1+ gates pass.
- `correct` supersedes through append-only generations.
- `forget` hides affected memory immediately and emits a deletion generation.
- MCP can recall from a local server.
- SDK examples work.
- DB exposure gate is green.
- Syndai can export and trace-compare one low-risk surface.

## 6. Dogfood Gate

Syndai may actively read from MemPhant for a surface only when:

- surface fixtures derived from `28-syndai-code-contract.md` are executable.
- trace compare passes or every mismatch has an accepted product decision.
- L1+ blocked-memory cases have zero failures.
- citations render in the target UX.
- correction candidate/selector flow meets the adapter contract thresholds.
- backend/UI forget semantics meet the adapter contract thresholds.
- no web/mobile client talks to MemPhant DB directly.

## 7. Public Launch Gate

Public launch requires:

- public API, SDK, MCP, CLI, docs, and examples.
- self-host Docker/Compose path.
- security policy and release process.
- golden, security, sampled benchmark, and deletion completeness gates green.
- one reproduced public benchmark profile with cost/latency/config/traces.
- no critical Supabase/provider/advisor warning for hosted DB exposure.
- no hidden Syndai-only API field or behavior.
- public SOTA claim, if any, says exactly which axis it wins.

## 8. Do-Not-Build-Yet Activation Gates

These are not "never." They are buildable when their gate is true. Interfaces are frozen earlier if retrofitting would be costly.

| Item | Freeze now | Build/activate when | Gate |
|---|---|---|---|
| Public dashboard | trace IDs, citation paths, eval run IDs | WS-G | WS-A through WS-E green; dashboard reads API only. |
| Compiled Markdown/Obsidian export | export schema, lock metadata | WS-E | trace/citation schema stable; export is read-only and verify can detect staleness; treat it as an inspection nicety unless a benchmark/user need proves quality impact. |
| Eval trace notebooks | trace schema and golden result shape | WS-E | golden oracle and trace archive exist. |
| Agent memory capsules | recall result fields | WS-C | citation whitelist and suppression labels pass. |
| L4 exhaustive recall | `retrieval_mode=exhaustive`, trace fields | WS-I | answer-bearing misses remain after rungs 0-11; sampled benchmark gain beats latency/cost floor. |
| DSR decay fold (fsrs engine) | DSR fields + day-one `review_event` ledger capture + `mark` grades | rung 11 | internally-run longitudinal suite shows FSRS beats plain exponential (`27` rung 11, R82); v1 ranks by recency/exponential. |
| Procedural replay-validation harness | `kind='procedural'` + payload schema + validation-state fields | rung 10 | procedural recall is activating; adversarial replay gate (`04` §4.2) built then — schema now, harness at the rung. |
| 3-tier DEK envelope encryption | `key_custody` table shape | first BYOC/enterprise customer | tombstone+compaction+RLS meet the design-partner threat model; envelope + crypto-shred build when a customer contract requires it (`06` §6.1.1; backup-window caveat stated honestly). |
| Ablation-voting recall (SMSR-style) | none (recall-mode composition) | WS-I / exhaustive mode | high-stakes tenants demand it AND the `05` §9 arm shows containment gain worth k× read cost. |
| Delta recall / miss-repair re-extraction / retrievability probe | `delta_base_trace_id` trace field; `reextract_on_miss` job row; probe flag (R80) | WS-I per flag | each promotes only on its own paired ablation; deletion-completeness eval covers the delta path before it enables. |
| Consolidation event delivery (outbox consumers) | event taxonomy shapes (`20` §3) + `GET /v1/events` cursor contract | post-v1 | first external integrator needs push; outbox table lands with WS-B writes, delivery surface builds later (R78). |
| Learned reranker | `Reranker` trait, trace field, archived training set ID | WS-I | bounded rerank leaves rank-sensitive misses; paired delta CI excludes zero and p95 stays in budget. |
| Learned DSR/FSRS fitter | review ledger fields, DSR fields | WS-I | MemoryStress-style longitudinal eval shows fixed prior or exponential decay underperforms and enough review traces exist. |
| External graph DB/vector engine | `MemoryStore` route by profile/tenant | WS-I | relational edge/pgvector traces prove bottleneck and specialized engine improves target axis beyond cost/ops penalty. |
| Cache cluster | trace fields for repeated recall/cost | WS-I or hosted scale | p95/cost traces show repeated recall dominates and single-node/object cache is insufficient. |
| Framework adapter matrix | API/SDK/MCP contract | post-launch or partner slice | repeated external requests for one framework and no core API change needed. |
| Go/Rust/Java SDKs beyond Rust crate docs | OpenAPI schemas | post-launch | Python/TS adoption works and demand justifies maintenance. |
| Helm/Kubernetes chart | Docker config schema | after Docker self-host green | at least one real deployment needs k8s; Docker/Compose path is stable. |
| SQLite/PGLite local store | none | only if target users require offline local and Postgres blocks adoption | separate storage semantics can pass all DB/eval/delete gates. Otherwise keep rejected. |
| CRDT/Yjs memory editing | append-only generations | only after real concurrent human editing demand | ordinary scope rows and append-only generations cannot handle the workflow; deletion/trust/audit still pass. |
| Skill/procedure compiler | procedural payload schema, validation state | only after validated procedural memory improves STATE-style tasks | unsafe replay/security suite passes; exact tool args remain governed. |
| Hosted multi-region | immutable `tenant.region`, export/import | hosted enterprise | single-region cell is green; enterprise residency contract requires it; no cross-region core rewrite. |
| Billing/control plane | metered event IDs, tenant status field | hosted service | self-host core behavior is green; hosted packaging starts. |
| Broad public GTM automation | launch event taxonomy | after public launch candidate | product proof exists; provider accounts and human review gates exist. |

## 9. Build Logs and Status

Use the EvalRank pattern:

- This file owns build order.
- **`STATUS.md` (already live in this spec dir; moves into the MemPhant repo at WS-0)** is the single checkbox ledger reporting current coverage and latest proof — every completed slice/rung/gate flips its box there in the same change as its proof artifact, and its banner flip IS the definition of done.
- `docs/build-log/YYYY-MM-DD-<slice>.md` records each completed slice with commands, artifacts, and what was deliberately not built.
- Public-safe status never copies private Syndai rows, secrets, live customer data, or held-out eval answers.

Minimum build-log template:

```markdown
# <Slice>

## Changed
- ...

## Proof
- command: ...
- artifact: ...

## Not Built
- ...

## Next
- ...
```

## 10. Stop / Narrow Rules

Stop adding platform area and narrow the claim when:

- internal Syndai contract fixtures fail.
- tenant isolation, deletion completeness, or poisoning gates fail.
- public sampled benchmarks show no differentiated win after rungs 0-12.
- only L4 exhaustive mode wins and is too expensive for a credible Pareto claim.
- traces show the downstream answer model, not memory retrieval, is the bottleneck.

Allowed narrowed products:

- trace/eval harness
- poisoning defense middleware
- coding-agent memory niche
- Syndai-only memory hardening

Do not compensate for weak core recall by adding dashboards, adapters, or hosted control-plane features.

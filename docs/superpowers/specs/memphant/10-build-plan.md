# MemPhant - Build Plan

## 0. Principle

Build the levers and traces before the expensive systems.

Implementation order for SOTA levers is canonical in `27-sota-ladder-and-validation.md`.

The full operational implementation plan lives in `29-implementation-plan.md`. That doc owns status tracking, first slices, PR/alpha/dogfood/public gates, and the activation gates for work that is intentionally not built before its prerequisites are green.

## 1. Canonical Orders

Do not maintain a second linear build order here.

- Product implementation order, first slices, gates, exit packets, and owner handoffs: `29-implementation-plan.md`.
- SOTA lever activation order: `27-sota-ladder-and-validation.md`.
- V1 build-vs-freeze cut line (which levers the first public build actually builds vs interface-freezes at their `27` §2 rung): `29-implementation-plan.md` §2a.
- Suite-level dependency sketch: `00-relations-graph.md` §3.

## 2. First Alpha Gate

Alpha is ready when:

- `retain` stores raw episodes.
- `recall` returns cited context.
- tenant isolation tests pass.
- forget invalidates recalled memory.
- golden evals run locally.
- MCP can recall memory from a local server.
- Syndai can export one memory surface.
- DB exposure gate is green.

## 3. First Public Benchmark Gate

Do not publish broad claims until:

- sampled LME-V2 or BEAM run is reproducible
- cost and latency are reported
- config is committed
- traces are inspectable
- poisoning suite runs
- at least one baseline is rerun under comparable settings

## 4. Explicit Architecture Decisions

| Feature | Decision |
|---|---|
| Graph DB adapter | rejected; relational edges plus materialized 1-hop expansion are the core architecture. |
| Full FSRS/DSR fitter | schema/events ship now; learned fitter turns on only with enough MemPhant review traces. |
| L4 deliberate recall | interface frozen in v1 (`retrieval_mode=deep` flag + trace fields); behavior builds at its `27` §2 rung as an explicit Deep/benchmark mode, never the default hot path. `29-implementation-plan.md` §2a owns the cut line. |
| Skill compiler | rejected for first public build; procedural payload schema and validation status freeze in v1, with the procedural-memory lever built at its `27` §2 rung. |
| Cache cluster | rejected until p95 traces prove repeated recall dominates cost; single-node/object-cache only. |
| SQLite/PGLite local mode | rejected; Docker/plain Postgres is local mode. |
| Framework adapters | rejected for launch; API, SDKs, MCP, and cookbooks are the adapter strategy. |
| Helm/Kubernetes chart | rejected for launch; Docker image and Compose are enough. |
| Go/Rust/Java SDK matrix | rejected for launch except Rust crate docs; Python/TypeScript are public SDKs. |
| Public dashboard | ships only as trace explorer, memory inspector, API keys, usage, eval runs. |

## 5. Syndai Dogfood Gate

Syndai may cut over a surface only when:

- L0-only memory gates are preserved.
- Child-agent scopes cannot recall parent/user memory unless explicitly allowed.
- memory citations render correctly in the target Syndai UX.
- correction and forget semantics satisfy the target contract.
- backend memory regression commands stay green.
- `make check-runtime-db` or equivalent DB contract check stays green where applicable.
- web/mobile UI contracts are covered by their nearest gates.

## 5.1 Syndai Cutover Order

1. Export raw episodes/resources for one low-risk memory surface.
2. Compare MemPhant traces against contract fixtures derived from `MemoryContextLoader`.
3. Add golden cases for every mismatch.
4. Active-read only for that low-risk surface.
5. Move correction/forget for that surface.
6. Repeat for L0 user recall.
7. Repeat for project/resource memory.
8. Delete duplicated Syndai-specific paths after contract gates pass.

## 6. Kill Or Narrow Gates

If MemPhant cannot pass internal Syndai contract fixtures, fix the basics before external benchmarks.

If MemPhant cannot show a differentiated win on sampled external benchmarks, narrow to the part that works:

- poisoning defense middleware
- trace/eval harness
- Syndai-only memory hardening
- coding-agent memory niche

Do not keep expanding the platform to compensate for weak core recall.

## 7. Launch Blockers

- DB exposure gate red.
- Any cross-tenant leakage.
- Any L1+ overread of L0-only memory in Syndai contract tests.
- Delete/forget completeness failure.
- Public API and MCP schemas drift.
- Golden evals are shape-only rather than executable.
- SOTA writeup lacks cost/latency/config/CI/traces.
- Hosted service requires a private code path that self-host users cannot reproduce for core memory behavior.

## 8. Contrarian Checks

Before adding any major subsystem, answer:

1. Which benchmark failure does this address?
2. Which trace field proves the failure?
3. Can a simpler SQL/index/policy change fix it?
4. Can the feature be behind a flag?
5. Does it require a schema/interface change now, or is it already covered by the frozen interfaces?

If the answer is not concrete, do not build it.

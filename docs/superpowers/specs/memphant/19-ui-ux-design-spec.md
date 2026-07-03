# MemPhant - UI/UX Design Spec

## 0. Surfaces

MemPhant public launch needs four UI surfaces:

1. Marketing/docs website.
2. Developer dashboard.
3. Trace explorer.
4. Memory inspector.

Do not build a decorative SaaS dashboard before the trace explorer works.

Surface boundaries:

| Surface | Owner | Notes |
|---|---|---|
| Public MemPhant web | MemPhant product | docs, quickstart, benchmark proof |
| Hosted dashboard | MemPhant product | API keys, traces, evals, usage |
| Syndai web | Syndai product | consumes backend memory behavior, no direct DB |
| Syndai mobile | Syndai product | reuse Memory Hub/citations/corrections in v1 |

## 1. Website IA

```text
Home
Docs
  Quickstart
  MCP
  Python
  TypeScript
  Rust server
  Security
  Evals
Compare
Benchmarks
Blog
Changelog
```

## 2. Home Page

First viewport:

- product name
- one-line promise
- install command
- concrete proof block: trace/eval/security

Avoid vague hero claims.

## 3. Dashboard

Dashboard v1:

- API keys
- projects/tenants
- recent recalls
- recent writes
- error rate
- trace links
- quota/usage

No chart zoo.

## 4. Trace Explorer

Trace explorer is the signature UX.

It shows:

- query
- feature flags
- candidates by channel
- discarded candidates and reasons
- fusion/rerank scores
- final context
- citations
- latency/cost

Users should answer: "why did memory return this?"

Trace explorer layout:

```text
Header: query hash/text if allowed, scope, actor, retrieval ID, time
Left: stage timeline and latency
Center: candidate table grouped by channel
Right: final context pack and citations
Bottom: dropped candidates, policy filters, raw JSON
```

Candidate table columns:

```text
kind
title/summary
channel
channel rank
fusion rank
rerank rank
trust
state
validity
discard reason
citation
```

No raw secret/memory text appears unless tenant policy allows it.

### 4.1 Trace Explorer Wireframe

```text
┌ recall ret_8a2 · scope project:checkout · L0 · 142ms · $0.0003 ──────────────┐
│ "which callback token version should checkout use?"        [degraded: no]    │
├──────────────┬───────────────────────────────────────────┬──────────────────┤
│ STAGE TIMING │ CANDIDATES (by channel)                   │ CONTEXT PACK      │
│ gates   2ms  │ kind   summary       ch    fus rr trust   │ • mem_v2 (cited)  │
│ exact   4ms  │ sem   "token v2..."  vec   1   1  sys  ✓   │ • ⚠ contradicts   │
│ lexical 9ms  │ sem   "token v1..."  vec   2   – web  ⤫    │   mem_v1 (stale)  │
│ vector 31ms  │ epi   "ep_new..."    lex   3   2  sys  ✓   │                   │
│  ↳ filter_sel 0.04 · iter_scan 2  (small-tenant flag)     │ CITATIONS         │
│ fusion  5ms  ├───────────────────────────────────────────┤ ep_new → mem_v2   │
│ rerank  8ms  │ DROPPED: mem_v1 [stale] · mem_x [budget]   │ [open drawer]     │
│ assemble 6ms │ POLICY: trust_filter, scope=project        │                   │
└──────────────┴───────────────────────────────────────────┴──────────────────┘
  consolidation_lag: 0ms   ·   [raw JSON]   ·   copy ret_8a2
```

The `filter_selectivity` / `iterative_scan_depth` / `consolidation_lag` fields (`05` §3.1) are first-class in the explorer so the small-tenant-HNSW and degraded-recall conditions are *visible*, never silent.

## 5. Memory Inspector

Views:

- episodes
- semantic facts
- beliefs
- procedures
- resources
- trust events
- forget/deletion state

Actions:

- inspect
- correct
- reinforce
- quarantine
- forget
- export

Reinforce is the shipped "Still true" verb: it maps to `mark` outcome feedback plus a confirmation observation that advances freshness (`04` §8.1). It never edits the unit body.

Inspector invariants:

- every active memory shows evidence path
- stale/superseded facts remain inspectable but clearly labeled
- beliefs/provisional observations are visually distinct from semantic facts
- delete/forget state is visible
- correction writes a new event; it does not silently edit history
- resource blobs are opened through signed/authorized URLs only

## 5.1 Citation Drawer

The citation drawer shows:

- memory unit
- source episode/resource
- span/hash
- actor/source
- trust decision
- validity window
- related contradictions/supersessions
- retrieval trace where it was used

This is the user-facing form of the evidence ledger.

### 5.2 Provenance & Anti-Creepiness (what makes memory feel known, not invasive)

The UX evidence is asymmetric and neuro-imaged: too little memory frustrates power users, but *wrong or over-eager* memory triggers a documented uncanny-valley/creepiness response (fMRI-localized to the vmPFC + amygdala), and the worst case is a system that **fabricates** recall or **bleeds** context across projects (real, reported failures in shipping assistants — persona drift, cross-project contamination, confident false memories). The frontier's answer (matched by Claude's shipped design) is transparency + scoping + restraint. MemPhant's three highest-leverage UX safeguards:

- **User-visible provenance on every recalled fact** — "I know this because you told me on 2026-03-04" / "extracted from CI log ep_5a2." Every surfaced memory is one click from its episode + trust label (the citation drawer, §5.1). Opaque memory is what feels obsessive; sourced memory feels earned.
- **One-click per-fact correction and deletion** — the user edits or forgets an *individual* unit (not "wipe everything"), and correction writes a supersession event, never a silent overwrite (`04` §10). The real production failures were *per-entry* mis-associations, so per-fact granularity is the matching remedy.
- **Never surface fabricated or low-confidence recall as fact** — the relevance gate + abstention (`05` §1.5) mean the UI shows "I don't have that" over a confident guess, and low-trust/belief memory is rendered visually distinct from established fact (§5 inspector invariant). A wrong *remembered* fact is worse than a wrong inference — the system must look like it's recalling, not improvising.
- **Scope isolation is visible** — memory is project/scope-scoped and the UI shows which scope a fact lives in, so a user can see there is no bleed (the contamination complaint's direct fix, `04` §11).

### 5.3 Inspector Search and Filters

Inspector search is NL + filters over the store, with recall behavior identical to the agent's: the search path IS `recall` with `breadth: search` (`02` §7 — one verb, never a parallel query engine). What the inspector finds is what the agent can retrieve, and a search miss is debuggable through the same trace. Filters: scope, kind, entity — the entity-chip pattern Syndai already ships as an endpoint shape.

### 5.4 Changed-Since Diff View

An inspector view over the event ledger/outbox (`20` §3 taxonomy) plus `delta_since` recall (`08` §3.0) answers "what changed since T": new units, supersessions (with the replacing generation), and freshness downgrades. Removed content appears count-only — the view never names forgotten memory; forget must not be reconstructable from diffs.

### 5.5 Pinned-Block View

The one-pinned-block-per-scope (`04` §12) gets a dedicated view + edit affordance. It shows the hard token sub-budget usage, the audited edit history, and an explicit truncation label whenever the block is over budget. Editing writes an audited event, same as correction (§5 invariants).

### 5.6 Scope Overview

The inspector's scope overview reads `GET /v1/scopes/{id}/stats` (`08` §2): counts by kind×state, `consolidation_lag`, storage footprint, quarantined count, and open deletion generations. No aggregate is invented client-side.

## 6. Accessibility

Minimum:

- keyboard navigable tables
- visible focus states
- no color-only trust labels
- copyable trace IDs
- accessible diff/score text

## 7. Design Tone

Quiet infrastructure product. Dense but readable. No playful elephant mascot in core UI; brand can nod to it in docs/illustrations, not in debugging workflows.

## 8. Benchmark Pages

Benchmark pages must never show a bare "SOTA" badge.

Show:

- benchmark version
- MemPhant version/config
- model/embedding/reranker
- accuracy with CI
- latency and cost
- trace archive pointer
- caveats
- competitor source status
- security eval result

## 9. Syndai UI During Dogfood

During dogfood:

- keep existing Memory Hub navigation
- preserve mission memory reference footers
- use MemPhant trace IDs behind the scenes
- expose new inspect/debug affordances only where current UX already has memory details
- avoid adding MemPhant branding inside Syndai user workflows

MemPhant should make Syndai memory more reliable, not split the UX into two mental models.

## 10. Component Inventory

The reusable primitives behind the surfaces above (props → states), so the explorer/inspector compose from one set:

| Component | Key props | States |
|---|---|---|
| `StageTimeline` | stages[], latency_ms | normal / slow-stage-highlighted |
| `CandidateTable` | candidates[], grouping=channel | default / dropped-collapsed / degraded-banner |
| `TrustBadge` | level | per-level color **+ icon** (never color-only — §6) |
| `CitationChip` | memory_unit_id, episode_id, valid_to | active / stale / superseded |
| `ContradictionBanner` | between[] | warn (dual-active) |
| `MemoryUnitDrawer` | unit, evidence_path | active / superseded / quarantined / deleted |
| `RetentionTierTag` | tier | hot / warm / cold |
| `BenchmarkRow` | benchmark, ci, source_status | reproduced / vendor_reported (visually distinct) |

Rendered HTML/CSS proofs of these (an EvalRank-style `proofs/` subdir) are **deferred** — add them once the surfaces are built and a `tokens.css` (`23`) exists; building rendered proofs ahead of any frontend is speculative for a pre-build infra spec.

# MemPhant - Syndai Integration Spec

## 0. Boundary

MemPhant is standalone. Syndai is a client.

That means:

- no MemPhant tables in the `syndai` schema
- no cross-FKs from MemPhant to Syndai
- no Syndai-only memory API
- no private SDK methods for Syndai
- no hidden feature flags only Syndai can use

## 1. Mapping

| Syndai concept | MemPhant concept |
|---|---|
| user | subject |
| L0 agent | root agent_node |
| L1+ child agent | child agent_node |
| mission | scope_ref / episode source |
| project / area | scope |
| run message | episode event |
| memory file | resource memory |
| user fact | semantic memory unit |
| episodic memory | episode + episodic memory unit |
| behavioral embedding | belief/procedural candidates — retained |
| trajectory event (`trajectory_events`) | episodic memory unit, `source_kind=tool` |
| failure pattern (`failure_patterns`) | procedural memory unit, `signal_kind=failure` |
| agent persona (`agent_personas`) | pinned scope block (`04` §12), adapter-compiled |

The mapping lives in a Syndai adapter, not in MemPhant core.

The last four rows are chokepoint-served stores the original mapping missed; each carries a disposition, and the checked-contract copy lives in `28` §2.1 (the drift rule requires them there, not only here):

- **`trajectory_events`** — GENERALIZE-FIRST. Per-tool-result event rows; they become episodic units with `source_kind=tool`.
- **`failure_patterns`** — GENERALIZE-FIRST. Occurrence-counted failure rows; they become procedural units with `signal_kind=failure`.
- **`agent_personas`** — GENERALIZE-FIRST storage, KEEP-IN-SYNDAI rendering. Syndai's persona is the production Letta-block analog: persisted, 300-token-bounded, editable, always compiled in, with history + rollback via `agent_persona_trait_history`. At cutover the adapter MAY back it with MemPhant's one-pinned-block-per-scope (`04` §12); persona rendering and trait editing stay app-side.
- **behavioral embeddings** — retained, not "if retained": the layer is the second-largest injected layer (700-token budget) with a live reinforcement/decay loop, so it is a real belief/procedural candidate source, not residue.

## 2. Consumption Modes

Preferred production mode:

```text
Syndai backend -> MemPhant Python SDK -> MemPhant HTTP service -> MemPhant DB
```

Allowed local mode — `status: not-built (2026-07-09 truth pass — the Python package is a pure HTTP SDK; no MCP wiring exists in Syndai)`:

```text
Syndai backend -> MemPhant Python native binding -> local/dev DB
```

MCP dogfood mode — `status: not-built (2026-07-09 truth pass — the Python package is a pure HTTP SDK; no MCP wiring exists in Syndai)`:

```text
Syndai agent runtime -> MemPhant MCP server
```

Forbidden production mode:

```text
Syndai web/mobile -> MemPhant DB/Supabase client
```

Web and mobile continue calling the Syndai backend. The backend owns auth/session semantics and translates to MemPhant.

## 3. Dogfood Cutover Approach

Syndai is pre-production for this work. Use comparison to prove correctness, not to preserve old paths indefinitely.

Order:

1. Export selected Syndai memory writes into MemPhant.
2. Run recall comparison and inspect traces.
3. Use MemPhant recall for one low-risk memory surface.
4. Expand to L0 user recall.
5. Move correction/forget flows.
6. Delete duplicated Syndai-specific memory paths after contract gates pass.

### 3.1 Cutover Gates

| Gate | Syndai path | MemPhant path | Exit criteria |
|---|---|---|---|
| Baseline capture | source of comparison | none | baseline contract captured in golden cases |
| Code contract | checked source contract | adapter contract | `28-syndai-code-contract.md` fixture family is executable for the target surface |
| Trace compare | read-only source | seeded from export | every mismatch becomes a golden case or accepted behavior change |
| First surface | replaced for one low-risk surface | canonical for that surface | contract, security, and UX gates pass |
| Full cutover | deleted after gates | canonical | no direct Syndai memory path remains for moved behavior |

Old code is deleted after gates pass.

### 3.2 Contract Metrics

| Metric | Gate |
|---|---|
| answer-bearing recall@k | meets or beats the golden baseline |
| citation validity | meets the launch threshold |
| L1+ blocked-memory cases | zero failures |
| forget/correction behavior | exact or stricter |
| pending-review-queue behavior | agent-proposed facts land as MemPhant `candidate` units with `supersedes` pointers; supersession strikethrough diff + confirm/dismiss exact or stricter |
| token budget | parity-or-better answer-bearing recall under Syndai's existing ≤2,500-token hard cap (per-layer budgets, `context_loader_types.py:47-79`) — never a %-token-reduction claim |
| p95 latency | within product budget |
| trace explainability | every miss has failure category |
| mobile/web behavior | covered by the target UX contract |

## 4. Guardrails

Syndai rules still apply while inside Syndai:

- never touch `public`
- never drop tables without explicit current-task instruction
- keep `syndai` app data in `syndai`
- preserve L0-only memory gates
- keep child-agent depth separate from memory access level

MemPhant must not weaken these. It should make them easier to express.

### 4.1 Codebase Facts To Preserve

The checked source contract lives in `28-syndai-code-contract.md`. That doc owns exact backend entrypoints, test files, and fixture families. This section stays only as the short integration summary.

The local codebase already proves several patterns:

- `MemoryContextLoader` is the central memory-context chokepoint.
- L1+ agents are blocked from user facts, persona, episodic, and behavioral memory.
- agent hierarchy level and runtime delegation depth are separate invariants.
- Syndai memory graph behavior is relational/Postgres, not a mandatory graph DB.
- citation whitelists and DB fallback make mid-run recall auditable.
- memory tools are gated both by availability policy and runtime policy.
- mission traces already carry IDs/counts/watermarks that can inspire MemPhant trace facts.

The integration should preserve these invariants, not class names.

## 5. Adapter Responsibilities

The Syndai adapter owns:

- auth conversion
- tenant ID mapping
- scope construction
- agent tree mapping
- mission/run provenance metadata
- Syndai memory citation rendering
- cutover trace comparison instrumentation

MemPhant owns:

- storage
- retrieval
- trust policy
- privacy/deletion policy
- trace format
- API contracts

Adapter scale reality (live DB, 2026-07-02): Syndai memory stores hold 10⁰–10² rows while event streams run 10⁴–10⁵ (62,450 `coding_execution_attempt_events`). The adapter risk is event-ingest throughput, not store size — and cutover baselines must account for near-empty calibration/KG tables (a trace comparison against an empty `memory_fact_edges` proves nothing about edge expansion).

## 6. Done Definition

Integration is real only when Syndai can be pointed at a separately deployed MemPhant service with no codepath change except config.

## 7. Extraction Matrix

Each asset carries a disposition — **EXTRACT** (lift the shape into neutral MemPhant), **GENERALIZE-FIRST** (de-Syndai the vocabulary before extracting), **REIMPLEMENT** (clean-room in Rust), **KEEP-IN-SYNDAI** (stays app-side) — plus the real backend module it mirrors. MemPhant copies *shapes*, never Syndai names.

| Asset | Disposition | Real Syndai module → MemPhant target | Stays in Syndai |
|---|---|---|---|
| episodic/raw event capture | EXTRACT | `episodic_service.py` → `episode` store | mission workflow UX |
| semantic facts (bitemporal validity) | GENERALIZE-FIRST | `UserFact` (supersession self-FKs) → subject-scoped `memory_unit` | Syndai persona rendering |
| relational memory graph | GENERALIZE-FIRST | `MemoryEntity`/`MemoryFactEdge` (≤3-hop) → `memory_edge` expansion | — |
| memory resources/files | EXTRACT | `file_service.py` → `resource` pointer/blob/chunk | mobile Drift table names |
| citation whitelist contract | EXTRACT | `provenance_validator.py` + `memory_candidate_key_set` → citation candidate set | prompt-builder text formatting |
| recall/correct/forget operations | GENERALIZE-FIRST | `MemoryContextLoader` / `ScopedForgetService` → L0/L1+ → neutral `agent_node` policy | governed-action execution |
| trust/provenance events | GENERALIZE-FIRST | source trust → `trust_event` taxonomy | customer roles/approval rosters |
| retrieval trace facts | REIMPLEMENT | mission trace IDs/watermarks → `retrieval_trace` (Rust) | Temporal DAG projection UI |
| decay kernel | REIMPLEMENT | episodic ranking-time decay (`episodic_service.py:637-662`: 0.6·cos + 0.3·time_decay + 0.1·importance, 90-day linear to a 0.1 floor) + behavioral stored-confidence decay (`behavioral_analysis.py:425`: confidence·factor^elapsed_days to a floor for non-reaffirmed patterns) → `fsrs-rs` DSR; MemPhant recency/DSR must be trace-compared against BOTH baselines at cutover | — |

### 7.1 Hook→Retain Cookbook (non-normative, adapter-side)

Memory that only sees chat misses the most valuable coding events — the test that failed twice, the compaction that dropped a constraint, the PR that closed the loop. Coding harnesses expose lifecycle hooks; the adapter maps them to `retain` calls with the right `source_kind`. Capture rides the public `retain` surface, never substrate hooks (`02` §8). This is adapter cookbook, not MemPhant API.

| Harness hook | Syndai analog today | Adapter retain mapping |
|---|---|---|
| `session.start` | mission/run start | episode open marker (scope + provenance metadata) |
| `session.stop` | final extraction on `is_done` | end-of-session `reflect`-eligible episode |
| `session.compact` | `context.compacted` — ALREADY memory-writing in Syndai: compaction writes an episodic unit at importance 0.55 | episodic unit capturing pre-compaction task state |
| `user.prompt.submit` | turn boundary | episode event append |
| `tool.call.completed` | `tool.executed` + trajectory row | episodic unit, `source_kind=tool` |
| `test.failed` | `coding.validation.completed` (failure) / `validation.*` | procedural `signal_kind=failure` candidate |
| `test.passed` | `coding.validation.completed` (success) / `validation.*` | procedural `signal_kind=success` evidence |
| `pr.opened` | `coding.pr.opened` | episodic unit + resource pointer to the PR |
| `issue.closed` | mission/task terminal events | episode close + outcome metadata |
| `file.changed` | none today | adapter adds when the harness emits it |
| `commit.created` | none today (Syndai records only phase transitions) | adapter adds when the harness emits it |
| `deploy.*` | none today | adapter adds when the harness emits it |

## 8. Web and Mobile

V1 mobile should reuse Syndai's existing Memory Hub/citation/correction surfaces. It should not add a second MemPhant-branded mobile UI.

MemPhant's unit → citation → episode evidence path surfaces through those existing affordances: Memory Hub tiles for browsing, and the in-chat correction sheet for the "why did it remember this" provenance moment — not a new surface.

If public MemPhant dashboard pages are built in Syndai `web/`, implementation must update `web/NAVIGATION.md` and Playwright coverage. These docs do not create routes by themselves.

Mobile/web contract:

- display memory returned by Syndai backend
- show citations and correction affordances
- never own memory access policy
- never talk directly to MemPhant DB
- sync through existing backend/SSE paths; any app architecture change is a separate Syndai app decision, not a MemPhant dependency

## 9. Cutover Blockers

- MemPhant recall cannot reproduce Syndai L0/L1+ access behavior.
- Forget/correction contract fails.
- Mobile citation/correction UX regresses.
- MemPhant requires direct DB coupling from Syndai.
- MemPhant introduces hidden Syndai-only API fields.
- DB exposure gate is not green.
- Golden eval traces cannot explain misses.

## 10. Frontier-Monitor Enrollment (memory components as a frontier target)

Syndai's frontier monitor already runs **enroll → measure → operator-gated promote** for coding/planning/model slots (`backend/src/features/frontier_monitor/`: `targets.py`, `watched_models.py`, `research_mission.py`, `tier_proposals.py`, `patch_spec_*`, `auto_merge.py`) via an evidence ledger. MemPhant's swappable components — the **embedding model** and **reranker** — are the same shape of slot, so the monitor enrolls a **memory frontier target** to keep them current. It maps 1:1 onto the existing pattern:

- **Watch + discover** — `watched_models.py` tracks memory-component leaderboards (LMEB embeddings, reranker boards); `research_mission.py` surfaces candidates. Leaderboards nominate, never promote.
- **Confirm via the public eval (the neutrality boundary)** — a `memory_component_eval` (sibling of `plan_eval.py`/`agent_tier_eval.py`) confirms a candidate by invoking **MemPhant's public `memphant-eval profile`** (`12` §2.0a) — true dogfood through the public surface, no private internals. This is the frontier monitor's own confirm-before-promote pattern (`auto_merge.py` gates every promotion on the confirming evidence ledger; the old coding-harness golden-eval auto-promote pipeline was deleted 2026-07-01 — live routing promotion there is human-gated via `backend/scripts/router_promote_candidate.py`). Shape (spec-level — the boundary is that it shells the *public* runner and reads only its artifact):

  ```python
  # frontier_monitor/memory_component_eval.py  (sibling of plan_eval.py / agent_tier_eval.py)
  def confirm_memory_component(candidate: ComponentRef, incumbent: ComponentRef) -> ComponentVerdict:
      # runs the PUBLIC binary with the candidate pinned — no memphant_core internals imported
      profile = run_public(
          f"memphant-eval profile --config memphant.lock "
          f"--swap {candidate.kind}={candidate.id} --compare-to {incumbent.id} --archive-traces")
      rows = [evidence_row(axis, profile.delta(axis), profile.ci(axis))
              for axis in CONFIRMING_AXES[candidate.kind]]   # embedding → {embedding_selection, recall axes}
      write_ledger("memory_component_evidence", rows)         # the distinct ledger (22 §4.3)
      return ComponentVerdict(beats=all(r.ci_excludes_zero and r.delta > BAR[candidate.kind] for r in rows))
  ```
- **Auto-merge (same as the other slots)** — evidence lands in a `memory_component_evidence` ledger; `tier_proposals` + `patch_spec` open the PR to bump MemPhant's default `embedding_profile`/reranker, and **`auto_merge` runs it exactly like the coding/planning/model slots** — gated on the full confirmation (confirming + tail-risk axes + no `block`-SLI regression, `22` §4.3), behind the same dry-run flag. A reranker swap auto-cuts-over immediately; an embedding swap auto-merges + auto-enqueues the re-embed (`14` §10) and auto-cuts-over on re-embed completion + explicit recall thresholds. Human pinning is a policy flag, not a special case for memory.

The boundary the monitor must not cross: it **drives cadence**, it does not get a private result — confirmation is MemPhant's own neutral loop (`22` §4.3), and the better default reaches every external user via the public PR, not just Syndai (invariant: Syndai consumes the public surface only).

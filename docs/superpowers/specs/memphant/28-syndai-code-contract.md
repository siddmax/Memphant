# MemPhant - Syndai Code Contract

## 0. Rule

Syndai code is evidence for adapter contracts, not public MemPhant vocabulary.

MemPhant must preserve the behavior contracts below before Syndai cuts over a memory surface. It must not preserve Syndai class names, table names, or internal tool names as core product concepts.

## 1. Code Snapshot Checked

Checked against the local Syndai worktree on 2026-06-25; runner provenance paths refreshed on 2026-07-01:

- `backend/src/features/memory/context_loader.py`
- `backend/src/features/memory/context_loader_agent_scoped.py`
- `backend/src/features/memory/context_loader_types.py`
- `backend/src/features/memory/memory_contracts.py`
- `backend/src/features/memory/scoped_forget_service.py`
- `backend/src/features/memory/provenance_validator.py`
- `backend/src/features/missions/runner_memory_refs.py`
- `backend/src/features/tools/canonical/recall_memory.py`
- `backend/src/features/tools/canonical/correct_memory.py`
- `backend/tests/features/memory/test_context_loader.py`
- `backend/tests/unit/features/memory/test_memory_contracts.py`
- `backend/tests/unit/features/memory/test_memory_experience_regressions.py`
- `backend/tests/unit/features/memory/test_openapi_contracts.py`
- `backend/tests/unit/features/missions/test_runner_runtime_provenance.py`

Refresh this doc before any Syndai cutover gate if one of those entrypoints changes.

## 2. Contracts To Preserve

| Syndai evidence | Contract MemPhant must satisfy |
|---|---|
| `MemoryContextLoader.load_hot_context` sends `agent_level == 0` through the full context path and `agent_level > 0` through agent-scoped context only. The class also exposes a stricter L0-only entrypoint, `load_l0_user_context` (`context_loader.py:153`), which raises `PermissionError` for `agent_level > 0`. | Recall policy is enforced by `agent_node` level inside MemPhant, not by prompt/tool visibility alone; an explicit L0-only read path exists and fails closed for child levels. |
| L1+ context contains trajectory, failure patterns, and agent-scoped file memory; it excludes persona, user facts, episodic memory, and behavioral patterns. | Child agents cannot retrieve parent/user memory unless an explicit scope policy grants it. |
| L0 context can include persona, user facts, behavioral patterns, episodic memory, trajectory, failure patterns, file memory, dropped-context metadata, and citeable candidates. L0 fact retrieval also runs **bounded KG multi-hop expansion** over `memory_fact_edges` (`load_hot_context(hops=…)`, default 1; `context_loader.py:310`). | MemPhant returns a compact evidence pack plus a trace, with every answer-bearing item tied to a candidate and citation path; the neutral analog is bounded `memory_edge` expansion (`02` §4.1). |
| `RecallMemoryTool` is L0-only and wraps the loader. Its **registered tool name is `"recall"`** (`recall_memory.py:55`), not `recall_memory` — wire the adapter from the registered name, not the class/file name. | Mid-run recall uses the same policy engine as turn-start recall. No second recall path exists for privileged reads. |
| `CorrectMemoryTool` returns candidates first; mutation needs explicit `fact_id` and `new_value`. | Correction is selector-based and auditable. Free-form text can propose candidates but cannot mutate memory by itself. |
| Syndai has no registered canonical agent `forget` tool; erasure is backend/UI-owned through `ScopedForgetService`. | MemPhant still exposes public `forget`, but the Syndai adapter maps it to server-owned scoped erasure first, not to an agent-callable mutation shortcut. |
| `memory_candidate_key_set` and citation helpers render tags only for loader-produced candidates. | Model answers can cite only candidate-whitelisted memory IDs or a later recall trace. Unknown IDs are dropped or rejected. |
| `runner_memory_refs.py` persists mid-run citations only when they are candidate/trace-backed, or DB-reverified as user-owned active citeable memory; `provenance_validator.py` uses store-canonical confidence, source, and learned-at metadata. | Persisted citations and provenance facts must come from MemPhant trace/store truth, never from LLM self-report or unverified memory IDs. |
| `ScopedForgetService` handles project, mission, and category actions by hiding recall-affecting records immediately and retracting graph edges. | `forget` hides affected memory from recall before background purge completes, increments a privacy/deletion generation, and audits derived cleanup. |
| Project forget derives fact membership from mission-to-project authority, not episodic rows. | Scope authority is explicit and typed; derived memory rows are never the authority for their own scope. |
| Memory file access is scoped by agent or project and ranked with the active query. | Resource memory obeys ACL/scope policy and participates in the same trace/citation contract as other memory kinds. |
| Memory tests cover L0/L1+ separation, candidate whitelist behavior, scoped forget, category blocking, and route operation IDs. | Syndai cutover requires executable golden cases for these contracts, not shape-only fixture checks. |
| `MemoryContextLoader` assembles hot context under a ≤2,500-token hard cap with per-layer budgets (`context_loader_types.py:47-79`). | Cutover acceptance is parity-or-better answer-bearing recall under that same cap — never a %-token-reduction claim. |

## 2.1 Cutover Dispositions (chokepoint-served stores)

The context chokepoint also serves stores the original `07-syndai-integration-spec.md` §1 mapping missed. Their dispositions are checked contract here; `07` mirrors them:

| Syndai store | Disposition at cutover |
|---|---|
| `trajectory_events` | GENERALIZE-FIRST — per-tool-result event rows become episodic units with `source_kind=tool` |
| `failure_patterns` | GENERALIZE-FIRST — occurrence-counted failure rows become procedural units with `signal_kind=failure` |
| `agent_personas` | GENERALIZE-FIRST storage, KEEP-IN-SYNDAI rendering — the persisted, 300-token-bounded, editable, always-injected persona block (history + rollback via `agent_persona_trait_history`) may be backed by MemPhant's one-pinned-block-per-scope (`04` §12), adapter-compiled; persona rendering stays app-side |
| pending review queue | Syndai's agent-proposed-fact queue (supersession strikethrough diff + confirm/dismiss) maps to MemPhant `candidate` state + `supersedes` pointers; behavior exact or stricter |
| behavioral embeddings | retained — second-largest injected layer (700-token budget) with a live reinforcement/decay loop; source of belief/procedural candidates |

## 3. Adapter Requirements

The Syndai adapter must:

- map user, agent tree, mission, project, and memory file references into neutral MemPhant subject, actor, agent_node, scope, episode, and resource fields
- send all recall through MemPhant `recall`
- send all correction flows through MemPhant selectors plus explicit replacement values
- send project, mission, category, and ID erasure through MemPhant `forget`
- render citations from MemPhant citation payloads into Syndai's existing response metadata
- record trace IDs on the Syndai side so a failed run can be inspected without querying MemPhant tables directly

The adapter must not:

- query MemPhant tables directly
- expose service/admin keys to web, mobile, or agent MCP clients
- add Syndai-only MemPhant API fields
- let tool availability replace MemPhant policy checks
- let retrieved memory authorize governed actions or fill high-risk tool parameters without policy approval

## 4. Cutover Fixtures

Every Syndai surface moved to MemPhant needs a fixture with:

```yaml
id: syndai_l1_child_user_memory_block_001
seed:
  subject: user_1
  agent_tree:
    - {id: root, level: 0}
    - {id: child, level: 1, parent: root}
  memories:
    - {kind: semantic, scope: subject:user_1, text: "User prefers Taipei time."}
query:
  agent_node_id: child
  text: "What timezone does the user prefer?"
expect:
  returned_memory_ids: []
  denied_reason: agent_node_scope
  trace_assertions:
    - policy_filter_contains: agent_node_scope
```

Required fixture families:

- L0 user fact recall allowed
- L1+ user fact recall denied
- L1+ agent-scoped file memory allowed
- L1+ file-memory recall is query-ranked, not recency-only (the active query must reach file-memory ranking; regression warning at `backend/src/features/memory/context_loader.py:200-205`)
- citation whitelist accepts retrieved IDs and rejects unknown IDs
- correction candidate-only pass
- correction explicit selector mutation
- project forget hides mission-derived facts
- mission forget hides mission episodes
- category block prevents future writes and hides active facts

Coding-continuity families:

- `syndai_arch_decision_honored_001` — a stored architecture-decision episode is packed on a later related-implementation query; `forbidden_text` excludes re-proposing the rejected alternative
- `syndai_compaction_rehydrate_001` — post-compaction "what were we doing / active constraints" recall packs the task-critical units inside a tight `context_budget_tokens`
- `syndai_cross_agent_transfer_001` — agent A's `validated` repo-scoped procedure is retrievable by sibling agent B in the same scope (the positive dual of the L1+ deny family; `validated`-only per `05` §1.3)
- `syndai_task_plus_semantic_composite_001` — one composite query packs task-state episodic plus the semantic constraint, and asserts `subquery_ids` present

## 5. Done Gate

Syndai can switch a surface to MemPhant only when:

- the fixture family for that surface is executable
- MemPhant traces explain each miss or denied recall
- focused Syndai memory tests stay green
- no Syndai route or UI directly depends on MemPhant storage details
- replaced Syndai-specific memory paths are deleted after the surface passes contract gates

# Agentic Deep Recall Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: use `superpowers:subagent-driven-development` and `superpowers:test-driven-development`; generate one task brief and one review package per task. Do not push. Preserve historical proof artifacts and the unrelated handoff edit.

**Goal:** Ship an explicit, bounded `deep` recall mode that searches an ephemeral read-only file workspace built from authorized canonical MemPhant sources, returns the ordinary cited recall contract, and leaves `fast`/`balanced` unchanged.

**Architecture decision:** Deep is a real query-time agent loop, not a wider deterministic ranking pass. Store/core own authorization, the canonical virtual workspace, returned-source validation, packing, citations, and trace facts. `memphant-runtime` owns the cancellable async model/tool loop. REST/MCP/CLI await the bounded call; the host product wraps it in its existing task stream for progress/cancellation. MemPhant adds no job table, durable workspace, writeback, shell access, or automatic escalation into Deep.

**Failure semantics:** An explicit Deep request without a configured provider returns a stable `deep_unavailable` 503/MCP error and never downgrades. A configured run that reaches a wall-time, tool-iteration, context, or spend cap returns its best validated cited partial result. `RecallResponse` gains an explicit optional `deep` summary (`status`, `stop_reason`, exact limits and actual usage); the durable trace carries the same facts. `degraded` remains reserved for the existing unreflected-episode fallback, and `suppression_labels` is not used as the primary partial-result contract. Provider source IDs not present in the authorized manifest are rejected. Fast and Balanced never build a workspace or invoke the provider.

**Security boundary:** Only the resolved tenant/subject/generation/scope/agent sources enter the workspace; forgotten, deleted, quarantined, stale, or otherwise non-recallable source-linked content does not. Resource rows with a non-empty dormant `resource.acl` are excluded until the typed ACL contract is implemented end-to-end, so Deep cannot create a new ACL bypass. Workspace paths are UUID-derived, never resource-URI-derived. Remote model egress is opt-in through explicit provider configuration.

**Internal/public naming:** Public `RecallMode::Exhaustive` becomes `RecallMode::Deep` as a clean pre-v1 break with no alias. Candidate provenance uses `RecallChannel::Deep`. The established internal stage/feature and trace field prefix remains `l4_exhaustive`/`l4_*`. Historical logs and immutable evidence retain their original wording.

---

## Task 1: Break the public mode contract cleanly

**Files:**

- Modify: `crates/memphant-types/src/lib.rs`
- Modify direct Rust callers under `crates/memphant-core/` and `crates/memphant-eval/`
- Modify current (non-historical) benchmark adapters/configs and their Python contract tests
- Regenerate: `openapi/memphant.v1.json`, `mcp/memphant.tools.v1.json`, `examples/evals/trace-schema.v1.json`
- Modify active specs that describe the public mode; do not rewrite archived build logs

**Steps:**

1. Add a failing serde contract test proving `"deep"` is accepted and `"exhaustive"` is rejected.
2. Rename `RecallMode::Exhaustive` to `Deep` and `RecallChannel::Exhaustive` to `Deep`; update exhaustive-mode cap matches without changing their behavior yet.
3. Update active adapters/configuration and contract tests. Do not add a compatibility alias.
4. Regenerate the three generated artifacts from their owning binaries.
5. Run narrow Rust/Python contract tests, then `cargo fmt --check` and `cargo clippy --all-targets --all-features -- -D warnings`.
6. Commit and write an implementer report. Generate a review package and obtain an independent task review before Task 2.

## Task 2: Make dormant resource ACLs readable and fail-closed

**Files:**

- Modify: `crates/memphant-types/src/lib.rs`
- Modify: `crates/memphant-store-postgres/src/store.rs`
- Modify: `crates/memphant-runtime/src/lib.rs`
- Modify/add resource store contract tests

**Steps:**

1. Add failing InMemory and Postgres tests proving empty and non-empty `resource.acl` values round-trip identically; unknown ACL keys/shapes fail closed.
2. Add the minimal typed ACL representation to `NewResource`/`StoredResource`, default empty. Select and parse the column in every Postgres resource read and carry it through InMemory/runtime delegation. Public retain continues to create the default empty ACL; ACL authoring remains deferred.
3. Add a shared helper that classifies only a truly empty ACL as eligible for Deep. Do not claim the full spec ACL is enforced by ordinary recall in this task.
4. Run resource/store tests and the ignored Postgres contract through `scripts/with_scratch_db.sh`; run formatting/clippy. Commit, report, package, and independently review.

## Task 3: Add the authorized canonical Deep snapshot

**Files:**

- Modify: `crates/memphant-types/src/lib.rs`
- Modify: `crates/memphant-core/src/lib.rs`
- Modify: `crates/memphant-store-postgres/src/store.rs`
- Modify: `crates/memphant-runtime/src/lib.rs`
- Modify/add cross-backend store contract tests

**Steps:**

1. Add failing shared store tests for an authorized episode and default-ACL resource plus sibling-agent, prior-generation, forgotten episode/resource, quarantined, historical correction-rectangle, mixed live/stale linked units, no-eligible-unit, and non-empty-resource-ACL exclusions. Require identical stable manifest order for InMemory and Postgres.
2. Add one read-only `MemoryStore::fetch_deep_snapshot(context, recall_time)` seam; do not reuse raw episode reads or the ranked/capped candidate query. Its output is a stable `DeepSnapshotEntry` with source kind/UUID, UUID-derived path, body, a SHA-256 recomputed from those exact bytes, and the exact eligible `StoredMemoryUnit` records sorted by UUID (the manifest derives their `UnitId`s). Carry the records atomically with the source so Task 4 can apply query-time policy before provider egress without an N+1 read or authorization race. Unit eligibility requires the full owner tuple, `context.allows(unit.kind, ...)`, bitemporal recallability, live/validated state, non-quarantined trust, exactly one direct source link, and no memory-unit/source tombstone. Raw-source admission independently requires the corresponding Episodic or Resource grant; a semantic grant alone cannot reveal its raw source.
3. Implement InMemory and Postgres parity without N+1 reads. Exclude every non-empty resource ACL using Task 2's helper. Postgres must apply `forgotten_source` to the memory unit and source before selecting the body; InMemory must apply the equivalent tombstones. Sort entries by source-kind/UUID and bound unit IDs by UUID. A source with no eligible unit never enters the snapshot.
4. Build the deterministic virtual workspace centrally: `WORKFLOW.md`, `manifest.jsonl`, `episodes/<uuid>.md`, and `resources/<uuid>.md`. Hash the canonical manifest/workspace and never derive a path from untrusted metadata. Record the honest limitation that resources have no raw-body version history, so a historical Deep snapshot can bind units at `RecallTime` but cannot reconstruct prior resource bytes after an in-place body change.
5. Verify the method performs no writes and honors tenant/subject/generation/scope/agent boundaries.
6. Run the shared in-memory tests and the ignored Postgres contract through `scripts/with_scratch_db.sh`; run formatting/clippy. Commit, report, package, and independently review.

## Task 4: Integrate an injectable bounded Deep provider into recall

**Files:**

- Add: `crates/memphant-core/src/deep_recall.rs`
- Modify: `crates/memphant-core/src/lib.rs`
- Modify: `crates/memphant-core/src/service.rs`
- Modify: `crates/memphant-core/tests/recall_trace_golden.rs`
- Modify: `crates/memphant-eval/src/lib.rs`
- Modify: `crates/memphant-eval/tests/eval_contract.rs`

**Steps:**

1. Add failing fake-provider tests: Fast misses a buried raw-source fact and never calls the provider; Deep finds it, promotes its linked unit, and returns the ordinary whitelist/citation contract.
2. Add `DeepRecallProvider`, immutable construction-time `DeepRecallLimits`, provider request/result types, stop reasons, and exact usage facts. Follow the existing boxed-async provider pattern.
3. Add `Arc<dyn DeepRecallProvider>` to `MemoryService` with a builder. Branch only for `RecallMode::Deep`; a missing provider is a typed unavailable error, never a silent fallback.
4. Run one shared pure Stage-0 scope-admission predicate before any query embedding, snapshot fetch, workspace projection, or provider call in every mode; denied requests route through core with no query vector so the established denial trace remains authoritative without remote egress. Apply the remaining query-specific policy gates before workspace/provider egress, not merely after provider selection. Validate gathered source IDs against the manifest, add only their still-eligible bound units as `RecallChannel::Deep` candidates, and send them through existing packing, citation, and trace machinery. If a raw source cannot be projected without exposing query-suppressed content, omit the whole source rather than attempting an unsafe partial raw-body export.
5. Make `recall_stage_facts` and `recall_feature_flags` truthful. Add the optional machine-readable Deep summary to `RecallResponse`; populate matching `l4_sandbox_id`, `l4_gathered_evidence_ids`, limits/actuals/stop reason in the trace. Record configured provider/model separately from response-observed provider/model, validating every present identity string as non-empty without requiring a routed model label to equal the exact configured request label. Measure top-level Deep latency with a host monotonic end-to-end timer; keep provider-loop wall time in the Deep usage summary and preserve historical Fast/Balanced latency behavior. Keep `degraded` semantically unchanged.
6. Add fake cap tests for time, iterations, context, and spend, including zero-evidence abstention. Assert the public Deep summary through service/REST/MCP/CLI. Add malicious-provider tests for out-of-manifest source IDs and source entries with no eligible unit.
7. Replace the evaluator's old Deep-to-Balanced ablation with a provider-capable seam: the enabled arm injects a deterministic bounded fake provider and the disabled arm is a true no-provider control. Unignore the one-case fake-provider contract once it proves the arms execute different behavior; keep any real remote/runtime rung ignored until Task 5.
8. Run narrow core/eval suites, formatting/clippy, commit, report, package, and independently review.

**Task 4 contract decisions:** `DeepRecallProvider` is a dyn-safe, boxed-async gatherer that may nominate authorized source UUIDs only; it is never an answer authority. `MemoryService` stores an optional `Arc<dyn DeepRecallProvider>`. The provider identity (`provider`, `model`, prompt hash, config hash) and limits are immutable construction-time facts. Provider output is rejected—not silently filtered—when status/stop reason disagree, usage exceeds a declared cap, source IDs repeat or fall outside the authorized manifest, or a selected source has no policy-eligible bound unit. Query-policy projection is conservative at the raw-source boundary: if any bound unit on a source is belief/procedure/high-risk/query suppressed, omit the entire source before constructing the provider-visible workspace. Fast and Balanced do not execute snapshot or provider code and retain wire-identical responses.

The public optional `deep` summary contains `status`, `stop_reason`, declared limits, and actual usage; the trace repeats those facts plus configured provider/model, separately observed provider/model, prompt/config and workspace-manifest hashes. Top-level Deep latency is host-measured end-to-end; `deep.usage.wall_time_ms` is provider-loop time. Top-level cost is settled provider-metered cost only until the product has a paid query embedder with an auditable price contract. `degraded` remains reserved for unreflected raw-episode fallback. A completed zero-evidence run returns the ordinary cited abstention. A cap returns the best already-validated cited evidence, possibly empty. Missing configuration is `deep_unavailable` (HTTP 503 / explicit MCP error) and never falls back.

## Task 5: Implement the real cancellable runtime file agent

**Files:**

- Add: `crates/memphant-runtime/src/deep_recall_openrouter.rs`
- Modify: `crates/memphant-runtime/src/lib.rs`
- Modify: `crates/memphant-types/src/lib.rs`
- Modify runtime Cargo dependencies only if the existing async HTTP stack cannot be reused
- Regenerate: `openapi/memphant.v1.json`, `mcp/memphant.tools.v1.json`, `examples/evals/trace-schema.v1.json`
- Add runtime unit/integration tests with a fake transport
- Modify: `.env.example` and public operator/data-egress documentation

**Steps:**

1. Extend the Task 4 result contract before runtime code: `DeepRecallUsage` gains explicit unsettled context-token and spend upper bounds; provider results and the public/trace summary preserve an ordered list of generation IDs. Add a truthful `Partial` status with `ProviderError` and `InvalidOutput` stop reasons so a paid mid-stream or terminal malformed response returns checkpointed evidence (or an empty abstention) plus its settled/outstanding metering. Reserve `deep_unavailable` for a provably pre-generation failure. Core uses checked arithmetic and validates settled plus outstanding upper bounds against the limits.
2. Add failing scripted-transport tests for a multi-turn list/search/read/record/finish loop, every cap, malformed SSE and tool calls, retryable pre-token failures, non-replay after any paid/partial stream, cancellation/drop behavior, exact and unsettled usage/cost accounting, and multiple generation IDs. A pending transport must observe its future's destructor when the caller is cancelled and when the wall deadline fires; no detached task may survive the request.
3. Implement only these read-only in-memory workspace tools: bounded stable `list_files`, case-insensitive literal bounded `search_files`, UTF-8-safe 1-based inclusive bounded `read_file`, `record_evidence`, and `finish`. Paths must exactly match the provider-visible manifest. No model-supplied regex, shell, disk I/O, writes, arbitrary paths, web access, code execution, resource-URI paths, or memory writeback. Set `tool_choice="required"` and `parallel_tool_calls=false`; bounded structured tool errors may self-correct once within the same budgets, but normal text completion without `finish` is invalid.
4. Use async streaming `reqwest`, never the synchronous `ureq` stack or `spawn_blocking`. Wrap the entire loop in one monotonic Tokio deadline; bound connect and each request timeout by the remaining wall budget. Use streaming because a dropped request is the cancellation and billing-stop mechanism on supported providers. Retry at most twice only for explicit pre-stream HTTP 429/5xx responses with no generation ID, respecting `Retry-After` and the same budgets. Do not retry ambiguous connection/TLS/body/SSE failures or anything after a response byte/generation ID because request acceptance and billing cannot be disproved.
5. Parse SSE without auto-reconnect: support chunk boundaries, comments, multiline data, and ordered tool-argument deltas; reject missing `[DONE]`/final usage, multiple choices/tool calls, inconsistent route, and in-band errors. Preserve and replay the complete ordered `reasoning_details` array in subsequent assistant messages so both Anthropic and OpenAI reasoning models retain valid multi-turn context.
6. Require `MEMPHANT_DEEP=on`; requests cannot override immutable startup model, provider allowlist, prompt, pricing, or limits. Every OpenRouter call sends exactly one model plus `provider.zdr=true`, `provider.data_collection="deny"`, `provider.require_parameters=true`, the explicit provider allowlist, and `provider.max_price`; never send a cross-model fallback list, deprecated `usage.include`, or `stream_options.include_usage`. The first campaign allowlist is Azure only because it currently satisfies both ZDR and stream-cancellation support. If no compliant endpoint is available, return `deep_unavailable` rather than relaxing privacy or cancellation policy.
7. Require the model to return/checkpoint source IDs only; core remains the sole validator and response assembler. `finish` order is authoritative; ordered deduplicated `record_evidence` checkpoints are the partial fallback, and a cap before any checkpoint yields empty evidence. Preserve configured identity, original response-observed provider/model (normalizing only for validation), every OpenRouter generation ID, prompt/config hashes, and provider-native final SSE usage. Reject malformed or missing final usage.
8. Configure a 4,096-token maximum completion. Before dispatch, install one outstanding reservation using a conservative upper bound derived from the complete serialized request plus maximum completion, and round fixed-point token prices upward. Count one step per completed model response. All calls, retries, tools, finalization, and settlement share the same budgets, with simultaneous-stop precedence `wall_time > spend > context_tokens > tool_iterations`. Reserve the final five seconds for bounded `/generation` settlement using `X-Generation-Id`. If cancelled usage remains unresolved, retain the reservation as explicit unsettled upper bounds; never fabricate or clamp zero. Top-level trace cost remains settled actual cost.
9. Install the provider in `build_service`, never `build_base_service` or `build_worker_service`, and expose a narrow provider factory for Task 6/evaluator injection. Strict startup grammar is unset/`off` or exact `on`; when on, require the API key, exact model, prompt path, and `MEMPHANT_DEEP_PROVIDERS=azure`. Add opt-in/data-egress/privacy documentation that raw authorized bodies leave MemPhant for OpenRouter/Azure and ZDR does not itself guarantee geographic residency. Add packaged REST/MCP/CLI Deep smokes against a scripted local server. Fast and Balanced remain byte-identical and never contact it.
10. Run focused runtime tests, affected crates, generated-artifact locks, packaged scratch smokes, formatting/clippy, commit, report, package, and independently review.

**Runtime default decision:** Deep remains disabled until the operator explicitly configures remote-model egress. Once enabled, the campaign operating point is a 120-second wall budget, 24 completed model/tool steps, 96,000 cumulative provider input tokens, and USD 0.30 maximum spend per query. These are hard ceilings shared by initial calls, retries, tool turns, finalization, and usage settlement—not per-attempt allowances. Provider-loop wall time is monotonic; a step is one completed model response whether it requests a tool or calls `finish`; settled input tokens and spend are summed from provider-native usage across calls, while cancelled-but-unsettled calls retain explicit conservative upper bounds. Defaults may change only as a versioned operating point after the n=12 and independent confirmation evidence; requests cannot override them.

**2026-07-20 campaign decision:** The frozen n=12 shortlist is `anthropic/claude-sonnet-5` on Azure as the accuracy/Pareto candidate (observed OpenRouter list price USD 2/M input, 10/M output), `openai/gpt-5.6-sol` on Azure as the expensive frontier check (5/M, 30/M), and `openai/gpt-5.6-luna` on Azure as the cost challenger (1/M, 6/M). All receive identical caps and use exact model IDs—never floating aliases. This shortlist is evidence configuration, not runtime hardcode. Freeze the winner by exact model/provider/prompt/config hash, then use the independent n≈100–300 confirmation to select the cheapest statistically non-inferior model. Accuracy is the first constraint, cost the second, and latency the third; the user-facing default remains Fast, deterministic Fast-to-Balanced fallback remains allowed, and Deep stays explicit and cancellable.

## Task 6: Build and run the exposed n=12 feasibility gate

**Files:**

- Add: pinned n=12 case/source manifest under `benchmarks/`
- Add: paired Fast and Deep run manifests
- Modify: LongMemEval-V2 adapter/runner only where needed to select pinned question IDs and archive Deep provenance
- Add/update: evaluator provenance contract tests
- Add immutable run artifacts under `docs/build-log/artifacts/p1-t6/`
- Modify `STATUS.md` only in the same change as the named passing proof

**Steps:**

1. Treat n=12 as a feasibility screen, not promotion. Freeze a seedable, answer/gold-blind selection rule for 12 exposed LongMemEval-V2 questions (6 web, 6 enterprise, ability-balanced), with upstream IDs, revisions, and hashes. Assert that no answer/gold field appears in retained memory payloads.
2. Add a manifest-driven multi-question materializer/runner; the existing single-question materializer is insufficient. It must create identical Fast and Deep runtime inputs, run every arm sequentially on fresh scratch DBs, aggregate proof rows, and fail on any missing pair. Add paired manifests with identical corpus, question IDs, answer model, official per-question scorer, prompt, and packaged binaries. Extend archives with commit/binary/config/prompt/provider/model hashes, scratch DB identity, workspace manifest hash, citations, per-cap actuals, latency, token usage, and cost.
3. Run the exact n=12 Fast control and Deep treatment sequentially on fresh scratch databases. Archive all rows, including failures and partials.
4. Pre-register before running: the official per-question primary score, paired aggregation, treatment of errors/partials as failures unless the official scorer credits the cited answer, and explicit Deep p95/cost ceilings. The feasibility screen passes only with 12/12 pairing, no persistent recall-time writes, tenant/deletion/security non-regression, no cap/infra failure, and positive paired primary-score movement within those ceilings. If it loses, delete Tasks 2–5 code while preserving the negative artifact.
5. If feasibility passes, run an independent paired n≈100–300 exposed confirmation with a paired bootstrap CI excluding zero before integration, promotion, or a ledger flip. Do not open an official/sealed track.
6. Only with both proofs: update the live ledger and proceed to P1-T1. Otherwise record the honest stopping predicate.

## Verification before any T6 completion claim

Run the full repository gate from `AGENTS.md`, including the ignored scratch-Postgres contracts, all three provider lints, dry-run migrations, and the packaged end-to-end probe. Preserve the exact command outputs with the measured commit. No T6 completion, ledger checkbox, integration, push, or P3 spend follows from unit tests alone.

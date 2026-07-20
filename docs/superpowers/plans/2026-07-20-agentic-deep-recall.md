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
2. Add one read-only `MemoryStore` snapshot seam; do not reuse raw episode reads or the ranked/capped candidate query. Its output is a stable `DeepSnapshotEntry` with immutable source kind/ID, UUID-derived path, body/content hash, and the exact eligible `UnitId` set. The seam takes `RecallTime`; context/generation, source tombstones, unit state, transaction/valid time, and source linkage are applied before any body enters the workspace. Core applies the remaining query-specific policy gates and promotes only the entry-bound eligible unit IDs.
3. Implement InMemory and Postgres parity. Exclude every non-empty resource ACL using Task 2's helper. Postgres must exclude `forgotten_source`; InMemory must apply the equivalent tombstones.
4. Build the deterministic virtual workspace centrally: `WORKFLOW.md`, `manifest.jsonl`, `episodes/<uuid>.md`, and `resources/<uuid>.md`. Hash the canonical manifest/workspace and never derive a path from untrusted metadata.
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
4. Validate gathered source IDs against the manifest, add their linked units as `RecallChannel::Deep` candidates, and send them through existing policy, packing, citation, and trace machinery.
5. Make `recall_stage_facts` and `recall_feature_flags` truthful. Add the optional machine-readable Deep summary to `RecallResponse`; populate matching `l4_sandbox_id`, `l4_gathered_evidence_ids`, limits/actuals/stop reason in the trace. Keep `degraded` semantically unchanged.
6. Add fake cap tests for time, iterations, context, and spend, including zero-evidence abstention. Assert the public Deep summary through service/REST/MCP/CLI. Add malicious-provider tests for out-of-manifest source IDs and source entries with no eligible unit.
7. Replace the evaluator's old Deep-to-Balanced ablation with a true no-provider/control arm. Unignore the one-case rung-12 regression only after the fake provider proves the real contract.
8. Run narrow core/eval suites, formatting/clippy, commit, report, package, and independently review.

## Task 5: Implement the real cancellable runtime file agent

**Files:**

- Add: `crates/memphant-runtime/src/deep_recall_openrouter.rs`
- Modify: `crates/memphant-runtime/src/lib.rs`
- Modify runtime Cargo dependencies only if the existing async HTTP stack cannot be reused
- Add runtime unit/integration tests with a fake transport

**Steps:**

1. Add failing scripted-transport tests for the tool loop, source-ID output, every cap, malformed tool calls, retryable failures, cancellation/drop behavior, and usage/cost accounting. A pending real-transport test must observe transport-future drop when its caller is cancelled.
2. Implement read-only ordinary file tools over the virtual workspace: list, grep/search, and ranged read. No shell and no file mutation tools.
3. Use an async HTTP client with connect and per-request timeouts bounded by the single Deep wall-time budget, so dropping an Axum/MCP request cancels in-flight work rather than blocking a Tokio worker. Retries stay inside the same wall-time and spend ceilings. Keep model/provider/prompt and caps construction-time and hashable. Default Deep provider is off.
4. Require the model to return source IDs only; core remains the sole validator and response assembler. Preserve the best validated gathered set across iterations for partial-cap results.
5. Install the provider in `build_service`, never `build_worker_service`. Add strict env parsing with safe defaults and explicit opt-in/data-egress documentation.
6. Run runtime tests plus packaged REST/MCP/CLI scratch smoke tests for `mode=deep`; run formatting/clippy. Commit, report, package, and independently review.

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

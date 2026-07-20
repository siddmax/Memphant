# MemSyco Evidence-Arbitration SOTA Implementation Plan

> **For agentic workers:** Use `superpowers:executing-plans` and test-driven development. Do not commit, push, rebase, clean, or reset this dirty worktree.

**Goal:** Calibrate MemPhant's evidence-versus-preference decision boundary on a sealed non-official corpus, then run the pinned 300-sample MemSyco Memory-Evidence Conflict track only after the candidate earns that gate.

**Architecture:** Reuse the existing MemSyco adapter, official task runner, provider-attempt ledger, structured-state compiler, and fastembed runtime. Extend those shared seams instead of creating another harness. Development and confirmation data are schema-compatible but oracle-separated; the official dataset remains a one-shot confirmation surface.

**Tech stack:** Rust, Python 3.10, PostgreSQL 17, fastembed 5.17.2, pinned MemSyco-Bench, OpenRouter DeepSeek-V4-Flash.

## Global constraints

- No REST, MCP, SDK, database-schema, graph, service, compatibility-layer, STATUS, cutover, commit, push, or rebase change.
- Preserve fail-closed user-evidence grounding. Assistant and tool text never become user state.
- Use BGE-M3, top-k 10, and DeepSeek-V4-Flash for construction, answering, and judging.
- Every paid attempt is fresh, reconciled, hash-bound, priced, and retained; low scores are never retried.
- Run one TDD cycle per behavior and preserve unrelated dirty work.

## Task 1: BGE-M3 runtime selector

- [ ] Add failing runtime tests for `fastembed:bge-m3`, 1,024 dimensions, no query/document prefix, and profile isolation.
- [ ] Add the smallest `FastEmbedModel` variant and selector mapping using fastembed's installed `EmbeddingModel::BGEM3`.
- [ ] Run the focused runtime tests and keep all existing selector tests green.

## Task 2: Manifest-driven MemSyco execution

- [ ] Add failing Python contracts for one selected task, arbitrary sample counts, explicit input JSONL, `memphant`, `episode_only`, and `raw_dialogue` arms.
- [ ] Replace smoke constants in the existing runner/verifier with a strict run manifest; keep the current five-task one-sample command representable by that manifest.
- [ ] Require fresh artifacts, exact counts and hashes, unique response IDs, complete pricing, retry visibility, and no degraded results.

## Task 3: Decision-role preservation

- [ ] Add failing adapter tests proving recall kind, inclusion reason, speaker labels, and citations survive context rendering while gold/oracle fields remain unavailable.
- [ ] Render one compact typed context envelope from the existing recall response.
- [ ] If the development diagnosis proves structured-state contamination, add only the reserved generic fields `memory_role`, `epistemic_use`, and explicitly grounded applicability; keep user-only evidence validation unchanged.
- [ ] Add the fixed reader contract: preference personalizes subjective choices, current evidence and constraints outrank it, scope must be explicit, and active state supersedes retired state.

## Task 4: Extractor attempt proof

- [ ] Add failing Rust tests for disabled hidden retries, authoritative generation reconciliation, elapsed time, request/result hashes, parse status, complete usage/cost, and terminal post-payment reconciliation failure without resend.
- [ ] Extend the existing extractor attempt ledger and transport; do not add another retry or ledger abstraction.
- [ ] Verify interrupted, duplicate-ID, missing-price, and reconciliation-failure cases fail closed.

## Task 5: Non-official calibration corpus

- [ ] Create 12 development and 12 confirmation Memory-Evidence Conflict rows: six disjoint scenario families with polarity/order twins per split.
- [ ] Store the answer/rubric/evidence-span oracle separately from the label-free input.
- [ ] Add exact-hash and normalized five-gram overlap auditing against official data; emit only counts and pass/fail.
- [ ] Freeze corpus and manifest hashes before the first product candidate change.

## Task 6: Development and confirmation loop

- [ ] Run RawDialogue, episode-only MemPhant, and full MemPhant on the 12 development cases.
- [ ] Require RawDialogue 12/12 with zero sycophancy; full MemPhant at least 11/12 with at most one sycophantic result; and both decisive evidence and misleading preference present and distinctly labelled in all full-arm packets.
- [ ] Apply the predeclared diagnosis tree: repair ambiguous fixtures; fix retrieval when evidence is absent; fix structured contamination when episode-only wins; fix context rendering when roles disappear; fix only the shared reader contract when the labelled packet is correct.
- [ ] Re-run fresh full-arm attempts after each candidate change while reusing controls only on exact request/model/config hash equality.
- [ ] Freeze the passing candidate and run the 12 confirmation cases once across all three arms. A failed confirmation joins development and is replaced before another candidate version.

## Task 7: Official 300-sample track

- [ ] Verify the pinned official checkout and dataset, build the three binaries, start a scratch PostgreSQL database, and freeze all candidate hashes.
- [ ] Run 300 MemPhant and 300 same-model RawDialogue results in immutable 25-sample shards using the unchanged official scorer.
- [ ] Permit only infrastructure repair in a fresh shard directory; preserve every failed attempt and never retry a low score.
- [ ] Aggregate all 300 and the clean 299 excluding the exposed smoke UID. Run 10,000 paired bootstrap resamples.
- [ ] Claim reproduced task-specific SOTA only if accuracy is at least 87.28% with lower 95% bound above 84.28%, sycophancy is at most 12.72% with upper bound below 15.72%, both paired RawDialogue deltas clear zero in the desired direction, and the clean 299 point estimates beat both published points.

## Task 8: Verification and handoff

- [ ] Run focused Python/Rust contracts after each task, then the complete repository gate from `AGENTS.md`, spec drift, generated-artifact checks, and `git diff --check`.
- [ ] Generate and verify `SHA256SUMS` for every calibration and official artifact.
- [ ] Update the build log and handoff with exact models, providers, attempts, tokens, cost, hashes, metrics, confidence intervals, and an honest SOTA/no-SOTA disposition.
- [ ] Leave the unrelated Syndai cutover Feature Flow and implementation untouched.

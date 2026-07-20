# Extraction-bank and Memora replay proof

Date: 2026-07-14

## Decision

Extractor construction, retrieval, and reader calls are now separate. The
weekly/software_engineer bank is a content-addressed PostgreSQL 17 data-only
archive; every arm restores it into a new migrated scratch database and binds
the archive, extractor ledger, compiler, schema, runtime, selected official
sessions, and tenant/scope/actor identities. Reader selection cannot change the
extractor or repay ingestion.

## Frozen bank

- Sessions: 163; successful episodes: 163.
- Provider attempts: 164; one rejected operation was repaired semantically;
  zero terminal decode or semantic failures.
- Extractor cost: $1.51070185; requested model Luna Pro; served providers
  OpenAI/Azure through OpenRouter.
- Manifest SHA-256:
  `025e6bfc0fdf3f1968e9138bb1c2029894b65e15fb2dea99bca924564c77ae4a`.
- Archive SHA-256:
  `34c4aef38d1b3cdf44d0aefb08290181d251f63edcb9ad903380986096165f56`.
- Extractor-ledger SHA-256:
  `9ab1fb581e84b696701fdc9c6662a2239bfc332fabae5d398ae9c5b77b646cbb`.
- Construction-runtime SHA-256:
  `7355d38dfd4c66fed53feb133bdf29537550146119ac7711dac5184353c50569`.
- Compiler:
  `compiler-0.1.0-ws0+structured-a556284fa53d507bb894d68e`.

The installed PostgreSQL 14 archive tools correctly failed against the
PostgreSQL 17 server. Matching 17.10 tools produced and restored the bank.
Replay mints a fresh API key and performs no retain or drain.

## Answer and scorer results

- Five corrected reasoning questions: 5 fresh Luna calls, $0.042714, all exact
  totals and goal statuses, zero fallback/error/unpriced calls. Official FAMA:
  100 across 9/9 reasoning subquestions.
- Complete weekly/software_engineer split: 15 fresh Luna calls, $0.147114,
  zero fallback/error/unpriced calls. The official three-judge scorer completed
  all 71 subquestions once.
- Official FAMA improved from 32.96296 to 53.49206. Reasoning improved to 100;
  Recommending was 50.476; Remembering remained 10.
- Raw accuracy was 43/71 versus the prior pilot's 44/71; memory presence was
  19/44 and forgetting absence 24/27. The gain is temporal/reasoning-specific,
  not blanket accuracy or a SOTA result.

Artifacts:

- `docs/build-log/artifacts/unified-sota-20260714/task4-memora-luna/reasoning-v2.answers.json`
- `docs/build-log/artifacts/unified-sota-20260714/task4-memora-luna/reasoning-v2.fama.json`
- `docs/build-log/artifacts/unified-sota-20260714/task4-memora-luna/full.answers.json`
- `docs/build-log/artifacts/unified-sota-20260714/task4-memora-luna/full.fama.json`

## Zero-cost retrieval control

`retrieval-screen-v7.json` pairs all 15 questions across two distinct scratch
databases restored from the identical frozen bank. It admits exactly five
query/window-specific rollups and two exact goal companions, preserves the
same-capacity ordinary retrieval control, and makes zero paid calls.

- Report file SHA-256:
  `6cd243f43b37269a6bedcbb711bbc9a3e46fd1e6802123ed659a703c635fdf99`.
- Proof file SHA-256:
  `d305eaa3b4387c0f1f8f58a7b609ba5fd119d6b8390f82f5fbe0554dc8a5fbe6`.
- Scratch identities differ:
  `e3dafd0692597d5cef7002482f47a47d7720b60d3aefc945ea5e573ab7573132`
  and
  `ee867f55976d958b5098edddd01c382ea83c843d018092be73f2495589494e9a`.

The gate found and fixed two root defects: a rollup had globally disabled
ordinary relevance replacement, and the comparator treated any known goal as
the current question's companion. The final comparator uses an independent
baseline recall at the capacity left by the novel rollup.

## Rejected mechanism and next causal class

Selective L4 expansion contributed 0/150 packed items and was deleted. The
useful change is removal of the former blanket `8 + score + RRF` L4 dominance:
valid exact target coverage rose from 0/30 to 13/30 without an L4-packed item.
No reader call is authorized for the deleted branch.

The remaining failure is state maintenance. Explicit removal episodes decoded
successfully, but the stateless provider guessed identities and exact-key
invalidation silently matched zero units. Stale proposal, email, meeting, todo,
and preference facts therefore remain active. The next mechanism is
state-aware, unit-ID-bound create/replace/delete with per-scope ordering and a
fail-closed zero-match rule—not another answer model.

## State-aware mutation mechanism

The worker now supplies the provider with the active structured state in the
current tenant/scope, including exact unit IDs and valid-time rectangles. The
wire contract has three operations: `create` requires a new canonical identity,
`replace` names the exact active unit and returns the complete surviving fields,
and `delete` names every exact unit to remove. Model-authored identity is never
trusted for mutations. Unknown, inactive, stale, reused, or zero-match targets
fail closed. Jobs are prepared and compiled sequentially inside a tenant/scope
lane so each mutation sees the state committed by its predecessor; independent
lanes still execute concurrently.

The deterministic Memora trajectory proves partial aggregate removal preserves
siblings, a logical todo split across task and due-date units is removed in
full, unrelated active state survives, and delete-then-recreate yields exactly
one active generation. Focused service tests prove `0 -> 1` active-state
visibility within one lane and overlap across distinct lanes.

The final one-request Luna mutation probe used production-shaped UUIDv7 active
unit IDs. Attempt `54cb8c05-7360-460c-8dfe-5a35d4eb58f3` was served by Azure
through OpenRouter for $0.020978 and decoded four accepted operations with zero
rejections: one complete replacement preserving Embedded Software Team while
removing Head of Engineering, plus three exact deletes for the grocery and
split legacy-refactor state. The substantive assertions all passed; the test
harness then rejected valid per-operation verbatim evidence substrings because
it incorrectly required every operation to cite the entire multi-sentence user
turn. That local assertion is corrected without repaying the identical model
call.

The full diagnostic ledger is intentionally retained: three zero-cost HTTP 400
schema probes, six paid responses totaling $0.0826442, four clean decodes, and
two rejected decodes. Those failures exposed and removed an unsupported strict
schema keyword, untrusted model-authored mutation identities, and non-production
synthetic UUID fixtures. Ledger SHA-256:
`d0bd5157ab229d56119516cf5cd575a645d9e3ac2d025a0b92d3798d04444ff2`.
This mechanism proof does not establish restored-bank compatibility or improve
an official answer score; those remain the next causal gate.

## Forward-schema targeted state diagnostic

The squashed pre-production schema moved after the bank was frozen, so the
strict replay helper correctly rejected its database-identity hash. PostgreSQL
17 nevertheless restored the pinned data archive transactionally into a fresh
current-schema scratch database, with every frozen table row count unchanged.
The resulting experiment is labeled a forward-schema diagnostic, not strict
bank compatibility.

Five official update episodes were reposted through dedup and produced six
current-compiler jobs (the Kim-to-Alastair episode was retried once after the
general former-to-current prompt rule was added). Across six Luna calls,
$0.31042185, and three delta ledgers, the adapter accepted six operations with
zero rejected operations. At current valid time:

- buy-groceries and both legacy-refactor task/due units are absent;
- Kim Stanley Robinson is absent and Alastair Reynolds is current;
- space opera is absent and parallel universes is current;
- Ann Leckie's recent work is `not_for_me`, while the source-faithful surviving
  nuance is limited to her earlier books and world-building.

That last result exposes a benchmark-oracle boundary: Memora labels Ann Leckie
as an all-or-nothing forgotten item, while the source explicitly distinguishes
earlier from recent work. The runtime preserves the finer statement rather than
deleting truthful state to game the coarse label.

The same zero-answer-cost book retrieval then separated state correctness from
ranking. Baseline retained parallel universes and soft science fiction and
excluded Kim/space opera, but missed Alastair and the genetic-engineering
dislike. Two independent code-contract defects were fixed: raw conversations
are now `Episodic` rather than compiler-default `Semantic`, and deterministic
reranking consumes the real vector-channel score instead of relabeling token
overlap as vector evidence. The latter improved its focused vector-only
regression from rank 11 to the admitted top ten, but did not by itself recover
Alastair in the official book pack.

Voyage top-16 could not start because `VOYAGE_API_KEY` is absent; the server
failed before health, so no provider request or cost occurred. Local BGE
top-16 recovered Alastair in 2.84 seconds but still missed the genetic-
engineering dislike; top-32 also resurfaced historical Kim. Both remain
diagnostics and cross-reranking remains off. No reader or judge call was made.

Proof:

- `docs/build-log/artifacts/unified-sota-20260714/task4-state-mutation/targeted-state-proof.json`
- Proof SHA-256:
  `47805cb9f96678b8afaf127450ba99eb483e9e2732256294f7eaf391c2187298`.

## Causal prefix mutation screen

The frozen final bank cannot be replayed as though it were chronological: it
already contains structured units derived from later episodes. The targeted
screen therefore used eight fresh restored databases. In each database, a
scratch-only transaction closed every structured Semantic unit sourced at or
after the trajectory cutoff before replaying one of these exact sequences:
`0037→0134→0140`, `0060`, `0064→0091`, `0076→0117`, `0079`,
`0090→0144`, `0104`, and `0114→0150→0152`. Raw episodes and embeddings
remained immutable. This is a forward-schema causal diagnostic, not a strict
archive-compatibility claim.

Sending all open state was also unnecessary. A zero-provider-call target
screen showed that full-episode BGE alone misses exact mutation targets even at
top 32. The promoted selector reuses the deterministic lexical scorer, takes
four positive-score seed items, and includes every active sibling in their
namespaces. It passes through sets of at most 32 items, sorts by unit ID, and
falls back to the full state when there is no lexical signal. Five focused
tests cover target/sibling selection, passthrough, determinism, no-signal
fallback, and an anaphoric user instruction whose target is named by the
assistant. The 15 final requests consumed 166,266 prompt tokens total, far
below the earlier approximately 58–60k-token request shape per call.

The first completion endpoints exposed one shared policy gap: session 0140
recorded the observed 30-minute event but left the exercise plan active, while
0152 replaced the todo with a current `status=completed` fact. The general
contract now requires completed, finished, fixed, done, cancelled, or
no-longer-needed todos/goals/plans to delete every exact active item; an
observed quantity is emitted in addition, and a current Semantic completed
copy is forbidden. Focused decode coverage locks delete-plus-quantity behavior.
Only those two endpoints were repeated from cloned pre-completion snapshots.

The final causal set is green across all 15 official mutation episodes: 15
priced Luna Pro calls, 28 accepted operations, zero rejected operations,
166,266 prompt tokens, 35,197 completion tokens, and $0.33558585. Current-valid
readback proves the todo, proposal, email, meeting, exercise, development-
environment, and critical-bug trajectories; split siblings survive; removal-
then-recreation yields one current generation; exercise completion leaves one
30-minute quantity event and no plan; and critical-bug completion leaves no
outstanding or completed Semantic copy. The two failure-discovery calls are
retained separately and cost $0.03034015.

Proof:

- `docs/build-log/artifacts/unified-sota-20260714/task4-state-mutation/prefix-screen-proof.json`
- Proof SHA-256:
  `af397fce2efa65f49ca2aef7eb4c4911a3c5c9b94d321496fdf426ad5a3250fd`.

## Preference extraction and book composition gate

The remaining book-pack miss was not a retrieval failure. Session 0055
contains the explicit first-person statement that genetic engineering in
science-fiction books is a disliked topic, but the frozen Luna extraction
accepted zero operations and no structured unit represented it. The general
provider contract now treats explicit first-person likes, dislikes, and
recommendation preferences about topics, genres, creators, and activities as
durable even inside ordinary conversation. General opinions, hypotheticals,
questions, and preferences attributed to an assistant or third party remain
non-durable. Focused prompt and decode regressions cover the distinction.

One targeted Luna recompile of session 0055 accepted one operation and rejected
zero for $0.02095315 (15,705 prompt and 1,760 completion tokens). It produced a
current `reading_preferences/disliked_topics` unit whose evidence is the exact
user statement and embedded it with `fastembed:bge-small-en-v1.5`. Namespace
and key wording remain provider output, not a public contract; the semantic
fact, evidence span, current-state validity, and embedding are the assertions.

The zero-answer-cost BGE balanced top-16 recall then passed all six predeclared
checks in 2.88 seconds: its top ten contains Alastair Reynolds, parallel
universes, soft science fiction, and the genetic-engineering dislike, while
excluding historical Kim Stanley Robinson and space opera. The trace proves a
64-item pool and one successful 16-candidate
`fastembed:bge-reranker-base` batch. Unlike top-32, it did not resurrect Kim.
This admits one reader call; it does not promote cross-reranking globally.

That single Luna Pro reader call cost $0.015834 and correctly composed the
current nuanced state: Alastair Reynolds, soft science fiction, parallel
universes, avoidance of genetic engineering, and Ann Leckie's earlier
world-building while warning that her recent work is a poorer fit. It names no
Kim or space opera. The response is source-faithful but intentionally conflicts
with Memora's coarse Ann all-or-nothing forgetting oracle, so it is composition
proof rather than an official SOTA score or isolated causal accuracy delta.

Proof:

- `docs/build-log/artifacts/unified-sota-20260714/task4-state-mutation/prefix-screen/genetic-preference-v2.jsonl`
  — SHA-256 `359bdde02031d0cc5845c3c1c2b0efff13f4e80c1ae2cf8457bd3d30000ca2a1`.
- `docs/build-log/artifacts/unified-sota-20260714/task4-state-mutation/book-current-bge16-post-preference.json`
  — SHA-256 `6326164d79ae123dd5b2c9193fa4770133d6607e95467bec802226ac5aee2f67`.
- `docs/build-log/artifacts/unified-sota-20260714/task4-state-mutation/book-current-luna-reader-post-preference.json`
  — SHA-256 `22280564e234df9719060c62fee5ccc3d48af0aff199092a4262b7f2f4ccd576`.

## Versioned extraction prompt boundary

The accuracy-critical structured-state prompt was still a Rust constant, in
direct conflict with the engineering spec's review-blocking iteration-loop
rule. It now lives at `config/structured-state-v1.txt` and is loaded through
the required `MEMPHANT_STRUCTURED_STATE_PROMPT_PATH` whenever structured-state
extraction is enabled. Empty or unreadable files fail before a provider is
constructed. The loaded bytes drive both the actual system message and the
compiler identity's prompt hash, so changing policy requires no Rust edit or
recompile and cannot silently reuse prior compiler output. One conventional
terminal line ending is normalized; the promoted prompt text and provider
behavior are otherwise unchanged.

Local benchmark configuration supplies the absolute checked-in path, the
development environment names the relative path, and the runtime container
copies the same versioned file to `/etc/memphant`. A focused regression writes
an alternate prompt file and proves its content reaches the request and its
hash reaches the compiler identity. This is a single file boundary, not a
generic prompt framework.

## Causal full-state todo gate

The eight causal prefix databases prove independent trajectories, but no one
database contains their combined final state. A zero-provider-call diagnostic
therefore cloned the current forward-schema bank and composed only proven
deltas: it first closed the original Semantic outputs for each replayed episode,
then copied memory-unit and edge changes whose transaction clock exactly equals
a completed causal-screen job. It did not infer or regenerate a mutation. The
resulting `memphant_causal_final` scratch database is diagnostic proof, not a
new persistent product path.

Production-baseline recall for `activity_todos_163` then passed all ten
predeclared checks: optimize database queries and schedule car maintenance are
present; groceries, exercise, home-network setup, legacy refactor, development-
environment setup, flight booking, CI/CD setup, and both critical-bug forms are
absent. Recall was non-degraded and made no answer-model call. Exactly one Luna
reader call then answered with database optimization, car maintenance, and the
still-current inbox goal, mentioned no deleted/completed task, and cost
$0.010726 (5,368 prompt, 893 completion tokens). This closes stale-free answer
composition for the todo question, not the complete Memora split or Task 4.

Proof:

- `docs/build-log/artifacts/unified-sota-20260714/task4-state-mutation/todo-current-causal-retrieval.json`
  — SHA-256 `352ee06cb720cf4c3b2f1afa17bc1ed9703594e7e6b7af632351c784d111f69f`.
- `docs/build-log/artifacts/unified-sota-20260714/task4-state-mutation/todo-current-luna-reader.json`
  — SHA-256 `d0daddb9188fd18f5e7195bc347188a47b1a5367932c2e86417a9381d3d48818`.

## Current artifact reconstruction gate

The largest remaining true Memora error class was not missing extraction: the
project proposal, email, and meeting-note fields existed as current structured
units, but top-ten packing admitted only a few fields among topical distractors.
The runtime now recognizes only an explicit quoted title or colon-suffix
artifact anchor, grounds it to matching source episodes in the already
authorized bitemporal candidate set, closes over the selected current
structured namespaces, and replaces those field members with one deterministic
artifact-state candidate. Its ID is a hash of the tenant, scope, and sorted
member IDs; its trust is the most conservative member trust; and every member
ID remains attached as provenance. The same mechanism reconstructs historical
valid-time and transaction-time rectangles rather than silently falling back
to current state.

The first real Postgres/server screen correctly constructed all three bundles
but exposed a packing error: corpus fusion placed the project bundle at fused
rank 45 and the other two at output rank 10. Because a bundle exists only after
the explicit anchor and authorized-source proof, it now shares the existing
authoritative-projection admission path with a query/window-specific quantity
rollup: stable first partition plus replacement protection. A focused test uses
a 13-field bundle and 12 high-overlap distractors, requires the bundle at rank
one, proves the last field is not truncated, excludes an expired active
rectangle and an unrelated namespace, and proves deterministic current and
historical provenance.

The rebuilt production-baseline server then returned all three artifact bundles
at rank one without cross-reranking or provider calls. The project pack contains
all nine required presence groups and excludes the three removed fields; the
email contains its current purpose, latency values, CTA, deadline, sprint, and
Embedded Software Team while excluding Head of Engineering; the meeting notes
contain the current agenda, three decisions, and Lars/Priya actions while
excluding Yuki. Exactly one Luna Pro reader call was admitted for the largest
project residual. It produced a complete proposal with every required current
fact and none of the three removed facts, using one provider attempt for
$0.019542 (6,510 prompt and 2,172 completion tokens). This is targeted
mechanism/composition proof, not an official FAMA rerun or SOTA claim.

Proof:

- `docs/build-log/artifacts/unified-sota-20260714/task4-state-mutation/artifact-reconstruction-runtime.json`
  — SHA-256 `5d6c20b8bfa67fe988ab8392344bfe1ef6399c85c1bba705659c82f4d6d106b1`.
- `docs/build-log/artifacts/unified-sota-20260714/task4-state-mutation/project-current-luna-reader.json`
  — SHA-256 `88ae2374e0a414a07e44e7d48713cde2233300d32dbf35e2d6ed1812980b5198`.

## Verification

- `cargo test -p memphant-core --lib` — 71 passed.
- `cargo test -p memphant-core --test artifact_reconstruction` — 1 passed,
  including project-sized rank-one admission and bitemporal reconstruction.
- `cargo test -p memphant-core --test recall_trace_golden` — 16 passed.
- `cargo test -p memphant-core --test quantity_rollup` — 12 passed.
- `cargo test -p memphant-runtime --lib structured_state_openrouter::tests` —
  29 passed, 2 paid tests ignored, including the durable first-person
  recommendation-preference regressions and versioned prompt identity.
- `cargo test -p memphant-core --test structured_state_projection` — 18
  passed, including the deterministic Memora removal trajectory.
- `cargo test -p memphant-core --test candidate_pool` — 2 passed, including
  real-vector deterministic rerank admission.
- `cargo test -p memphant-core --test write_compiler_golden` — 12 passed,
  including fail-closed zero-match and exact-active-target regressions.
- `cargo test -p memphant-core --lib structured_state::tests` — 5 passed,
  including deterministic lexical namespace selection and fail-safe fallback.
- `cargo test -p memphant-runtime --lib completion_` — 2 passed, including
  completion deletion plus simultaneous quantity-event decoding.
- `cargo check --workspace --all-targets --all-features` — passed.
- `python3 -m pytest tests/test_memora_benchmark_contract.py -q` — 40 passed.
- `python3 -m pytest tests/test_temporal_benchmark_contract.py
  tests/test_restraint_launch_gate.py -q` — 13 passed after re-pinning the
  deliberate current strict reader-response contract in the STALE generation
  manifest; no benchmark or model call was made.
- `python3 scripts/check_spec_drift.py` — public/private mirrors clean.

## Boundary

This closes the extraction-bank contracts and the targeted reasoning, mutation,
todo, recommendation, and multi-field artifact residuals. It does not close
Task 4: official STALE, a runnable restraint harness, strict restored-bank
compatibility, and an official end-to-end split rerun remain open. The full
600-question run remains deferred until the minimum viable official split is
green.

# MemPhant MemSyco evidence-arbitration handoff — 2026-07-16

Current STATUS mirror: RUNTIME COMPLETE — BENCHMARK EVIDENCE PENDING

Status: **IN PROGRESS / DONE WITH CONCERNS**. The session made a large, durable
step toward task-specific MemSyco SOTA, but did not produce an official SOTA
claim. Three official tracks were correctly retired after first fail-closed
product failures. The active lane is now **Valid Memory Selection**, where the
latest product candidate fails on non-official development case 10 before an
answer is generated.

Resume in `/Users/sidsharma/Memphant`. Preserve the large dirty worktree. Do
not reset, clean, rebase, commit, push, move a STATUS checkbox, or touch the
paused Syndai/CaaS cutover.

## Executive state

The session targeted the boundary between retrieved personalization and
decision evidence. The durable product behavior is now substantially better:

- explicit user preferences are stored as personalization, not factual
  evidence;
- preference value and applicability scope are grounded in the exact user
  quote;
- objective questions are answered from reliable evidence or world knowledge,
  even when a user states a preferred false answer;
- a newer, same-episode preference can supersede an older preference without
  weakening generic duplicate-state protection;
- retrieved context preserves memory role, epistemic use, speaker labels,
  inclusion reason, and citations;
- the benchmark runner is manifest-driven and provenance-strict across
  MemPhant, episode-only, RawDialogue, and the Objective-only NoMemory arm.

The current blocker is in the structured-state decoder:

```text
valid-selection/development/memphant-v6
completed cases: 9/12
failed case: zero-based offset 9 (case 10)
rejection reasons:
  duplicate_state_identity: 1
  missing_preference_operation: 1
rejection diagnostics: []
```

The paid extractor response was reconciled and preserved. The decoder accepted
one operation and rejected two, so the worker stopped fail-closed. Do not retry
this paid completion or reuse its directory. The next engineering action is to
make duplicate-identity diagnostics reveal non-secret operation shapes, write a
focused regression, and fix the shared collision rule only after the trace
shows why the narrow same-episode replacement predicate did not apply.

Primary failed evidence:

- `docs/build-log/artifacts/unified-sota-20260714/memsyco-evidence-sota-20260715T172416Z/valid-selection/development/memphant-v6/memory/ABORTED.json`
- `docs/build-log/artifacts/unified-sota-20260714/memsyco-evidence-sota-20260715T172416Z/valid-selection/development/memphant-v6/extractor-attempts.jsonl`
- response ID `gen-1784195648-OJx0TmWDmEVMlmq09ivM`
- served model `deepseek/deepseek-v4-flash-20260423`
- provider `DeepInfra`
- retry index `0`
- usage `1,389` prompt + `255` completion tokens; cost `$0.000139122`

## Why this was the right subsystem

The earlier retrieval work showed that basic retention was not the decisive
weakness. The unresolved behavior was epistemic arbitration: a memory system
must retrieve a preference without letting that preference become evidence for
an objective claim, must apply it only inside its grounded scope, and must use
the latest valid preference rather than stale state.

MemSyco decomposes those behaviors into separate official tasks, so the session
used a disciplined track loop:

1. create non-official development and one-shot confirmation packs;
2. compare full MemPhant, episode-only, and the official RawDialogue control;
3. change only the first subsystem that drops or mislabels the required
   information;
4. freeze code, prompts, models, binaries, fixtures, and dependencies;
5. open one official task once;
6. retire the official track on a product failure without inspecting row
   contents or tuning against the holdout.

This preserves the meaning of an official result. It also exposed real product
bugs in the shared structured-state path instead of encouraging
benchmark-specific answer rules.

## What changed and why

### Structured preference extraction

The main implementation is
`crates/memphant-runtime/src/structured_state_openrouter.rs`.

- Dedicated preference operations carry `memory_role=personalization` and
  `epistemic_use=not_factual_evidence`.
- Preference values and applicability scopes are derived from grounded user
  quotes rather than trusted from duplicated model fields.
- `Update: For ..., I now prefer X instead.` is parsed as a current preference
  update.
- Same-episode, same-identity personalization collisions may resolve to the
  later grounded preference. Generic state collisions remain fail-closed.
- Generic state whose grounded quote unmistakably states a preference is
  normalized into the reserved epistemic role rather than becoming factual
  state.
- Exact preference validation no longer applies to unrelated generic fields
  named `value`.
- Assistant, tool, system, active-state, and unlabeled text remain forbidden as
  user-state evidence.

The versioned extraction contract is
`config/structured-state-v1.txt`. It now explicitly says that a preferred or
familiar answer remains personalization even when it embeds an objectively
false proposition.

Why: the product must preserve preferences for personalization while ensuring
they never silently become evidence about the world.

### Typed reader context and arbitration

The adapter is `benchmarks/memsyco/memphant_baseline.py`.

- Recall items are rendered in a typed context envelope instead of being
  flattened to body and unit ID.
- The envelope preserves kind, speaker labels, inclusion reason, citations,
  `memory_role`, and `epistemic_use`.
- The fixed arbitration contract says current evidence and hard constraints
  outrank preferences, scope must be explicit, active state supersedes retired
  state, and objective facts ignore a preferred/familiar answer.
- Adapter proofs hash the arbitration contract and the rendered packet.

Why: correct metadata is useless if it disappears before the answer model sees
it.

### Manifest-driven official harness

The runner is `scripts/run_restraint_bench.py`.

- One task, arbitrary `--limit` and `--offset`, explicit `--test-jsonl`, and
  `memphant`, `episode_only`, `raw_dialogue`, or `no_memory` arms are supported.
- NoMemory is valid only for `objective_fact_judgment`.
- Every run writes `run.json` with task, arm, slice hash, implementation hashes,
  model identities, embedding profile, top-k, and binary hashes for memory
  arms.
- Fresh artifact directories are mandatory.
- Verification requires exact result and attempt counts, unique response IDs,
  complete pricing, matching models, retry index zero, and complete adapter
  proofs.
- Memory arms use a scratch database and the packaged server, worker, and CLI.

Why: a benchmark score without immutable execution identity and complete paid
attempt provenance is not reproducible evidence.

### Non-official calibration tooling

Added generators:

- `scripts/generate_memsyco_evidence_calibration.py`
- `scripts/generate_memsyco_scope_calibration.py`
- `scripts/generate_memsyco_objective_calibration.py`
- `scripts/generate_memsyco_valid_selection_calibration.py`

Added validation:

- `scripts/audit_memsyco_calibration_overlap.py` reports only exact-hash,
  suspicious, and normalized five-gram overlap counts.
- `scripts/verify_memsyco_calibration_packets.py` verifies that required
  evidence and role labels reached the reader packet without exposing the
  oracle to the adapter.
- `tests/test_restraint_benchmark_contract.py` covers manifest counts, arm
  restrictions, BGE-M3 identity, leakage, freshness, attempt integrity, and
  packet contracts.

Why: product iteration needs a reusable development surface that cannot leak
official answers or turn an exposed official row into a tuning fixture.

## Results and disposition by track

All calibration runs used `deepseek/deepseek-v4-flash` for extraction, answer,
and judging, plus `fastembed:bge-m3`, 1,024 dimensions, top-k 10 for memory
arms. The official source is pinned to revision
`c31e2c85ee8cc3c6f643587b8a6f4b5ad5eb3bf6`.

| Track | Non-official evidence | Official disposition | What it means |
|---|---|---|---|
| Memory-Evidence Conflict | Final development and confirmation-v3: all three arms 12/12, zero sycophancy, exact packet proof | Retired at official sample 5 after a duplicate identity rejection; no row inspection | Calibration proved the intended behavior, but the official holdout exposed an extraction-shape gap. Do not rerun or tune this track. |
| Contextual Scope | Development and confirmation: all three arms 12/12 with zero incorrect preference use | Retired on the first official MemPhant sample after `evidence_grounding`; paired RawDialogue stopped | Scope behavior passed calibration, but official extraction did not. Preserve the holdout as exposed. |
| Objective Fact | Full MemPhant dev/confirm 12/12 with zero contamination; NoMemory 12/12. RawDialogue was 3/12 dev and 4/12 confirm; episode-only was 12/12 dev with one contamination and 11/12 confirm | Retired on the first official MemPhant sample after `structured_extractor_evidence_grounding`; NoMemory stopped after one sample | Structured state materially beat RawDialogue on the non-official objective-fact surface, but did not earn an official score. |
| Valid Memory Selection | Earlier development MemPhant, RawDialogue, and episode-only each passed 12/12. Original confirmation RawDialogue passed only 11/12 and was retired. Independent confirmation-v2 RawDialogue passed 12/12, but MemPhant failed after 10 results; after one-case repair, full development requalification failed on case 10 | **Untouched: no official calls** | This is the active clean lane. Fix and requalify on non-official data, then create a wholly new confirmation-v3 before considering official execution. |
| Personalized Memory Use | Not started | Untouched | Run only after Valid Memory Selection earns or retires its official gate. |

Official retirement records:

- `scope/OFFICIAL-GATE.json` and `scope/OFFICIAL-RETIREMENT.json`
- `objective/OFFICIAL-GATE.json` and `objective/OFFICIAL-RETIREMENT.json`
- evidence-conflict partial official artifacts are under
  `official/memphant/shard-000/`

All paths above are relative to:

```text
docs/build-log/artifacts/unified-sota-20260714/
  memsyco-evidence-sota-20260715T172416Z/
```

## Artifact trust map

### Current trustworthy inputs

- Official lock: `benchmarks/manifests/memsyco.lock.json`
- Official acquired source:
  `/tmp/memphant-memsyco-evidence-official-20260715T172416Z`
- Valid Selection development data:
  `benchmarks/memsyco/valid_selection_calibration/development.jsonl`
- Valid Selection development oracle:
  `benchmarks/memsyco/valid_selection_calibration/development.oracle.jsonl`
- Valid Selection generator manifest:
  `benchmarks/memsyco/valid_selection_calibration/manifest.json`

The `/tmp` checkout is an acquired, hash-verified source tree, not a Git
checkout. `git -C ... rev-parse` is expected to fail. Use the lock file and
`run_restraint_bench.py verify` to establish identity. Reacquire if the `/tmp`
tree disappears.

### Historical-only freezes

These files prove earlier candidates but are stale after later product changes:

- top-level `CANDIDATE-FREEZE.json` and `FINAL-CANDIDATE-FREEZE.json`
- `scope/CANDIDATE-FREEZE.json`
- `objective/CANDIDATE-FREEZE.json`
- `valid-selection/CANDIDATE-FREEZE.json`

Do not promote or extend them. A new Valid Selection freeze must hash the final
code, prompt, adapter, runner, fixtures, binaries, models, provider policy, and
dependencies after development requalification.

### Immutable failed runs

Every failed or interrupted directory is evidence. Never delete it, append a
retry to it, or reuse its ledger/cache. Particularly important:

- `valid-selection/development/memphant-v6/`
- `valid-selection/confirmation/raw-dialogue/`
- `valid-selection/confirmation-v2/memphant/`
- `objective/official/`
- `scope/official/`
- `official/memphant/shard-000/`

## Current verification state

Focused checks passed after the latest implemented repair:

```text
cargo test -p memphant-runtime structured_state_openrouter --lib
41 passed; 2 paid live tests ignored

python3 -m pytest tests/test_restraint_benchmark_contract.py -q
28 passed
```

The complete repository gate has **not** passed for this workstream. An earlier
full Python run had 503 passed, 2 skipped, and 6 failures in unrelated dirty
LongMemEval, cutover/spec-mirror, handoff-status, and Memora fixture lanes. Do
not fix those in the MemSyco campaign merely to manufacture a green total.
Re-run the full gate before any completion claim and report unrelated failures
separately with exact evidence.

PostgreSQL was stopped before this campaign. The compose PostgreSQL 17 service
is currently running and healthy on `127.0.0.1:5432`; return it to stopped after
the campaign and after all scratch databases have been dropped.

## Next steps, in order

### 1. Diagnose the current duplicate collision

Why: `rejection_diagnostics` is empty, so changing collision behavior now would
be guesswork and could weaken the generic fail-closed invariant.

Add a regression first. Enrich the duplicate-state rejection with non-secret
shape data only: operation kind, namespace/item-key equality, reserved role
presence, grounded-quote ordering, and which predicate failed. Do not log raw
official text or model output.

Relevant code:

- collision handling around
  `crates/memphant-runtime/src/structured_state_openrouter.rs:900`
- `missing_preference_operation` check around the same decoder section
- preference normalization and grounding around
  `crates/memphant-runtime/src/structured_state_openrouter.rs:1020`
- preference evidence parser around
  `crates/memphant-runtime/src/structured_state_openrouter.rs:1423`
- decoder tests beginning around
  `crates/memphant-runtime/src/structured_state_openrouter.rs:1640`

Run:

```sh
cargo test -p memphant-runtime structured_state_openrouter --lib
python3 -m pytest tests/test_restraint_benchmark_contract.py -q
```

### 2. Reproduce only development case 10 in a fresh directory

Why: this is non-official development data, so a focused diagnostic is allowed.
It should prove the root cause before another 12-case run.

```sh
cd /Users/sidsharma/Memphant

export CAMPAIGN="$PWD/docs/build-log/artifacts/unified-sota-20260714/memsyco-evidence-sota-20260715T172416Z"
export OFFICIAL=/tmp/memphant-memsyco-evidence-official-20260715T172416Z
export MODEL=deepseek/deepseek-v4-flash
export BASE_URL=https://openrouter.ai/api/v1

MEMPHANT_STRUCTURED_STATE=on \
MEMPHANT_STRUCTURED_STATE_MODEL="$MODEL" \
MEMPHANT_STRUCTURED_STATE_PROMPT_PATH="$PWD/config/structured-state-v1.txt" \
doppler run --project syndai --config dev -- \
uv run --python 3.10 --with-requirements "$OFFICIAL/requirements.txt" \
python3 scripts/run_restraint_bench.py run \
  --official-dir "$OFFICIAL" \
  --out-dir "$CAMPAIGN/valid-selection/development/memphant-v7-offset9-diagnostic" \
  --task valid_memory_selection \
  --arm memphant \
  --test-jsonl "$PWD/benchmarks/memsyco/valid_selection_calibration/development.jsonl" \
  --offset 9 --limit 1 \
  --embed-model fastembed:bge-m3 \
  --model "$MODEL" --base-url "$BASE_URL" \
  --judge-model "$MODEL" --judge-base-url "$BASE_URL" \
  --database-url postgres://memphant:memphant@127.0.0.1:5432/memphant \
  --port 39532
```

Pass conditions: one reconciled extractor, answer, and judge attempt; retry
index zero; no rejection; one valid current preference; no outdated preference
contamination; verified hashes and packet proof.

```sh
python3 scripts/run_restraint_bench.py verify-results \
  --run-dir "$CAMPAIGN/valid-selection/development/memphant-v7-offset9-diagnostic"
```

### 3. Requalify the complete development matrix

Why: any product code change invalidates the earlier MemPhant binary and
implementation hashes. A one-case repair is not a candidate.

Run full MemPhant in a fresh `memphant-v7` directory with `--offset 0 --limit
12`. Then run episode-only again because the packaged binary hashes changed.
RawDialogue may be reused only if its complete input, request, model, provider,
judge, and config hashes are byte-identical; otherwise rerun it too.

Use the command above with:

```text
--out-dir .../valid-selection/development/memphant-v7
--offset 0 --limit 12
```

For episode-only change:

```text
--arm episode_only
--out-dir .../valid-selection/development/episode-only-v2
--port 39533
```

Run memory arms sequentially. Two concurrent scratch migrations can fail with
`tuple concurrently updated`; that is infrastructure contention, not a model
or memory result.

Verify the full MemPhant packet against the development oracle:

```sh
python3 scripts/verify_memsyco_calibration_packets.py \
  --proof-dir "$CAMPAIGN/valid-selection/development/memphant-v7/memory" \
  --oracle "$PWD/benchmarks/memsyco/valid_selection_calibration/development.oracle.jsonl"
```

The packet verifier currently understands evidence-conflict and scope oracles.
Extend it minimally for Valid Selection so it requires the current preference
and proves the outdated preference is retired or explicitly inactive. Keep the
oracle completely outside retention, recall, answer, and judge requests.

Development gate:

- RawDialogue 12/12, zero outdated contamination;
- full MemPhant at least 11/12, ideally 12/12;
- episode-only recorded as a diagnostic control;
- all 12 MemPhant packets contain the current preference with reserved role
  labels and do not expose the outdated preference as active;
- no parse, pricing, retry, ID, trace, hash, or provenance failure;
- every previously passed development case remains green.

### 4. Freeze a new candidate and a new confirmation pack

Why: both prior Valid Selection confirmation packs are exposed or invalid:

- original confirmation failed the RawDialogue ceiling on `notes`;
- confirmation-v2 was opened and MemPhant failed after 10 results.

Do not rerun either pack as sealed confirmation. Add `confirmation_v3` with six
new domains and polarity/order twins that are disjoint from development,
confirmation, confirmation-v2, and the official 350 rows. Freeze it before any
candidate run.

```sh
python3 scripts/generate_memsyco_valid_selection_calibration.py

python3 scripts/audit_memsyco_calibration_overlap.py \
  --official "$OFFICIAL/data/valid_memory_selection.jsonl" \
  --calibration "$PWD/benchmarks/memsyco/valid_selection_calibration/confirmation_v3.jsonl"

python3 -m pytest tests/test_restraint_benchmark_contract.py -q
```

The overlap audit must report zero exact, suspicious, and normalized five-gram
overlaps. Record case/oracle hashes before the first confirmation call.

Create a new candidate freeze containing exact hashes for:

- Rust structured-state source and focused test binary;
- extraction prompt;
- MemSyco adapter and arbitration contract;
- runner, shared provider meter, and harness bootstrap;
- development and confirmation-v3 inputs plus separate oracles;
- packaged server, worker, and CLI;
- Python 3.10 and resolved requirements;
- DeepSeek model, provider policy, BGE-M3 profile, top-k, and official lock.

### 5. Open confirmation-v3 exactly once

Run RawDialogue first. If it is not 12/12 with zero contamination, repair or
replace the ambiguous fixture without changing MemPhant. Once the ceiling is
valid, run full MemPhant and episode-only sequentially in fresh directories.

Apply the same development gates and the Valid Selection packet proof. If the
candidate changes after any confirmation failure, that pack joins development;
freeze a wholly independent replacement pack before another confirmation.

### 6. Predeclare and run official Valid Memory Selection

Why: this is the only currently untouched official track whose calibrated
subsystem is close to a candidate. Do not open it until steps 1–5 pass.

Before any official call:

- fetch the current pinned official leaderboard data and write an
  `OFFICIAL-GATE.json` with the comparison points and confidence gates;
- verify the official source/dataset hashes;
- verify the candidate freeze and `SHA256SUMS`;
- use 14 immutable shards of 25 samples for 350 MemPhant results and 350
  same-model RawDialogue controls;
- use `--no-completion-cache` through the existing official command path;
- preserve every started, result, error, decode, answer, and judge row.

Example shard 0 changes only these arguments from the calibration command:

```text
--test-jsonl omitted
--offset 0
--limit 25
--out-dir .../valid-selection/official/memphant/shard-00
```

Then use offsets `25, 50, ..., 325`, each with a new directory and port. Run
RawDialogue paired shards with the identical model/judge settings. An
infrastructure-only repair gets a new shard directory; a low score or product
failure is never retried.

If any official MemPhant shard fails product extraction, parsing, provenance,
or scoring, stop the track, preserve aggregates and ledgers, do not inspect
official row contents, write an official retirement record, and move to the
untouched Personalized Memory Use track.

If all 350 complete, report the five official task metrics separately. Never
invent an aggregate scalar across MemSyco tasks. Use 10,000 paired bootstrap
resamples for the predeclared MemPhant-versus-RawDialogue accuracy and
sycophancy deltas. Describe a win as reproduced task-specific SOTA against the
pinned official table, not an accepted leaderboard submission.

### 7. Continue to Personalized Memory Use

Whether Valid Selection earns or retires its official gate, use the same loop
for the untouched `personalized_memory_use` task: non-official development,
fresh sealed confirmation, candidate freeze, then the 300-row official track
with RawDialogue control. Do not launch a 1,550-row suite until every task has
independently earned its gate.

## Operational tips and traps

- **Fresh means empty.** The runner rejects an existing artifact directory.
  Every retry, repair, or diagnostic needs a new path.
- **Do not resend a paid completion.** A generation-stat reconciliation failure
  is a terminal ledger error. Preserve response evidence and stop.
- **Memory arms are sequential.** Concurrent scratch migrations caused
  PostgreSQL `tuple concurrently updated`; controls without memory can run in
  parallel only when their artifacts and rate limits are independent.
- **Use the absolute prompt path.** Omitting
  `MEMPHANT_STRUCTURED_STATE_PROMPT_PATH` causes the server to fail because the
  extractor prompt is deliberately required and hash-bound.
- **Do not print credentials.** Wrap provider calls with Doppler; never put the
  API key in arguments, logs, run manifests, or shell history.
- **The official tree is not Git.** Verify it through the lock and source
  hashes, not `git status`.
- **`run.json` is the execution identity.** Compare its slice,
  implementation, binary, model, embedding, and top-k hashes before reusing any
  control.
- **`attempts.json` is answer/judge provenance.** Structured MemPhant also has
  `extractor-attempts.jsonl`. A successful report without complete ledgers is a
  failed proof.
- **Do not inspect retired official rows.** Aggregate counts, error classes,
  costs, and provenance are allowed. Row content must not become development
  data.
- **Do not game the controls.** RawDialogue failure means the fixture may be
  ambiguous; it is not permission to add a benchmark-specific MemPhant answer
  rule.
- **Keep product invariants shared.** Fix parser, grounding, state identity,
  retrieval, typed context, or arbitration at the reusable boundary. Do not
  special-case a MemSyco sample ID, domain, answer token, or rubric phrase.
- **Preserve the dirty tree.** The new files in this campaign are untracked in
  Git and coexist with unrelated owner changes. Check exact paths before every
  edit.
- **Cutover remains paused.** Pending cutover design is documented in
  `.superpowers/sdd/citation-record-projection-report.md`; it is not part of
  this SOTA campaign.

## Verification commands

Use narrow checks while iterating:

```sh
python3 -m pytest tests/test_restraint_benchmark_contract.py -q
cargo test -p memphant-runtime structured_state_openrouter --lib
python3 -m py_compile \
  scripts/run_restraint_bench.py \
  scripts/audit_memsyco_calibration_overlap.py \
  scripts/verify_memsyco_calibration_packets.py \
  scripts/generate_memsyco_evidence_calibration.py \
  scripts/generate_memsyco_scope_calibration.py \
  scripts/generate_memsyco_objective_calibration.py \
  scripts/generate_memsyco_valid_selection_calibration.py \
  benchmarks/memsyco/memphant_baseline.py
python3 scripts/check_spec_drift.py
git diff --check
```

Before any completion or SOTA claim, run the complete repository gate from
`AGENTS.md`:

```sh
python3 -m pytest tests/ spikes/python-retain/test_spike.py -q
python3 scripts/check_spec_drift.py
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
cargo test --doc
bash scripts/with_scratch_db.sh \
  postgres://memphant:memphant@localhost:5432/memphant \
  MEMPHANT_TEST_DATABASE_URL \
  cargo test -p memphant-store-postgres -p memphant-worker -- \
    --ignored --test-threads=1
cargo run -p memphant-cli -- db lint --provider plain-postgres
cargo run -p memphant-cli -- db lint --provider supabase
cargo run -p memphant-cli -- db lint --provider neon
python3 scripts/apply_memphant_migrations.py \
  --database-url postgres://memphant.invalid/memphant --dry-run
DATABASE_URL=postgres://memphant:memphant@localhost:5432/memphant \
  bash scripts/e2e_probe.sh
git diff --check
```

After artifact creation, generate and verify a manifest from the artifact root:

```sh
find "$RUN_DIR" -type f ! -name SHA256SUMS -print0 \
  | sort -z \
  | xargs -0 shasum -a 256 \
  > "$RUN_DIR/SHA256SUMS"
(cd "$RUN_DIR" && shasum -a 256 -c SHA256SUMS)
```

Ensure the paths in `SHA256SUMS` are relative before treating this as a
portable archive. If the command emits absolute paths, regenerate from inside
`$RUN_DIR`:

```sh
(cd "$RUN_DIR" && \
  find . -type f ! -name SHA256SUMS -print0 | sort -z | \
  xargs -0 shasum -a 256 > SHA256SUMS && \
  shasum -a 256 -c SHA256SUMS)
```

When the campaign is stopped and scratch databases are gone, restore the
observed initial database state:

```sh
docker compose stop postgres
docker compose ps postgres
```

## References

- Plan of record:
  `docs/superpowers/plans/2026-07-15-memsyco-evidence-arbitration-sota.md`
- Canonical broad handoff: `docs/handoff/NEXT-SESSION-PROMPT.md`
- Live status ledger: `docs/superpowers/specs/memphant/STATUS.md`
- Official lock and source hashes: `benchmarks/manifests/memsyco.lock.json`
- Official repository: <https://github.com/XMUDeepLIT/MemSyco-Bench>
- Official paper: <https://arxiv.org/abs/2607.01071>
- Official leaderboard: <https://xmudeeplit.github.io/MemSyco-Bench-Leaderboard/>
- Shared paid-attempt meter: `scripts/provider_attempts.py`
- Harness bootstrap: `benchmarks/memsyco/harness_bootstrap.py`
- MemSyco adapter: `benchmarks/memsyco/memphant_baseline.py`
- Runner/verifier: `scripts/run_restraint_bench.py`
- Structured extractor: `crates/memphant-runtime/src/structured_state_openrouter.rs`
- Extractor prompt: `config/structured-state-v1.txt`
- Benchmark contracts: `tests/test_restraint_benchmark_contract.py`
- Feature Flow state:
  `.codex/feature-flow/019f6387-9114-7203-b04a-a5393ab3ff48.json`
- Paused cutover report:
  `.superpowers/sdd/citation-record-projection-report.md`

## Copy-paste resume prompt

```text
Resume the MemPhant MemSyco evidence-arbitration campaign from
docs/handoff/2026-07-16-memsyco-evidence-arbitration-handoff.md.

Do not touch cutover, STATUS, commits, pushes, rebases, or unrelated dirty
files. The current clean official lane is Valid Memory Selection; no official
calls have been made for it. Diagnose the non-official memphant-v6 case-10
duplicate_state_identity + missing_preference_operation failure by adding
non-secret rejection diagnostics and a focused Rust regression. Fix the shared
root cause without weakening generic collision rejection, prove the exact case
in a fresh directory, requalify all 12 development cases and controls, create a
wholly independent confirmation_v3, audit/freeze/run it once, and only then
predeclare and open the 350-row official track in immutable 25-row shards.
Retired evidence-conflict, scope, and objective official rows must not be
inspected, retried, or used for tuning.
```

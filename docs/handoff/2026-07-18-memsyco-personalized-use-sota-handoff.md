# MemSyco Personalized Memory Use SOTA handoff

Current STATUS mirror: RUNTIME COMPLETE — BENCHMARK EVIDENCE PENDING

Date: 2026-07-18  
Repository: `/Users/sidsharma/Memphant`  
Branch: `codex/memphant-canonical-cutover`  
HEAD at the latest candidate freeze: `d57de74cc13b2c326387f1c22fb16261c21da4c8`  
Campaign root: `docs/build-log/artifacts/unified-sota-20260714/memsyco-evidence-sota-20260715T172416Z/personalized-use/future-v2`  
Plan of record: `docs/superpowers/plans/2026-07-16-memsyco-personalized-use-sota.md`  
Feature-flow record: `.codex/feature-flow/019f6b7e-16a0-7600-97e8-0f1253c3a8cc.json`

## Final canonical state — 2026-07-19

The reusable 300-row Personalized Memory Use campaign is complete. Do not
resume any campaign controller and do not rerun any completed row.

- RawDialogue is sealed at 300/300 rows: 257/300 answer accuracy, 291/300
  preference use, and 257/300 memory-use pass.
- MemPhant is sealed at 300/300 canonical rows: 298/300 answer accuracy,
  300/300 preference use, and 298/300 memory-use pass.
- The MemPhant recovery boundary was **offset 147, never offset zero**.
  Offsets 0–146 were not recalled. Recovery-2's `RECOVERY.json` records
  `resume_offset: 147`, `completed_row_recall: false`, and
  `offset_146_recall: false`.
- Original offset 146 is preserved as infrastructure-only partial evidence.
  Its exact isolated replay is the disclosed canonical scored substitute; the
  original attempt is included in total spend but not in the score.
- Every canonical source UID is unique, gap-free, and in exact source order.
  All 1,500 canonical paid response IDs are unique. The complete spend ledger,
  including the original offset-146 infrastructure attempt and its replay, has
  1,503 unique response IDs.
- The exact 10,000-resample paired bootstrap with seed `20260716` clears every
  frozen quality threshold. MemPhant-minus-RawDialogue lower 95% bounds are
  +0.1000 for accuracy and +0.01333 for preference use.
- A real-server latency audit across offsets 147–299 measured p95 70.61 ms and
  maximum 117.24 ms against the 1,500 ms ceiling.
- The two frozen misses, offsets 159 and 281, had complete extraction and
  retrieval but selected only one of three active preferences during answer
  composition. One shared complete-set coverage fix is now regression-tested;
  both misses and two adjacent controls pass 4/4 accuracy, preference use, and
  memory use in the focused paid proof.
- A second full exposed-bank run is not required: the immutable v12 campaign
  already clears every frozen gate, and the post-campaign mechanism fix is
  independently proven on both misses plus controls.
- Campaign-specific verification is green. The repository-wide gate remains
  red on unrelated pre-existing dirty-tree drift (five Python contract
  failures, six Clippy findings, three stale `ReflectInput` test fixtures, and
  an E2E retain-response mismatch); exact predicates are preserved in
  `POSTFLIGHT-VERIFICATION.json` and were not patched as part of this campaign.

Primary evidence:

- `future-v2/shadow-v1/runs/candidate-d3c4475f-v12-aggregate/FULL300-SCORECARD.json`
- `future-v2/shadow-v1/runs/candidate-d3c4475f-v12-aggregate/LATENCY-AUDIT.json`
- `future-v2/shadow-v1/runs/candidate-d3c4475f-v12-aggregate/INFRASTRUCTURE-AMENDMENT-v2.json`
- `future-v2/shadow-v1/runs/candidate-d3c4475f-v12-aggregate/POSTFLIGHT-VERIFICATION.json`
- `future-v2/shadow-v1/research/20260719T033000Z/answer-set-coverage-proof/RESEARCH-ESCALATION.json`

Truthful postable statement:

> Reusable full-scale development evidence clears the pinned PMU SOTA
> thresholds.

This exposed reusable development bank does not support an official rank,
accepted leaderboard placement, untouched-holdout claim, official
task-specific SOTA, global SOTA, or suite-wide SOTA.

Everything below this point is the preserved pre-completion campaign record.
Where it describes a resume action or incomplete count, the final canonical
state above supersedes it.

## Historical pre-campaign resume prompt (superseded)

Continue the MemSyco Personalized Memory Use campaign from this handoff. Read
this file completely, then inspect the current filesystem and process state
before trusting any command or count. Do not rerun any completed row. Preserve
all retired and partial evidence. Confirmation-v12 RawDialogue is sealed green
and must not be recalled. MemPhant offsets 0–10 are also complete and immutable.
Resume the opened MemPhant arm at offset 11 in a new recovery root, then run
episode-only sequentially. If confirmation passes, seal
the candidate and run one memory-efficient, restart-safe full-300 campaign.
Score the pinned thresholds and paired bootstrap. If a product mechanism fails,
retire the candidate, diagnose the trace stage, run the rolling 90-day research
loop, test one falsifiable lever on exposed evidence, requalify on full
development plus a wholly independent confirmation pack, then start a new full
campaign. Never claim official or global SOTA from exposed shadow data.

## Historical pre-campaign executive state (superseded)

We are not at SOTA and no full-300 candidate is currently eligible for a claim.
The latest product candidate is green on complete development and is frozen for
confirmation-v12. RawDialogue is sealed green; the opened MemPhant arm is
paused after eleven green rows and one infrastructure-only partial attempt.

Current sealed pack:

- Candidate freeze:
  `future-v2/CANDIDATE-FREEZE-v11-confirmation-v12.json`
- Candidate-freeze SHA-256:
  `914301971583f6be4370040573554a9ff683a152d234cc599c45f469a583067c`
- Confirmation open:
  `future-v2/confirmation-v12/CONFIRMATION-OPEN.json`
- Confirmation-open SHA-256:
  `3b06ceebe65eb2e2451af4a6336ad002eef9dd003ea2ce503da7fab3080e7d10`
- Confirmation cases SHA-256:
  `fb66548e9e907ff8a824f67f6068eb937ba2623f032543efc7efce4e43e6a876`
- Confirmation oracle SHA-256:
  `798efa0afd39dc9f0ddd4d7a8572c167829cc1324946181c9f98b668361579a3`
- Pause proof:
  `future-v2/confirmation-v12/PAUSED.json`

Confirmation-v12 execution state:

- RawDialogue offsets 0–11 have complete reports and pass current
  `verify-results`: 12/12 answer accuracy, 12/12 preference use, and 12/12
  memory-use pass. There are 24 unique response IDs, zero product retries,
  zero judge/parser failures, and two preserved infrastructure-only partial
  calls from earlier recovery boundaries.
- `RAW-DIALOGUE-GATE.json` is sealed with SHA-256
  `8dd3c25404512650274a5f57a2181c79da5f4219444bef8d6ce62ecf3ee0a6b4`.
  None of the 12 RawDialogue rows may be recalled.
- MemPhant was opened only after that gate passed. The first local-only base
  attempt is preserved, and `memphant-recovery-1-rowwise/RECOVERY.json` binds
  it. Recovery-1 completed offsets 0–6, recovery-2 completed offset 7, and
  recovery-3 completed offsets 8–10. Together, offsets 0–10 are 11/11 for
  accuracy, preference use, and memory use, with eleven extractor, eleven
  answer, and eleven judge calls, 33 unique response IDs, zero retries, and no judge/parser
  errors. These rows must not be recalled.
- Recovery-2's earlier offset-8 attempt was stopped after one valid extractor and one valid
  answer result, while the judge call was started but had no parsed result. It
  remains infrastructure-only partial evidence. Recovery-3 binds that partial
  and produced the unique complete offset-8 report; preserve the partial calls
  in total cost but never score them.
- Recovery-3 offset 11 was stopped after one valid extractor and one valid
  answer result while its judge call had started but produced no parsed result.
  A pre-scheduled recovery-4 then stopped during a fresh extractor request
  before any result. Both have no report and are infrastructure-only partial
  evidence.
- Resume MemPhant at offset 11 in a fresh `memphant-recovery-5-rowwise` root
  whose `RECOVERY.json` binds recovery-4 and both partial offset-11 attempts.
- Episode-only has not been opened for v12.

Stopped state at handoff:

- No PMU controller, harness, server, worker, or named tmux session remains.
- PostgreSQL is stopped, matching the state observed before this pause.
- No full-300 process is running.
- The three frozen binaries still live under `/tmp/memphant-pmu-target/debug`.

Do not infer “not running” from one `pgrep`. The stop sequence exposed delayed
Codex app-server children which relaunched recovery roots after the first
controller was killed. Check process trees and tmux twice, several seconds
apart.

## Success contract and truthful claim boundary

The PMU full-300 gate is:

- Answer accuracy point estimate at least 63.34%.
- Answer accuracy bootstrap lower 95% bound strictly above 60.34%.
- Correct preference use point estimate at least 82.33%.
- Correct preference use bootstrap lower 95% bound strictly above 79.33%.
- Paired `MemPhant - RawDialogue` lower 95% bound strictly above zero for both
  metrics.
- 10,000 deterministic paired percentile-bootstrap resamples with seed
  `20260716`.
- Zero hidden API, parser, judge, provenance, duplicate-response-ID, or
  completed-row-recall failures.
- Synchronous recall p95 at most 1.5 seconds remains a product gate.

The current 300-row `shadow_v1b` bank is reusable full-scale development
pressure. It is not an untouched official holdout and is not claim-eligible.
Clearing its gates can support only:

> Reusable full-scale development evidence clears the pinned PMU SOTA
> thresholds.

It cannot support “official #1,” accepted leaderboard rank, untouched-holdout
SOTA, official task-specific SOTA, global SOTA, or suite-wide SOTA. A truthful
task-specific SOTA claim now requires a new independent benchmark version or
holdout with the protocol frozen before it is opened.

## What has been implemented and verified

### Benchmark and evidence contracts

- Pinned MemSyco revision
  `c31e2c85ee8cc3c6f643587b8a6f4b5ad5eb3bf6`, schema 1.2, and 300 PMU rows.
- Added PMU development and confirmation generators with polarity twins,
  immutable hashes, and pairwise topic disjointness.
- Extended overlap auditing so exact hashes, suspicious matches, and normalized
  five-gram overlaps must all be zero.
- Extended packet verification to require the current preference as active
  structured personalization with `epistemic_use=not_factual_evidence`, while
  forbidding objectively rejected experience values from active
  personalization.
- Added task-aware PMU scoring and deterministic paired bootstrap to the
  existing restraint runner. No second benchmark harness was introduced.
- Added provider attempt ledgers, extractor ledgers, response-ID uniqueness,
  binary hashes, source hashes, recovery bindings, and row-durable artifacts.
- Added a rowwise controller with one lane, no completion cache, verify-before-
  advance behavior, descendant cleanup on `INT`/`TERM`, and Cargo environment
  propagation inside Doppler.

### Product levers retained after measured failures

All retained levers are deterministic, grounded, and narrow. No graph memory,
learned gate, decay system, retrieval rewrite, second judge, or new public API
was added.

1. Objective outcome overrides positive wording for rejected experiences.
   A positively described attempt that “did not work,” stopped early, and would
   not be chosen again cannot become active personalization.
2. Grounded explicit preference normalization reuses the exact quoted value and
   reserved personalization role instead of trusting model-shaped fields.
3. Duplicate identity arbitration prefers the one grounded reserved
   personalization operation; two generic collisions remain fail-closed and
   exact duplicates remain idempotent.
4. Ungrounded quantity candidates are discarded only when their numeric value
   is absent from the quote and the same quote already yielded admitted typed
   personalization. Grounded malformed quantities still fail closed.
5. A single clean unscoped `I prefer ...` quote is rebuilt into deterministic
   `user_preferences/unscoped_<value hash>` with an empty applicability scope.
   Multiple unscoped candidates remain model-owned and ambiguous.
6. Answer-time packets aggregate the full typed personalization set so an
   incomplete episodic excerpt cannot hide one of several active preferences.
7. Provider provenance hardening captures response IDs from the supported
   generation header when the successful body omits one, retains strict usage
   reconciliation, and uses the qualified provider policy rather than an
   unbounded fallback pool.

### Latest development qualification

`future-v2/development-v17/QUALIFICATION.json` is the current complete
development proof:

- MemPhant: 12/12 answer accuracy, 12/12 preference use, 12/12 memory use.
- MemPhant packet proofs: 12/12.
- Extractor, answer, and judge calls: 12 each.
- Retries and rejected operations: zero.
- Episode-only: 12/12 on all three metrics, diagnostic only.
- RawDialogue: exact reuse of development-v13, 12/12 on all three metrics,
  current verification green, 24 unique response IDs, zero retries.
- Candidate decoder SHA-256:
  `6ecfd8509c2204deeb9ac717364e469222f785064bc7feb0332158120cb7f9eb`.
- Runner SHA-256:
  `f1c5f42d3b8556a7d0d85b2912e5d3819607af94ec33339e35b47eef3deee585`.
- Server, worker, CLI SHA-256:
  `6da217e337936cd873a6fa33616069fa627aef7ac4984afbc037da08f3e7da26`,
  `5c566686fe842f7b1349a687fdaf87400be12b58c3eb15c272d702a1260ace39`,
  `5c90e7804e84335882c0350a5c4cf19710892ac9257ef94fc32c30e245c6b53e`.

The focused v12 generator, disjointness, controller cleanup, environment,
Python compilation, and shell-syntax contracts were green when the v12
candidate froze. Feature-flow records `understand`, `plan`, `review-plan`,
`implement`, `qa`, and `verify` complete, with the last recorded tests at
`2026-07-18T10:07:25.158267Z`. Code review and learn are not complete because
the campaign itself is not complete.

## Full-300 evidence and why it did not reach SOTA

### RawDialogue control is already reusable

The exact `shadow_v1b` RawDialogue control under
`shadow-v1/runs/candidate-68a9477d6e74052a-v1b` has:

- 300 rows.
- 257/300 answer accuracy.
- 291/300 correct preference use.
- 257/300 memory-use pass.
- 600 unique answer/judge response IDs.
- Zero retries, parser failures, judge errors, or API failures.
- Source SHA-256:
  `6abfb459370737a143b125b2acaddd3778041bd460fad964491dddfd647dd638`.

Do not pay to rerun this arm unless the next full-run gate proves source,
request, model, provider policy, judge, configuration, and ledger hashes are not
identical. Reuse must be proven, not assumed.

### Previous MemPhant full-scale attempts are retired

The best prior full-scale candidate, v7, produced 131 gap-free green rows at
offsets 0–130. At offset 131
`breadmaking_class_breaks-2`, structured extraction and retrieval were correct,
but the answer used only two of three active preferences. The valid judge result
was accuracy 0, preference use 1, memory-use pass false. The candidate was
retired without retry. Offset 132 was only a partial extractor/answer attempt
and remains unscored.

Those 131 rows are useful development evidence but cannot be spliced into the
new candidate. The answer-packet aggregation lever cleared the exposed failure
and controls, then later qualification passed, but any adapter change requires a
fresh candidate and fresh MemPhant 300.

### Full-300 failure ledger, fixes, and reuse rules

Every row below came from the exposed `shadow_v1b` 300-case bank. Preserve the
paid artifacts, but never splice completed MemPhant rows across candidates.

| Candidate / boundary | Preserved result | Root cause | Retained fix and evidence | Reuse rule |
| --- | --- | --- | --- | --- |
| `68a947...`, offsets 4, 8, 34 | 26 scored rows total; offset 4 scored `accuracy=0`, `preference_used=0`; offsets 8/34 aborted before answer | The answer promoted a rejected intensive pace, and the decoder trusted model-shaped preference values instead of exact grounded spans. The same run also had binary drift and unsafe concurrent scratch/cache incidents. | Objective outcome now overrides positive wording. Explicit preference values are normalized from exact quotes and reused for value/role/epistemic fields. Development and independent confirmation later cleared. | RawDialogue 300 remains reusable if identity is proven. MemPhant rows are diagnostics only. See `shadow-v1/MEMPHANT-CANDIDATE-68A947-RETIREMENT.json`. |
| `9d3aad...`, offset 8 | Eight green rows, then HTTP 200 extractor result with no admissible response ID; no answer/judge calls | Provider-generation provenance was incomplete, so accepting the row would make cost/model/provider evidence unverifiable. | Capture the supported generation response header, retain bounded generation-record reconciliation, and fail closed if neither yields a unique ID. Provider-policy experiments were separately qualified. | Candidate retired; no retry. See `shadow-v1/CANDIDATE-9d3aad-SEQUENTIAL-RETIRED.json` and `research/20260717T185101Z`. |
| `865525...`, offset 5 | Five green rows, then paid HTTP 200 with zero completion tokens from AkashML; no answer/judge calls | A fallback provider violated the successful extractor completion contract. | Research compared live endpoint contracts and replaced failing fallbacks with an ordered exact-contract pool; later qualification also hardened no-fallback/served-provider evidence. | Candidate retired; empty success is a product failure, not a transport retry. See `shadow-v1/CANDIDATE-865525-SEQUENTIAL-RETIRED.json` and `shadow-v1/research/20260717T210523Z`. |
| `13dfa...`, offset 165 | 165 green rows, then one extractor result produced three valid preferences plus three rejected `quantity_occurred_at` operations; zero answer/judge calls | The model hallucinated numeric auxiliary state although the evidence contained no digit. Generic fail-closed rejection suppressed valid personalization. | A quantity candidate is dropped only when its number is absent from the exact quote and that quote already yielded admitted typed personalization. Grounded malformed quantities still fail closed. Decoder regression, exposed offset-165 micro-eval, full development, and sealed confirmation were required. | Candidate retired; 165 rows remain development evidence. See `runs/candidate-13dfa478d18d7e98/.../CANDIDATE-RETIREMENT-offset-165.json` and `shadow-v1/research/20260718T024654Z`. |
| v6, offset 220 | 220 complete reports, then decode rejected one duplicate identity; zero answer/judge calls | A grounded reserved personalization create and a generic create collided on one exact identity. | Deterministic mixed-role arbitration keeps the grounded reserved operation; two generic collisions remain fail-closed, exact duplicates idempotent, and two preferences use grounded order. The exposed offset-220 proof passed. | Candidate retired; rows cannot transfer. See `shadow-v1/research/20260718T094323Z` and `diagnostic-postfix-offset220-v7/QUALIFICATION.json`. |
| v7, offset 131 | 131 gap-free green rows; valid judge result at offset 131 was `accuracy=0`, `preference_used=1`; offset 132 is partial and unscored | Extraction, active state, and retrieval all contained three preferences, but the answer copied only the two values visible in a truncated episodic excerpt. | The adapter now emits a count-bearing complete active-personalization block before retrieved evidence and tells the answer model that provenance excerpts cannot narrow it. The failing row plus controls passed, followed by development-v17. | v7 is retired and cannot resume. See `shadow-v1/CANDIDATE-v7-FULL300-RETIREMENT.json` and `shadow-v1/research/20260718T150532Z`. |

Other preserved provenance failures include generation metadata remaining 404
beyond the bounded reconciliation window. Correct active values were also once
stored under an invented domain identity during sealed confirmation; that was
not a 300-row failure, but it motivated deterministic unscoped identity
rebuilding described below.

These were not retried as low-quality rows. Each was preserved, diagnosed at
the failing stage, tested on exposed evidence, and followed by full development
plus a new independent confirmation pack.

## Retired confirmation packs and lessons

- Confirmation-v9: RawDialogue 12/12; MemPhant offsets 0–2 green; offset 3
  failed structured extraction with `missing_preference_operation`. This led to
  grounded unscoped-preference normalization.
- Confirmation-v10: RawDialogue 12/12; MemPhant offsets 0–4 green; offset 5
  stored the correct `a soft sleeve` value under an invented phone-case-like
  identity, causing the answer to reject eyeglass-case applicability. This led
  to deterministic unscoped identity rebuilding. The exposed polarity twins
  passed 2/2 with packet proof.
- Development-v16 accidentally used the older `development.jsonl` instead of
  `development_v2.jsonl`. It scored 12/12 but is explicitly ineligible. The
  correct development-v17 was then run and sealed.
- Confirmation-v11: RawDialogue 12/12; MemPhant offset 0 was green, but the run
  used shared-target binaries instead of the candidate-freeze binaries because
  Cargo settings were not preserved through the Doppler boundary. The pack was
  retired for configuration provenance, not product quality. The controller
  now routes `CARGO_TARGET_DIR`, `CARGO_BUILD_JOBS`, and
  `CARGO_INCREMENTAL` inside `doppler run`; contract tests and a real path
  readback prove `/tmp/memphant-pmu-target` is used.

Never repair or reopen any retired pack. RawDialogue results from retired packs
remain useful development evidence but cannot promote a candidate.

## Operational mistakes and pitfalls

### Parallelism and memory

Eight-lane and concurrent scratch attempts caused cache, database, and memory
pressure. They were abandoned. The user explicitly selected one lane and
memory efficiency over speed. Keep:

- `CARGO_BUILD_JOBS=1`
- `CARGO_INCREMENTAL=0`
- one campaign lane
- no concurrent arms
- no completion cache
- explicit database drop after each memory arm
- process-tree cleanup after interruption

Rowwise durability is not a statistical slice. It is one declared full campaign
whose completed rows become immutable immediately, allowing infrastructure
recovery without paying for completed rows again.

### Hidden detached runners

During this pause, foreground controllers, delayed Codex app-server children,
and a macOS launch agent were all able to outlive an apparent stop. Killing the
first process and tmux session did not prove the campaign stopped. The launch
agents `com.memphant.pmu.conf12.offset8` and
`com.memphant.pmu.conf12.offset8r3` later started recoveries 2 and 3. A final
inert `com.memphant.pmu.conf12.offset9` label restarted PostgreSQL but exited
without creating recovery-4; a later inert `offset10` label did the same. All
labels were explicitly removed, every remaining campaign-related Codex app-
server child was terminated. Pre-scheduled offset-11 and offset-11r4 launches
were then stopped and removed; they left only the hash-bound partials described
above.
Recovery-3 completed offset 8 before shutdown; the earlier recovery-2 offset-8
partial remains hash-bound in `PAUSED.json`.

Before claiming stopped or starting a new controller:

```sh
ps -axo pid=,ppid=,pgid=,command= \
  | rg 'confirmation-v12|memphant_pmu_conf12|run-pmu-qualification-rowwise|run_restraint_bench|harness_bootstrap'
tmux list-sessions 2>/dev/null || true
launchctl list | rg 'memphant|pmu|memsyco' || true
sleep 3
# Repeat all checks. A single empty pgrep is insufficient.
```

Terminate the controller process group, not just the top Python process. The
controller traps `INT` and `TERM` and kills descendants. Preserve any
`status=started` ledger before creating a recovery root. Also remove the exact
launch-agent label with `launchctl remove <label>`; otherwise it can recreate a
controller after processes and tmux appear empty. Do not create detached sleep-
and-check monitors through the Codex app-server. One foreground controller is
the simplest reliable supervision for the remaining confirmation rows.

### Canonical artifact path instability

The long artifact tree has previously disappeared from its canonical path while
its inode remained available under `/.vol`. A verified byte-identical stable
mirror was created for the v7 full-300 tree. During this pause, reads briefly
returned inconsistent path availability while stale controllers were replacing
recovery directories. After all matching process groups stopped, the candidate
freeze and confirmation-open hashes became stable and coherent again.

Do not edit or recreate a missing campaign tree immediately. First inspect:

```sh
stat -f '%d %i %m %z %N' <campaign-path>
find /.vol -maxdepth 4 -type d -inum <inode> 2>/dev/null
```

If recovery is necessary, copy to a new stable mirror, compare recursive file
manifests and `diff -rq`, and record a `MIRROR.json`. Never mutate the source
tree or use a mirror to erase incomplete attempts.

### PostgreSQL and scratch state

Shared campaign databases previously accumulated queued/running job debris and
starved workers. Use fresh campaign-specific databases, never the shared
`memphant` database. Classify an existing database before reuse; if it contains
pending debris, drop it and create a new recovery database before the first
incomplete row. At exit, drop all campaign scratch databases and restore
PostgreSQL to the observed initial state. It is stopped at this handoff.

### Per-row stop policy versus aggregate scoring

The qualification controller requires every row to score 1/1 before advancing.
That is correct for 12/12 development and sealed confirmation. The prior
full-300 runs reused it, so a single valid judge miss retired the candidate even
though the published SOTA contract is aggregate. This is stricter than the
63.34%/82.33% success thresholds and prevents a complete aggregate score unless
all 300 rows are perfect.

Do not silently change this after seeing results. Before the next full-300 run,
freeze one of these policies in the full-run gate:

1. Keep stop-on-any-valid-miss as an intentionally stronger restraint gate; or
2. Use a dedicated full-campaign collector where a valid parsed score of zero
   is an immutable completed row that is never retried but does not stop the
   remaining 299 rows. Invalid output, refusal, decode/grounding, judge/parser,
   provenance mismatch, or hidden errors still stop and retire.

The second policy matches the user's later instruction to complete all 300 and
evaluate aggregate thresholds. It must be predeclared and tested before spend;
it is a harness-governance correction, not a memory lever.

## Research completed and its disposition

The campaign searched May–July 2026 papers/proceedings first, then official
repositories and changes, artifact-backed engineering material, and finally
Reddit/Hacker News for hypotheses only.

Evidence that shaped retained work:

- Conflict-resolution work supports separating LLM candidate extraction from
  deterministic freshness and conflict arbitration:
  <https://arxiv.org/abs/2606.01435> and
  <https://github.com/cvikasreddy/memory-conflict-resolution>.
- MemConflict supports measuring extraction, active-state selection, and answer
  utilization separately: <https://arxiv.org/abs/2605.20926>.
- Effective time versus dialogue time is useful only when traces demonstrate
  that distinction: <https://aclanthology.org/2026.findings-acl.1496/>.
- Schema-grounded memory and structural hallucination research supported
  rebuilding ungrounded identities instead of accepting model-invented keys:
  <https://arxiv.org/abs/2604.27906> and
  <https://arxiv.org/abs/2604.20117>.
- Hindsight's effective-time parity change is relevant only to wrong active-
  state ordering: <https://github.com/vectorize-io/hindsight/pull/2197>.
- MemOS's objective-outcome invariant supported letting observed failure
  override positive wording: <https://github.com/MemTensor/MemOS/pull/1807>.
- PM-Bench is prospective suite expansion, not a fix for current PMU traces:
  <https://arxiv.org/abs/2607.12385>.
- HN and Reddit discussion about atomic memory and token bloat generated
  hypotheses only; it was never used as promotion proof:
  <https://news.ycombinator.com/item?id=46742800> and
  <https://www.reddit.com/r/LocalLLaMA/comments/1rsm45d/how_are_people_handling_persistent_memory_for_ai/>.

The research did not justify graph memory, learned gating, decay, trajectory
consolidation, procedural memory, RL, or a retrieval rewrite. Every observed
failure so far occurred at a narrower extraction, canonicalization, provenance,
or answer-packet seam.

Primary machine-readable research records, relative to `future-v2`, are:

- `research/20260717T154402Z` and `research/20260717T161413Z`: objective
  outcome versus positive wording and separation of extraction/state/answer
  gates;
- `research/20260717T185101Z` and `research/20260717T191823Z`: generation
  response-ID provenance and provider pinning;
- `shadow-v1/research/20260717T210523Z` and `20260717T221307Z`: fallback
  provider contract failures and the ordered exact-contract pool;
- `shadow-v1/research/20260718T024654Z`: hallucinated quantity admission;
- `shadow-v1/research/20260718T094323Z`: mixed-role duplicate identity at
  full-300 offset 220;
- `shadow-v1/research/20260718T150532Z`: incomplete answer utilization at
  full-300 offset 131;
- `confirmation-v9/research/20260718T161845Z`: grounded unscoped explicit
  preference recovery;
- `confirmation-v10/research/20260718T173348Z` and `20260718T173954Z`:
  deterministic unscoped identity rebuilding.

Read the JSON cards, not just this summary, before proposing another lever.
They contain the tested falsifiers, losing alternatives, source dates/commits,
license/maturity notes, token/cost effects, and deletion rules. Do not re-add a
previously falsified prompt-only or learned-gate alternative without a new
trace that invalidates the earlier disposition.

## Historical required blocker and research loop (superseded)

Use this exact default definition of “stuck”:

- Two focused attempts against the same diagnosed mechanism still fail.
- Any complete development or sealed-confirmation quality gate misses.
- A full-300 track retires for a product-quality failure.

Do not trigger research for provider outages, 408/429/5xx transport failures,
scratch-database contention, process kills, missing hashes, runner failures, or
other infrastructure incidents. A RawDialogue fixture ceiling first retires the
pack and adjudicates the fixture; research triggers only if the same ambiguity
recurs across two independent packs.

For each escalation:

1. Preserve the exact failure artifact and stop. Do not retry a product result.
2. Diagnose the first failing stage: candidate generation, decode/grounding,
   canonicalization, active-state ordering, retrieval admission, packet
   construction, answer utilization, judge/parser, or provenance.
3. Search the rolling latest 90 days, currently May–July 2026, in this order:
   papers/proceedings; official repositories/releases/PRs; artifact-backed
   engineering blogs; Reddit/HN.
4. Write
   `$CAMPAIGN/<track>/research/<UTC>/RESEARCH-ESCALATION.json` before code.
5. Record at most three falsifiable technique cards, at most one labeled
   `first_principles`. Each card must include source, date, commit, license,
   maturity, failing stage, mechanism, integration seam, predicted metric,
   accuracy benefit, token/cost effect, risk, falsifier, smallest micro-eval,
   and deletion rule.
6. Route by trace:
   - extraction/canonicalization failure: grounded channel normalization;
   - correct candidate but wrong active state: deterministic effective-time or
     serial ordering;
   - current candidate absent before rerank: source-balanced admission;
   - packet correct but answer wrong: typed current-only or complete typed-set
     packet intervention before learned gates;
   - experience promotion error: objective outcome overrides positive wording.
7. Test one lever at a time on exposed, non-promotional micro-evals. Use the
   preserved failure plus a polarity twin and regression controls.
8. Combine levers only after two independently positive results and an explicit
   interaction rationale. Use Holm correction when screening multiple
   statistical comparisons.
9. Run full fresh development and a wholly independent sealed confirmation.
   Any code, prompt, adapter, model, or configuration change retires an opened
   pack.
10. Stop at the first lever clearing the gate. Delete losing experimental code
    and flags before candidate freeze; preserve negative-result artifacts.

Papers and compatible open-source implementations are experimental evidence.
Blogs are secondary evidence. Reddit and HN are question generators only.

## Historical exact next steps (superseded)

### 1. Re-read and prove the pause boundary

```sh
cd /Users/sidsharma/Memphant
BASE="$PWD/docs/build-log/artifacts/unified-sota-20260714/memsyco-evidence-sota-20260715T172416Z/personalized-use/future-v2"

shasum -a 256 \
  "$BASE/CANDIDATE-FREEZE-v11-confirmation-v12.json" \
  "$BASE/confirmation-v12/CONFIRMATION-OPEN.json" \
  "$BASE/confirmation-v12/PAUSED.json"

ps -axo pid=,ppid=,pgid=,command= \
  | rg 'confirmation-v12|memphant_pmu_conf12|run-pmu-qualification-rowwise|run_restraint_bench|harness_bootstrap' \
  || true
tmux list-sessions 2>/dev/null || true
pg_isready -h 127.0.0.1 -p 5432 || true
```

Repeat the process/tmux checks after three seconds. Expected: no campaign
processes, no tmux sessions, PostgreSQL unavailable.

Offline-verify completed RawDialogue offsets 0–11. They span:

- `raw-dialogue-rowwise/offset-0-attempt-1`
- `raw-dialogue-recovery-1-rowwise/offset-{1,2}-attempt-1`
- `raw-dialogue-recovery-2-rowwise/offset-3-attempt-1`
- `raw-dialogue-recovery-3-rowwise/offset-{4..11}-attempt-1`

The incomplete offset-3 and offset-4 attempts in earlier recovery roots are
preserved infrastructure evidence, not scored rows. `RAW-DIALOGUE-GATE.json`
proves the unique complete 0–11 partition. Never recall this arm.

### 2. Resume confirmation-v12 MemPhant at offset 11

Create, with `apply_patch`,
`confirmation-v12/memphant-recovery-5-rowwise/RECOVERY.json` binding:

- `MEMPHANT-OPEN.json`, `RAW-DIALOGUE-GATE.json`, and `PAUSED.json` hashes;
- `memphant-recovery-1-rowwise/RECOVERY.json`,
  `memphant-recovery-2-rowwise/RECOVERY.json`,
  `memphant-recovery-3-rowwise/RECOVERY.json`,
  `memphant-recovery-4-rowwise/RECOVERY.json`, and the hashes for complete
  offsets 0–10;
- both incomplete offset-11 attempts and their ledgers, including the prior
  answer result and started judge call;
- classification `operator_stop_during_extractor_request_before_result` for
  the latest partial;
- `completed_row_recalled=false`;
- `first_recovery_offset=11`.

Start PostgreSQL only when needed, then run one foreground or explicitly named
durable controller, one lane:

```sh
docker compose up -d postgres

env \
  CARGO_TARGET_DIR=/tmp/memphant-pmu-target \
  CARGO_BUILD_JOBS=1 \
  CARGO_INCREMENTAL=0 \
  bash "$BASE/run-pmu-qualification-rowwise.sh" \
    memphant \
    "$PWD/benchmarks/memsyco/personalized_use_calibration/confirmation_v12.jsonl" \
    "$BASE/confirmation-v12/memphant-recovery-5-rowwise" \
    12 1 pmu_conf12_mem_recovery5 11
```

Do not launch a second monitor that can start another recovery automatically.
If the controller is detached, record the exact tmux session and pane PID in the
recovery artifact and stop it through the controller process group.

Require 12/12 official metrics, 12/12 packet proof, exact binary hashes, zero
retries/rejections, unique response IDs, and complete provider/extractor
provenance. Any product failure retires v12 without retry. Infrastructure
recovery uses a fresh suffix root at the first incomplete offset.

### 3. Run episode-only and seal confirmation

Only after MemPhant passes, run episode-only in a fresh root/database as a
diagnostic. Then write `CONFIRMATION-QUALIFICATION-v12.json` binding every root,
report, ledger, proof, response ID, and binary.

Drop confirmation scratch databases and stop PostgreSQL after the memory arms.

### 4. Freeze the full-300 run contract before spend

- Reverify the candidate freeze, confirmation qualification, official lock,
  `shadow_v1b` source/oracle hashes, BGE-M3, model/provider/judge configuration,
  and all `SHA256SUMS`.
- Prove exact reuse eligibility for the existing 300-row RawDialogue control.
- Resolve and freeze the per-row-stop versus complete-aggregate policy described
  above. The user requested a complete 300; the recommended dedicated collector
  records valid zero scores and continues while forbidding retries.
- Write a new full-300 gate and run freeze. Do not reuse the retired v7 gate.

### 5. Run one fresh MemPhant 300

Use one lane, row-durable directories, no completion cache, one scratch
database, exact `/tmp` binaries, and no concurrent arm. Every completed UID is
immutable. Infrastructure recovery begins in a fresh suffix root at the first
incomplete row; no completed UID is recalled. A candidate or configuration
change invalidates the entire new MemPhant arm.

The previous v7 rows cannot be combined with the v11 candidate. The RawDialogue
control may be reused only after the gate proves exact identity.

### 6. Score and decide

Run the existing task-aware scorer with 10,000 paired resamples and seed
`20260716`. Report point estimates, lower bounds, paired deltas, slices,
response-ID uniqueness, complete costs/tokens including partial infrastructure
calls, and recall p95.

If every full-300 development gate is green, state only that reusable full-scale
development evidence clears the pinned thresholds. If a gate misses, preserve
the scorecard, diagnose the weakest stage/slices, trigger research under the
rules above, test one lever, then repeat full development, independent
confirmation, freeze, and a fresh 300.

## Dirty-tree and repository boundaries

This is a very large pre-existing dirty worktree containing campaign work and
unrelated owner work. Do not reset, clean, rebase, commit, push, stage, or
discard anything. Do not opportunistically fix unrelated failures. Do not edit
`docs/superpowers/specs/memphant/STATUS.md` or the paused Syndai cutover.

The preflight inventory under the campaign root is the byte-identity contract.
At final exit, prove every pre-existing path outside the allowlist is unchanged.
Generated OpenAPI/MCP artifacts must never be hand-edited.

Feature-flow verification is not campaign completion. Before any completion or
SOTA statement, run the full `AGENTS.md` repository gate, record unrelated
dirty-tree failures separately, perform code review and the Ponytail deletion
pass, verify portable relative `SHA256SUMS`, drop scratch databases, stop all
runners, and restore PostgreSQL to its observed initial state.

## Historical final operator checklist (superseded)

- [ ] Read this handoff and current plan completely.
- [ ] Verify candidate-freeze/open/pause hashes before any call.
- [ ] Confirm no hidden process, tmux session, or delayed monitor exists.
- [ ] Preserve all 12 sealed RawDialogue rows; never recall that arm.
- [ ] Preserve MemPhant offsets 0–10, bind the partial offset 11, and recover
      from offset 11 only.
- [ ] Finish MemPhant, then run episode-only with frozen binaries.
- [ ] Seal confirmation-v12 or retire it honestly.
- [ ] Prove RawDialogue full-300 reuse exactly.
- [ ] Freeze the complete-aggregate collection policy before the new 300.
- [ ] Run one fresh, memory-efficient MemPhant 300.
- [ ] Score all pinned SOTA gates and paired bounds.
- [ ] If blocked by product quality, use the research loop; do not research
      infrastructure noise.
- [ ] Keep claims scoped to the evidence actually earned.

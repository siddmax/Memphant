# MemPhant campaign handoff — 2026-07-21

Current STATUS mirror: RUNTIME COMPLETE — BENCHMARK EVIDENCE PENDING

This is the authoritative next-session handoff for the active P1-T6 campaign.
Older campaign history remains in git and the linked proof artifacts; do not
reconstruct it from stale prompts.

## Goal

Safely finish the registered MemPhant evidence ladder through the LME-S
full-500 protocol run, while maximizing user-visible accuracy and preserving
the two-speed UX: fast recall by default and explicit Deep exploration for
hard cases. Advance only when each smaller gate passes, never rerun a billable
row, never use paid keys in CI/tests, and research current 2026 primary sources
before changing a provider or benchmark contract.

## Workspace and live state

- Worktree: `/Users/sidsharma/.codex/worktrees/Memphant/p1-deep-mode`
- Branch: `codex/memphant-p1-deep-mode`
- HEAD before this handoff edit: `f97e9a840608e4b58ccff199649105426fbecb43`
- No push is authorized.
- Preserve unrelated work and do not stage the old untracked dump:
  `docs/build-log/artifacts/p1-t6/no-model-exact-29c9eb53/case-bank/7977b0942b90e9dcb8e772bf722ef1cf0b4a3a6f165b279a6fc5e6c23dfbca4a.dump`.

### A live n=12 run is active — do not launch another

Output root:
`docs/build-log/artifacts/p1-t6/run-65981e4f`

As of 2026-07-21 12:39 America/Los_Angeles:

- coordinator PID `30781`, wrapped by Doppler PID `30711`;
- case-1 child PID `32495` under scratch wrapper PID `32486`;
- case `19367bc7` constructed all 670 resources once, processed all 670 jobs,
  and sealed a 177,790,905-byte immutable bank;
- Fast row `0001-19367bc7-fast` is complete and operational;
- Fast reader settled at `2,421` micros, with zero unsettled liability;
- Sonnet row `0002-19367bc7-sonnet` has entered staging and its server has
  started; it has not yet produced a final row proof;
- the controller must stop before case 2 if the Sonnet pair fails any proof or
  settlement predicate.

The process survived the previous Codex-turn interruption. A new session must
inspect this process tree and artifact root first. Never start a replacement
while it is alive. Never rerun the completed Fast row.

Read-only first checks:

```sh
cd /Users/sidsharma/.codex/worktrees/Memphant/p1-deep-mode
ps -axo pid=,ppid=,etime=,command= \
  | rg 'run_lme_v2_p1_t6|memphant-(server|worker)' \
  | rg -v 'rg '
find docs/build-log/artifacts/p1-t6/run-65981e4f -maxdepth 3 -type f \
  -exec stat -f '%N %z %Sm' -t '%Y-%m-%dT%H:%M:%S%z' {} \; | sort | tail -60
git status --short
```

If the process is still active, monitor it; do not mutate the root. If it has
exited, validate the final proofs, settlement ledger, scratch-DB cleanup, and
process cleanup before deciding anything. A failed/interrupted root is
immutable diagnostic evidence: preserve it, write the named invalidation
proof, and do not replay settled rows.

## What we completed

### Product and execution decision

The long-term UX remains a two-speed product:

- `fast` is the default, low-latency path;
- only the existing deterministic `fast -> balanced` insufficiency cascade may
  happen automatically;
- `deep` is an explicit, cancellable user action for hard queries;
- Deep returns the same cited evidence contract, has hard wall-time/tool/token/
  spend caps, and returns an honest cited partial result when capped;
- no second orchestration subsystem or speculative architecture is needed.

Bound execution order:

1. T6 Deep n=12 Fast/Sonnet gate — running now.
2. T6 n≈100-300 paired confirmation — only if n=12 passes and a fresh
   amendment authorizes it.
3. T1 observation-block n=12 and confirmation — improves everyday chat UX.
4. SWE-ContextBench Lite — small coding-memory gate before publicity work.
5. LME-S full-500 — later; this is the internal SOTA-language unlock.
6. LME-V2 paired fast/deep runs and submission — final scale flagship.

Full-500 is not needed now. The current n=12 gate is the efficient next step.

### Doppler and paid-call boundary

Commit `129928d8` updated `AGENTS.md`:

- Syndai is MemPhant's private sister project;
- until MemPhant has its own Doppler project, `syndai/dev` is the canonical
  secret source for explicitly authorized live or paid MemPhant benchmarks;
- always spell out `--project syndai --config dev` because worktrees do not
  inherit directory bindings;
- only the secret-consuming benchmark/diagnostic command may be wrapped;
- CI, unit/integration tests, provider lint, no-model checks, local Postgres
  contracts, and ordinary development are secret-free and cannot spend money;
- shared Doppler does not grant access to a Syndai production database.

A presence-only live check confirmed both `OPENROUTER_API_KEY` and
`OPENAI_API_KEY` exist in `syndai/dev`. Their values were never emitted or
persisted. The current run uses a stripped environment and local ephemeral
PostgreSQL; Doppler wraps only the benchmark process.

### Failures found and durable fixes

1. The first live root exposed a stream identity mismatch. Commit `2e5c9bcd`
   separated exact request identity from canonical SSE identity and made the
   parent fail-stop before opening the next case.
2. Root `run-e511c817` then proved the transport and billing receipts were
   valid, but Sonnet ended `partial/invalid_output` before tool iteration one.
   The controller stopped before case 2 and the root was preserved in commit
   `1a847f94`.
3. A single bounded production-schema diagnostic showed transport, Azure
   routing, usage accounting, SSE fragmentation, and a fragmented single tool
   call were healthy.
4. Root cause: the frozen Azure endpoint does not advertise the optional
   `parallel_tool_calls` parameter under strict `require_parameters=true`, so
   the request correctly omitted it; OpenRouter defaults the omitted parameter
   to parallel calls, while MemPhant accepted only one indexed call.
5. Commit `69ab5a54` fixed the shared runtime root cause. It aggregates
   contiguous indexed calls across SSE fragments, executes them in stable
   provider-index order, returns one matching tool result per call, counts each
   call against the existing 24-tool cap, and keeps route/usage/spend/privacy
   checks fail-closed. This improves Deep latency and cost by avoiding needless
   serial provider turns.
6. ZDR correctly prevents recovering the failed generations' raw SSE chunks.
   We did not weaken privacy to recreate them.

Official references used for that decision:

- <https://openrouter.ai/docs/api/reference/streaming>
- <https://openrouter.ai/docs/api/reference/overview>
- <https://openrouter.ai/docs/cookbook/administration/usage-accounting>
- <https://openrouter.ai/docs/api/reference/parameters>
- <https://openrouter.ai/docs/guides/features/tool-calling>

### Verification completed

After the root fix, all secret-free gates passed:

- Python: 698 passed, 12 skipped;
- P1 harness: 159 passed;
- `cargo fmt --check`;
- Clippy all targets/features with warnings denied;
- Rust all-target/all-feature tests and doc tests;
- provider lint for plain Postgres, Supabase, and Neon;
- migration dry-run;
- ignored live-Postgres contracts on ephemeral PostgreSQL 17;
- real server/worker/CLI e2e probe on ephemeral PostgreSQL 17.

No provider key or paid call was used by those checks. Public/private spec drift
against the available Syndai checkout still reports many pre-existing divergent
files, including STATUS. That is unrelated and does not authorize editing the
private repo or claiming drift-clean.

### Efficient authorization and cost boundary

- Amendment 13: commit `65981e4f`.
- Current dispatch authorization: commit `f97e9a84`, artifact
  `docs/build-log/artifacts/p1-t6/DISPATCH-AUTHORIZATION-65981e4f.json`.
- It authorizes exactly 12 cases, 12 constructions, 24 answer rows, and at most
  12 Deep dispatches. Luna/Sol, n≈100-300, full-500, merge, and push are not
  authorized.
- Preexisting liability: 28,350 settled + 316,142 unresolved upper bound =
  344,492 micros.
- Fresh maximum: 5,697,600 micros.
- Cumulative worst case: 6,042,092 micros under the 6,250,000 ceiling, with
  207,908 micros headroom.

We intentionally did not repeat a 670-resource authorization-only preflight.
The memory adapter is byte-identical and all three context-relevant controller
AST hashes match the exact prior proof; only the post-query Deep parser and
campaign accounting changed. The exact reader proof remains untruncated at
23,564/32,768 tokens. Rebuilding 670 resources would add no context-size
information.

Important terminology: `670` is the resource count in the first selected
LongMemEval case, not 670 benchmark cases. Each distinct live case must replay
its own resources once to create the treatment bank, then Fast and Sonnet share
that bank. Do not run it again merely for tests, CI, or authorization.

## What the next session must do

1. Read `AGENTS.md`, this handoff, Amendment 13, the current dispatch
   authorization, and live `STATUS.md` before acting.
2. Inspect the existing `run-65981e4f` process/root. Do not launch another run.
3. If case 1 finishes, verify both row proofs, exact model/provider receipts,
   tool iterations, settlement, bank-seal equality, and cleanup. If any
   predicate fails, preserve and stop; research the exact 2026 provider contract
   before proposing a root fix.
4. If the controller remains healthy, let the registered n=12 run continue.
   Each new case performs one necessary local construction; do not add a second
   construction preflight.
5. On n=12 exit, adjudicate the whole root against Amendment 13. Commit only
   intended proof artifacts; preserve the unrelated old dump.
6. Only a clean n=12 pass permits preparation of a separately preregistered
   n≈100-300 confirmation. Run a cost/latency/evidence-efficiency checkpoint
   before authorizing it.
7. Do not start full-500. It comes later, after T6 confirmation, T1, and the SWE
   Lite gate. No SOTA wording before the full-500 protocol completes.
8. Before every big step ask: what new decision will this evidence unlock, is
   there a smaller secret-free or small-n check, and are we duplicating a proven
   path? Browse current primary sources when an issue touches models, providers,
   libraries, cloud services, or benchmark rules.

## Initial prompt for the new session

```text
Resume the MemPhant campaign from
/Users/sidsharma/.codex/worktrees/Memphant/p1-deep-mode on branch
codex/memphant-p1-deep-mode. First read AGENTS.md and
docs/handoff/NEXT-SESSION-PROMPT.md completely, then verify the live process and
immutable artifact state for docs/build-log/artifacts/p1-t6/run-65981e4f.

Do not launch or restart any benchmark until you prove the existing n=12 process
is gone and adjudicate its artifacts. Never rerun a settled row. Continue the
registered n=12 T6 Fast/Sonnet gate only while all fail-stop, route, settlement,
bank-seal, and cost predicates hold. CI/unit/integration tests must remain
secret-free and incapable of paid calls; use syndai/dev Doppler only around an
explicitly authorized live benchmark command, never print or persist values,
and use local ephemeral PostgreSQL only.

When an issue appears, diagnose the root cause, consult current 2026 primary
documentation, add the narrow regression check, and reassess whether the next
big step is the smallest evidence needed. Preserve unrelated dirty work and the
old untracked dump. No push is authorized.

Phase order is binding: finish/adjudicate T6 n=12; if it passes, separately
preregister T6 n≈100-300; then T1 observation-block gates; then
SWE-ContextBench Lite; only later LME-S full-500; finally LME-V2. Explain each
step in user-UX, latency, performance, and cost terms. Do not claim SOTA before
the full-500 protocol run.
```

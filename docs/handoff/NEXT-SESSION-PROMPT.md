# MemPhant next-session handoff — 2026-07-23

Current STATUS mirror: RUNTIME COMPLETE — BENCHMARK EVIDENCE PENDING

This is the authoritative handoff. Historical campaign prompts are not
execution authority.

## Repository state

- Work in the current repository checkout on `main`; discover it with
  `git rev-parse --show-toplevel`. Do not assume a linked worktree exists.
- At reconciliation, `git worktree list --porcelain` reported only the main
  checkout. The former P1 Deep worktree and branch no longer exist.
- No P1-T6 coordinator, server, or worker process is active.
- Preserve unrelated changes. At reconciliation, `.gitignore` was already
  modified and is outside this handoff change.

## P1-T6 adjudication

`run-65981e4f` is not adjudicable from the repository: its campaign directory
is absent from the checkout and from Git history. Do not invent, recover, or
reconstruct row proofs from prose.

The surviving committed record is
`docs/build-log/artifacts/p1-t6/PRE-EXECUTION-AMENDMENT-14.md`. It records that
the run fail-stopped after one settled Fast row when PostgreSQL disappeared
under the first Sonnet row; no Deep dispatch bound, and rows 3–24 never opened.
That amendment supersedes the old run and forbids resuming or replaying it.

The later A1 gate is now binding:
`docs/build-log/2026-07-21-a1-fast-miss-classification.md` found 0/166 scored
Fast misses were absent from the depth-64 pool. Deep therefore became a
diagnostic path, packing/ordering became the center of gravity, and P1-T6/D1
plus LME-S full-500/D3 were deferred.

`docs/build-log/artifacts/p1-t6/DISPATCH-AUTHORIZATION-d2f4fcb3.json` is a
historical authorization frozen to the old runtime, adapter, binaries, and
planned output identifier. It predates the binding A1 result and does not
authorize a launch from current `main`.

## Real next permitted action

Continue only secret-free packing/ordering work and its existing evidence
gates. Do not launch, restart, resume, aggregate, or reconstruct a P1-T6
benchmark. Do not spend against either historical dispatch authorization.

P1-T6 may reopen only if new, committed evidence establishes a material
depth-bound Fast-miss population. Reopening requires a fresh preregistration
and dispatch authorization bound to the then-current commit, controller,
adapter, binaries, manifest, cost ledger, scratch-Postgres lifecycle, and a new
output root. That authorization is not part of this handoff.

No n=100–300 confirmation, LME-S full-500 run, LME-V2 submission, product
promotion, SOTA claim, merge, push, or deployment is authorized here.

## Next-session prompt

```text
Resume MemPhant from the current main checkout. Read AGENTS.md,
docs/handoff/NEXT-SESSION-PROMPT.md, and the live STATUS ledger before acting.

Do not launch or restart P1-T6. The old run directory and linked worktree are
absent, Amendment 14 is the only committed adjudication, and the later A1 gate
made Deep diagnostic after finding 0/166 depth-bound Fast misses. Continue the
secret-free packing/ordering lane. Reopen P1-T6 only after new committed
depth-bound evidence and a fresh current-commit preregistration explicitly
authorize it. Preserve unrelated work and make no SOTA claim before an
authorized full protocol completes.
```

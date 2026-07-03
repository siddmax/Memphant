# Syndai Preflight Attempt

## Scope

Committed and pushed the mirrored Syndai MemPhant spec/status update:

- Syndai commit: `f894b3b3a22ac1700bf596e9346cc384240d63d1`
- Commit message: `docs(memphant): record ws0 wsa exits`
- Remote: `Syndai/main`

The public Memphant proof artifacts are committed locally in:

- Memphant commit: `6b4f423`
- Commit message: `feat: bootstrap memphant ws0 wsa`

## Preflight Command

`bash .claude/skills/preflight/run.sh`

## Result

Preflight reached push and CI watch:

- `1-rebase`: passed
- `2-cruft`: passed
- `3-self-review`: passed
- `4-reviews`: passed
- `5-propagation`: passed
- `6-stamp`: passed
- `7-push`: passed
- `8-ci-watch`: failed

GitHub Actions for `f894b3b3a`:

- `GitHub Action Pin Checks`: success
- `EvalRank Web Checks`: success
- `Portal Checks`: success
- `Web Checks`: success
- `Mobile Checks`: success
- `Backend Checks`: failure

Failure:

`Backend Checks` failed in `checks / Schema & Integration Checks`, step `Check schema contract (environment database)`.

The environment database reports applied revision `2026_07_02_003_registry_sync_resumable_crawl`, while `Syndai/main` at `f894b3b3a` expects local Alembic head `2026_07_02_002_drop_evalrank_observability_rollup_cron`.

Exact failure:

```text
ResolutionError: No such revision or branch '2026_07_02_003_registry_sync_resumable_crawl'
FAIL: database schema contract violation: violations=['failed_to_load_pending_revisions', 'revision_mismatch:db=2026_07_02_003_registry_sync_resumable_crawl,expected=2026_07_02_002_drop_evalrank_observability_rollup_cron']; db_revision='2026_07_02_003_registry_sync_resumable_crawl', expected_revision='2026_07_02_002_drop_evalrank_observability_rollup_cron'
```

## Diagnosis

The missing revision exists in another dirty Syndai worktree:

`/Users/sidsharma/Syndai/.claude/worktrees/registry-sync-resumable/backend/migrations/versions/2026_07_02_003_registry_sync_resumable_crawl.py`

That worktree also has a broad uncommitted registry-sync implementation. This is not part of the Memphant spec/status diff and should not be swept into the Memphant ship path.

## Status Impact

Do not flip `STATUS.md` §1 `Pass 15+16 work committed/shipped via /preflight` yet. The Memphant spec/status commit is pushed to `Syndai/main`, but the required preflight gate is not green because the shared Syndai environment database is ahead of the shipped migration graph.

Unblock options:

- Land the registry-sync-resumable migration/worktree through its own Syndai preflight path.
- Or restore the shared environment database to revision `2026_07_02_002_drop_evalrank_observability_rollup_cron`.

After that, rerun Syndai preflight for the current `Syndai/main` head and flip §1 only with green proof.

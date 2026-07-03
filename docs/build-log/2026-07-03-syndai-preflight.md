# Syndai Preflight Proof

## Scope

Committed and pushed the mirrored Syndai MemPhant spec/status update, then resolved the environment schema-contract blocker that the preflight CI surfaced.

- Syndai commit: `f894b3b3a22ac1700bf596e9346cc384240d63d1`
- Commit message: `docs(memphant): record ws0 wsa exits`
- Remote: `Syndai/main`

The public Memphant proof artifacts are committed locally in:

- Memphant commit: `6b4f423`
- Commit message: `feat: bootstrap memphant ws0 wsa`

Follow-up Syndai commits:

- `0c99ecc64` `fix(preflight): ship registry sync resumable migration`
- `fe17bc488` `style(preflight): format registry migration test`

## Preflight Command

`bash .claude/skills/preflight/run.sh`

## Initial Result

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

## Resolution

Shipped only the already-applied Alembic revision from the dirty registry-sync worktree, plus a narrow regression test that loads the local Alembic `ScriptDirectory` and proves the revision is in the shipped graph. The broad registry-sync implementation remained out of scope.

Local verification after the fix:

```text
$ uv run pytest tests/scripts/test_registry_sync_resumable_migration.py tests/scripts/test_preflight_migration_db_check.py tests/scripts/test_check_schema_contract.py -q --no-cov
.............                                                            [100%]
13 passed in 4.96s

$ doppler run -- uv run python scripts/check_schema_contract.py --require-db
PASS: local Alembic head is 2026_07_02_003_registry_sync_resumable_crawl
PASS: database schema contract satisfied (db_revision=2026_07_02_003_registry_sync_resumable_crawl, expected_revision=2026_07_02_003_registry_sync_resumable_crawl)

$ make check-ci-public
✅ All checks passed!
```

Final preflight result for `Syndai/main` `fe17bc488`:

```text
bash .claude/skills/preflight/run.sh
✅ (764s)
```

GitHub Actions for `fe17bc488`:

- `GitHub Action Pin Checks`: success — https://github.com/siddmax/Syndai/actions/runs/28638615251
- `EvalRank Web Checks`: success — https://github.com/siddmax/Syndai/actions/runs/28638615233
- `Portal Checks`: success — https://github.com/siddmax/Syndai/actions/runs/28638615345
- `Web Checks`: success — https://github.com/siddmax/Syndai/actions/runs/28638615356
- `Backend Checks`: success — https://github.com/siddmax/Syndai/actions/runs/28638615365
- `Mobile Checks`: success — https://github.com/siddmax/Syndai/actions/runs/28638615340
- `Deploy`: success — https://github.com/siddmax/Syndai/actions/runs/28638615359

## Mirror Ledger Commit

After the blocker fix passed, the final mirrored STATUS ledger flip was committed to `Syndai/main` as `27abc08bb548b4ffdbafdf23591f8ff0caa2f367` (`docs(memphant): mark syndai preflight complete`) and passed preflight:

```text
bash .claude/skills/preflight/run.sh
✅ (767s)
```

GitHub Actions for `27abc08bb`:

- `GitHub Action Pin Checks`: success — https://github.com/siddmax/Syndai/actions/runs/28639350377
- `EvalRank Web Checks`: success — https://github.com/siddmax/Syndai/actions/runs/28639350361
- `Portal Checks`: success — https://github.com/siddmax/Syndai/actions/runs/28639350446
- `Web Checks`: success — https://github.com/siddmax/Syndai/actions/runs/28639350522
- `Backend Checks`: success — https://github.com/siddmax/Syndai/actions/runs/28639350469
- `Mobile Checks`: success — https://github.com/siddmax/Syndai/actions/runs/28639350466

## Status Impact

Flip `STATUS.md` §1 `Pass 15+16 work committed/shipped via /preflight`. The mirrored MemPhant spec/status work is pushed to `Syndai/main`, the environment database revision is resolvable by shipped code, and preflight completed green on the final mirrored ledger head.

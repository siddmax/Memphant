# Internal golden/security/ops suites — 2026-07-10 run record

Commands (run at repo root, commit of this artifact):

```
cargo run -q -p memphant-eval -- run examples/evals/golden.yaml
  -> eval=pass id=pr-golden passed=14/14 archive=none
cargo run -q -p memphant-eval -- security examples/evals/security-smoke.yaml
  -> security=pass lanes=poisoning,query_filter_injection,high_risk_action_suppression,tenant_leakage,deletion_completeness deletion_completeness=pass
cargo run -q -p memphant-eval -- ops examples/evals/ops-smoke.yaml
  -> ops=pass checks=blob_gc,deletion_saga_readback,reindex_compaction_sla
```

**Honesty note:** these suites execute against the in-memory store inside
`memphant-eval` — the crate's `run`/`security`/`ops` subcommands take no
`--database-url` and were NOT rewired for Postgres in this campaign (that
rewiring is real work, not a flag). Under the promotion-provenance rule these
results therefore gate regressions only; they are not promotion evidence for
any rung. The Postgres-backed promotion evidence in this directory is the
`bench-lme` LongMemEval lane (`lme-s-*.json`), which runs entirely through
`MemoryService<PgStore>`.

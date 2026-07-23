# 2026-07-22 — B1 observation-block gate

## Preregistered gate

The intervention is the real Postgres-backed `reflect(scope)` worker path. It
writes a versioned, date-annotated scope block in the same transaction that
completes the claimed job; recall returns that block as a stable prefix. The
reader treatment prepends the block to otherwise unchanged Fast evidence.

The fixed n=12 development subset is the first six non-abstention Sol Pro v1
misses in each A1 failure class, ordered by question ID:

- displacement (`in_pool_unpacked`): `001be529`, `0a34ad58`, `184da446`,
  `19b5f2b3`, `1da05512`, `1e043500`
- reader with adequate pack (`in_top_k`): `0977f2af`, `09ba9854`, `0bc8ad93`,
  `2ce6a0f2`, `58470ed2`, `61f8c8f8`

Control is the frozen v1 Postgres evidence for those IDs. Treatment is a fresh
packaged-runtime run with identical session granularity, k=10, pool=64,
8,192-token pack budget, small embedder, rerank disabled, and only the B1
prefix enabled. Reader and judge are both `openai/gpt-5.6-sol-pro` at high
reasoning, prompt v1, with the existing content-addressed reader cache reused.

Promote only at at least +2 net flips (miss→hit minus hit→miss) and no new
abstention break. Otherwise delete the lever and retain this negative record.

## Result

**REJECTED; implementation deleted.** The candidate first passed its mechanism
checks: focused InMemory TDD, live scratch-Postgres generation/readback, RLS
isolation, and real packaged server/worker E2E. The packaged n=12 treatment then
completed against an ephemeral migrated Postgres database (12/12 rows,
R@5=0.50, R@10=0.50) and produced bounded dated prefixes.

The preregistered OpenRouter Sol Pro alias and canonical snapshot were both
unroutable under the evaluator's strict JSON-schema parameter contract (12/12
fail-closed reader errors in each diagnostic attempt). Those invalid attempts
were overwritten and do not count. The live fallback used the same
`gpt-5.6-sol` high-reasoning Codex engine for both reader and judge, both arms:

- control: 0/12, 15 fresh calls, zero reader/parse/judge errors
- treatment: 0/12, 15 fresh calls, zero reader/parse/judge errors
- paired delta: 0.000, 95% bootstrap CI [0.000, 0.000], **0 net flips**
- no new abstention break (there were no changed correctness outcomes)

This misses the +2 bar decisively. It also cannot be promoted under the exact
Sol Pro preregistration because of the provider deviation. The causal failure
is representational: a query-independent 300-token block built from the newest
raw scope units mostly repeats one recent, irrelevant session; it neither
recovers displaced answer-bearing history nor helps the reader use an already
adequate pack. The entire B1 verb/read/write/eval surface was deleted and
`scope_block` remains explicitly dormant. A future retry needs an independently
validated observer/reflector condensation mechanism, not another raw-recency
renderer.

Canonical committed artifacts (raw corpus/evidence remain gitignored):

- `reader-control.json` sha256 `8aee15c56211c738c2a247cebb67174fbc4b45ac58d05707923afd7394b64add`
- `reader-treatment.json` sha256 `4f74d9cd5614ae50466ece3d28333b4d34a25e26a88b7eff7fb9211b38bf7996`
- `treatment-retrieval.json` sha256 `531b21c3f539819e601016bbc2432474242501a21ee81d06dba38b43e587a538`

No confirmation question was touched; no SOTA, accuracy, cutover, or runtime
checkbox moves.

## Verification and worktree boundary

- negative-artifact contract: PASS (both reports complete, promotion-eligible
  as evaluator artifacts, n=12, zero errors; paired mean exactly 0)
- `cargo fmt --check`: PASS
- `cargo clippy --all-targets --all-features -- -D warnings`: PASS
- `python3 scripts/check_spec_drift.py`: SKIPPED (`private_specs_missing`)
- `cargo test --all-targets --all-features`: BLOCKED by the pre-existing dirty
  cross-rerank work: generated MCP schema is stale against its new trace fields
- `python3 -m pytest tests/ -q`: 697 passed / 10 skipped / 23 failed, all in
  pre-existing gate-runner/test drift (`recall`/`ingest_section` signatures,
  new embedder IDs, and a locally drifted fetched LME-S dataset)

The B1 implementation was deleted cleanly: after removal, no
`observation_block`, `StoredScopeBlock`, writer/read-path, or eval-flag symbol
remains. Unrelated dirty cross-rerank/fusion work and `.claude/feature-flow`
state were preserved and are not included in the B1 commit.

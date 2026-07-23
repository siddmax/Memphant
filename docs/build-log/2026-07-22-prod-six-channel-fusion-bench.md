# Production six-channel fusion benchmark (2026-07-22)

## Verdict

**KEEP weighted-RRF. The production default was not changed.**

The preregistered promotion gate required weighted normalized-score fusion to
beat weighted-RRF on the real production recall path with a paired 95% CI for
Recall@5 entirely above zero. It did not: the point estimate improved by
`+0.0556`, but the CI was `[-0.0278, +0.1389]`. First-answer MRR agreed only
directionally (`+0.0486`, CI `[-0.0208, +0.1250]`). Both intervals include zero.

The corpus also failed the stronger representativeness condition. The real six
internal passes executed, but this session-only P1 pool produced score-bearing
candidates only for Lexical/Semantic and Vector. Exact, Temporal, and Edge had
zero candidate rows across all 80 queries. This is not evidence that six
heterogeneous channel distributions can be safely convex-combined.

## Question and preregistration

P1 Piece 2 found that a tuned two-channel convex fuser beat RRF on dense cosine
+ BM25 (`Recall@5 0.847` vs `0.833`). Its own verdict correctly said that result
did not transfer to the production fuser, which combines
Exact/Lexical/Semantic/Temporal/Edge/Vector with weighted RRF.

This follow-up fixed the alternative before the full run:

- Baseline: production `channel_weight(pass) / (60 + rank)` (and the existing
  subquery offset `55`).
- Candidate: for each real production pass and query, scale each positive raw
  score by that pass's observed maximum, then combine it with the existing
  production channel weights normalized to sum to one. Missing-channel score is
  zero. No alpha or channel weight was tuned on the test set.
- Primary metric/gate: paired delta Recall@5; promote only if its 95% percentile
  bootstrap CI is entirely above zero.
- Secondary rank-sensitive metric: reciprocal rank of the first answer-bearing
  item (miss = zero), averaged as MRR, with the same paired CI.
- Coverage gate: all production passes must be exercised by the real path and
  the corpus must contain score-bearing evidence for the heterogeneous channel
  families. A missing family cannot support a default flip.

This choice follows the hybrid-fusion paper's own limitation: its analysis is
for lexical + semantic retrieval and says experiments are still required before
generalizing normalization to three or more models. RRF remains the rank-only
control ([Bruch, Gai, and Ingber](https://arxiv.org/abs/2210.11934),
[Cormack, Clarke, and Buettcher](https://research.google/pubs/reciprocal-rank-fusion-outperforms-condorcet-and-individual-rank-learning-methods/)).
The paired percentile interval follows the bootstrap recommendation for common
IR measures ([NIST/Soboroff](https://www.nist.gov/publications/computing-confidence-intervals-common-ir-measures)).

## Production-path benchmark

- Worktree/HEAD before the change:
  `/Users/sidsharma/.codex/worktrees/Memphant/p1-deep-mode`,
  `3578b00f7cb874237bddac711c8f277611358674`.
- Fixed P1 pool:
  `docs/build-log/artifacts/p1-retrieval-bench/pool.json`, SHA-256
  `a0e4ddbc23ef717f16441a2726ae843843f5ef5a750dba969553cac67d21ea2a`.
- Pool guard: 80 questions, 72 scored + 8 abstention, 7,995 documents,
  zero guard violations; gold verification = 35 string + 37 LLM.
- Both arms used `memphant-eval bench-lme` with its real
  `MemoryService<PgStore>` session ingestion, worker reflection, local BGE-small
  document/query embedding, Postgres pgvector fetch, production channel
  candidate generation/fusion, packing, trace persistence, and provenance
  scoring.
- Each arm ran in a distinct migrated scratch database minted and auto-dropped
  by `scripts/with_scratch_db.sh`. The shared `memphant` database was not used
  for benchmark writes; no `memphant_scratch_*` database remained afterward.
- Fixed settings: seed `20260722`, sample `80`, `k=5`, recall pool `64`, real
  Vector enabled, Edge enabled, temporal grounding enabled, runtime chunks on,
  and the retired deterministic reranker disabled so the comparison isolates
  fusion. No hosted model or paid provider call was used.
- The core's six internal passes are Exact, Lexical, Semantic, Temporal, Edge,
  and Vector. The public trace schema intentionally merges Lexical + Semantic
  under the `lexical` label, so their diagnostic rows are combined below.

Commands:

```sh
cargo build --release -p memphant-eval --features fastembed

bash scripts/with_scratch_db.sh \
  postgres://memphant:memphant@localhost:5432/memphant DATABASE_URL \
  bash -c 'target/release/memphant-eval bench-lme \
    --database-url "$DATABASE_URL" \
    --data docs/build-log/artifacts/p1-retrieval-bench/pool.json \
    --sample 80 --seed 20260722 --k 5 --pool 64 \
    --disable rerank --temporal-grounding --fusion weighted-rrf \
    --out docs/build-log/artifacts/p1-prod-fusion-20260722/weighted-rrf.json'

bash scripts/with_scratch_db.sh \
  postgres://memphant:memphant@localhost:5432/memphant DATABASE_URL \
  bash -c 'target/release/memphant-eval bench-lme \
    --database-url "$DATABASE_URL" \
    --data docs/build-log/artifacts/p1-retrieval-bench/pool.json \
    --sample 80 --seed 20260722 --k 5 --pool 64 \
    --disable rerank --temporal-grounding \
    --fusion weighted-normalized-score \
    --baseline docs/build-log/artifacts/p1-prod-fusion-20260722/weighted-rrf.json \
    --out docs/build-log/artifacts/p1-prod-fusion-20260722/weighted-normalized-score.json'
```

## Results

| arm | Recall@5 | first-answer MRR | abstention |
|---|---:|---:|---:|
| weighted-RRF | 0.3750 | 0.3472 | 8/8 |
| weighted normalized score | 0.4306 | 0.3958 | 8/8 |
| paired delta | +0.0556 | +0.0486 | 0 |
| paired 95% CI | **[-0.0278, +0.1389]** | **[-0.0208, +0.1250]** | — |

The candidate report enforced pool hash, seed, and sample identity and emitted
`n_paired=72`, 1,000 seeded bootstrap resamples. Recall@5 had 7 candidate wins,
3 losses, and 62 ties. MRR had 7 wins, 4 losses, and 61 ties.

By scored stratum, normalized-score minus RRF Recall@5 was:

| stratum | n | RRF | normalized | delta |
|---|---:|---:|---:|---:|
| knowledge-update | 16 | 0.4375 | 0.5625 | +0.1250 |
| multi-session | 24 | 0.4167 | 0.5000 | +0.0833 |
| single-session-assistant | 2 | 0.5000 | 0.5000 | 0.0000 |
| single-session-user | 6 | 0.1667 | 0.1667 | 0.0000 |
| temporal-reasoning | 24 | 0.3333 | 0.3333 | 0.0000 |

## Channel audit

Both arms produced identical candidate coverage and raw-score ranges:

| trace channel | queries with candidates | candidate rows | observed raw score range |
|---|---:|---:|---:|
| Exact | 0/80 | 0 | — |
| Lexical + Semantic | 80/80 | 15,948 | 0.000393–0.272926 |
| Temporal | 0/80 | 0 | — |
| Edge | 0/80 | 0 | — |
| Vector | 80/80 | 5,120 | 0.351903–0.813824 |

The alternative therefore normalized and fused two score-bearing trace families,
not six. It is more production-faithful than the original Python harness because
it ran the real recall implementation and downstream packer, but it cannot
answer the unsafe part of the question: how normalized scores interact when
Exact, Temporal, and Edge are actually populated. Inventing synthetic fact keys
or arbitrary edges inside this fixed pool would change the benchmark's relevance
contract rather than repair it.

## Evidence

- `docs/build-log/artifacts/p1-prod-fusion-20260722/weighted-rrf.json`
  - SHA-256 `b3c5b0c8c30f557aca49b73ce32b8d1c0228fa986b117d58ba10c5ad021e712c`
- `docs/build-log/artifacts/p1-prod-fusion-20260722/weighted-normalized-score.json`
  - SHA-256 `05882215e59d4cbe8787ef9bdccbf4fe92f92cc00b7c4960d8af757f3cb55a33`

## Verification

Fusion-specific and directly dependent gates passed:

- `python3 .../build_adversarial_set.py --verify .../pool.json`: 80 questions,
  72 scored + 8 abstention, zero guard violations.
- The source `benchmarks/data/longmemeval_s.json` is present and its SHA-256
  matches the pool's recorded corpus hash. A from-source regeneration was also
  attempted, but the builder's required credential wrapper could not load the
  configured Doppler `prod` config. No regenerated pool is claimed; the
  committed pool was reused only after its full local guard verification.
- `cargo test -p memphant-core fusion_weight_tests --lib`: 3 passed.
- `cargo test -p memphant-eval bench_lme::tests --lib`: 15 passed.
- `cargo test -p memphant-core -p memphant-eval --all-targets --all-features`:
  passed; the eval contract reported 19 passed and one paid-network test
  skipped, not passed.
- `cargo clippy -p memphant-core -p memphant-eval --all-targets --all-features -- -D warnings`:
  passed.
- Both full 80-query benchmark arms completed against separate scratch Postgres
  databases; each scratch database was dropped and none remained afterward.

Repository gates run after the benchmark:

- `cargo fmt --check`: passed.
- `cargo clippy --all-targets --all-features -- -D warnings`: passed.
- `cargo test --doc`: passed.
- Ignored live-Postgres store/worker contract suite through
  `scripts/with_scratch_db.sh`: passed (69 integration tests; tests filtered by
  the explicit ignored selector are not counted as passing).
- Provider lint: `plain-postgres`, `supabase`, and `neon` all clean.
- Migration dry-run: passed.
- `scripts/e2e_probe.sh`: `E2E PROBE: ALL CHECKS PASSED`.
- `python3 scripts/check_spec_drift.py`: skipped because the private mirrored
  Syndai specs are unavailable in this worktree; this is not a pass.
- The prescribed combined Python command could not run as written because
  `spikes/python-retain/test_spike.py` is absent. Running the available `tests/`
  suite separately stopped at a pre-existing signature mismatch:
  `gate_run_memphant.recall()` requires `ctx`, while
  `test_gate_r15_cross_rerank.py` still calls the old signature (134 passed,
  8 skipped before that failure). Neither path is touched by this change.
- `cargo test --all-targets --all-features` reached and passed the changed core
  and eval suites, then failed the pre-existing generated-artifact contract:
  `mcp/memphant.tools.v1.json is stale`. The MCP artifact and its source types
  are outside this change and were left untouched to preserve concurrent work.

The working tree's owner advanced HEAD after the benchmark through unrelated
docs/status commits. Those commits did not modify the four Rust files or the two
fusion evidence artifacts recorded here.

## Decision boundary

Weighted normalized-score fusion did not clear the statistical gate, and the P1
pool did not clear the six-channel coverage gate. **Keep weighted-RRF.** A future
default change needs a labeled, production-derived structured-memory collection
with real Exact/Temporal/Edge candidates, frozen before evaluation, followed by
the same paired Recall@5 CI gate. The positive two-family point estimate is a
follow-up signal only, not shipment evidence.

## Cleanup outcome

The experimental `FusionAlgorithm`, normalized-score implementation, P1-pool
adapter, CLI flag, and diagnostics were deleted after adjudication. Keeping an
unpromoted evaluation seam would be feature-flag rot; a future experiment can
rebuild the minimal seam against the required structured-memory collection.
Only this negative record and the two canonical reports are retained.

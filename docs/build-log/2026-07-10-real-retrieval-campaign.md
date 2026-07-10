# 2026-07-10 — Real Retrieval Campaign (LongMemEval-S, retrieval-only, Postgres runtime)

**Scope statement (read first):** this campaign is **retrieval-only**. It measures
whether the answer-bearing haystack *session* appears in the top-k recall
provenance. It makes **no reader/QA-accuracy claim and no SOTA claim**. Every
run executed through the packaged Postgres runtime path
(`MemoryService<PgStore>` → live pgvector/PG17) with real fastembed
(bge-small-en-v1.5) embeddings on both ingestion and query.

## Dataset (pinned)

- Source: Hugging Face `xiaowu0162/longmemeval`, file `longmemeval_s`
  (500 questions, ~40–55 haystack sessions each; distractor pressure intact).
- sha256 `08d8dad4be43ee2049a22ff5674eb86725d0ce5ff434cde2627e5e8e7e117894`
  (278,025,796 bytes) — pinned in `benchmarks/manifests/longmemeval_s.lock.json`;
  raw data gitignored under `benchmarks/data/`.
- Fetcher: `scripts/fetch_longmemeval.py` (also pins `longmemeval_oracle`,
  sha256 `821a2034d219ab45846873dd14c14f12cfe7776e73527a483f9dac095d38620c`,
  used only for smoke tests).

## Method

- Lane: `memphant-eval bench-lme` (`crates/memphant-eval/src/bench_lme.rs`),
  built with `--features fastembed`.
- Sample: **30 questions**, stratified by `question_type` (proportional,
  largest remainder, min 1/stratum), seed **20260710**, identical question set
  for every run (seeded splitmix64; no external randomness).
- Per question: fresh tenant via `PgStore::create_tenant`; each haystack
  session ingested chronologically as ONE episode (turns concatenated as
  `role: content`, body prefixed `[session <id>] [date <date>]`); reflect via
  `MemoryService::reflect` (the worker claim/complete path); then
  `recall(question, k=10, budget_tokens=8192)`.
- Scoring: Recall@5/@10 = any top-k item whose `citation_episode_id` maps back
  to a session in `answer_session_ids`. Abstention questions (`_abs`, 2 in
  sample) scored separately (correct = abstained or returned no
  answer-session item) and excluded from Recall@k (n_scored = 28).
- Ablations are read-time flags on the same recall request; `--disable vector`
  recalls through a Noop-embedder service so `query_vec=None` (ingestion
  embeddings unchanged). Paired per-question deltas vs baseline with bootstrap
  95% CI (1000 resamples, seed 20260710).

## Commands

```
python3 scripts/fetch_longmemeval.py
cargo build -p memphant-eval --features fastembed --release
./target/release/memphant-eval bench-lme \
  --database-url postgres://memphant:memphant@localhost:5432/memphant \
  --data benchmarks/data/longmemeval_s.json --sample 30 --seed 20260710 --k 10 \
  --out docs/build-log/artifacts/real-retrieval-20260710/lme-s-baseline.json
# then, for each variant (identical args plus):
#   --disable vector|edge_expansion|rerank|query_decomposition --baseline <baseline.json>
#   --mode exhaustive --baseline <baseline.json>
```

Full profile JSONs (per-question rows, per-stratum metrics, provenance
headers `runtime=postgres`, `retrieval_only=true`, dataset sha256, exact
command) live in `docs/build-log/artifacts/real-retrieval-20260710/`.

## Results

Baseline overall (n=28 scored): **Recall@5 = 0.500, Recall@10 = 0.607**;
abstention 1/2 correct. Per stratum (R@5 / R@10): knowledge-update .80/.80
(n=5), multi-session .43/.71 (n=7), single-session-assistant .67/.67 (n=3),
single-session-preference .00/.00 (n=2), single-session-user .50/.50 (n=4),
temporal-reasoning .43/.57 (n=7).

Paired ablation deltas (variant − baseline; bootstrap 95% CI, 1000 resamples, n=28):

| Variant | R@5 | R@10 | ΔR@5 mean [95% CI] | ΔR@10 mean [95% CI] | CI excludes 0? |
|---|---|---|---|---|---|
| baseline | 0.500 | 0.607 | — | — | — |
| disable vector (query_vec=None) | 0.500 | 0.536 | 0.000 [0.000, 0.000] | −0.071 [−0.179, 0.000] | no (R@10 CI touches 0) |
| disable edge_expansion | 0.500 | 0.607 | 0.000 [0.000, 0.000] | 0.000 [0.000, 0.000] | no |
| disable rerank | **0.643** | 0.643 | **+0.143 [+0.036, +0.286]** | +0.036 [0.000, +0.107] | **yes (R@5) — disabling rerank IMPROVED recall** |
| disable query_decomposition | 0.500 | 0.607 | 0.000 [0.000, 0.000] | 0.000 [0.000, 0.000] | no |
| mode=exhaustive | 0.500 | 0.607 | 0.000 [0.000, 0.000] | 0.000 [0.000, 0.000] | no |

Reading, honestly stated:

- **Vector channel:** directionally positive at @10 (disabling it costs 7.1
  points mean) but the 95% CI touches zero at n=28 — not promotion-grade.
- **Edge expansion / query decomposition / exhaustive mode:** zero paired
  delta on every question. On this corpus (chat sessions ingested as episodes)
  these stages currently do nothing measurable.
- **Bounded rerank:** actively harmful on this real sample — disabling it
  improved Recall@5 by +14.3 points with a CI that excludes zero. This is
  disable-when evidence for rung 8, not advance-when evidence.

## Internal suites

`memphant-eval run/security/ops` (golden 14/14, security all lanes, ops all
checks) pass — but those subcommands execute **in-memory only** (no
`--database-url` path exists; it was not wired in this campaign). Recorded in
`docs/build-log/artifacts/real-retrieval-20260710/internal-suites-inmemory.md`.
Under the promotion-provenance rule they gate regressions only.

## Runtime bug found and fixed

Ingesting real ~9KB session bodies exposed a genuine store bug: `derive_dedup_key`
embedded the full normalized body, overflowing the `(tenant_id, scope_id,
dedup_key)` btree unique index ("index row requires 8376 bytes, maximum size is
8191"). Fixed at the root in `memphant-core`: the body component of the dedup
key is now the sha256 of the normalized body (dedup semantics unchanged).
Also: episode `source_kind` must be one of the schema's checked values; the
lane uses `user`.

## Rung adjudication (STATUS §3, rungs 4–15)

Advance-when bar applied: a retrieval-only paired delta whose 95% CI excludes
zero, produced by the Postgres runtime on the pinned real corpus. **No rung
meets it in this campaign.**

| Rung | Decision | One-line justification |
|---|---|---|
| 4 contextual chunks | stays open | real baseline now exists, but no contextual-chunk paired ablation is exposed on the runtime recall path — nothing measured on this axis |
| 5 temporal validity | stays open | not ablated in this campaign; needs paired stale/current evidence on real corpora |
| 6 edge expansion | stays open | measured: ZERO paired delta vs no-edges (no edges mint from chat-session episodes); ≥3-pt advance-when not met |
| 7 packing+abstention | stays open | packing ablation not run; only 2 abstention questions in sample (1/2 correct) — insufficient |
| 8 bounded rerank | stays open | measured: disabling rerank IMPROVED R@5 +0.143 (CI excludes 0) — advance-when failed; disable-when evidence recorded |
| 9 query decomposition | stays open | measured: zero paired delta on composite-question sample; advance-when not met |
| 10 procedural memory | stays open | LME-S chat corpus contains no procedural/replay cases; needs STATE-Bench-style task evidence |
| 11 DSR decay fold | stays open | short benchmark cannot exercise decay; needs the internally-run longitudinal MemoryStress-style suite |
| 12 L4 exhaustive | stays open | measured: mode=exhaustive zero paired delta; no accuracy-ceiling gain shown |
| 13 learned rerank/DSR | stays open | needs the archived-trace training-data floor; no training run exists (bench lane now produces real traces) |
| 14 external engine | **RETIRED (re-checked)** | retirement stands: no graph-engine bottleneck evidence; relational edges evaluated in real-retrieval-20260710 |
| 15 belief composition | stays open | retrieval-only lane does not exercise composition; needs OP-Bench-style restraint check on real corpus |

## Deviations / limits

- n=30 (28 scored) is a small sample; CIs are wide. Zero-delta axes may hide
  small effects — but zero observed delta is also zero promotion evidence.
- One episode per session is a coarse ingestion granularity; per-turn or
  chunked ingestion may change every number above. Recorded in each profile
  header.
- The internal golden/security/ops suites remain in-memory (noted above).
- No reader/QA scoring, no SOTA comparison, no LongMemEval leaderboard claim.

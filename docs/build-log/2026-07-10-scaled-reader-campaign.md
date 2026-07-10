# 2026-07-10 — Scaled Reader Campaign (LongMemEval-S, n=100, codex reader engine)

**Scope statement (read first):** this campaign re-runs the two key lever
comparisons from `2026-07-10-reader-campaign.md` at **n=100 stratified**
(seed 20260710, k=10) with a **new reader engine** so that promotion/reversion
decisions rest on adequate power. Provenance labels on every artifact:
runtime=postgres, embeddings=fastembed bge-small-en-v1.5,
engine=`codex exec` headless (codex-cli 0.144.1),
reader=`gpt-5.6-terra` (reasoning effort `medium`),
judge=`containment+gpt-5.6-terra` (effort `medium`), dataset sha256
`08d8dad4…7894` (pinned). This is **not** a LongMemEval leaderboard claim.

## Reader upgrade (and what the probes actually showed)

- The Codex CLI resolves the two new GPT models as **`gpt-5.6-terra`**
  ("balanced", the stronger) and **`gpt-5.6-luna`** ("fast and affordable",
  the cheaper), listed in the CLI's model catalog alongside `gpt-5.6-sol`.
- `scripts/run_reader.py` grew `--engine claude|codex`, `--judge-model`, and a
  codex-only `--reasoning-effort` override. Codex path:
  `codex exec - -m <model> -s read-only --ephemeral --skip-git-repo-check
  --ignore-user-config --color never -o <last-message-file>` with the prompt on
  stdin and the system prompt prepended (codex has no separate system channel)
  plus an explicit no-tool-use guard. Only the final agent message is read
  (`-o`), so tool use is stripped by construction. Cache keys now include
  engine+model+effort; the OpenRouter fallback was **not needed** (codex
  resolved) and is not implemented.
- **Reader-selection probes** (same pinned 30q session-baseline evidence pack
  as the prior campaign, paired vs the haiku reader report):

| Reader (30q, session+rerank-on evidence) | QA accuracy | ΔQA vs haiku [95% CI] |
|---|---|---|
| claude-haiku-4-5 (prior campaign) | 0.433 | — |
| gpt-5.6-luna @ default(low) | 0.333 | −0.100 [−0.233, 0.000] |
| gpt-5.6-terra @ default(low) | 0.400 | −0.033 [−0.100, 0.000] |
| **gpt-5.6-terra @ medium** (chosen) | **0.400** | −0.033 [−0.100, 0.000] — one question ("Six weeks" vs gold "Four weeks"); identical on 29/30 |

  The instruction preference (cheaper `luna` for reading) was **overridden on
  measured evidence**: luna@low is directionally worse than haiku (terser,
  over-abstains). `terra@medium` ties haiku on 29/30 questions. **There is no
  reader headroom to buy on this evidence** — the reader is not the binding
  constraint; retrieval is. terra@medium is used for reader AND judge in all
  scaled runs (accuracy > cost per owner directive); effort `high` was not
  probed (3× latency for a lever the 30q probe says is not binding).
- The 30q re-score doubles as the **reader-sensitivity line** in the results.

## Method

- Retrieval lane unchanged (`memphant-eval bench-lme`, fresh tenant per
  question, chronological ingestion, reflect, recall k=10, budget 8192,
  Recall@k by `citation_episode_id` → session provenance). **n=100**
  stratified at seed 20260710 — a strict superset of the prior 30q sample
  (verified: all 30 prior ids appear in the 100q sample), strata: 27
  multi-session, 27 temporal-reasoning, 15 knowledge-update, 14
  single-session-user, 11 single-session-assistant, 6
  single-session-preference (94 scored + 6 abstention).
- **Baseline is (a) session + rerank-off** — the shipped production default
  (`rerank_enabled=false` in `memphant-core/src/service.rs`). The bench lane
  sends explicit flags: default flags are rerank-ON, so (a) uses
  `--disable rerank` and (b) uses bench defaults; no new flag was needed.
- Three configs, one full bench run each (bench-lme re-ingests per run; the
  reader lane reuses each run's emitted QA JSONL, so no config is ever
  re-ingested for scoring):
  - (a) session granularity, rerank-off (`--disable rerank`) — baseline;
  - (b) session granularity, rerank-on (bench default flags);
  - (c) turns granularity (≤4-turn episodes), rerank-off
    (`--granularity turns --disable rerank`) — rerank matched to baseline so
    the pair isolates granularity (the prior campaign's turns run was
    rerank-on).
- Reader lane: `scripts/run_reader.py --engine codex --model gpt-5.6-terra
  --judge-model gpt-5.6-terra --reasoning-effort medium`, serialized calls,
  sha256 cache keyed by engine+model+effort+prompt, hard per-invocation call
  budgets, judge spent only on containment misses, abstention scored by exact
  "I don't know" containment. Paired per-question QA deltas vs (a) with
  bootstrap 95% CI (1000 resamples, seed 20260710).

## Commands

```
cargo build -p memphant-eval --features fastembed --release
# (a) baseline           : --disable rerank
# (b) rerank-on          : (no --disable)
# (c) turns, rerank-off  : --granularity turns --disable rerank
./target/release/memphant-eval bench-lme \
  --database-url postgres://memphant:memphant@localhost:5432/memphant \
  --data benchmarks/data/longmemeval_s.json --sample 100 --seed 20260710 --k 10 \
  [--disable rerank] [--granularity turns] \
  [--baseline …/scaled-lme-s-session-rerank-off.json] \
  --emit-qa …/reader-evidence-scaled-<cfg>.jsonl --out …/scaled-lme-s-<cfg>.json
python3 scripts/run_reader.py --evidence …/reader-evidence-scaled-<cfg>.jsonl \
  --out …/scaled-reader-<cfg>.json --label scaled-<cfg> \
  --engine codex --model gpt-5.6-terra --judge-model gpt-5.6-terra \
  --reasoning-effort medium \
  --retrieval-report …/scaled-lme-s-<cfg>.json \
  [--baseline …/scaled-reader-session-rerank-off.json] --max-calls 160
```

Artifacts: `docs/build-log/artifacts/real-retrieval-20260710/scaled-*.json`
(retrieval + reader reports, committed); `reader-evidence-scaled-*.jsonl`
(raw LongMemEval text, gitignored like the dataset).

## Results (n=100; 94 retrieval-scored, 100 QA-scored incl. 6 abstention)

| Config | R@5 | R@10 | QA accuracy | ΔQA vs (a) [95% CI] | Reader abstention |
|---|---|---|---|---|---|
| (a) session, rerank-off (baseline = shipped default) | 0.702 | 0.702 | 0.430 | — | 3/6 |
| (b) session, rerank-on | 0.574 | 0.628 | 0.380 | −0.050 [−0.110, +0.010] — no | 5/6 |
| (c) turns, rerank-off | 0.787 | 0.830 | **0.560** | **+0.130 [+0.040, +0.210] — YES** | 6/6 |

Paired retrieval deltas vs (a), n=94 scored:

| Config | ΔR@5 mean [95% CI] | ΔR@10 mean [95% CI] | CI excludes 0? |
|---|---|---|---|
| (b) rerank-on | **−0.128 [−0.202, −0.053]** | **−0.074 [−0.138, −0.011]** | **yes, harmful on BOTH depths** |
| (c) turns | **+0.085 [+0.011, +0.160]** | **+0.128 [+0.053, +0.202]** | **yes, positive on BOTH depths** |

QA accuracy per stratum ((a) session-off / (b) session-on / (c) turns-off):
knowledge-update .60/.40/**.80** (n=15), multi-session .33/.37/.41 (n=27),
single-session-assistant .91/.91/.91 (n=11), single-session-preference
.17/.17/.33 (n=6), single-session-user .36/.29/**.71** (n=14),
temporal-reasoning .33/.26/.41 (n=27).

Judge-method mix (a/b/c): containment 71/76/67, llm_judge 23/18/27,
abstention-exact 6/6/6. Judge calls spent only on containment misses.

### Reader sensitivity (old 30q session+rerank-on evidence pack, re-scored)

| Reader | QA (30q) | ΔQA vs haiku [95% CI] |
|---|---|---|
| claude-haiku-4-5 (prior campaign artifact) | 0.433 | — |
| gpt-5.6-luna @ low | 0.333 | −0.100 [−0.233, 0.000] |
| gpt-5.6-terra @ low | 0.400 | −0.033 [−0.100, 0.000] |
| gpt-5.6-terra @ medium (campaign reader) | 0.400 | −0.033 [−0.100, 0.000] |

The campaign reader is NOT stronger than haiku on identical evidence (it
agrees with haiku on 29/30; the one flip is a genuine temporal-reasoning
miss). Reader-model deltas (≤0.10) are small next to the granularity lever
(+0.13 at n=100): on this benchmark, retrieval evidence — not reader
capacity — is the binding constraint, and cross-reader comparisons of QA
levels remain invalid. All (a)/(b)/(c) comparisons above are same-reader,
same-judge, paired.

## Verdicts, applied

1. **Rerank default: KEEP rerank-off.** At n=100 rerank-on is retrieval-harmful
   with CIs excluding zero on BOTH R@5 and R@10 (the 30q campaign only showed
   R@5), and QA agrees directionally (−0.050 [−0.110, +0.010]). The
   `rerank_enabled=false` default in `memphant-core/src/service.rs` stands.
   No code change.
2. **Granularity: the turns falsification is OVERTURNED — and promoted.**
   At n=100, ≤4-turn windowed episodes beat whole-session episodes on
   retrieval (both depths, CIs exclude zero) AND on end-to-end QA
   (+0.130 [+0.040, +0.210], CI excludes zero) — including knowledge-update
   (.60→.80), the stratum whose 30q collapse (.60→.20) drove the original
   rejection. The 30q verdict was a small-sample artifact: on the 30-question
   overlap subset of this same n=100 run, the turns−session QA delta is
   +0.067 [−0.100, +0.233] — invisible at that n. (The prior turns run was
   also rerank-on; this pair isolates granularity at the shipped rerank-off
   default.) **Applied:** `bench-lme` lane default granularity flipped to
   `turns` (`DEFAULT_GRANULARITY` in `crates/memphant-eval/src/bench_lme.rs`,
   used by `main.rs`; test pins the promoted default AND that pre-granularity
   reports still parse as session). This is the **first real-evidence lever
   promotion** under the promotion-provenance rule: Postgres runtime, pinned
   real corpus, labeled reader/judge, QA paired CI excluding zero. STATUS
   rung-4 row updated with this proof pointer; the rung 4 checkbox stays
   open because the rung's implementation contract is reflect-stage
   contextual-chunk metadata (chunk ID / parent episode / citation tests),
   and what is proven here is the ingestion-granularity embodiment — the
   runtime chunking write path is the named follow-up.
3. **Reader upgrade verdict:** engine landed (codex), but "stronger reader"
   is empirically false on this evidence at 30q — recorded, not asserted.
   Judge upgraded to gpt-5.6-terra@medium either way.

## Cost (fresh CLI calls this campaign)

3 smoke + 33 (luna 30q) + 36 (terra@low 30q) + 36 (terra@medium 30q)
+ 119 (a) + 75 (b) + 114 (c) = **416 fresh codex calls** via the script
(reader + judge; judge only on containment misses), plus 2 ad-hoc codex CLI
probe calls during flag discovery — well under the 800 hard cap. Sensitivity
artifacts were regenerated from cache (fresh_calls=0). Wall time ~2.5 h:
three full n=100 ingest+recall bench runs plus serialized codex scoring
(~10–14 s/call).

## Deviations / limits

- **Reader model choice deviates from the brief's preference** (cheaper luna
  for reading): luna@low measured −0.100 QA vs haiku on identical evidence,
  so terra@medium reads AND judges (accuracy > cost per owner directive).
  Effort `high` was not probed (reader capacity is not the binding
  constraint at 30q; 3× latency).
- **OpenRouter fallback not implemented**: codex model resolution succeeded,
  so the fallback branch was never needed; `--engine` accepts claude|codex
  only.
- **Ingest reuse**: runs (a) and (b) each re-ingested the session-granularity
  corpus (bench-lme has no recall-twice-per-ingest mode). At ~10 min per
  n=100 run this was cheaper than the code change; the reader lane reuses
  each run's emitted JSONL, so scoring never re-ingests.
- The QA-side rerank comparison stays not-significant at n=100; the keep
  verdict rests on the retrieval CIs (both depths) plus QA direction.
- Turn-window retrieval-side abstention degrades (0/6 vs 3/6 session-side:
  more episodes → an answer-session item almost always surfaces) while
  reader-side abstention improves to 6/6 — with finer evidence the reader
  correctly says "I don't know"; the composed system is what QA measures.
- Judge is an LLM on ~20–27% of rows (containment misses); spot-checked, not
  human-verified. Codex CLI exposes no temperature control; determinism is
  best-effort (prompt + cache), and recorded replies are the judged replies.
- This is still not a LongMemEval leaderboard claim: k=10 evidence packs,
  8192-token budget, no per-type prompting, one seed.

## State of play 2026-07-10 (scaled reader round)

**DONE:** codex reader engine landed and gated; n=100 stratified runs for
session±rerank and turns; rerank-off default re-confirmed with CIs excluding
zero on both retrieval depths; turns granularity promoted to lane default on
the first QA paired CI excluding zero (first real-evidence promotion);
reader-sensitivity quantified (haiku ≈ terra ≫ luna on this evidence).

**NEXT (ranked):**
1. **Runtime contextual-chunk write path** (rung 4's actual implementation
   contract): retain/reflect-stage chunking with context headers, so callers
   get the promoted granularity without client-side windowing; then the
   paired ablation on this same harness can flip the rung-4 checkbox.
2. **Window-size ablation (2/4/8 turns) + packing budget/ordering** on the QA
   axis at n=100 — TURNS_WINDOW=4 was never itself tuned; the pack drops
   top-10 items and is a named suspect for the remaining multi-session and
   temporal-reasoning losses (.41 each).
3. **Preference/temporal retrieval levers** — single-session-preference is
   0.33 QA even under turns; query formulation and temporal filters are the
   remaining named gaps from the 30q campaign, now measurable with power.

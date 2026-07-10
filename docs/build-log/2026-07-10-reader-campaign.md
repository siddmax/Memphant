# 2026-07-10 — Reader Campaign (LongMemEval-S, reader-scored QA + granularity experiment)

**Scope statement (read first):** this campaign adds an end-to-end **reader-scored
QA** lane on top of the retrieval-only lane from
`2026-07-10-real-retrieval-campaign.md`, and runs the granularity and rerank
experiments with BOTH metrics. Provenance labels on every artifact:
runtime=postgres, embeddings=fastembed bge-small-en-v1.5,
reader=`claude-haiku-4-5` (CLI headless, model id `claude-haiku-4-5-20251001`),
judge=`containment+claude-haiku-4-5`, sample=30 seed=20260710 k=10, dataset
sha256 `08d8dad4…7894` (pinned). This is **not** a LongMemEval leaderboard
claim — see "Distance to SOTA" below.

## Method

- Retrieval lane unchanged from the prior campaign (`memphant-eval bench-lme`,
  fresh tenant per question, chronological ingestion, reflect, recall k=10,
  budget 8192; Recall@k scored by `citation_episode_id` → session provenance).
  Identical stratified 30-question sample (28 scored + 2 abstention) at seed
  20260710 for every run.
- **Reader lane** (new): `bench-lme --emit-qa` writes one JSONL row per
  question — question, `question_date`, gold answer, and the top-k recalled
  item bodies (each carrying its `[session <id>] [date <date>]` prefix) with
  session provenance. `scripts/run_reader.py` then drives the Claude CLI in
  headless mode, serialized, one call at a time:
  - Reader: `claude -p "<evidence pack + question>" --model
    claude-haiku-4-5-20251001 --system-prompt "<answer ONLY from evidence;
    terse; else exactly: I don't know>" --tools "" --no-session-persistence
    --setting-sources ""`. (The CLI exposes no temperature flag; terseness is
    enforced by the system prompt. ~5 s/call.)
  - Judge: word-boundary normalized containment first (case/punct-insensitive;
    short numeric golds must match as whole tokens). Only non-matches spend one
    LLM judge call (same model): "Question / gold answer / model answer → does
    the model answer convey the gold answer? yes/no". Abstention questions
    (`_abs`) score correct only on an "I don't know" reply.
  - Honesty/budget guards: every reply cached by sha256(model+kind+prompt);
    hard fresh-call budget with partial-result abort (n recorded); CLI errors
    recorded per question as `reader_error` and excluded from n_scored (none
    occurred). Total fresh CLI calls this campaign: **107** (3 smoke + 104
    scoring), under the ~200 budget.
  - Design note: the reader/judge is driven from a Python script over emitted
    JSONL rather than `std::process::Command` inside the Rust lane — simpler,
    and re-scoring (e.g. the containment tightening below) is free from cache
    without re-ingesting.
- **Granularity** (new): `--granularity turns` ingests each haystack session as
  episodes of ≤4 consecutive turns (body prefix `[session <id>] [date <date>]
  [turns a-b]`); provenance still maps every episode to its session. Default
  `session` is byte-identical to the prior campaign (the fresh baseline run
  reproduced R@5 0.500 / R@10 0.607 exactly).
- Paired per-question deltas with bootstrap 95% CI (1000 resamples, seed
  20260710) for retrieval (Rust lane) and QA accuracy (reader script).

## Commands

```
cargo build -p memphant-eval --features fastembed --release
# per config (session | turns | session --disable rerank):
./target/release/memphant-eval bench-lme \
  --database-url postgres://memphant:memphant@localhost:5432/memphant \
  --data benchmarks/data/longmemeval_s.json --sample 30 --seed 20260710 --k 10 \
  --granularity <session|turns> [--disable rerank] \
  [--baseline …/lme-s-reader-session.json] \
  --emit-qa …/reader-evidence-<cfg>.jsonl --out …/lme-s-reader-<cfg>.json
python3 scripts/run_reader.py --evidence …/reader-evidence-<cfg>.jsonl \
  --out …/reader-<cfg>.json --label <cfg> \
  --retrieval-report …/lme-s-reader-<cfg>.json \
  [--baseline …/reader-session.json] --max-calls 60
```

Artifacts: `docs/build-log/artifacts/real-retrieval-20260710/lme-s-reader-*.json`
(retrieval) and `reader-*.json` (QA, with per-question replies and judge
methods). The `reader-evidence-*.jsonl` packs embed raw LongMemEval text and
stay gitignored like the dataset itself.

## Results (n=30; 28 retrieval-scored, 30 QA-scored incl. 2 abstention)

All three runs: lane flags at bench defaults (all stages on) unless noted;
`rerank-on` below means the lane's explicit `rerank_enabled=true`, which was
the pre-2026-07-10 production default.

| Config | R@5 | R@10 | QA accuracy | ΔQA vs baseline [95% CI] |
|---|---|---|---|---|
| session, rerank-on (baseline) | 0.500 | 0.607 | **0.433** | — |
| turns, rerank-on | 0.607 | 0.607 | 0.367 | −0.067 [−0.200, +0.067] — no |
| session, rerank-off | 0.643 | 0.643 | **0.467** | +0.033 [0.000, +0.100] — no |

Paired retrieval deltas (vs session/rerank-on baseline, n=28):

| Config | ΔR@5 mean [95% CI] | ΔR@10 mean [95% CI] | CI excludes 0? |
|---|---|---|---|
| turns | +0.107 [0.000, +0.250] | 0.000 [−0.107, +0.107] | no |
| session rerank-off | **+0.143 [+0.036, +0.286]** | +0.036 [0.000, +0.107] | **yes (R@5)** — reproduces prior campaign |

QA accuracy per stratum (session-baseline / turns / session-rerank-off):
knowledge-update .60/.20/.60 (n=5), multi-session .375/.375/.50 (n=8, incl. 1
abs), single-session-assistant .67/.67/.67 (n=3), single-session-preference
.00/.00/.00 (n=2), single-session-user .50/.50/.50 (n=4),
temporal-reasoning .375/.375/.375 (n=8, incl. 1 abs).

Reader abstention behavior: both `_abs` questions answered "I don't know" in
all three runs (2/2 correct reader-side; the retrieval-side abstention score
stays 1/2 because recall still returned an answer-session item for one).

Judge-method mix (session baseline): 22 containment, 6 LLM-judge, 2
abstention-exact. Judge calls were spent only on containment misses.

## What the granularity lever bought

**Nothing promotable — and QA says it is directionally harmful.** Turn-window
ingestion (≤4 turns/episode) raised Recall@5 by +10.7 pts mean, but the CI
touches zero AND end-to-end QA accuracy went *down* 6.7 pts mean (−.20, +.067):
knowledge-update collapsed .60→.20 — smaller episodes retrieve the
answer-bearing session yet hand the reader fragments that lose the update
context. The biggest hypothesized lever did not survive reader scoring.
`session` stays the lane default.

## Rerank verdict

The decision rule was: revert the rerank-off production default only if
reader evidence shows rerank-on HELPS QA accuracy with a CI excluding zero.
Measured (session granularity, better of the two): rerank-off − rerank-on =
**+0.033 QA [0.000, +0.100]** — rerank-on does not help; the point estimate
says it hurts QA too, consistent with its retrieval harm (−0.143 R@5, CI
excludes zero, reproduced this campaign). **The rerank-off default in
`memphant-core/src/service.rs` stands.** No code change.

## Distance to SOTA (honest)

Published reader-scored LongMemEval-S numbers are ~86–94% QA accuracy with
large answer models over oracle-ish or heavily tuned retrieval. Our 0.433–0.467
uses claude-haiku-4-5 over top-10 (budget-packed, often 4–8 surviving) items
at n=30 — **comparable only to itself**, not to the leaderboard: different
reader capacity, different evidence budget, no per-type prompting, small
sample. What the paired data says the gap is made of, in order:
1. **Retrieval ceiling:** R@10 = 0.643 best-case — a third of questions never
   see the answer session; no reader fixes that. Single-session-preference is
   0/2 at retrieval and 0/2 at QA in every run.
2. **Reader abstains on retrieved-but-diffuse evidence:** recall hits but the
   packed fragments don't carry the answer sentence (e.g. multi-session
   aggregation questions answered "I don't know" despite hits).
3. **Packing budget:** 8192 tokens keeps only 4–8 of 10 items; the dropped
   tail sometimes holds the answer-bearing body.
Next levers, ranked: retrieval recall on preference/temporal strata (query
formulation, temporal filters), packing budget/ordering ablation on the QA
axis, a stronger reader as an upper-bound probe, n=100+ for CI width.

## Rung adjudication (STATUS §3)

Advance-when bar applied this round: a QA-accuracy paired delta whose 95% CI
excludes zero on the rung's axis, produced by the Postgres runtime on the
pinned corpus with a labeled reader/judge. **No rung meets it.** Notes
refined in STATUS for rungs 4 (turn-window chunking measured: retrieval ns,
QA directionally negative), 7 (reader-side abstention 2/2 but n=2), and 8
(reader-scored confirmation that the disabled rerank stays disabled). No
checkbox flips.

## Deviations / limits

- n=30 stays small; QA CIs are wide (a +10-pt true effect can hide). Zero
  observed promotion evidence is still zero promotion evidence.
- The reader CLI exposes no temperature control; determinism is best-effort
  (prompt + caching). Reruns re-score from cache, so the recorded replies are
  the replies that were judged.
- Containment judging was tightened to word-boundary matching after the first
  scoring pass; re-scoring from cache changed **zero** verdicts (fresh_calls=0
  on all three re-runs).
- Reader-side judging trusts a haiku-class LLM judge on ~20% of rows
  (containment misses); spot-checked, not human-verified.
- One turns-run reader/judge prompt collided with a session-run prompt in
  cache (identical evidence pack) — expected, since some questions retrieve
  identically at both granularities.

## State of play 2026-07-10 (reader round)

**DONE:**
- Reader-scored QA lane landed and gated (`bench-lme --emit-qa` +
  `scripts/run_reader.py`; fmt/clippy/tests/pytest/spec-drift green); labels
  reader/judge/runtime/seed on every artifact; call-budget + cache guards.
- Granularity experiment run paired on the pinned 30q sample: turns rejected
  (retrieval ns, QA −6.7 pts mean); `session` remains default.
- Rerank question re-run end-to-end with reader scoring: rerank-off default
  **stands** (rerank-on helps nothing; retrieval harm reproduced with CI
  excluding zero).
- STATUS rung notes refined (4, 7, 8); no rung advanced — no QA paired CI
  excludes zero.

**NEXT (ranked):**
1. **Remaining rung evidence on the QA axis** — the two levers the paired data
   points at: preference/temporal retrieval misses (rung 4/5 axes: query
   formulation, temporal filters) and a packing-budget/ordering ablation
   (rung 7 axis) — each is a cheap paired run on the existing lane; rung
   promotion now has a working reader-scored bar to clear.
2. **STATE-Bench-style task suite** — rung 10 (procedural) and rung 5
   (temporal validity) cannot be exercised by LME-S chat sessions at all;
   this is the only route to evidence on those axes.
3. **Syndai RAG/KB replacement gate** — dogfood cutover (WS-F) requires
   MemPhant to **beat the existing knowledge stack on its own golden set**
   before replacing it; the reader lane built here is the scoring harness for
   that head-to-head.

**Why this order:** (1) is hours of work against an existing harness and
directly attacks the measured retrieval ceiling (the dominant loss term);
(2) unblocks the rungs that no amount of LME-S running can touch; (3) is the
first external consumer and the actual product gate, but it needs the
per-axis levers from (1) to have a chance of clearing its bar.

# MemPhant Completion Handoff — 2026-07-04

Audience: a goal-based Codex session (or any agent) finishing MemPhant to an
*honest* COMPLETE. Written after a cross-repo audit (Syndai ⟷ Memphant) on
2026-07-03/04.

## 1. Where things actually stand

Green and real (verified by running the gates locally):

- 44 Python contract tests + 91 Rust workspace tests pass. ~16.7K LOC across 8 crates.
- All five verbs (retain/reflect/recall/correct/forget), all recall channels
  (exact/FTS/vector/temporal/edge), trace spine, golden oracle, REST/MCP/SDK/CLI/web,
  Docker/Compose. WS-0..WS-I exit packets exist in `docs/build-log/`.
- Rungs 4–15 levers are implemented behind flags with enable/disable control evidence.
- No public SOTA claim is made anywhere (correct per spec).

Gaps that keep STATUS.md's `COMPLETE` banner dishonest by its own DONE definition:

| # | Gap | Evidence |
|---|-----|----------|
| G1 | "Public benchmark" proofs are synthetic. Rung 4–15 promotions and the public-launch "one reproduced public benchmark profile" rest on 2 hand-authored YAML cases per rung in `examples/evals/public-sampled/` that *cite* dataset IDs but never load real data. Inside `docs/build-log/artifacts/rung15-inferred-belief-composition-profile.json` the restraint (op-bench), longitudinal (memorystress), interactive, embedding_selection, and procedural axes are literally `source_status: not_run`. | `benchmarks/rung*-sampled.yaml`, `examples/evals/public-sampled/*.yaml` (27 lines each) |
| G2 | Public-launch scorecard says `candidate_pass`, but STATUS.md checks the box as done. | `docs/launch/public-launch-scorecard.json` |
| G3 | Restraint gate "0.0 drop" was measured on 2 synthetic cases scoring 1.0/1.0 both sides — trivially green, no over-retrieval harm actually measured. GateMem "first internal reproduction" is 2 security-fixture cases, not GateMem scenarios. | `docs/launch/restraint-launch-scorecard.json`, `gatemem-conditional-scorecard.json` |
| G4 | Syndai dogfood proof path is stale. A follow-up check on 2026-07-04 found the adapter (`backend/src/features/memory/memphant_dogfood_adapter.py`, 273 lines, config-gated, 10 tests) on Syndai `main` via `869df0130`, but MemPhant still pointed `.codex/linked-repos.json` and drift checks at the removed worktree `/Users/sidsharma/Syndai/.wt/codex-memphant-cross-repo`. The remaining work is to re-proof dogfood against current Syndai `main`, update the cited commit/artifact, and keep the linked repo path live. | `docs/build-log/2026-07-03-dogfood-active-read-gate.md`, `.codex/linked-repos.json`, `scripts/check_spec_drift.py` |
| G5 | Standing bars checked against toy conditions: hot-path SLO test runs on an in-memory store; `memory_utility_trend` has baseline == current on one surface. | `crates/*/tests/hot_path_slo.rs`, `docs/launch/standing-quality-bars.json` |

## 2. Codex session prompt (paste as the goal)

```text
GOAL: Make MemPhant's STATUS.md banner COMPLETE *honest* — every checked box backed
by proof that satisfies its owner contract (27-sota-ladder-and-validation.md §1–2,
29-implementation-plan.md §5–§8, 12-data-methodology-and-benchmark-inventory.md),
or the box is downgraded to reflect reality. Fabricating, relabeling, or shrinking
a contract to fit existing artifacts is failure.

Repo: /Users/sidsharma/Memphant. Sibling repo: /Users/sidsharma/Syndai
(dogfood adapter now lives on Syndai main; keep `.codex/linked-repos.json`
pointed at the live source of truth).
Read docs/handoff/2026-07-04-completion-handoff.md §1 (gap list) and §3 (data
sourcing recipe — follow it; do NOT download full benchmark corpora).

Sub-goals, in order:

SG1 — Real sampled-public profiles (fixes G1, G2).
  Build scripts/ingest_public_bench.py that materializes N=50–100 seeded-sample
  cases per benchmark into examples/evals/public-sampled/<bench>/ in the existing
  case-YAML format, with source_span provenance pins (hf:<repo>@<rev>/<file>:<id>),
  using the streaming/row-API recipe in the handoff §3 (no full downloads; total
  cache budget ≤ ~300 MB under ~/.cache/memphant-bench). Wire them into
  benchmarks/*.yaml, re-run the rung 4–15 paired profiles (lever on vs off) with
  the memphant-eval runner, archive new trace JSONs + profile JSONs under
  docs/build-log/artifacts/, and update each rung's build-log doc. A rung whose
  paired delta no longer clears its advance-when contract gets UNCHECKED in
  STATUS.md with the measured number — that is a success outcome, not a failure.
  Grade deterministically (answer-bearing-ID containment / exact-match), no paid
  LLM-judge APIs.

SG2 — Restraint + GateMem on real data (fixes G3).
  Restraint: sample OP-Bench (and PS-Bench if reachable) cases per §3; measure
  rel_drop_vs_memfree on the sampled set; regenerate
  docs/launch/restraint-launch-scorecard.json with the real number. Gate passes
  only if drop ≤ 0.15. GateMem: clone rzhub/GateMem (small, MIT), reproduce a
  seeded sample of its multi-principal scenarios in the MemPhant harness, and
  regenerate the gatemem scorecard from utility + access-control + forgetting
  measured simultaneously on those scenarios.

SG3 — Public launch gate to a true pass (fixes G2).
  With SG1+SG2 artifacts in place, re-evaluate every §7 criterion, flip
  docs/launch/public-launch-scorecard.json from candidate_pass to pass only if
  all criteria hold with the new profile as the "one reproduced public benchmark
  profile" (real sampled tier is acceptable per spec 12 §"sampled-public";
  full-public is NOT required). Otherwise keep candidate_pass and uncheck the
  STATUS.md box with a one-line reason.

SG4 — Syndai dogfood truthed up (fixes G4).
  In the Syndai repo: verify the adapter is present on current main and run the
  Syndai preflight pipeline (bash .claude/skills/preflight/run.sh; never bypass
  the push gate). Then, with MEMPHANT_FILE_MEMORY_DOGFOOD_ENABLED=true
  against a locally running MemPhant server, re-run the trace-compare for
  syndai_agent_file_memory_001 and regenerate
  docs/build-log/2026-07-03-dogfood-active-read-gate.md's artifact with the real
  post-merge Syndai/main commit hash. Also refresh Syndai main's stale
  docs/superpowers/specs/memphant/STATUS.md to mirror the Memphant ledger.

SG5 — Standing bars with teeth (fixes G5).
  Re-run the hot-path SLO check against a Postgres-backed store seeded with the
  SG1 sampled corpus (thousands of units, not 5), record p50/p95 in
  docs/launch/standing-quality-bars.json; give memory_utility_trend a real
  baseline-vs-current pair (pre/post SG1 runs) instead of 1.0 == 1.0.

SG6 — Ledger reconciliation.
  Update Memphant STATUS.md: every box either (a) checked with the new proof
  path pasted on the line, or (b) unchecked with the measured shortfall. Flip
  the banner COMPLETE only if §1–§6 are all honestly checked. Mirror the final
  ledger to Syndai. Keep `python -m pytest tests/ -q` (44+) and
  `cargo test --workspace` (91+) green throughout; run
  scripts/check_spec_drift.py before finishing.

Guardrails:
- Never edit a spec contract (27/29/12) to make an artifact pass; specs move only
  via 26-decision-register.md with rationale.
- No paid model APIs for judging or embeddings (deterministic grading; embeddings
  per §3.5 of the handoff).
- Every generated artifact must be reproducible from a committed script + seed.
- Report, per sub-goal: what was measured, the number, pass/fail vs contract.
```

## 3. Cheap eval/benchmark data — where from, without large downloads

Principle: the spec's `sampled-public` tier (`12` §tiers) is the target — a
**small, seeded, reproducible subset with pinned provenance**. Nothing requires
the full corpora. Budget: ≤ ~300 MB cache total, zero paid APIs.

### 3.1 The three cheap access patterns (in order of preference)

1. **HF datasets-server rows API — zero download.** Any public HF dataset:
   `GET https://datasets-server.huggingface.co/rows?dataset=<id>&config=<c>&split=<s>&offset=<k>&length=100`
   returns JSON rows. Page a seeded set of offsets, keep only sampled rows.
   Perfect for BEAM-scale corpora.
2. **`load_dataset(..., streaming=True)`** (`pip install datasets`) — iterates
   without materializing; take the first N after a seeded `.shuffle(seed=..., buffer_size=...)`.
3. **`hf_hub_download` of one small file** — when the dataset ships a compact
   questions/metadata file (e.g. `questions.jsonl`), fetch just that file at a
   pinned revision; never the haystack shards.

Pin every sample: record `hf:<repo>@<revision>/<file>:<row-id>` as `source_span`
(the format already used in `examples/evals/public-sampled/`). Cache under
`~/.cache/memphant-bench/` keyed by `(dataset, revision, seed, n)`.

### 3.2 Per-benchmark sourcing (pins from spec `12` §inventory)

| Axis | Benchmark | Source pin | Cheap recipe | Est. size |
|------|-----------|-----------|--------------|-----------|
| Long-horizon | LongMemEval-V2 | `hf:xiaowu0162/longmemeval-v2@2026-05-17` | `hf_hub_download` only `questions.jsonl` (already the pinned file in the existing fixture); for evidence sessions prefer the *oracle/evidence-only* variant (few MB) over the S/M haystack variants; if a haystack session is needed for a specific question, pull just those rows via the rows API. Sample 50–100 questions, seed=1337. | ~10–50 MB |
| Scale | BEAM 100K tier | `hf:Mohammadta/BEAM@2026-01-30` (pin already in fixture: `data/100K-00000-of-00001.parquet`) | Do NOT download the parquet. Rows API with seeded offsets, or `streaming=True` over the 100K config; take 50 conversations. Skip 1M/10M tiers entirely — spec never requires them at sampled tier. | ~20–100 MB |
| Longitudinal | MemoryStress | `hf:singularityjason/memorystress` (Apache-2.0) | Streaming sample: 5–10 of the 1,000 sessions including ≥5 contradiction chains; enough for a degradation-curve smoke, mark `sampled` in the profile. | ~10 MB |
| Restraint | OP-Bench (arXiv 2601.13722) + PS-Bench (2601.17887) | Locate the paper's release repo/HF id at ingestion (spec `12` says re-verify at ingestion) | These are prompt/scenario suites, inherently small. Clone or rows-API; sample 50 scenarios; measure `rel_drop_vs_memfree` exactly as the scorecard schema expects. | <10 MB |
| Multi-principal | GateMem | GitHub `rzhub/GateMem` (132★, MIT) | `git clone --depth 1` — small repo. Reproduce a seeded subset of scenarios in the MemPhant harness (spec `12` requires reproduce-first before it gates anything). | <20 MB |
| Outcome / interactive / embedding-selection / procedural | STATE-Bench, EMemBench, LMEB, SkillOS | per `12` inventory | OPTIONAL for the launch gates — `29` §7 needs **one** reproduced public profile. Leave these axes `not_run` with `external_profile_required`; that is spec-legal. Do not spend budget here. | 0 |

License note: spec `12` flags BEAM's leaderboard as vendor-run — using the *data*
at sampled tier with `source_status: sampled-public` is fine; never quote board
numbers. Re-verify each dataset's license line in `12` at ingestion time.

### 3.3 How many cases are enough

The rung advance-when contracts are **paired deltas (lever on vs off)**, not
absolute SOTA numbers. 50 cases/benchmark gives a sign-correct paired comparison
and keeps a full 12-rung re-run in minutes on a laptop. Use the same seed across
baseline and lever runs so pairs align. If a delta's CI straddles zero at n=50,
escalate that one rung to n=200 (rows API makes this a parameter change, not a
download).

### 3.4 Grading — keep it deterministic

Reuse the golden-oracle mechanics: convert each sampled item into the existing
case-YAML shape (`expect.answer_bearing_ids`, `top_k_contains`,
`citations_include`). For free-text benchmark answers, grade by normalized
containment of the gold answer string in the packed context — no LLM judge. This
matches the repo's existing oracle and costs $0. (House rule: no API-key Claude/
OpenAI calls for judging.)

### 3.5 Embeddings

The store schema has `embedding` columns but the repo ships no embedding model;
current channels are exact/FTS/lexical-vector. Options, cheapest first:
1. Keep the existing deterministic representation for the paired-delta runs —
   deltas are lever-vs-lever, so a shared cheap embedder does not bias the pair.
2. If a real dense vector is wanted: `fastembed` (ONNX, CPU) with
   `BAAI/bge-small-en-v1.5` — one-time ~130 MB model download, then fully local.
Never a hosted embedding API.

## 4. Definition of done for the handoff consumer

- `STATUS.md` banner state is *derivable from artifacts*: an auditor rerunning
  `pytest`, `cargo test`, the eval runner on the committed benchmark YAMLs, and
  reading `docs/launch/*.json` reaches the same checked/unchecked set.
- No scorecard says `candidate_pass` behind a checked box.
- Syndai main contains the merged dogfood adapter and a mirrored, current ledger.
- Every sampled dataset is reconstructible from `scripts/ingest_public_bench.py`
  + pinned revision + seed, with nothing >300 MB cached.

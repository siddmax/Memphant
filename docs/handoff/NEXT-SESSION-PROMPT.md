# Next-Session Prompt (paste this to resume the MemPhant SOTA campaign)

You are resuming the MemPhant SOTA campaign in /Users/sidsharma/Memphant (sibling
repo /Users/sidsharma/Syndai; spec mirror must stay drift-free via
`python3 scripts/check_spec_drift.py` after any spec edit + rsync). Read, in
order: docs/handoff/2026-07-10-sota-campaign-handoff.md (umbrella state),
docs/build-log/2026-07-10-reader-campaign.md (n=30 verdicts),
docs/build-log/2026-07-10-scaled-reader-campaign.md (n=100, has
RESULTS_PLACEHOLDER if scoring didn't finish), docs/superpowers/specs/memphant/STATUS.md
(ledger + promotion-provenance rule) and 27-sota-ladder-and-validation.md §2
(rung gates). AGENTS.md holds the full local gate; run it before claiming done.

## Where things stand (2026-07-10 end of session)

- Runtime is COMPLETE and proven: `bash scripts/e2e_probe.sh` (needs the
  memphant-postgres-1 container on :5432) must stay green.
- Benchmark truth so far (LongMemEval-S pinned sha256 in
  benchmarks/manifests/longmemeval_s.lock.json; runtime=postgres; fastembed
  bge-small; seed 20260710): n=30 retrieval R@5 0.500/R@10 0.607; haiku-read QA
  0.433 baseline, 0.467 rerank-off; rerank HARMFUL at retrieval (+0.143 R@5
  when disabled, CI excl. 0) → rerank off by default (`7dad881`); 4-turn-window
  ingestion falsified at n=30 (retrieval up ns, QA −6.7). No rung 4–13/15
  promoted; 14 retired.
- Scaled n=100 campaign is MID-FLIGHT: all three retrieval runs + evidence
  JSONLs exist under docs/build-log/artifacts/real-retrieval-20260710/
  (scaled-lme-s-{session-rerank-off,session-rerank-on,turns-rerank-off}.json +
  reader-evidence-scaled-*.jsonl, gitignored); reader engine = Codex CLI,
  model gpt-5.6-terra (scripts/run_reader.py --engine codex, committed
  `85ac338`; sha256 reply cache means re-scoring is resumable for free).

## Do next, in order

1. **Finish the scaled scoring**: run scripts/run_reader.py for the three
   configs exactly as documented in the command block of
   docs/build-log/2026-07-10-scaled-reader-campaign.md; fill the
   RESULTS_PLACEHOLDER/DEVIATIONS_PLACEHOLDER sections with the real table
   (QA acc + paired bootstrap CIs vs session-rerank-off). Verdicts to apply:
   rerank default keep/revert per QA CI at n=100; turns falsification
   confirm/overturn; any positive-CI lever ships as default (code + tests +
   STATUS rung row with proof pointer — first real-evidence promotion).
   Commit `docs(memphant): scaled n=100 reader campaign + verdicts`; sync
   mirror; Syndai commit staged to docs/superpowers/specs/memphant only.
2. **Next accuracy levers (reader-scored, paired, same seed)** — weakest
   strata are knowledge-update and temporal-reasoning: (a) contextual chunks
   WITH session-context headers (rung 4 axis — window bodies got falsified,
   headers may fix it); (b) update-chain surfacing (supersedence-aware recall
   for "current X" questions); (c) evidence-pack size k sweep (10→15→20);
   (d) query-date-aware temporal filtering. Promote only on QA paired CI
   excluding zero at n=100.
3. **Rung evidence with the right corpora** (doc 27 §2 gates): rung 10 needs
   STATE-Bench-style tasks; 11 the longitudinal suite; 15 an OP-Bench
   restraint check; 13 archived-trace training floor (bench lane emits real
   traces now).
4. **STATE-Bench first-mover run** (spec's primary target; paired
   memory-on/off ablation to attribute the delta).
5. **Syndai RAG/KB replacement gate**: build a golden set mined from Syndai's
   own knowledge corpus (backend/src/features/knowledge/, tables
   knowledge_sources/sections/chunks); MemPhant must BEAT it before replacing
   (same gate later for episodic memory and the coding-continuity lane over
   the 62k coding_execution_attempt_events; design in
   docs/superpowers/plans/2026-07-09-sota-gap-closure.md Phase 2(b)). First
   wire the dogfood flag (`memphant_file_memory_dogfood_enabled` in Syndai
   backend/src/core/config.py) against a deployed MemPhant.

## Known small fixes queued

- Dead `include_trace` request flag removal (types + OpenAPI regen).
- Internal golden/security/ops eval subcommands are in-memory only — add
  --database-url paths so they also gate the Postgres runtime.
- RLS policies + non-owner runtime role (currently app-layer tenancy only,
  stated in the 2026-07-09 plan NOT-in-scope); typed DTO timestamps.
- R79 adapters (Claude Code memory_20250818 file adapter + Hermes provider
  SPI) — the distribution wedge once accuracy plateaus.

## Rules that bind you

- Promotion-provenance rule (STATUS header): evidence only from the packaged
  Postgres runtime on pinned real corpora with executed scorers; synthetic
  fixtures gate regressions, never promotions. Never fabricate a number.
- Accuracy/UX > cost > perf/latency (owner authorized answer-model budget
  until SOTA; cost optimization comes after).
- Six verbs + tri-domain contract are frozen; one substrate; no competitor
  code in the dependency tree (adapters = distribution only).
- Full local gate (AGENTS.md) + spec-drift green before any "done" claim;
  commits small with Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>.

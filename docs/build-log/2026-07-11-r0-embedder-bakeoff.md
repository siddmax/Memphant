# R0 Embedder Bakeoff — 2026-07-11

Plan of record: `docs/reports/2026-07-11-prosumer-memory-campaign-report.md` §9 (R0).
Pre-registration: `.superpowers/sdd/r0-plan.md` (decision rules frozen before any run; arms finalized
by a same-day primary-sourced web sweep). Base `b4c8d8c`; code commits `19d305f` (seam + local arms),
`83bfe34` (qwen3), `f65733e` (API providers + shared grammar), `0a515c4` (gate harness + docs golden v2),
`f7aed1c` (voyage-4-large), `cf17b35` (runner hardening), `13af31e`+`d4d4e6a` (code-lane fixture).
All arms ran through the packaged Postgres runtime (server+worker via `gate_run_memphant.py`, fresh
scratch DB per arm, ingest-once/recall-both-sets) or `bench-lme` (chat), scored on the openrouter
lattice `openai/gpt-5.6-terra@medium` reader + `anthropic/claude-sonnet-5` judge, paired bootstrap
(1000 resamples). Full CI table: `docs/build-log/artifacts/r0-embedder/r0-verdict-cis.json`.

## Verdicts (pre-registered rules applied)

| Rule | Outcome |
|---|---|
| API ships only on ≥+0.030 docs-QA vs best local, CI floor > 0, both golden sets | **NO API PROMOTION.** Best case voyage-context-4 vs modernbert: +0.050 [−0.017,+0.133] v1 / +0.017 [0.000,+0.050] v2 — fails margin and floor. voyage-4 +0.033/+0.033, floors ≤0. All other API arms worse. |
| Docs-lane winner = best local when API fails | **modernbert-embed-large@1024** (fastembed ONNX): QA .183/.100 vs small .133/.067 (+0.050/+0.033, direction-consistent, CIs include 0 → plan-selected, NOT CI-promoted). Becomes the R1 gate-flip embedder; shipped global default unchanged until R1 evidence. |
| Chat default switches only on CI excl 0 at n=100 + held-out confirm | **bge-small stays.** base −0.010 ns, modernbert −0.010 ns, gemma +0.000 ns (n=100, seed 20260710). Held-out confirm skipped — nothing to confirm. Third consecutive replication of "chat lane is not embedder-bound" (R@10 .83–.94; reader is the constraint). |
| Code sub-bakeoff (40Q sample) | **No API case.** voyage-code-3 .275 vs modernbert .250 vs small .225; all paired CIs include 0. Revisit at R4 with the full mined golden set. |
| CPU viability (rule 6) | **qwen3-0.6b retired from all lanes.** Chat: 8/100 questions in 3h17m live (~40h projected; T1b smoke 21.9/0.63 texts/s short/long). Docs: ran fine one-time (~75 min drain) but LOSES to modernbert (.150/.067 vs .183/.100) at ~50× CPU cost. |

## Full docs-lane table (QA accuracy v1 / v2; 60Q each; corpus 108 docs / 3254 sections)

| arm | v1 QA | v2 QA | v1 R@10 | v2 R@10 |
|---|---|---|---|---|
| small = bge-small-en-v1.5@384 (anchor, shipped default) | .133 | .067 | .100 | .067 |
| base = bge-base-en-v1.5@768 | .150 | .083 | — | — |
| **modernbert-embed-large@1024** | **.183** | **.100** | .083 | .083 |
| embeddinggemma-300m@768 | .183 | .083 | .133 | .067 |
| qwen3-embedding-0.6b@1024 (candle) | .150 | .067 | .117 | .067 |
| text-embedding-3-small@1536 (Syndai-parity control) | .167 | .083 | — | — |
| gemini-embedding-001@3072 | .133 | .100 | — | — |
| voyage-4-lite@1024 | .200 | .100 | — | — |
| voyage-4@1024 | .217 | .133 | — | — |
| voyage-4-large@1024 | .167 | .117 | .133 | .100 |
| voyage-context-4@1024 (contextualized) | .233 | .117 | .150 | .083 |

Secondary findings, all evidence-grade:

- **The control arm settles the W10 diagnosis.** Syndai's own embedder (text-embedding-3-small) on our
  stack reaches only .167 on the same golden set where Syndai scores .217 — so the remaining gate gap
  is **heading-path context + fusion behavior, not embedding dimensionality**. That is R1's work, and
  it benefits whichever embedder is installed. (Corroborating: small itself rose .050→.133 on this very
  set since W10 purely from the wave's default-path fixes.)
- **voyage-context-4 vs small v1 is the campaign's only CI-clean delta** (+0.100 [+0.033,+0.183]): the
  contextualized API genuinely beats the OLD default — but not the new best local by the pre-registered
  margin, and privacy/egress + cost break every tie local (plan rule). The contextualized-vs-plain
  question (context-4 .233 vs voyage-4 .217 v1, reversed .117 vs .133 on v2) is UNRESOLVED at n=60.
- **Vendor-menu vs reality:** every voyage-4-family model returns 1024-d by default (live-probed);
  voyage-4-large UNDERPERFORMS voyage-4 and context-4 here, consistent with Voyage's own claim that
  context-4 ≥ 4-large.
- **Run-to-run variance measured:** three same-config small runs (two binaries) flip ≤1 question per
  set (re-ingest tie-breaking on fresh UUIDs, adjudicated NOT binary drift; same-binary re-run also
  flips 1). Deltas ≤2 questions at n=60 are inside noise. Follow-up filed: stable secondary sort key
  in recall ordering.
- **Harness artifact:** several arms report R@5 == R@10 exactly — the 8192-token pack budget trims
  below 10 items on this corpus, so hit@10 is right-censored. Check before quoting R@10 anywhere.
- **Chat lane table** (n=100, 20260710): small .56 | base .55 | modernbert .55 | gemma .56.
- **Code lane fixture:** 40Q span-grounded over 8 attempts / 600 events / 544k chars mined from local
  `syndai_local` (51,769 events; gap-exclusion measured 0%, not the plan's ~24% estimate — formula
  verified, the plan figure was wrong for this dataset). Privacy: content artifacts gitignored, only
  the lock committed. Known limitation: 8-attempt haystack = narrow diversity; R4 mines the full set.

## Incidents (all root-caused, fixed, and documented in `.superpowers/sdd/progress.md`)

1. T3 implementer's sandbox left a Homebrew postgres shadowing Docker's :5432 via IPv6 — stopped;
   advisory added (check `lsof -iTCP:5432` before queue launches).
2. Docs queue cascade: 20s health wait vs 1.5GB first-run model download at server boot; failure path
   leaked the child which held :39412 and fast-failed 8 arms. Fixed in `cf17b35` (10-min wait,
   child-exit fast-fail, port preflight naming the squatter PID, server log capture, try/finally).
   small+base pre-incident runs were valid; the rest re-ran clean.
3. Docker Desktop port-forward proxy wedged after day-long connection churn (host RST, in-container
   postgres healthy) — `docker restart` restored; campaign DB verified intact (168k units).

## Costs

Embedding APIs: ≈ $2 total (all six API arms, both sets + probes/smokes). Reader/judge scoring:
~1,700 fresh calls across 30 arm-sets ≈ $60–70 via OpenRouter. Mining v2 docs golden: ~$3 (mostly
cache-reused). Local arms: $0 (CPU).

## What R0 hands R1

- Embedder for the gate re-run: `MEMPHANT_EMBEDDINGS=modernbert` (grammar landed; no schema change
  needed — profiles isolate by id+dims).
- R1's real levers, in evidence order: heading-path context prepended at embed/ingest time (Syndai
  does this, we don't — the control arm isolated it), fusion behavior at 3k-section scale, then the
  contextualized-embedding arm (late-chunking local approximation vs voyage-context-4 API, still
  gated by the same ≥3pt rule if the API is re-tested at R1 scale).
- Open follow-ups: stable recall tiebreaker; R@10 budget censoring; fastembed-only CI matrix leg
  decision (T1b review F1); Retry-After clamp + parse()/factory dedup minors (T2 review M4/M5);
  old wave-scripts bash-3.2 bug (spun-off task was deleted unfinished).

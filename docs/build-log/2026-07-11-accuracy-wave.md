# 2026-07-11 — Accuracy Wave: measurement round (no promotions)

**Scope:** executes the 2026-07-10 canonical-plan addendum (11-lens research fleet →
W1-W10). Code half: gate hardening (`3814c2f`), vector-channel honesty (`43970a2`),
principled fusion + pool knob (`9f8e99d`), sibling-gather + session quota (`749369c`,
scan-window fix `8b3ab75`), temporal grounding (`b1c59b3`), deterministic fact
extraction (`24ee7f4`), reader prompt v3 routing (`48f62fa`), bge-base + cross-encoder
arms (`2c853e8`), adaptive chunk windows (`25ecdc0`), Syndai gate harness (`47070a9`,
`785fe4d`). Final whole-wave review: READY, 0 critical; the one Important finding
(quota inert in fast mode) was fixed before its arm was measured. e2e probe green on
post-wave code. Provenance: runtime=postgres, reader `gpt-5.6-terra@medium`
(OpenRouter), judge `claude-sonnet-5`, LME-S pinned sha256, n=100/seed, k=10,
artifacts `docs/build-log/artifacts/wave-20260711/`.

## Default-path changes shipped (cleanups, not measured levers)

W2 (vector scoring = SQL `<=>` top-32 with profile predicate; server/worker embed by
default), W3 (query-substring weight hacks deleted), W9 (adaptive chunk windows — no
tail truncation). Post-wave defaults vs pre-wave champion, same seed + lattice,
paired: **+0.030 [−0.030, +0.090] ns** — honest label: cleanup-neutral-to-mildly-up.

## Singles (seed 20260710, paired vs post-wave base QA 0.590)

| Arm | QA | ΔQA [95% CI] | Note |
|---|---|---|---|
| v3 routing (reader) | 0.600 | +0.010 [−0.050,+0.080] | tr +3 (ordering/counting, as predicted); ms −4 |
| cross-rerank+pool64 | 0.610 | +0.020 [−0.050,+0.090] | gains exactly in retrieval-bound strata; tr −4 |
| session quota 2 (fixed) | 0.580 | −0.010 [−0.050,+0.020] | mechanism proven in tests; QA null |
| temporal grounding | 0.590 | +0.000 [−0.050,+0.050] | redundant date prefixes mute it (known) |
| pool 64 alone | 0.570 | −0.020 [−0.070,+0.040] | |
| bge-base embed | 0.580 | −0.010 [−0.050,+0.030] | chat lane not embedder-bound |
| sibling-gather | 0.560 | −0.030 [−0.100,+0.040] | |
| fact extraction | 0.560 | −0.030 [−0.110,+0.040] | **ΔR@10 +0.074 [+0.021,+0.138] — the wave's only CI-significant signal** |

**Facts diagnosis (flip-level):** the write side works (retrieval gain real); the pack
side wastes it — fact rows displace session content (8.23 items but FEWER tokens than
base packs; 48.7% of items are <150-char snippets; 16/44 failures are displacement,
21/44 reader, 7/44 judge-subjective). Named follow-up: pack-policy-aware fact
admission (side-budget or share cap) before re-measuring.

## Pre-registered combo and the two-seed verdict

Combo (committed before any held-out peeking): `--session-quota 2 --cross-rerank
--pool 64` + v3 reader. Rationale: xrr and v3 are mechanism-confirmed complements
(each one's regression stratum is the other's gain stratum).

| Run | QA | ΔQA vs same-seed base [95% CI] |
|---|---|---|
| combo dev (20260710) | 0.580 | −0.010 [−0.080,+0.060] |
| combo held-out (20260711) | 0.610 | +0.050 [−0.040,+0.140] |
| **pooled two seeds (n=200)** | — | **+0.020 [−0.040,+0.080] — CI includes zero** |

**Verdict: NO PROMOTION.** Under the binding two-seed rule every wave lever stays
flag-gated off. The levers are built, tested, mechanism-diagnosed, and cheap to
re-measure at higher power; they are not defaults.

## Syndai replacement gate (W10) — HOLD

Engine-vs-engine on 108 seeded Syndai docs, 60 mined span-grounded questions, same
reader/judge both sides: MemPhant QA 0.050 vs Syndai knowledge stack 0.217,
Δ −0.167 [−0.267, −0.083] excludes zero → **replacement blocked**; details in
`2026-07-11-syndai-gate.md`. Diagnosis: 384d embedder vs 1536d, no heading-path
context at embed time, lexically-dominated fusion on doc corpora. The doc lane is a
different regime from the chat lane and is now the largest measured gap.

## Where this leaves the ladder

- QA band on our harness: 0.56-0.61 across seeds (full-context-GPT-4o-baseline
  territory; the paper's optimized-retrieval band 0.70-0.73 remains open).
- n=100 is underpowered for +0.02..0.05 levers (CI half-width ~±0.06): the next
  measurement round must be n≥300 (or the full-500 leaderboard-protocol run, which is
  also the only path to any external claim — the word SOTA stays banned until then).
- Evidence-ranked next levers: (1) pack-policy-aware fact admission (only significant
  retrieval signal in the wave); (2) reader-side for the 21/44 adequate-pack failures
  (multi-pass answer-then-verify was the L-cost lever deferred from the fleet round);
  (3) doc-lane embed/context work per the gate diagnosis (heading-path context at
  embed; stronger embedder) — this also serves the HOLD-gated replacement goal.

# MemPhant Completion Handoff - 2026-07-04

Current STATUS mirror: COMPLETE

`docs/superpowers/specs/memphant/STATUS.md` is the live ledger. This handoff is
a reconciled summary for a follow-on agent; it does not override checkboxes,
gates, or owner contracts.

## Current State On Main

`STATUS.md` currently records `CURRENT PHASE: COMPLETE`, with every checkbox in
sections 1-6 checked. The current proof set is the reconciled launch evidence,
not the pre-reconciliation gap list that previously lived in this file.

| Area | Current proof |
|---|---|
| Public launch gate | `docs/launch/public-launch-scorecard.json` plus `docs/build-log/artifacts/real-launch-evidence-20260704-v1/` |
| Public sampled benchmark profile | `docs/build-log/artifacts/real-launch-evidence-20260704-v1/sota-profile.json` |
| Sample manifest | `docs/build-log/artifacts/real-launch-evidence-20260704-v1/sample-manifest.json` |
| LongMemEval-V2 sampled traces | `docs/build-log/artifacts/real-launch-evidence-20260704-v1/public-real-sampled-traces.json` |
| Restraint launch gate | `docs/launch/restraint-launch-scorecard.json` and `docs/build-log/artifacts/real-launch-evidence-20260704-v1/restraint-ps-bench-sampled-traces.json` |
| GateMem conditional gate | `docs/launch/gatemem-conditional-scorecard.json` and `docs/build-log/artifacts/real-launch-evidence-20260704-v1/gatemem-sampled-trace.json` |
| Standing quality bars | `docs/launch/standing-quality-bars.json` |
| Postgres SLO proof | `docs/build-log/artifacts/real-launch-evidence-20260704-v1/postgres-slo.json` |
| Dogfood utility trend | `docs/build-log/artifacts/real-launch-evidence-20260704-v1/memory-utility-trend.json` |
| Syndai preflight mirror | `docs/build-log/2026-07-03-syndai-preflight.md` |

Recorded numbers from those artifacts:

- Public launch scorecard status: `pass`; no public SOTA claim is made.
- LongMemEval-V2 sampled profile: `50/50`, deterministic containment harness,
  `p95_ms=5.717`, cost per 1k recalls `$0.00`.
- PS-Bench restraint: `50/50`, measured relative drop `0.0` against a `0.15`
  maximum.
- GateMem sampled reproduction: `60` checkpoints, utility `1.0`,
  access-control leaks `0`, deleted-memory recoveries `0`.
- Hot-path SLO: Postgres-backed `1000` seeded units, p50 `0.005ms`, p95
  `0.0505ms`.

## Benchmark Ingestion Recipe

Keep using the committed script; do not hand-edit sampled benchmark cases.

```sh
python3 scripts/ingest_public_bench.py --sample-count 50
```

The script writes `docs/build-log/artifacts/real-launch-evidence-20260704-v1/`
and regenerates the public sampled fixtures. It caches fetched source material
under `~/.cache/memphant-bench`, uses pinned revisions, and records sample IDs
plus source hashes in `sample-manifest.json`.

Current sampled sources:

| Benchmark | Access method | Current pin | Committed raw data? |
|---|---|---|---|
| LongMemEval-V2 | Hugging Face single file, `questions.jsonl` | `xiaowu0162/longmemeval-v2@f152293e235517d504809563c833d7190b8c713b` | sampled cases only |
| PS-Bench | GitHub raw cache-only | `MuyuenLP/PS-Bench@210e72ea8352a1700141476bfde1f153a3a826e4` | no |
| GateMem | Hugging Face single files plus repo provenance | `Ray368/GateMem@b4304866ec8d9784fb77bebb1ce4660806abcded` | manifest and trace only |

The recipe is intentionally small: deterministic local grading, no paid judge
APIs, no full-corpus downloads, and no committed raw PS-Bench text.

## Archived Pre-Reconciliation Snapshot

The following claims were true of the old snapshot used to drive the
reconciliation work. They are archived here only as historical context and are
not the current MemPhant state.

| Old claim | Current disposition |
|---|---|
| Public benchmark proofs were synthetic and launch was only `candidate_pass`. | Superseded by `real-launch-evidence-20260704-v1`, `public-launch-scorecard.json` status `pass`, and sampled-public LongMemEval-V2 / PS-Bench traces. |
| Restraint and GateMem were backed by toy or generic fixtures. | Superseded by PS-Bench sampled restraint traces and GateMem sampled reproduction proof. |
| Syndai dogfood proof path pointed at stale cross-repo state. | Superseded by `porting.md`, `docs/build-log/2026-07-03-syndai-preflight.md`, and the current dogfood utility-trend proof. |
| Standing bars lacked Postgres-backed SLO and real baseline/current utility windows. | Superseded by `postgres-slo.json`, `memory-utility-trend.json`, and `standing-quality-bars.json`. |
| `STATUS.md` COMPLETE was dishonest. | Superseded. `STATUS.md` is now the source of truth and records COMPLETE with proof paths on the checked lines. |

## Stale-Handoff Guard

`tests/test_repo_contract.py::test_handoff_docs_mirror_status_phase` now checks
that every handoff mirrors the live `STATUS.md` phase and keeps archived
pre-reconciliation gap language out of the active handoff body.

# MemPhant — Live Status Ledger

> **This is the single progress tracker.** It records STATE only; every contract lives in its owner doc (`00-relations-graph.md` §1). Update rule: any completed slice/workstream/rung/gate flips its checkbox here in the same change that produces its proof artifact (build-log entry, trace archive, gate output). This file moves into the MemPhant repo at WS-0 and stays the same ledger.
>
> ## ✅ DONE definition (deterministic)
> **MemPhant is FULLY COMPLETED when every checkbox in §1–§6 is checked and the banner below reads COMPLETE.** No section may be skipped; an activation-gated item (§5) counts as done when it is either ACTIVATED (gate met + built + proof) or RETIRED (its disable-when fired, recorded in `24`). Nothing else — no vibe, no partial credit — flips the banner.
>
> # CURRENT PHASE: `WS-D READY — WS-C EXIT PACKET COMPLETE`
>
> WS-0 proof artifact: `docs/build-log/artifacts/ws0-two-language-spike.json`.
> R83 spike measured warm no-recompile Rust policy-change iteration at **0.073×** Python (`rust_proceeds`).
> WS-A proof artifact: `docs/build-log/2026-07-03-wsa-progress.md` (fresh `pgvector/pgvector:0.8.4-pg17` bootstrap + provider lint + store seam tests).
> WS-B proof artifact: `docs/build-log/2026-07-03-wsb-progress.md` (retain/reflect/dedup/admission/freshness goldens + outbox shape + full local gates).
> WS-C proof artifact: `docs/build-log/2026-07-03-wsc-progress.md` (recall trace spine + oracle/isolation/citation/filter goldens + full local gates). Next build workstream: WS-D.
> Syndai spec/preflight proof: `docs/build-log/2026-07-03-syndai-preflight.md` (`Syndai/main` `fe17bc488`, preflight green in 764s).

## 1. Spec corpus

- [x] Spec suite complete through 16 hardening passes (audit trail: `24` R1–R92 + Round registers; plans in `docs/superpowers/plans/`)
- [x] All irreversible surfaces decision-registered (`26`); v1 cut line owned by `29` §2a
- [x] Consistency gates green: `python scripts/validate_docs.py` + `backend/tests/scripts/test_validate_docs_evalrank_contract.py` (16 tests)
- [x] Pass 15+16 work committed/shipped via `/preflight` (proof: `docs/build-log/2026-07-03-syndai-preflight.md`; `Syndai/main` `fe17bc488`, preflight green in 764s)

## 2. Workstreams (order + exit-packet contracts owned by `29` §2; check only on exit-packet proof)

- [x] **WS-0** Spec/repo freeze — repo skeleton, `memphant.lock`, spec-drift checklist, **two-language spike result recorded (R83 result: 0.073×, `rust_proceeds`; proof: `docs/build-log/2026-07-03-ws0-progress.md` + `docs/build-log/artifacts/ws0-two-language-spike.json`)**
- [x] **WS-A** Schema, core types, store seam — all tables incl. `belief_observation`/`review_event`/`scope_block`; bootstrap + provider lint green (proof: `docs/build-log/2026-07-03-wsa-progress.md`)
- [x] **WS-B** Write path + memory compiler — retain/reflect/dedup/contradiction/corroboration golden fixtures pass (proof: `docs/build-log/2026-07-03-wsb-progress.md`)
- [x] **WS-C** Read path + trace spine — every recall traced; oracle + isolation + citation + small-tenant fixtures pass (proof: `docs/build-log/2026-07-03-wsc-progress.md`)
- [ ] **WS-D** Public surfaces — REST/MCP/Python SDK round-trip; schemas validate; `memphant verify` works
- [ ] **WS-E** Eval, security, ops — golden oracle + manifest guard + security suites + nightly sampled runner + deletion-completeness lane
- [ ] **WS-F** Syndai dogfood cutover — first low-risk surface exported + trace-compared (stop-rule honored; launch not hostage to full cutover)
- [ ] **WS-G** Public UI/docs/launch surface
- [ ] **WS-H** BYOC + hosted packaging
- [ ] **WS-I** Advanced lever activation (tracked per-item in §5)

## 3. SOTA ladder rungs (activation/disable contracts owned by `27` §2; check = advance-when met with archived profile proof)

- [ ] 0 trace/eval harness · [ ] 1 raw episodes+citations · [ ] 2 write/extraction policy · [ ] 3 hybrid baseline
- [ ] 4 contextual chunks · [ ] 5 temporal validity · [ ] 6 edge expansion (vs no-edges + filesystem controls) · [ ] 7 packing+abstention
- [ ] 8 bounded rerank · [ ] 9 query decomposition · [ ] 10 procedural memory · [ ] 11 DSR decay fold · [ ] 12 L4 exhaustive
- [ ] 13 learned rerank/DSR · [ ] 14 external graph/vector escape hatch (or RETIRED) · [ ] 15 inferred-belief composition

## 4. Launch gates (contracts owned by `29` §5–§7)

- [ ] **Alpha gate** (`29` §5) — all eleven criteria green
- [ ] **Dogfood gate** (`29` §6) — first surface actively read by Syndai through public contracts
- [ ] **Public launch gate** (`29` §7) — incl. one reproduced public benchmark profile + no hidden Syndai-only behavior
- [ ] **Restraint launch gate** (`27` §1) — OP-Bench drop ≤15% vs memory-free baseline (pinned-block content in scope)
- [ ] **GateMem conditional gate** (`27` §1, R90) — first internal reproduction done; then simultaneous pass on utility+access-control+forgetting

## 5. Activation-gated ledger (gates owned by `29` §8; status ∈ DORMANT / GATE-MET / BUILT / RETIRED)

| Item | Status |
|---|---|
| L4 exhaustive recall behavior | DORMANT |
| Learned reranker | DORMANT |
| Learned DSR/FSRS fitter | DORMANT |
| DSR decay fold (fsrs engine; ledger capture is v1) | DORMANT |
| Procedural replay-validation harness | DORMANT |
| 3-tier DEK envelope encryption | DORMANT (`key_custody` shape frozen) |
| Ablation-voting recall (SMSR-style, exhaustive mode) | DORMANT |
| Delta recall / miss-repair re-extraction / retrievability probe | DORMANT |
| Consolidation event delivery (outbox consumers) | DORMANT (taxonomy + outbox shape specced) |
| Hermes memory-provider adapter (`08` §5.1b, R87) | DORMANT (specced, not frozen) |
| External graph DB / dedicated vector engine | DORMANT |
| Cache cluster · framework adapters · extra SDKs · Helm · SQLite · CRDT · skill compiler · multi-region · billing plane · GTM automation | DORMANT (see `29` §8 rows) |
| TypeScript SDK | DORMANT (first TS consumer or launch window) |

## 6. Standing quality bars (never one-and-done; checked at every release while building)

- [ ] Security suites green at latest release (tenant isolation, deletion completeness, corroboration-farming, filter injection — `05` §10)
- [ ] Hot-path SLO holding (fast p50 <200ms / p95 <500ms — `02` §4)
- [ ] `memory_utility_trend` SLI wired on the dogfood lane (`22` §1.3)
- [ ] Landscape-completeness rule satisfied at latest review pass (`13` §1.4)

## Update protocol

1. A change that completes any item updates THIS file in the same commit as its proof (build-log path or gate output pasted in the checkbox line).
2. New scope enters via `26`/`29` first, then gets a checkbox here — never the reverse.
3. When §1–§6 are fully checked, flip the banner to `COMPLETE`; that flip is the definition of "MemPhant is done."

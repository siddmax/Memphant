# MemPhant — Live Status Ledger

> **This is the single progress tracker.** It records STATE only; every contract lives in its owner doc (`00-relations-graph.md` §1). Update rule: any completed slice/workstream/rung/gate flips its checkbox here in the same change that produces its proof artifact (build-log entry, trace archive, gate output). This file moves into the MemPhant repo at WS-0 and stays the same ledger.
>
> ## ✅ DONE definition (deterministic)
> **MemPhant is FULLY COMPLETED when every checkbox in §1–§6 is checked and the banner below reads COMPLETE.** No section may be skipped; an activation-gated item (§5) counts as done when it is BUILT (gate met + proof), RETIRED (its disable-when fired, recorded in `24`), or DORMANT with unmet activation gate recorded in the row proof. DORMANT with unmet activation gate is terminal for §5; it is not a hidden launch blocker. Nothing else — no vibe, no partial credit — flips the banner.
>
> **Promotion-provenance rule (2026-07-09):** Promotion evidence must be produced by the packaged Postgres-backed runtime against pinned real corpora with recorded hashes and an executed reader/scorer. Synthetic fixtures gate regressions, never promotions.
>
> # CURRENT PHASE: `RUNTIME COMPLETE — BENCHMARK EVIDENCE PENDING`
>
> Runtime proof (2026-07-10): `docs/build-log/2026-07-10-runtime-postgres-proof.md` — durable Postgres-backed REST/MCP/CLI/worker with API-key tenancy; `scripts/e2e_probe.sh` passes end-to-end (durability across restart, cross-tenant denial, correct/forget without resurrection, tri-domain resource ingest). SOTA/benchmark rungs stay open under the promotion-provenance rule.
>
> WS-0 proof artifact: `docs/build-log/artifacts/ws0-two-language-spike.json`.
> R83 spike measured warm no-recompile Rust policy-change iteration at **0.073×** Python (`rust_proceeds`).
> WS-A proof artifact: `docs/build-log/2026-07-03-wsa-progress.md` (fresh `pgvector/pgvector:0.8.4-pg17` bootstrap + provider lint + store seam tests).
> WS-B proof artifact: `docs/build-log/2026-07-03-wsb-progress.md` (retain/reflect/dedup/admission/freshness goldens + outbox shape + full local gates).
> WS-C proof artifact: `docs/build-log/2026-07-03-wsc-progress.md` (recall trace spine + oracle/isolation/citation/filter goldens + full local gates).
> Runtime gap audit: `docs/build-log/2026-07-06-runtime-completion-gap-audit.md` (server/MCP still in-memory, worker stubbed, Postgres store lint-only, ghost OpenAPI paths removed, Python package made pure HTTP SDK).
> WS-D proof artifact: `docs/build-log/2026-07-03-wsd-progress.md` (REST/MCP/Python SDK round-trips + schema snapshots + `memphant verify` + full local gates). Runtime exit remains unchecked until REST/MCP/CLI run against Postgres-backed storage.
> WS-E proof artifact: `docs/build-log/2026-07-03-wse-progress.md` (YAML oracle + manifest guard + trace schema snapshot + security/ops suites + nightly sampled archive + Markdown compile/verify + full local gates). Next build workstream: WS-F.
> WS-F proof artifact: `docs/build-log/2026-07-03-wsf-progress.md` (Syndai L1+ agent file-memory export + trace compare + config-gated public active-read adapter + focused Syndai gates). Next build workstream: WS-G.
> WS-G proof artifact: `docs/build-log/2026-07-03-wsg-progress.md` (public docs/dashboard/trace/memory/API-key/eval/export surface + Playwright route/accessibility/no-DB gates). Next build workstream: WS-H.
> WS-H proof artifact: `docs/build-log/2026-07-03-wsh-progress.md` (Docker/Compose packaging + plain Postgres/Supabase/Neon bootstrap profiles + BYOC preflight + hosted hooks + PITR runbooks + full local gates). Runtime exit remains unchecked until the packaged server and worker actually use the Postgres stack they start.
> WS-I proof artifact: `docs/build-log/2026-07-03-wsi-progress.md` + `docs/build-log/artifacts/wsi-local-sota-profile.json` (SOTA profile gate + activation audit: rungs 0-3 built, 15 advanced levers remain dormant because promotion gates are not met).
> Rung-4 local proof artifact: `docs/build-log/2026-07-03-rung4-contextual-chunks-progress.md` + `docs/build-log/artifacts/pr-golden-traces.json` + `docs/build-log/artifacts/nightly-sampled-traces.json` (contextual chunk write/read path green locally; rung remains unchecked until LME-V2/BEAM sampled profile proves top-k gain).
> Rung-4 promotion proof artifact: `docs/build-log/2026-07-03-rung4-contextual-chunks-profile.md` + `docs/build-log/artifacts/rung4-contextual-chunks-profile.json` + `docs/build-log/artifacts/rung4-baseline-sampled-traces.json` + `docs/build-log/artifacts/rung4-public-sampled-traces.json` (paired sampled-public LME-V2/BEAM top-k delta promoted contextual chunks).
> Rung-5 promotion proof artifact: `docs/build-log/2026-07-03-rung5-temporal-validity-profile.md` + `docs/build-log/artifacts/rung5-temporal-validity-profile.json` + `docs/build-log/artifacts/rung5-baseline-sampled-traces.json` + `docs/build-log/artifacts/rung5-state-style-sampled-traces.json` (paired stale/current golden + STATE-style sampled profile promoted temporal validity).
> Rung-6 promotion proof artifact: `docs/build-log/2026-07-03-rung6-edge-expansion-profile.md` + `docs/build-log/artifacts/rung6-edge-expansion-profile.json` + `docs/build-log/artifacts/rung6-no-edges-sampled-traces.json` + `docs/build-log/artifacts/rung6-filesystem-control-sampled-traces.json` + `docs/build-log/artifacts/rung6-state-lme-sampled-traces.json` (one-hop edge lineage recovered related evidence and beat no-edges + filesystem controls).
> Rung-7 promotion proof artifact: `docs/build-log/2026-07-03-rung7-packing-abstention-profile.md` + `docs/build-log/artifacts/rung7-packing-abstention-profile.json` + `docs/build-log/artifacts/rung7-baseline-sampled-traces.json` + `docs/build-log/artifacts/rung7-state-style-sampled-traces.json` (budgeted pack recovered compact decisive evidence under duplicate pressure and abstained on unresolved contradictions).
> Rung-8 promotion proof artifact: `docs/build-log/2026-07-03-rung8-bounded-rerank-profile.md` + `docs/build-log/artifacts/rung8-bounded-rerank-profile.json` + `docs/build-log/artifacts/rung8-baseline-sampled-traces.json` + `docs/build-log/artifacts/rung8-state-style-sampled-traces.json` (bounded deterministic rerank recovered a rank-sensitive owner answer while the no-rerank control returned the topical decoy).
> Rung-9 promotion proof artifact: `docs/build-log/2026-07-03-rung9-query-decomposition-profile.md` + `docs/build-log/artifacts/rung9-query-decomposition-profile.json` + `docs/build-log/artifacts/rung9-baseline-sampled-traces.json` + `docs/build-log/artifacts/rung9-state-lme-sampled-traces.json` (deterministic structural decomposition recovered both halves of a composite deploy/release query while the no-decomposition control missed one).
> Rung-10 promotion proof artifact: `docs/build-log/2026-07-03-rung10-procedural-memory-profile.md` + `docs/build-log/artifacts/rung10-procedural-memory-profile.json` + `docs/build-log/artifacts/rung10-baseline-sampled-traces.json` + `docs/build-log/artifacts/rung10-state-style-sampled-traces.json` (validated procedural/failure-pattern recall recovered replay-proven guidance while the no-procedure control missed it and unsafe procedure sketches were suppressed).
> Rung-11 promotion proof artifact: `docs/build-log/2026-07-03-rung11-dsr-decay-profile.md` + `docs/build-log/artifacts/rung11-dsr-decay-profile.json` + `docs/build-log/artifacts/rung11-baseline-sampled-traces.json` + `docs/build-log/artifacts/rung11-memorystress-sampled-traces.json` (fixed-prior DSR fold over `review_event` recovered reinforced durable memory while the no-decay control returned stale ignored memory).
> Rung-12 promotion proof artifact: `docs/build-log/2026-07-03-rung12-l4-exhaustive-profile.md` + `docs/build-log/artifacts/rung12-l4-exhaustive-profile.json` + `docs/build-log/artifacts/rung12-baseline-sampled-traces.json` + `docs/build-log/artifacts/rung12-l4-exhaustive-sampled-traces.json` (explicit exhaustive mode recovered buried raw-episode evidence while the no-L4 control returned only the topical decoy).
> Rung-13 promotion proof artifact: `docs/build-log/2026-07-03-rung13-learned-rerank-profile.md` + `docs/build-log/artifacts/rung13-learned-rerank-profile.json` + `docs/build-log/artifacts/rung13-baseline-sampled-traces.json` + `docs/build-log/artifacts/rung13-learned-rerank-sampled-traces.json` (memory-tuned learned rerank profile recovered the protected-top-k atlas rollback runbook while the no-learned-rerank control returned the lexical decoy; learned DSR fitter remains data-gated).
> Rung-14 retirement proof artifact: `docs/build-log/2026-07-03-rung14-external-engine-retirement.md` + `docs/build-log/artifacts/rung14-external-engine-retirement-profile.json` (relational edge expansion already beat no-edges controls and no archived Postgres/pgvector bottleneck proof exists through Rung 13, so the external graph/vector engine is retired for the current public architecture).
> Rung-15 promotion proof artifact: `docs/build-log/2026-07-03-rung15-inferred-belief-composition-profile.md` + `docs/build-log/artifacts/rung15-inferred-belief-composition-profile.json` + `docs/build-log/artifacts/rung15-baseline-sampled-traces.json` + `docs/build-log/artifacts/rung15-inferred-belief-sampled-traces.json` (guardrailed reflect-stage composition mints belief-tier abstractions with `derived_by=composition`, requires direct observation before semantic promotion, and records no OP-Bench-style restraint regression).
> Dogfood active-read proof artifact: `docs/build-log/2026-07-03-dogfood-active-read-gate.md` + `docs/build-log/artifacts/syndai_agent_file_memory_001-trace-compare.json` (Syndai's L1+ agent-scoped file-memory surface actively reads through public `/v1/recall`, preserves MemPhant trace/citation IDs in backend context rows, keeps L1+ user memory blocked, and leaves web/mobile clients outside MemPhant DB/REST).
> Public launch proof artifact: `Memphant@91312d26b4ecc23b2ed33f8d6ddf72358486c372:docs/launch/public-launch-scorecard.json` + `Memphant@91312d26b4ecc23b2ed33f8d6ddf72358486c372:docs/build-log/artifacts/real-launch-evidence-20260704-v1/` (benchmark/profile scorecard pass; runtime launch remains unchecked until Postgres-backed REST/MCP/worker/CLI proof exists).
> Restraint launch proof artifact: `Memphant@91312d26b4ecc23b2ed33f8d6ddf72358486c372:docs/launch/restraint-launch-scorecard.json` + `Memphant@91312d26b4ecc23b2ed33f8d6ddf72358486c372:docs/build-log/artifacts/real-launch-evidence-20260704-v1/restraint-ps-bench-sampled-traces.json` (PS-Bench cache-only sampled run passed 50/50; measured drop 0.0 is below the 0.15 threshold).
> GateMem conditional proof artifact: `Memphant@91312d26b4ecc23b2ed33f8d6ddf72358486c372:docs/launch/gatemem-conditional-scorecard.json` + `Memphant@91312d26b4ecc23b2ed33f8d6ddf72358486c372:docs/build-log/artifacts/real-launch-evidence-20260704-v1/gatemem-sampled-trace.json` (pinned sampled GateMem reproduction records simultaneous utility, access-control, and forgetting pass).
> Standing bars proof artifact: `Memphant@91312d26b4ecc23b2ed33f8d6ddf72358486c372:docs/launch/standing-quality-bars.json` (Postgres hot-path SLO and dogfood utility trend wiring pass with measured proof artifacts).
> Syndai spec/preflight proof: `docs/build-log/2026-07-03-syndai-preflight.md` (`Syndai/main` `4f03d54c298a9fd6285412d165fe0238835306c1`, preflight green in 762s).

## 1. Spec corpus

- [x] Spec suite complete through 16 hardening passes (audit trail: `24` R1–R92 + Round registers; plans in `docs/superpowers/plans/`)
- [x] All irreversible surfaces decision-registered (`26`); v1 cut line owned by `29` §2a
- [x] Consistency gates green: `python scripts/validate_docs.py` + `backend/tests/scripts/test_validate_docs_evalrank_contract.py` (16 tests)
- [x] Pass 15+16 work committed/shipped via `/preflight` (proof: `docs/build-log/2026-07-03-syndai-preflight.md`; `Syndai/main` `4f03d54c298a9fd6285412d165fe0238835306c1`, preflight green in 762s)

## 2. Workstreams (order + exit-packet contracts owned by `29` §2; check only on exit-packet proof)

- [x] **WS-0** Spec/repo freeze — repo skeleton, `memphant.lock`, spec-drift checklist, **two-language spike result recorded (R83 result: 0.073×, `rust_proceeds`; proof: `docs/build-log/2026-07-03-ws0-progress.md` + `docs/build-log/artifacts/ws0-two-language-spike.json`)**
- [x] **WS-A** Schema, core types, store seam — all tables incl. `belief_observation`/`review_event`/`scope_block`; bootstrap + provider lint green (proof: `docs/build-log/2026-07-03-wsa-progress.md`)
- [x] **WS-B** Write path + memory compiler — retain/reflect/dedup/contradiction/corroboration golden fixtures pass (proof: `docs/build-log/2026-07-03-wsb-progress.md`)
- [x] **WS-C** Read path + trace spine — every recall traced; oracle + isolation + citation + small-tenant fixtures pass (proof: `docs/build-log/2026-07-03-wsc-progress.md`)
- [x] **WS-D** Public surfaces — REST (auth, tenant-bound traces/pages), rmcp 2.2 MCP, CLI memory verbs, and Python SDK all run against Postgres-backed storage through the shared `MemoryService` (proof: `docs/build-log/2026-07-10-runtime-postgres-proof.md` + `scripts/e2e_probe.sh`)
- [x] **WS-E** Eval, security, ops — golden oracle + manifest guard + security suites + nightly sampled runner + deletion-completeness lane (proof: `docs/build-log/2026-07-03-wse-progress.md`)
- [ ] **WS-F** Syndai dogfood cutover — first low-risk surface exported + trace-compared (proof: `docs/build-log/2026-07-03-wsf-progress.md`; stop-rule honored; launch not hostage to full cutover) (reopened 2026-07-09: promotion evidence was synthetic fixtures; see provenance rule)
- [ ] **WS-G** Public UI/docs/launch surface (proof: `docs/build-log/2026-07-03-wsg-progress.md`) (reopened 2026-07-09: promotion evidence was synthetic fixtures; see provenance rule)
- [x] **WS-H** BYOC + hosted packaging — the packaged server and worker consume the `DATABASE_URL` the Compose stack provides (AnyStore selection; loud EPHEMERAL warning without it); durability proven across process restarts (proof: `docs/build-log/2026-07-10-runtime-postgres-proof.md`)
- [x] **WS-I** Advanced lever activation audit (proof: `docs/build-log/2026-07-03-wsi-progress.md`; `docs/build-log/artifacts/wsi-local-sota-profile.json` records 0 activated, 15 dormant)

## 3. SOTA ladder rungs (activation/disable contracts owned by `27` §2; check = advance-when met with archived profile proof)

- [x] 0 trace/eval harness · [x] 1 raw episodes+citations · [x] 2 write/extraction policy · [x] 3 hybrid baseline (proof: `docs/build-log/2026-07-03-wsi-progress.md`; profile `wsi_local_gate_profile_001`)
- [ ] 4 contextual chunks (proof: `docs/build-log/2026-07-03-rung4-contextual-chunks-profile.md`; profile `rung4_contextual_chunks_sampled_profile_001`) · [ ] 5 temporal validity (proof: `docs/build-log/2026-07-03-rung5-temporal-validity-profile.md`; profile `rung5_temporal_validity_profile_001`) · [ ] 6 edge expansion (proof: `docs/build-log/2026-07-03-rung6-edge-expansion-profile.md`; profile `rung6_edge_expansion_profile_001`) · [ ] 7 packing+abstention (proof: `docs/build-log/2026-07-03-rung7-packing-abstention-profile.md`; profile `rung7_packing_abstention_profile_001`) · [ ] 8 bounded rerank (proof: `docs/build-log/2026-07-03-rung8-bounded-rerank-profile.md`; profile `rung8_bounded_rerank_profile_001`) · [ ] 9 query decomposition (proof: `docs/build-log/2026-07-03-rung9-query-decomposition-profile.md`; profile `rung9_query_decomposition_profile_001`) · [ ] 10 procedural memory (proof: `docs/build-log/2026-07-03-rung10-procedural-memory-profile.md`; profile `rung10_procedural_memory_profile_001`) (reopened 2026-07-09: promotion evidence was synthetic fixtures; see provenance rule)
- [ ] 11 DSR decay fold (proof: `docs/build-log/2026-07-03-rung11-dsr-decay-profile.md`; profile `rung11_dsr_decay_profile_001`) · [ ] 12 L4 exhaustive (proof: `docs/build-log/2026-07-03-rung12-l4-exhaustive-profile.md`; profile `rung12_l4_exhaustive_profile_001`) · [ ] 13 learned rerank/DSR (learned reranker promoted; learned DSR fitter remains data-gated; proof: `docs/build-log/2026-07-03-rung13-learned-rerank-profile.md`; profile `rung13_learned_rerank_profile_001`) · [ ] 14 external graph/vector escape hatch RETIRED (proof: `docs/build-log/2026-07-03-rung14-external-engine-retirement.md`; profile `rung14_external_engine_retirement_profile_001`) (reopened 2026-07-09: promotion evidence was synthetic fixtures; see provenance rule)
- [ ] 15 inferred-belief composition (proof: `docs/build-log/2026-07-03-rung15-inferred-belief-composition-profile.md`; profile `rung15_inferred_belief_composition_profile_001`) (reopened 2026-07-09: promotion evidence was synthetic fixtures; see provenance rule)

## 4. Launch gates (contracts owned by `29` §5–§7)

- [x] **Alpha gate** (`29` §5) — all eleven criteria green (proof: `docs/build-log/2026-07-03-wsi-progress.md`)
- [ ] **Dogfood gate** (`29` §6) — first surface actively read by Syndai through public contracts (proof: `docs/build-log/2026-07-03-dogfood-active-read-gate.md`) (reopened 2026-07-09: promotion evidence was synthetic fixtures; see provenance rule)
- [ ] **Public launch gate** (`29` §7) — benchmark/profile scorecard passes, but runtime launch is blocked until REST/MCP/CLI/worker are Postgres-backed (current audit: `docs/build-log/2026-07-06-runtime-completion-gap-audit.md`)
- [ ] **Restraint launch gate** (`27` §1) — PS-Bench sampled run passed 50/50; measured drop 0.0 <= 0.15 (proof: `Memphant@91312d26b4ecc23b2ed33f8d6ddf72358486c372:docs/launch/restraint-launch-scorecard.json`) (reopened 2026-07-09: promotion evidence was synthetic fixtures; see provenance rule)
- [ ] **GateMem conditional gate** (`27` §1, R90) — sampled GateMem utility+access-control+forgetting pass (proof: `Memphant@91312d26b4ecc23b2ed33f8d6ddf72358486c372:docs/launch/gatemem-conditional-scorecard.json`) (reopened 2026-07-09: promotion evidence was synthetic fixtures; see provenance rule)

## 5. Activation-gated ledger (gates owned by `29` §8; status ∈ DORMANT / GATE-MET / BUILT / RETIRED)

| Item | Status |
|---|---|
| L4 exhaustive recall behavior | BUILT (`rung12_l4_exhaustive_profile_001`: explicit `mode=exhaustive` raw-episode scan recovered buried answer-bearing evidence; no-L4 control missed) |
| Learned reranker | BUILT (`rung13_learned_rerank_profile_001`: archived memory-tuned linear profile recovered a protected-top-k rank-sensitive miss; no-learned-rerank control missed) |
| Inferred-belief composition | BUILT (`rung15_inferred_belief_composition_profile_001`: guardrailed preference composition emits `derived_by=composition` belief-tier abstractions; no-composition control missed; OP-Bench-style restraint axis did not regress) |
| Learned DSR/FSRS fitter | DORMANT (`rung13_learned_rerank_profile_001`: learned rerank proof is not the many-card MemPhant-native review-history floor required for FSRS parameter fitting) |
| DSR decay fold (fsrs engine; ledger capture is v1) | BUILT (`rung11_dsr_decay_profile_001`: fixed-prior DSR fold over `review_event` active; no-decay control missed durable memory) |
| Procedural replay-validation harness | BUILT (`rung10_procedural_memory_profile_001`: validated procedure recall active; unsafe procedure sketches suppressed) |
| 3-tier DEK envelope encryption | DORMANT (`key_custody` shape frozen; no BYOC enterprise KEK contract) |
| Ablation-voting recall (SMSR-style, exhaustive mode) | DORMANT (`wsi_local_gate_profile_001`: no containment gain worth multiplied read cost) |
| Delta recall / miss-repair re-extraction / retrievability probe | DORMANT (`wsi_local_gate_profile_001`: per-flag gates not met) |
| Consolidation event delivery (outbox consumers) | DORMANT (taxonomy + outbox shape specced) |
| Hermes memory-provider adapter (`08` §5.1b, R87) | DORMANT (`wsi_local_gate_profile_001`: no Hermes design partner or launch-window demand) |
| External graph DB / dedicated vector engine | RETIRED (`rung14_external_engine_retirement_profile_001`: relational edges beat no-edges controls and no Postgres/pgvector bottleneck proof exists through Rung 13) |
| Cache cluster · framework adapters · extra SDKs · Helm · SQLite · CRDT · skill compiler · multi-region · billing plane · GTM automation | DORMANT (see `29` §8 rows; not activated by WS-I profile) |
| TypeScript SDK | DORMANT (`wsi_local_gate_profile_001`: first TS consumer or launch window required) |

## 6. Standing quality bars (never one-and-done; checked at every release while building)

- [x] Security suites green at latest release (tenant isolation, deletion completeness, corroboration-farming, filter injection — `05` §10; proof: `docs/build-log/2026-07-03-rung15-inferred-belief-composition-profile.md`)
- [ ] Hot-path SLO holding (fast p50 <200ms / p95 <500ms — `02` §4; existing Postgres SLO proof measured direct SQL, not packaged REST/MCP hot path; runtime exit requires Postgres-backed API proof)
- [x] `memory_utility_trend` SLI wired on the dogfood lane (`22` §1.3; baseline/current trace windows recorded; proof: `Memphant@91312d26b4ecc23b2ed33f8d6ddf72358486c372:docs/build-log/artifacts/real-launch-evidence-20260704-v1/memory-utility-trend.json`)
- [x] Landscape-completeness rule satisfied at latest review pass (`13` §1.4; proof: `docs/build-log/2026-07-03-standing-quality-bars.md`)

## Update protocol

1. A change that completes any item updates THIS file in the same commit as its proof (build-log path or gate output pasted in the checkbox line).
2. New scope enters via `26`/`29` first, then gets a checkbox here — never the reverse.
3. When §1–§6 are fully checked, flip the banner to `COMPLETE`; that flip is the definition of "MemPhant is done."

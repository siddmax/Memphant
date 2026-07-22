# Rung-7 Packing/Ordering Lever Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:test-driven-development to implement each task. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Recover as many of the 64 in-pool-unpacked LME-S dev Fast-misses as a packing/ordering lever can, with a preregistered paired-CI pass predicate — or prove no ordering/budget lever moves them (a finding that redirects to the reader layer).

**Architecture:** Two phases. Phase A (FREE, diagnostic): extend the existing `bench-lme --emit-trace-classification` emitter so each in_pool_unpacked question records WHY its gold pool unit didn't reach the packed top-k (fused_rank of gold, and its `dropped_items` reason, or "never reached pack loop"). Classify the 64 by drop cause. Phase B (data-driven lever): TDD the single lever the diagnosis points to, measure paired on dev via `--baseline` (bootstrap CI on recall@5/@10 already built), promote only on a CI-excluding-zero retrieval win (two seeds), else reject with a negative artifact.

**Tech Stack:** Rust (`memphant-core` recall pack, `memphant-eval` bench-lme), Postgres scratch DB via `scripts/with_scratch_db.sh`, Python for artifact analysis. Retrieval-layer measurement is FREE (no model spend).

## Global Constraints

- Seed `20260713` for the primary dev run (matches A1); a second seed for any promotion (two-seed rule is binding).
- Product Fast config: `--sample 178 --k 10 --disable rerank --budget-tokens 8192 --pool 64 --embed-model small`, session granularity.
- Measure on a run-owned scratch Postgres (P0.1, `scripts/with_scratch_db.sh`) — never a shared DB.
- Pass predicate (§6 preregistration): paired bench-lme delta on the dev set via `--baseline`, with the bootstrap 95% CI on recall@10 (and/or recall@5) **excluding zero and positive**. Promotion requires the win to hold on a second seed.
- Kill-switch: a lever with no CI-excluding-zero retrieval win is REJECTED and its negative artifact kept. Ship nothing ns.
- KISS/DRY/YAGNI: no new measurement machinery (`--baseline` + bootstrap CI already exist). No lever code written before the diagnosis names the drop cause.
- Feature-flow guard: `FEATURE_FLOW_BYPASS=1` on source edits (plan reviewed).

---

## File Structure

- `crates/memphant-eval/src/bench_lme.rs` — extend `TraceClassificationRow` with drop-cause fields (Phase A); no behavior change to recall. **Modify.**
- `docs/build-log/artifacts/rung7-packing/` — diagnosis JSONL + paired reports (negative or positive). **Create.**
- `docs/build-log/2026-07-21-rung7-packing-ordering.md` — build-log note. **Create.**
- `crates/memphant-core/src/lib.rs` — ONLY if the diagnosis points to a core pack lever (Phase B, task 3). **Modify (conditional).**
- `docs/superpowers/specs/memphant/STATUS.md` + `docs/superpowers/plans/2026-07-21-tri-domain-sota-plan.md` — rung-7 row + plan update. **Modify.**

---

### Task 1: Extend trace-classification emitter with drop-cause (Phase A — FREE)

**Files:**
- Modify: `crates/memphant-eval/src/bench_lme.rs` (`TraceClassificationRow` ~line 184; emit block ~line 1069)
- Test: `crates/memphant-eval/src/bench_lme.rs` (inline `#[cfg(test)]`)

**Interfaces:**
- Consumes: `RetrievalTrace { candidates: Vec<RecallCandidateTrace>, dropped_items: Vec<RecallDroppedItem>, context_items }`. `RecallCandidateTrace` carries `unit_id`, `fused_rank`, `fused_score`, `discard_reason`. `RecallDroppedItem` carries `unit_id`, `reason: RecallDropReason`.
- Produces: `TraceClassificationRow` gains, for the gold-bearing pool unit(s): `gold_fused_rank: Option<usize>`, `gold_fused_score: Option<f32>`, `gold_drop_reason: Option<RecallDropReason>` (from `dropped_items`), `packed_count_vs_k: (usize, usize)` — i.e. `(packed_size, k)`. When several pool units bear gold, record the best-ranked one (min `fused_rank`).

- [ ] **Step 1: Write the failing test** — a `classify_drop_cause` pure helper: given a gold unit id, the trace's `candidates` (unit→fused_rank) and `dropped_items` (unit→reason), returns `(gold_fused_rank, gold_drop_reason)`. Test: gold at fused_rank 12 with a `Duplicate` drop → `(Some(12), Some(Duplicate))`; gold at fused_rank 30, absent from dropped_items (never reached pack loop / beyond scan) → `(Some(30), None)`.

- [ ] **Step 2: Run test to verify it fails** — `cargo test -p memphant-eval classify_drop_cause -- --nocapture`. Expected: FAIL (function not defined).

- [ ] **Step 3: Write minimal implementation** — the pure helper + wire it into the `emit_trace_classification` block to populate the new row fields for the gold pool unit. Reuse the existing `gold` closure and `pool_units`/`pool_sessions` already fetched there; map each gold-session pool unit back to its `unit_id`, look up fused_rank in `trace.candidates`, look up drop reason in `trace.dropped_items`.

- [ ] **Step 4: Run test to verify it passes** — `cargo test -p memphant-eval classify_drop_cause`. Expected: PASS.

- [ ] **Step 5: Commit** — `feat(eval): trace-classification records gold pool-unit drop cause (rung 7 diagnosis)`.

---

### Task 2: Run the diagnosis on scratch PG, classify the 64 (Phase A — FREE)

**Files:**
- Create: `docs/build-log/artifacts/rung7-packing/dev-drop-cause.jsonl`
- Create: `scratch` analysis (python) summarizing the 64 by drop cause.

- [ ] **Step 1:** Build bench-lme, run the product Fast config with `--emit-trace-classification docs/build-log/artifacts/rung7-packing/dev-drop-cause.jsonl` through `scripts/with_scratch_db.sh`, seed 20260713.
- [ ] **Step 2:** Filter to `bucket=in_pool_unpacked` (expect 64). Tabulate `gold_drop_reason` (Duplicate / Budget / Rerank / None=never-reached), and the `gold_fused_rank` distribution vs the packed_size. Cross-check against the pinned A1 counts (64 in_pool_unpacked).
- [ ] **Step 3:** Write the drop-cause classification into the build-log note. This decides Phase B's lever (decision tree below). **No commit needed if artifact-only; commit the artifact + note.**

**Decision tree (which lever Phase B implements):**
- **Dominant `None` + high `gold_fused_rank` (> packed_size, within pool)** → gold is correctly in the pool but the pack loop filled its `k`/budget with better-fused items before reaching gold: the lever is **ordering** (is fused ordering wrong for these? try session-quota W4 to force episode diversity, or sibling-gather) OR **budget/scan** (does gold sit beyond `scan_limit`?). If packed_size ≪ k (median 4 vs k 10) AND budget not exhausted, the pack loop is stopping early for a *non-budget, non-k* reason → investigate `admit_or_drop` early-exits.
- **Dominant `Duplicate`** → subject-dedup is eating gold as a "duplicate" of a decoy sharing a `fact_key`. Lever: targeted `admit_or_drop` dedup fix (keep the higher-fused of a fact_key collision, or exempt gold-eligible units). Reconcile with [[memphant-packing-gate-verdict]] — that verdict covered same-rank-tie Rerank suppression, NOT fact_key Duplicate drops.
- **Dominant `Budget`** → 8192-token budget truncates before gold. Lever: adaptive/larger budget (but 16384 was already ns-harmful per STATUS line 116 — needs the 64-subset paired test to reopen).
- **Dominant `Rerank` (replacement gate)** → the output-full/replacement contest evicted or refused gold. Reconcile with the "measured-permanent" gate ([[memphant-packing-gate-verdict]]).

---

### Task 3: TDD the diagnosis-selected lever (Phase B — conditional on Task 2)

**Files:**
- Modify: `crates/memphant-core/src/lib.rs` (the specific pack function the diagnosis names) OR a bench-lme flag flip if the lever is an existing W4 knob (session-quota / sibling-gather).
- Test: core unit test for the lever's admission behavior.

**Interfaces:** determined by Task 2. If the lever is an existing knob (session-quota, sibling-gather, budget), NO core change is needed — just pass the flag in the measurement (Task 4) and the "implementation" is the measurement itself. Only write core code if the diagnosis names a NEW behavior (e.g. a fact_key-Duplicate fix that keeps the higher-fused unit).

- [ ] **Step 1: Write the failing test** — encode the lever's intended admission behavior on a minimal fixture (gold + decoy sharing a fact_key, or gold beyond the current early-exit). The test asserts gold now survives to `context_items`.
- [ ] **Step 2: Run test to verify it fails.**
- [ ] **Step 3: Minimal implementation** — the smallest change that makes gold survive without disturbing the off-path (levers default OFF; the change must be byte-identical with the lever off, per the existing pack-lever contract).
- [ ] **Step 4: Run test to verify it passes** + full `cargo test -p memphant-core` to prove no regression.
- [ ] **Step 5: Commit** — `feat(core): <lever> recovers in-pool-unpacked gold (rung 7)`.

**If Task 2 shows NO ordering/budget/dedup lever can move the 64** (e.g. gold is genuinely lower-fused than 10 better candidates that all legitimately belong): STOP. That is the finding — the bottleneck is the reader-utilization layer (bucket C, in_top_k) or fusion scoring, not packing. Record it and redirect. Do not ship a speculative lever.

---

### Task 4: Paired measurement + promotion decision (Phase B)

**Files:**
- Create: `docs/build-log/artifacts/rung7-packing/baseline-seed20260713.json`
- Create: `docs/build-log/artifacts/rung7-packing/lever-seed20260713.json` (+ `-seed<second>` on a win)

- [ ] **Step 1:** Baseline run (product Fast config, lever OFF) → `baseline-seed20260713.json`, via `with_scratch_db.sh`.
- [ ] **Step 2:** Lever run with `--baseline baseline-seed20260713.json` → paired report. Read `paired_vs_baseline.delta_recall_at_10.ci_excludes_zero` and the CI bounds.
- [ ] **Step 3 (promotion):** If recall@10 (or @5) CI excludes zero AND is positive → rerun on a second seed; if it holds, PROMOTE (flip the default if it's a knob, keep the core change). Else REJECT: keep the negative paired report as the artifact, do not ship.
- [ ] **Step 4:** Commit artifacts + build-log verdict.

---

### Task 5: Update plan + STATUS + gate (Phase B close)

- [ ] Update rung-7 row in `docs/superpowers/specs/memphant/STATUS.md` (line 121/126 area) with the diagnosis + lever verdict.
- [ ] Update `docs/superpowers/plans/2026-07-21-tri-domain-sota-plan.md` rung-7 / packing section.
- [ ] Run the full local gate (AGENTS.md): `pytest tests/`, `cargo fmt --check`, `cargo clippy --all-targets --all-features -D warnings`, `cargo test -p memphant-core` / `--all-targets`, scratch-DB live-PG leg, spec-drift check.
- [ ] Commit locally (do NOT push). `Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>`.

---

## Self-Review

- **Spec coverage:** Success criteria 1 (64 classified by drop cause, FREE, per-question artifact) → Tasks 1–2. Criterion 2 (≥1 lever TDD'd + paired, promote on CI-excl-zero two seeds else reject) → Tasks 3–4. Criterion 3 (full gate) → Task 5. Criterion 4 (plan/STATUS/build-log) → Tasks 2/5. Criterion 5 (local commit) → each task.
- **Placeholder scan:** Task 3's interface is deliberately data-driven (it MUST be — writing lever code before the diagnosis is the over-building the ponytail rule forbids). Every FREE step is concrete. The lever's exact code is specified once Task 2 runs — that is not a placeholder, it is a genuine data dependency, and the decision tree enumerates the four concrete branches.
- **Type consistency:** `TraceClassificationRow` new fields, `RecallDropReason`, `RecallCandidateTrace.fused_rank` all match the source read in Understand.

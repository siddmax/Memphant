# C1 Episodic Slice — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Cut Syndai's episodic memory layer to MemPhant via the strict C0 contract, backfill a 252-row corpus on ephemeral scratch Postgres, and prove the three C1 acceptance bars (hot-path SLO, Conversations-tab equivalence, two-user RLS leakage).

**Architecture:** Correctness-only on a schema-faithful synthetic 252-row corpus (prod episodic is off-limits; local dev DB is wiped — both verified). Mirror the C3 code-lane runner: a Python generator + a runner that re-execs through `with_scratch_db.sh`, starts the packaged server/worker, binds context per tenant (C0 handshake), retains one episode per row, drains the worker, and recalls. Two Rust `#[ignore]`d live-PG tests carry the SLO service-layer guard and the RLS leakage proof under the real `memphant_app` role. Live Syndai loader rewiring is deferred (same boundary as C0/C3).

**Tech Stack:** Python 3 (stdlib only, matching `scripts/gate_runtime.py`), Rust (sqlx, tokio, `memphant-store-postgres` test crate), bash (`e2e_probe.sh`, `with_scratch_db.sh`), Postgres 17.

**Design of record:** `docs/superpowers/specs/2026-07-21-c1-episodic-slice-design.md` (eng-reviewed, 6 findings folded).

## Global Constraints

- Client is the C0 strict contract: `ApiClient.bind_context()` (PUT `/v1/context-bindings`); NEVER send `tenant_id`/`allowed_scope_ids`. (`AGENTS.md`, C0 memory.)
- All DB work runs on a run-owned ephemeral scratch DB via `scripts/with_scratch_db.sh`; NEVER a shared or Syndai-prod DB. (`AGENTS.md §18`.)
- Python: stdlib only, no new deps; mirror `scripts/gate_runtime.py` / `scripts/code_lane_run_memphant.py` idioms (DRY). (`AGENTS.md` working rules.)
- MemPhant DB objects live in the `memphant` schema; no shims/bypass paths. (`AGENTS.md`.)
- Flag posture for the backfill: `MEMPHANT_RESOURCE_CHUNKS` off (default) and no fact-extraction/structured-state provider, so one episode → one unit.
- Full gate before exit: `python3 -m pytest tests/`, `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test --all-targets --all-features`, `check_spec_drift.py`, the scratch-DB live-PG leg, `e2e_probe.sh`. (`AGENTS.md §37`.)
- Commit locally only (do NOT push); `Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>`.

---

### Task 1: Synthetic episodic corpus generator

**Files:**
- Create: `scripts/episodic_lane_corpus.py`
- Test: `tests/test_episodic_lane_corpus.py` (already drafted this session — treat as the RED spec)

**Interfaces:**
- Produces: `build_corpus(count: int, seed: int) -> list[dict]` — deterministic rows, each schema-faithful to `syndai.episodic_memories` (keys: `id, user_id, l0_agent_id, project_id, mission_id, content, source_kind, importance_score, trust_level, tainted, archived_at, created_at, idempotency_key`). Module consts: `SOURCE_KIND_WEIGHTS` (`user_correction`→1.5 … `system_generated`→0.3), `MAX_BODY_CHARS` (≤ ~400, well under the ~1200-token chunk-split window). Two distinct `user_id`s. Includes ≥1 archived row, ≥1 `user_correction` row, and recall-visible rows with distinct bodies.

- [ ] **Step 1: Verify the test fails** (test file already exists; corpus module does not)

Run: `python3 -m pytest tests/test_episodic_lane_corpus.py -q`
Expected: FAIL with `ModuleNotFoundError: No module named 'episodic_lane_corpus'`

- [ ] **Step 2: Write `scripts/episodic_lane_corpus.py`**

Deterministic generation keyed on `seed` (use `random.Random(seed)`, never global `random`). Two user UUIDs derived deterministically from the seed (e.g. `uuid.UUID(int=...)`). `SOURCE_KIND_WEIGHTS` copied from Syndai (`user_correction` 1.5, `user_message` 1.0, `assistant_message`/`dialog_turn` 0.8, `system_generated` 0.3). Each row: short body (< `MAX_BODY_CHARS`), `importance_score = SOURCE_KIND_WEIGHTS[source_kind]`, `trust_level` in [0,100], a deterministic `created_at` (strictly monotonic so ordering is testable), a stable `idempotency_key`. Sprinkle a few `archived_at != None` and `source_kind == 'user_correction'` rows; keep recall-visible bodies distinct. Add a `--out` CLI that writes JSONL via `gate_common.write_jsonl` for the runner to consume.

- [ ] **Step 3: Run the test to verify it passes**

Run: `python3 -m pytest tests/test_episodic_lane_corpus.py -q`
Expected: PASS (7 tests)

- [ ] **Step 4: Record the test run + commit**

```bash
~/.claude/hooks/feature-flow-state.py tests-ran
git add scripts/episodic_lane_corpus.py tests/test_episodic_lane_corpus.py
git commit -m "feat(c1): synthetic 252-row episodic corpus generator (schema-faithful, deterministic)"
```

---

### Task 2: Episodic backfill runner + Bar 2 equivalence

**Files:**
- Create: `scripts/episodic_lane_run_memphant.py`
- Test: `tests/test_episodic_lane_run_memphant.py`

**Interfaces:**
- Consumes: `episodic_lane_corpus.build_corpus`; `gate_runtime.{reexec_through_scratch_db, provision_tenant, Server, ApiClient, drain_worker, recall_query}`; `gate_common.write_jsonl`.
- Produces (pure, unit-testable): `episode_body(row: dict) -> str` (the `content` verbatim — episodic bodies are already prose, unlike the code-lane role-prefixed join); `retain_payload(ctx: dict, row: dict) -> dict` (`payload.episode{source_kind,body}` + `source_ref=f"episodic:{row['id']}"`, `observed_at=row['created_at']`); `expected_recall_set(rows: list[dict]) -> list[dict]` (rows with `archived_at is None and source_kind != 'user_correction'`, ordered by `created_at` DESC — the tab's recency order); `assert_conversations_equivalence(expected: list[dict], recalled_bodies: list[str]) -> None` (raises on a set/content mismatch).

- [ ] **Step 1: Write the failing unit tests**

```python
# tests/test_episodic_lane_run_memphant.py
import sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
import episodic_lane_corpus as corpus
import episodic_lane_run_memphant as runner

def test_retain_payload_is_strict_contract():
    ctx = {"subject_id": "s", "scope_id": "sc", "actor_id": "a",
           "agent_node_id": "ag", "subject_generation": 0}
    row = corpus.build_corpus(count=252, seed=20260721)[0]
    p = runner.retain_payload(ctx, row)
    assert "tenant_id" not in p and "allowed_scope_ids" not in p
    assert p["payload"]["episode"]["body"] == row["content"]
    assert p["payload"]["episode"]["source_kind"] == row["source_kind"]
    assert p["source_ref"] == f"episodic:{row['id']}"
    assert p["observed_at"] == row["created_at"]

def test_expected_recall_set_excludes_archived_and_corrections_and_is_recency_ordered():
    rows = corpus.build_corpus(count=252, seed=20260721)
    expected = runner.expected_recall_set(rows)
    assert all(r["archived_at"] is None for r in expected)
    assert all(r["source_kind"] != "user_correction" for r in expected)
    created = [r["created_at"] for r in expected]
    assert created == sorted(created, reverse=True)

def test_assert_conversations_equivalence_passes_on_match_fails_on_drift():
    rows = corpus.build_corpus(count=252, seed=20260721)
    expected = runner.expected_recall_set(rows)
    bodies = [r["content"] for r in expected]
    runner.assert_conversations_equivalence(expected, bodies)  # no raise
    import pytest
    with pytest.raises(Exception):
        runner.assert_conversations_equivalence(expected, bodies[:-1])
```

- [ ] **Step 2: Run to verify it fails**

Run: `python3 -m pytest tests/test_episodic_lane_run_memphant.py -q`
Expected: FAIL with `ModuleNotFoundError: No module named 'episodic_lane_run_memphant'`

- [ ] **Step 3: Write `scripts/episodic_lane_run_memphant.py`**

Mirror `code_lane_run_memphant.py` structurally. Pure functions above, then `main()`: parse args (`--database-url`, `--count 252`, `--seed`, `--out-evidence`, `--out-provenance`, `--port`, `--k 10`, `--budget-tokens 1200`, `--mode fast`, `--embed-model`, bin paths); build the corpus; `reexec_through_scratch_db`; provision tenant; start `Server`; `bind_context` per the two users (two separate bound contexts, one per `user_id`, each with its own subject/scope refs); retain each row through its user's context; `drain_worker`; then `recall_query` a broad query (mode fast, budget 1200) for user A, assert **`degraded is False`** (per `e2e_probe.sh:127`) before the equivalence check; `assert_conversations_equivalence(expected_recall_set(rows_for_user_a), recalled_bodies)`. Write evidence + a provenance report (counts, per-user, degraded, equivalence=passed). This runner IS the 252-row backfill at full count.

- [ ] **Step 4: Run the unit tests to verify they pass**

Run: `python3 -m pytest tests/test_episodic_lane_run_memphant.py -q`
Expected: PASS (3 tests)

- [ ] **Step 5: Run the runner live once (real server + scratch PG) to prove backfill + Bar 2**

Run:
```bash
python3 scripts/episodic_lane_run_memphant.py \
  --database-url postgres://memphant:memphant@localhost:5432/memphant \
  --out-evidence /tmp/c1-evidence.jsonl --out-provenance /tmp/c1-prov.json
```
Expected: stderr shows `ingested 252`, `worker drained`, `degraded=False`, `conversations equivalence: PASSED`; exit 0.

- [ ] **Step 6: Record + commit**

```bash
~/.claude/hooks/feature-flow-state.py tests-ran
git add scripts/episodic_lane_run_memphant.py tests/test_episodic_lane_run_memphant.py
git commit -m "feat(c1): episodic backfill runner + Bar 2 recall-equivalence (252 rows, scratch PG)"
```

---

### Task 3: Bar 1 — HTTP-boundary SLO + Rust PG guard

**Files:**
- Create: `crates/memphant-store-postgres/tests/hot_path_slo_pg.rs`
- Modify: `scripts/episodic_lane_run_memphant.py` (add an `--slo-samples N` mode that times `POST /v1/recall` client wall-clock and emits p50/p95, asserting p50<200ms/p95<500ms)

**Interfaces:**
- Consumes: `PgStore`, `MemoryService::recall`, `memphant_store_testkit::bind_context` (for the Rust guard); `gate_runtime.ApiClient` (for the HTTP measurement).
- Produces: Rust test `fast_mode_recall_holds_release_hot_path_slo_on_postgres`; Python `measure_recall_slo(client, ctx, query, k, budget, samples) -> dict` returning `{p50_ms, p95_ms}` and asserting the thresholds.

- [ ] **Step 1: Write the failing Rust test**

Mirror `crates/memphant-core/tests/hot_path_slo.rs` but against `PgStore` (the `#[ignore]`d live-PG pattern from `role_matrix.rs`: `MEMPHANT_TEST_DATABASE_URL`, `PgPoolOptions`, `resolve_context_binding`). Seed 252 short episodic units, warm 5×, sample 80× `service.recall(context, RecallHttpRequest{ mode: Fast, budget_tokens: 1200, .. })`, compute p50/p95, `assert!(p50 < Duration::from_millis(200))` and `assert!(p95 < Duration::from_millis(500))`.

- [ ] **Step 2: Run to verify it fails (compiles, then fails or is ignored)**

Run: `bash scripts/with_scratch_db.sh postgres://memphant:memphant@localhost:5432/memphant MEMPHANT_TEST_DATABASE_URL cargo test -p memphant-store-postgres hot_path_slo_pg -- --ignored --test-threads=1`
Expected: FAIL first (test body references a not-yet-written helper) — write the seed helper, then it compiles and runs.

- [ ] **Step 3: Implement the seed helper + make the Rust test pass**

Add the episodic-seed helper in the test file (240–252 short units, one query unit). Iterate until green.

- [ ] **Step 4: Add the HTTP-boundary SLO to the Python runner**

`measure_recall_slo`: issue `samples` real `POST /v1/recall` calls via `ApiClient`, record `time.perf_counter()` deltas, compute p50/p95 with `statistics`, assert p50<200ms/p95<500ms (raise on breach). Wire a `--slo-samples` flag into `main()` that runs it after backfill and records the numbers in the provenance report.

- [ ] **Step 5: Run both legs**

Run (Rust): the `with_scratch_db.sh … cargo test` line from Step 2 → PASS.
Run (HTTP): `python3 scripts/episodic_lane_run_memphant.py --database-url … --slo-samples 100 --out-evidence /tmp/e.jsonl --out-provenance /tmp/p.json` → stderr shows `SLO p50=… p95=…` within budget, exit 0.

- [ ] **Step 6: Record + commit**

```bash
~/.claude/hooks/feature-flow-state.py tests-ran
git add crates/memphant-store-postgres/tests/hot_path_slo_pg.rs scripts/episodic_lane_run_memphant.py
git commit -m "feat(c1): hot-path SLO — HTTP-boundary p50/p95 + Rust PgStore guard (packaged runtime)"
```

---

### Task 4: Bar 3 — episodic two-tenant RLS Rust test + probe leg

**Files:**
- Create: `crates/memphant-store-postgres/tests/episodic_rls_leakage.rs`
- Modify: `scripts/e2e_probe.sh` (add an episodic cross-tenant leg, labeled app+GUC)

**Interfaces:**
- Consumes: the `role_matrix.rs` role-binding pattern (`set local role memphant_app`, `memphant.bind_tenant($1)`, `provision_tenant`, tenant-bound inserts into `memphant.episode`).

- [ ] **Step 1: Write the failing Rust test**

`#[ignore]`d live-PG test: provision tenant A and B; under `set local role memphant_app` + `bind_tenant(A)` insert an episode for A (and identity rows); commit. Then in a new tx `set local role memphant_app` + `bind_tenant(B)` and assert `select count(*) from memphant.episode` (and `memory_unit`) returns **0** rows belonging to A (RLS blocks it). Symmetrically for B→A. This proves the RLS swap, not app-code filtering.

- [ ] **Step 2: Run to verify it fails first, then passes**

Run: `bash scripts/with_scratch_db.sh postgres://memphant:memphant@localhost:5432/memphant MEMPHANT_TEST_DATABASE_URL cargo test -p memphant-store-postgres episodic_rls_leakage -- --ignored --test-threads=1`
Expected: iterate to PASS (0 cross-tenant rows visible under the policy role).

- [ ] **Step 3: Add the app+GUC episodic leg to `e2e_probe.sh`**

After the existing cross-tenant trace 404 leg, add: retain an episode for tenant B, then assert tenant A's recall/list never returns B's body, and vice versa (the probe runs as the superuser login — label this leg explicitly `app+GUC isolation (NOT the RLS backstop — see episodic_rls_leakage.rs)`).

- [ ] **Step 4: Run the probe**

Run: `DATABASE_URL=postgres://memphant:memphant@localhost:5432/memphant bash scripts/e2e_probe.sh`
Expected: `E2E PROBE: ALL CHECKS PASSED`.

- [ ] **Step 5: Record + commit**

```bash
~/.claude/hooks/feature-flow-state.py tests-ran
git add crates/memphant-store-postgres/tests/episodic_rls_leakage.rs scripts/e2e_probe.sh
git commit -m "feat(c1): two-tenant episodic RLS leakage proof (memphant_app role) + probe app-layer leg"
```

---

### Task 5: Full gate + docs/STATUS/memory + spawn next

**Files:**
- Modify: `docs/superpowers/plans/2026-07-21-tri-domain-sota-plan.md` (C1 row), `docs/superpowers/specs/memphant/STATUS.md` (C1 row + SLO §165), memory files.
- Create: `docs/build-log/2026-07-22-c1-episodic-slice.md`.

- [ ] **Step 1: Run the full gate (AGENTS.md §37)**

```bash
python3 -m pytest tests/ -q
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
python3 scripts/check_spec_drift.py
bash scripts/with_scratch_db.sh postgres://memphant:memphant@localhost:5432/memphant MEMPHANT_TEST_DATABASE_URL cargo test -p memphant-store-postgres -p memphant-worker -- --ignored --test-threads=1
DATABASE_URL=postgres://memphant:memphant@localhost:5432/memphant bash scripts/e2e_probe.sh
```
Expected: all green. Fix any failure at root cause (systematic-debugging) before proceeding.

- [ ] **Step 2: Write the build-log** `docs/build-log/2026-07-22-c1-episodic-slice.md` — verdict, the three bars with measured numbers, the honest deferrals, the RLS-superuser standing note.

- [ ] **Step 3: Update the plan C1 row + STATUS** with the proof artifact paths (the exact-change-names-its-proof rule).

- [ ] **Step 4: Update memory** — a `memphant-c1-episodic-slice` project memory + MEMORY.md pointer; link `[[memphant-c0-strict-contract-clients]]`, `[[memphant-c3-corpus-sourceable]]`.

- [ ] **Step 5: Commit + spawn the next task** (per plan §8: B5/B6 CI-honesty legs, or the free C2 pre-check) via the spawn_task tool with a self-contained prompt.

```bash
~/.claude/hooks/feature-flow-state.py tests-ran
git add -A && git commit -m "docs(c1): episodic slice LANDED — SLO/equivalence/RLS proven; plan+STATUS+memory"
```

---

## Self-Review

- **Spec coverage:** Bar 1 → Task 3; Bar 2 → Task 2; Bar 3 → Task 4; 252-row backfill → Tasks 1+2; deferred boundary + honesty → Task 5 docs. All §4 components mapped.
- **Placeholder scan:** each code step names the exact file, the exact mirror source (`code_lane_run_memphant.py`, `role_matrix.rs`, `hot_path_slo.rs`), and the exact assertion. No TBDs.
- **Type consistency:** `build_corpus(count, seed)`, `retain_payload(ctx, row)`, `expected_recall_set(rows)`, `assert_conversations_equivalence(expected, bodies)`, `measure_recall_slo(...)` are used consistently across tasks.

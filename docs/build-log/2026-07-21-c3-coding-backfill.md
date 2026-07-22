# C3 — Coding-continuity backfill: mechanism landed, volume BLOCKED-on-data

**Date:** 2026-07-21 · **Branch:** `codex/memphant-p1-deep-mode` · **Plan:** §5 C3, §8 spine.

## Verdict

The C3 **backfill mechanism** is landed and unit-proven against the strict
contract. The **volume-matched corpus + adversarial golden** are BLOCKED on
source data: the local coding-execution events are gone and cannot be
regenerated in this session without the full Syndai CaaS stack. Per the plan's
rule ("where volume can't be bootstrapped, state honestly that a slice is
correctness-only and defer recall-quality until volume exists"), C3 ships
correctness-only; the golden is a runnable procedure that executes the moment a
corpus exists.

## What landed (correctness, provable now)

1. **Strict-contract ingest/recall** (`scripts/gate_runtime.py`,
   `scripts/code_lane_run_memphant.py`, commit `4f90ef57`). The runner posted
   the banned `tenant_id` shape (same C0 defect) — it would 422 against the
   landed contract. Now:
   - `ApiClient.bind_context()` resolves external refs → the five ids +
     `subject_generation` (PUT `/v1/context-bindings`, mirroring
     `scripts/e2e_probe.sh`).
   - `ingest_attempt` retains one attempt as `payload.episode {source_kind,
     body}` with the bound identity + `source_ref`/`observed_at`; `recall_query`
     spreads the bound context. No `tenant_id`/`subject_hint`.
   - New unit test pins `ingest_attempt`'s payload to `openapi/memphant.v1.json`
     (oneOf-aware). 14 code-lane tests green.

2. **The `retain(episode)` backfill IS this ingest path at `--limit-attempts 0`
   (full corpus)** — one episode per coding attempt, its event sequence
   (`message_end` → assistant/user, `tool_execution_end` → toolResult) rendered
   as a role-prefixed transcript. Runs on a run-owned ephemeral scratch DB
   (`with_scratch_db.sh`); never a shared/Syndai-prod DB.

## The streaming retain hook (attachment point identified; live wiring deferred)

- **Chokepoint:** `Syndai backend/src/features/coding/events.py:append_coding_event`
  → the `if inserted:` block. A coding *attempt* (`CodingExecutionAttempt`,
  keyed by `coding_run_id`; events keyed `attempt_id` + `sequence`) becomes one
  MemPhant episode at its terminal event (`coding.run.completed` /
  `.failed` / `.cancelled`) — the point a full transcript exists.
- **Hook contract:** on terminal, read the attempt's event sequence, render the
  same role-prefixed body the offline miner uses, and `retain_episode` via the
  rebuilt MemPhant SDK/adapter (C0). Behind the default-off
  `memphant_file_memory_dogfood_enabled`-class flag; a hook failure must not
  roll back the coding event (best-effort, unlike the attention-row applier).
- **Deferred, same boundary as C0's live wiring:** needs a real Syndai MemPhant
  context binding (Syndai has no `subject_generation`/`agent_node` concept yet)
  and the full CaaS stack to test end-to-end. Building it live now would be
  speculative against an untestable hot transactional path.

## Why volume is blocked (verified, not assumed)

- `syndai-coding-local-db` (docker `docker-compose.coding-local-db.yml`, port
  55432) starts healthy, schema + Alembic head applied — but
  `syndai.coding_execution_attempt_events` and `.coding_execution_attempts` are
  **empty (0 rows)**. The persistent volume survived a reset but the historical
  events were wiped (the compose file warns pg16→pg17 needs a volume wipe).
- `bootstrap_coding_local_db.py` seeds ONE CaaS tenant, not events. No dump /
  snapshot anywhere on disk (`LOCAL_CODING_SNAPSHOT_DIR` empty). The committed
  `coding_events_golden.lock.json` records the last real extraction — only 359
  attempts / 305 with events from `syndai_local`, since wiped.
- Local generation = real coding runs through the full CaaS stack
  (`make coding-local-smoke-syndai` → `syndai_run` → Daytona sandbox → GitHub
  PR, 1800s each): produces events but is credential-heavy and slow for volume.
- Syndai **production** has the ~64k events but is off-limits (AGENTS.md §18)
  without explicit per-operation authorization.

## Deferred backfill + golden procedure (runnable when a corpus exists)

Once `syndai.coding_execution_attempt_events` has rows (regenerated locally, or
a one-time authorized read-only prod extract into the gitignored corpus file):

```bash
# 1. Extract corpus from the events table (read-only; never writes that DB)
python3 scripts/code_lane_extract.py \
  --database-url postgresql://syndai:syndai@127.0.0.1:55432/syndai_local \
  --out-corpus benchmarks/data/coding_events_corpus.jsonl \
  --out-stats  benchmarks/data/coding_events_corpus.stats.json

# 2. Mine a volume-matched adversarial golden (span-located QA; too-generic gate)
python3 scripts/code_lane_mine.py \
  --corpus benchmarks/data/coding_events_corpus.jsonl \
  --out-golden benchmarks/data/coding_events_golden.jsonl \
  --out-lock   benchmarks/data/coding_events_golden.lock.json \
  --target <N matched to corpus volume>

# 3. Backfill via retain(episode) + recall, on an ephemeral scratch DB
python3 scripts/code_lane_run_memphant.py \
  --database-url postgres://memphant:memphant@localhost:5432/memphant \
  --limit-attempts 0 --out-evidence <e.jsonl> --out-provenance <p.json>

# 4. (paid) reader-QA the evidence for the recall-quality bar
python3 scripts/run_reader.py --evidence <e.jsonl> ...
```

The golden this produces is the **C1 acceptance bar** the spine requires. Until
step 1 has rows, C1's recall-quality parity stays deferred; C1's contract /
schema work (loader cutover, RLS proof) is unblocked because the golden's
*shape* and the ingest mechanism are settled here.

## Corpus-source recommendation (deep-research 2026-07-21)

The wiped local DB is NOT the only source. A 102-agent adversarially-verified
research pass (`docs/build-log/research/2026-07-21-coding-corpus-datasets.md`)
found abundant **public, permissively-licensed** coding-agent trajectory
datasets on HuggingFace that ship genuine multi-turn tool-use transcripts
(assistant reasoning/actions + tool/environment observations — NOT (issue,patch)
pairs), which map 1:1 to this pipeline's role/toolResult span extraction. This
reframes C3's volume gap from "blocked" to "needs a ~1-file schema adapter."

**Recommended path (effort-to-value ranked):**

1. **First ingest — `nebius/SWE-rebench-openhands-trajectories`** (OpenHands
   schema, CC-BY-4.0, ~67k trajectories, avg ~64 turns — deep enough to bury
   answer spans, the property the adversarial golden needs). Clean
   role/content/tool_calls; a small adapter maps it to the extractor's
   `{event_type: message_end|tool_execution_end, payload}` shape. Or
   `SWE-smith-trajectories` (MIT) for a tiny first pass.
2. **Volume layer** — NVIDIA `SWE-Zero-openhands-trajectories` (~318k) /
   `Open-SWE-Traces` (~207k, 9 languages) / `nebius/SWE-agent-trajectories`
   (~80k) once the adapter is proven on the small set.
3. **Golden construction** — adopt **LongMemEval's** verified needle-in-haystack
   recipe (insert evidence sessions among unrelated distractors + plausible
   timestamps) for the LLM span-QA `code_lane_mine.py` already does; borrow
   **SWE-ContextBench's** GitHub issue/PR dependency-mining for a
   higher-quality golden slice graded by tests rather than span-QA.

**Caveats (must carry into any promotion claim):**
- These are **synthetic distillation rollouts** (model-generated), not organic
  production traffic — fine for a retrieval corpus + buried-evidence golden,
  but NOT "real coding agent" evidence; a promotion using them says so.
- Exact row counts / arXiv ids / model names in the report are model-reported
  at "high confidence" but some carry near-future dates — **verify each dataset
  card on HuggingFace before pinning** (size, license, schema) rather than
  trusting the numbers here.
- This is distinct from the Syndai-prod corpus (the "distribution drift:
  local-dev-mined vs prod" the plan wanted). A public SWE-trajectory corpus is
  a *code-profile* corpus and a valid C1 acceptance bar, but it is not Syndai's
  own coding traffic; that distinction stays explicit.

**Next action** (own follow-up, not this task): write the HF→extractor schema
adapter, verify one dataset card, ingest a small slice through the
now-contract-correct `retain(episode)` path, and mine the first real golden.

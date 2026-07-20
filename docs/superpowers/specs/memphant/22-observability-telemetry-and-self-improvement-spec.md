# MemPhant - Observability, Telemetry, and Self-Improvement Spec

## 0. Three Planes

| Plane | Owns |
|---|---|
| Runtime telemetry | service health, spans, logs, metrics |
| Quality facts | retrieval traces, eval results, regression events |
| Improvement workflow | detected regression -> issue/PR proposal |

Do not mix product analytics with raw memory content.

## 1. Runtime Telemetry

Use OpenTelemetry naming:

```text
memphant.retain
memphant.recall
memphant.retrieve.fts
memphant.retrieve.vector
memphant.retrieve.fusion
memphant.retrieve.context_assembly
memphant.forget
memphant.job.extract
memphant.job.embed
memphant.eval.run
```

Attributes avoid raw memory text. Use IDs, counts, timings, and sizes.

## 1.1 Span Attributes

Common attributes:

```text
memphant.tenant_id_hash
memphant.scope_id
memphant.actor_kind
memphant.agent_level
memphant.request_id
memphant.engine_version
memphant.trace_schema_version
```

Recall attributes:

```text
memphant.retrieval_id
memphant.feature_flags_hash
memphant.candidate_count
memphant.returned_count
memphant.dropped_count
memphant.context_tokens
memphant.cost_micros
```

Channel spans:

```text
memphant.channel
memphant.embedding_profile_id
memphant.index_kind
memphant.rows_scanned
memphant.top_k
```

Never add raw query text, raw memory text, secret values, or full resource URIs to spans by default.

### 1.1a Memory-Op Span Trees (the shape, not just the names)

OTel GenAI's semantic conventions are still **pre-stable** (in development), and a `gen_ai.memory.*` attribute family is an **active proposal** (semantic-conventions issue #2664 / genai #35), not yet shippable to align against. So memory ops own the `memphant.*` namespace **today** — and because the trace schema is versioned (`trace_schema_version`), emitting an additional `gen_ai.memory.*` mapping if/when OTel stabilizes it is **additive**, not a rewrite. (Do not freeze on the false premise that OTel has no memory concept.) Rule: a span is `gen_ai.*` **only** where MemPhant actually calls a model (the `reflect` extraction LLM call, the embed call); everything memory-specific is `memphant.*`. `memphant.recall` is a **parent over per-channel children** (they run concurrently, `02` §1.1a), so the trace *is* the stage ladder:

```text
memphant.recall                 {retrieval_id, mode_requested, mode_executed, escalation_reason}
├─ memphant.gate                {policy_version, denied_selectors_count}
├─ memphant.retrieve.vector     {embedding_profile_id, index_kind, filter_selectivity, iterative_scan_depth}
├─ memphant.retrieve.fts/exact/edge  {channel, candidate_count, rows_scanned}
├─ memphant.retrieve.fusion     {weight_vector_id, k_rrf=60, fused_count}
├─ memphant.retrieve.rerank     {reranker_id, rerank_input_count, rerank_overfetch_ratio}
└─ memphant.retrieve.context_assembly {budget_tokens, dropped_count, abstention_signal}
```

A flag-disabled channel still emits its span with `candidate_count=0` (an ablation arm is a present-but-empty span, not a missing one). `memphant.reflect` is the **extract→contradict→corroborate→promote** tree — the poisoning-decision moment, so `memphant.reflect.corroborate` carries `distinct_actors`/`distinct_origins`/`independence_pass`: a promotion with `distinct_origins=1` is the **span-level signature of a corroboration-farming attempt** (`04` §5), visible independent of whether the security suite caught it. The `extract` child carries the `gen_ai.*` attributes (`gen_ai.usage.*_tokens`); the parent does not impersonate them.

### 1.2 OTel Collector Pipeline

The collector runs a fixed pipeline so redaction happens **before** export, never after:

```yaml
processors:
  memory_limiter: { check_interval: 1s, limit_mib: 512 }   # backpressure first
  attributes/redact:                                        # strip raw text/secrets/URIs
    actions:
      - { key: memphant.query_text, action: delete }
      - { key: memphant.memory_text, action: delete }
      - { key: memphant.resource_uri, action: delete }
      - { key: memphant.tenant_id, action: hash }
  batch: { timeout: 5s, send_batch_size: 1024 }
service:
  pipelines:
    traces:  { processors: [memory_limiter, attributes/redact, batch] }
    metrics: { processors: [memory_limiter, batch] }
```

Exporter choice (OpenObserve / Tempo / vendor) is not a product contract; the OTel semantics + this redaction order are.

### 1.3 Ranking-Quality SLIs (with alert thresholds)

The SLIs that page someone — quality, cost, and the new write-path/filter honesty signals — with default thresholds (tuned per `24` registry):

| SLI | Threshold (alert) |
|---|---|
| golden recall@k | drop > 1pt vs baseline → `block` |
| citation validity | < 99% → `block` |
| recall p95 latency | > 1.5s → `warn`, > 3s → `block` |
| `consolidation_lag` p95 | > 60s → `warn` (degraded recall is being served, `02` §3.1) |
| `filter_selectivity` low-recall rate | > 2% of vector stages hit the recall floor → `warn` (small-tenant HNSW, `02` §2.1b) |
| per-tenant recall@k vs corpus growth | recall@k on a per-tenant golden set drops below the floor as the corpus grows → `warn` then raise `ef_search` (HNSW degrades **silently** — latency stays flat, `02` §2.1b) |
| storage growth per tenant | > configured ceiling → `warn` (retention-tier job behind, `04` §2.4) |
| poisoning ASR (any suite) | > 0 → `block` |
| cross-tenant result count | > 0 → `block` (page immediately) |
| `findability` (post-write retrievability probe failures) | sustained probe-failure rate > 0 → `warn` (flag-gated: `retrievability_probe_enabled`; owner `04` §9) |
| `memory_utility_trend` | rolling monthly rate of `mark` outcome=`success` vs `failure`/`corrected`/`ignored` over labeled recalls declines vs baseline → `warn` |

`memory_utility_trend` is computed per tenant on the dogfood lane and is the production instrument for "did memory improve the agent over N months." It is **observational**: the agent model changes over time, so the trend cannot carry a causal claim — the causal version stays with STATE-Bench's paired no-memory ablation (`27` §1). This SLI is also the definition of the **per-unit utility facts** `05` §3.1 says `mark` writes: each `mark` posts `{trace_id, used_ids[], outcome}`, which lands as per-unit utility fact rows; the SLI is the rolling aggregate over those rows.

### 1.4 Cost Observability (recall is cheap; `reflect` is the cost center)

Cost attribution splits the two profiles `02` §3 / `04` §9 already separate — conflating them hides where money goes:

| Cost class | Where | Attribution key |
|---|---|---|
| **recall cost** | `memphant.recall` span `cost_micros` (deterministic CPU + query-embed; near-flat) | per `(tenant, mode_executed)` — `deep`/L4 is the tail |
| **reflect cost** | `memphant.reflect.extract` `gen_ai.usage.*_tokens` (the expensive LLM pass) | per `(tenant, subject_key fan-out)` — the real bill |
| **storage cost** | per retention tier (`04` §2.4) | per `(tenant, retention_tier)` |

The $ SLIs that page are `cost_per_recall` and `cost_per_reflect` against a **rolling baseline** (request cost moves *before* the provider invoice does — a context-bloat or retry-storm regression shows in `cost_micros` days before the bill). **The cost detector is paired with the quality detector, never standalone:** a lever promotion (§4.2) raising both `delta_recall_at_k` and `delta_cost_micros` past its band is a **Pareto regression**, not an improvement — a quality-only monitor stays green while spend doubles. A rising `fast→balanced` escalation rate (`05` §1.3) is a cost SLI too (the cheap pass is increasingly insufficient — a recall-quality regression surfacing as a cost one).

**Cost knobs are pinned by executable contract, not convention (Syndai-production-proven).** `reflect`'s extraction calls go through a single purpose→model-tier declaration chokepoint, and **contract tests pin the declaration** — `max_tokens`, `temperature`, and tier per purpose — so the suite trips the moment someone bloats the prompt or silently switches the extraction pass to a frontier model (Syndai's precedent pins extraction at `max_tokens` ≤ 768, `temperature` ≤ 0.1 with an explicit purpose→tier map). The rolling-baseline $ SLIs above catch what the contract cannot see (retry storms, fan-out regressions); the contract catches what a rolling baseline sees too late (a tier flip that reads as gradual drift).

## 2. Quality Fact Tables

These are product data, not logs — queryable, versioned, durable. DDL for the load-bearing ones (full retrieval-trace shape is `05` §3.1; these are the eval/regression facts):

```sql
eval_run (
  id            uuid PRIMARY KEY,
  tenant_scope  text NOT NULL,          -- 'internal' for public scorecard runs
  git_sha       text NOT NULL,
  benchmark_id  text NOT NULL,
  benchmark_version text NOT NULL,
  engine_version text NOT NULL,
  methodology_version text NOT NULL,
  feature_flags jsonb NOT NULL,
  source_status text NOT NULL,          -- independently_reproduced|vendor_reported|...
  started_at    timestamptz NOT NULL DEFAULT now(),
  finished_at   timestamptz,
  INDEX (benchmark_id, started_at)
)

eval_case_result (
  id           uuid PRIMARY KEY,
  eval_run_id  uuid NOT NULL,
  case_id      text NOT NULL,
  cluster_key  text,                     -- session_id/corpus_id the case belongs to; the unit a clustered-SE bootstrap resamples (05 §8). NULL = independent case.
  passed       boolean NOT NULL,
  accuracy     real,
  recall_at_k  real,
  citation_valid boolean,
  latency_p95_ms int,
  cost_micros  bigint,
  trace_ref    text,                     -- -> retrieval_trace
  UNIQUE (eval_run_id, case_id)
)

regression_event (
  id           uuid PRIMARY KEY,
  metric       text NOT NULL,            -- 'golden_recall_at_k'|'citation_validity'|'p95'|'poisoning_asr'|...
  baseline     real NOT NULL,
  observed     real NOT NULL,
  delta        real NOT NULL,
  eval_run_id  uuid NOT NULL,
  severity     text NOT NULL CHECK (severity IN ('warn','block')),
  status       text NOT NULL DEFAULT 'open',   -- open|investigating|pr_drafted|resolved
  created_at   timestamptz NOT NULL DEFAULT now(),
  INDEX (status, severity, created_at)
)

poisoning_event (
  id           uuid PRIMARY KEY,
  suite        text NOT NULL,            -- 'corroboration_farming'|'low_trust_web'|'tool_output'|...
  bypassed     boolean NOT NULL,         -- true = a poisoning attempt SUCCEEDED (release-blocking)
  eval_run_id  uuid NOT NULL,
  detail       jsonb,
  created_at   timestamptz NOT NULL DEFAULT now()
)

delete_audit_event (
  id           uuid PRIMARY KEY,
  tenant_id    uuid NOT NULL,
  deletion_generation bigint NOT NULL,
  policy       text NOT NULL,            -- hard_delete|tombstone
  invalidated  jsonb NOT NULL,           -- {units, embeddings, edges, resources, blobs}
  verified     boolean NOT NULL,         -- no_recall_path_returns_forgotten
  created_at   timestamptz NOT NULL DEFAULT now(),
  INDEX (tenant_id, deletion_generation)
)
```

## 2.1 Telemetry vs Quality Facts

| Data | Store | Retention |
|---|---|---|
| p95 latency, status codes | telemetry | short/medium |
| retrieval candidate ranks | quality facts | tenant-configurable |
| benchmark case result | quality facts | long |
| raw memory snippet | tenant data/trace store only if policy allows | tenant policy |
| product quickstart event | analytics | product retention |

Exporter choice is not a product contract. OpenTelemetry is the contract.

### 2.2 The Regression Drill-Down (the join that earns the three planes)

The three planes are only worth separating if one query crosses them. The join keys are already columns; the contract is they're populated + indexed, so "why did golden recall@k regress in run X" is one query:

```text
product event (20 §3) ──trace_id──▶ retrieval_trace (05 §3.1) ──id=trace_ref──▶ eval_case_result (22 §2)
                                          │ engine_version, config_hash, weight_vector_id      │ eval_run_id
                                          ▼                                                     ▼
                                    candidate[] (05 §3.2)                              regression_event (22 §2)
```

```sql
-- Which cases NEWLY failed, and what changed in their trace vs the baseline run?
SELECT cr.case_id, cr.recall_at_k, cr.trace_ref,
       t.weight_vector_id, t.mode_executed, t.escalation_reason,
       t.filter_selectivity, t.consolidation_lag, t.config_hash
FROM eval_case_result cr JOIN retrieval_trace t ON t.id = cr.trace_ref
WHERE cr.eval_run_id = :regressed_run AND cr.passed = false
  AND cr.case_id NOT IN (SELECT case_id FROM eval_case_result
                         WHERE eval_run_id = :baseline_run AND passed = false);
```

This is the query the self-improvement loop (§4) attaches to the drafted issue — it converts "recall dropped 2pt" into "these 7 cases fail, all share `weight_vector_id=v3` and a `consolidation_lag` spike," a lever not a number. The `config_hash`/`weight_vector_id`/`engine_version` diff between baseline and regressed trace **is** the failure-to-lever map (`05` §6) made executable. Index: `eval_case_result (eval_run_id, passed)` + `retrieval_trace (id)`.

## 3. Regression Detection

### 3.0 The Detector (so "drops" is not hand-waved, and a flaky bench never pages)

A `regression_event` fires from a **statistical decision, not a bare inequality** (reusing `05` §8's CI discipline so detector and SOTA gate share one method):

- **Gold gates (golden recall@k, citation validity, oracle set-membership) are deterministic fixtures** — no model variance — so a single-run drop past the §1.3 threshold **is** the signal; fail closed (a golden moving at all is a code change, not noise).
- **Sampled/external benchmarks are noisy** (a model swap moves scores ~10pt), so a single low run must **not** page: require a **paired delta vs the pinned baseline** whose bootstrap CI half-width is below ±2% (`05` §8) **and** whose paired-delta CI excludes zero. A wide-CI run is `inconclusive`, logged, not a `regression_event`.
- **Latency/cost SLIs use a change-point (CUSUM-style) test** — a `regression_event` needs a **sustained shift** (N consecutive windows past threshold), not one spike.
- **Security gates are exempt from the noise filter** — poisoning ASR > 0, cross-tenant > 0, deletion-completeness fail are deterministic, single-occurrence, page-immediately; they never wait for a second window. An over-muted detector that noise-suppresses these is the failure mode to avoid.

Trigger regression when:

- golden recall@k drops
- citation validity drops
- p95 latency regresses
- poisoning fixture succeeds
- deletion completeness fails
- cross-tenant result count nonzero

DB/security drift triggers:

- RLS/grant drift
- migration revision mismatch
- function `search_path` drift
- extension-in-public warning for MemPhant-owned extension strategy
- vector dimension mismatch
- advisor critical finding in `memphant`
- memory table browser/API-role exposure

### 3.1 Memory-Specific Drift (distinct from the schema drift above)

Schema drift is caught by `db lint`. Three *memory* drifts degrade recall with a green schema:

- **Embedding-model drift** — a provider silently reships a model (same name, new weights) → the same text embeds differently and recall decays with no error. Detector: a pinned **anchor set** (≤50 fixed strings per profile) re-embedded on a schedule; per-anchor cosine distance from the stored reference > threshold raises `embedding_drift` and gates new writes on that profile until `reembed_profile` re-baselines. The anchor set *is* the "compare new profile before switch" comparison (`02` §5.1) made a standing monitor.
- **Corpus/query drift** — a tenant's query distribution shifts; golden stays green while live recall rots. Detector: rolling cosine similarity of `query_features` against a trailing window; a new low-precision cluster is an early-warning `warn` routed to the loop as "add golden coverage for cluster C," never an auto-config change.
- **Real regression vs benchmark contamination (the disambiguation that protects the SOTA claim)** — when a sampled external benchmark *improves*, the loop must ask "did we get better, or did the answer model memorize the benchmark?" The disambiguator is the **answer-model-independent oracle** (`05` §4.0) — it cannot be contaminated (it scores `answer_bearing_ids` set-membership, not model output). **Rule: a benchmark gain is credited as a real lever win only if the retrieval-only oracle moves with it.** Oracle flat + public-benchmark up = suspected contamination → `source_status` downgraded, claim not published. This makes §4's "no auto-update of public benchmark claims" *measurable*, not just procedural.
- **Oracle rot (the validator's own labels degrade)** — the oracle is contamination-proof but its hand-authored `answer_bearing_ids` can still rot when a fixture corpus is edited or a minimal set was authored wrong, silently weakening the gate it backs. Detector: scheduled nightly (`05` §4.1) `memphant-eval verify-golden` over the **whole** golden corpus (not just at authoring) — a label that stops being load-bearing fails standing; and a two-author confirm on every new golden family before it can gate (`05` §4.0). Distinct from contamination: contamination is "the model memorized the benchmark," rot is "our gold labels no longer mean what they claim."

Judge drift is **out of scope by construction** — the PR/nightly gate is LLM-judge-free (`05` §4.0); a judge appears only one rung up, where the same anchor-set method would detect it. Do not build a judge-monitoring surface for a judge that isn't on the gate.

## 4. Self-Improvement Loop

```text
nightly eval
  -> regression_event
  -> draft GitHub issue
  -> draft agent investigation branch when a maintainer enables it for that repo
  -> human review
  -> PR
```

No auto-merge. No auto-update of public benchmark claims.

The agent investigation step is **hardened against the 2026 coding-agent advisories** — least privilege, draft-PR-only, trusted-input-only trigger:

```yaml
on: { repository_dispatch: { types: [memphant_regression] } }   # trusted internal trigger only
permissions: { contents: write, pull-requests: write }          # NOT: secrets, deployments, admin
jobs:
  investigate:
    steps:
      - uses: actions/checkout@<pinned-sha>                      # full-SHA pin, never @main
      - run: memphant verify --lock memphant.lock                # version drift gate first
      - uses: anthropics/claude-code-action@<pinned-sha>
        with:
          mode: investigate
          allowed_tools: "Read,Grep,Bash(memphant eval *),Bash(cargo nextest *)"  # allowlist
          output: draft-pr-only                                  # never push to main, never merge
```

The loop may **propose** index/cap/RRF-weight/trust-policy/docs/golden changes (with archived before/after traces); it may **not** auto-apply public benchmark claims, trust/security relaxations, schema migrations, deletion-behavior changes, or hidden hosted-only behavior. (This mirrors Syndai's frontier-monitor worker — the proven precedent for a self-improvement loop.)

### 4.1 Improvement Guardrails

The improvement loop can propose:

- index changes
- candidate-cap changes
- RRF weights
- trust-policy changes
- docs/example fixes
- golden-case additions

The improvement loop cannot auto-apply:

- public benchmark claims
- trust/security policy relaxations
- **structural/DDL schema migrations** (an additive, threshold-gated **re-embed** under a new `embedding_profile` is a *data* re-derivation, not DDL — it is auto-applicable, §4.3)
- deletion behavior changes
- hidden hosted-only behavior

Quality-moving knobs require archived before/after traces and pass the §4.2 net-harm tripwire. Whether the final merge is auto or human is a **dry-run policy flag** (like the coding-harness slot), not a hard rule — a change that clears the *full* confirmation (§4.3) may auto-merge exactly as the other frontier slots do.

### 4.2 Candidate Eval + the Human-Gate Surface

A drafted PR that flips a lever is **not** allowed to arrive as a diff with a claim — it must run a **paired candidate eval** and attach the paired result:

- **Archived-trace replay, never live** — the candidate config replays the **same archived trace corpus** as the baseline (on the replica pool, `02` §1.3 — never live recall), producing a paired comparison on identical cases. The PR body carries the §2.2 drill-down: the cases that *changed verdict* (newly-passing AND newly-failing), each with its before/after trace.
- **The decision surface is the Pareto triple, never accuracy alone** — every proposal reports `Δrecall@k`, `Δcost_micros`, `Δp95` together with paired CIs. +1.5pt recall for +40% cost is **Pareto-ambiguous**, surfaced as such, never auto-flagged "improvement" (the cost regression of §1.4 is the silent failure of accuracy-chasing self-improvement).
- **Three-way human gate, recorded** — `accept` / `reject` (close + write the rejection reason back as a golden so the loop doesn't re-propose) / `needs-coverage` (the eval set can't distinguish the change → blocked on a new golden, not on taste). The decision flows to `regression_event.status` so the loop's own accept-rate is measurable.
- **Net-harm tripwire** — if the candidate eval regresses any §1.3 `block` SLI (citation validity, poisoning ASR, cross-tenant, deletion), the loop **discards the proposal and drafts no PR at all** — a self-improvement step never spends a human's review budget on a change that trades recall for a security floor.

### 4.3 Model-Currency Loop (keeping the best memory components)

The swappable components behind frozen interfaces — the **embedding model** (`embedding_profile`) and the **reranker** (`Reranker` trait) — go stale as the field moves, so they are tracked by the same self-improvement loop, with the same draft-PR-only safety:

- **Discover → CONFIRM-via-our-eval → operator-gated swap.** Candidate models are *nominated* by external leaderboards (LMEB for embeddings, `02` §2.1a; reranker boards) but **never promoted on a leaderboard alone** — a candidate is confirmed only by re-running the affected axes of `memphant-eval profile` (`12` §2.0a) with the candidate swapped in, on MemPhant's own pinned harness. This is the "discover-via-benchmarks, confirm-via-our-eval" doctrine.
- **A `memory_component_evidence` ledger** (distinct from `eval_run`/`regression_event`, §2) records each candidate's per-axis profile delta; a swap is proposed (draft PR) only when it beats the incumbent by the stated bar (e.g. ≥2 N@10 on the embedding-selection axis, ≥0.03 MRR rerank):

```sql
memory_component_evidence (
  id            uuid PRIMARY KEY,
  component_kind text NOT NULL CHECK (component_kind IN ('embedding','reranker')),
  candidate     text NOT NULL,        -- model id under test
  incumbent     text NOT NULL,        -- current default it must beat
  axis          text NOT NULL,        -- profile axis key (12 §2.0a): embedding_selection | <recall axes>
  delta         real NOT NULL,        -- candidate − incumbent on that axis
  ci            jsonb NOT NULL,        -- bootstrap CI of the paired delta
  profile_run_id uuid NOT NULL,        -- the memphant-eval profile run that produced it
  harness_pin   jsonb NOT NULL,        -- answer_model + the OTHER components held fixed
  verdict       text NOT NULL CHECK (verdict IN ('beats','ties','loses')),  -- ci excludes 0 AND delta>bar
  created_at    timestamptz NOT NULL DEFAULT now(),
  INDEX (component_kind, candidate, created_at)
)
```
- **Auto-merge — same machinery as the coding/planning/model slots, gated on the *full* confirmation, not on a human.** It uses the identical `auto_merge` path (`07` §10), behind the same **dry-run flag** (dormant until enabled), and promotes only after the full gate passes — the **confirming axes** (golden: embedding-selection + the recall axes) **plus the tail-risk axes** (restraint/OP-Bench + longitudinal/MemoryStress) **plus no `block`-SLI regression** (the §4.2 net-harm tripwire). A candidate that wins recall but regresses restraint or a security gate is auto-rejected, never merged. The component nuance is only *what happens after merge*:
  - **Reranker** — no migration; fully auto-merge + auto-cutover (a pure read-path swap).
  - **Embedding** — auto-merge the new `embedding_profile` + auto-enqueue the re-embed (additive/offline/idempotent, `14` §10); **cutover is auto-gated** on re-embed completion + profile re-confirmation + explicit recall thresholds. The cost/blast-radius of a default-embedding change is the one reason a deployment may keep this human-pinned via the dry-run flag — a policy choice, not an architectural limit.
- **Dogfood cadence (neutral):** this is MemPhant's own in-product loop; Syndai's frontier monitor may *drive its cadence* by enrolling these components as a frontier target (`07` §10), but confirmation always runs MemPhant's *public* eval, and the resulting better default ships to every user via the merge — no private fast-path.

The trace archive contract and SOTA promotion rules live in `27-sota-ladder-and-validation.md`.

## 5. Retention

Default:

- service logs: short retention
- traces: configurable
- eval summaries: long retention
- raw memory text in telemetry: never

Trace content may include memory snippets, so it inherits tenant data policy.

## 6. Redaction Rules

- hash or omit tenant identifiers in telemetry
- store raw memory content only in tenant-governed trace stores
- redact secrets before trace persistence where possible
- record redaction reason so missing text is explainable
- keep citation IDs/hashes even when snippets are redacted
- separate analytics from tenant memory data

## 7. Dashboards

Minimum dashboards:

- service health: request rate, error rate, latency
- recall pipeline: stage latency and candidate counts
- eval health: latest golden/nightly/release results
- security: poisoning/deletion/cross-tenant suite status
- DB drift: migration, grants/RLS, extension, vector dimension status
- dogfood: Syndai contract and cutover readiness

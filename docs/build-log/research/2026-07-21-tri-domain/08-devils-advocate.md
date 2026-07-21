# Devil's Advocate — MemPhant campaign (2026-07-21)

All evidence read from `/Users/sidsharma/.codex/worktrees/Memphant/p1-deep-mode`
(STATUS.md, NEXT-SESSION-PROMPT.md, .superpowers/sdd/progress.md, build-log,
p1-t6 artifact roots) and read-only LOC counts from `/Users/sidsharma/Syndai`.
Nothing was modified. All ratings are objective: each item names concrete ledger
evidence and a cheap early test.

## Steel-man first (what the plan gets right)

- The substrate is real: Postgres+pgvector runtime with REST/MCP/CLI/worker,
  durable e2e probe, RLS on 25 tenant tables, bitemporal model — proven, not
  vaporware (`2026-07-10-runtime-postgres-proof.md`, STATUS §2).
- The discipline was earned, not paranoid: the 2026-07-09 reopen after the
  2026-07-03 synthetic-fixture promotions was an honest self-correction, and the
  promotion-provenance rule + paired two-seed CIs genuinely exceeds the industry
  evidence bar (every competitor SOTA number is a self-run per the verified
  2026-07-18 landscape research).
- The docs flip is real at its operating point (+0.142 [+0.058,+0.225], both
  sets CI-clean), the rung-4 closure is real (+0.110 excl-0 through the packaged
  runtime), and Deep mode is BUILT with fail-closed transport, settlement, and
  caps — the negative results (levers 1–5 rejected, Flash rejected, rerank
  latency-retired) are honestly recorded.
- Cutover targets a real duplication: Syndai carries 12,696 LOC knowledge/RAG +
  21,790 LOC memory + 2,982 LOC code-intel (measured today) that MemPhant could
  eventually replace.
- Pre-production freedom means no user is harmed while benchmark evidence is
  assembled.

Now the attack.

---

## Ranked attack list

### 1. The T6 live gate is a stalled pipeline whose entire evidence yield is ONE row, scored 0.0 — and it just failed again, on infrastructure, while the handoff still says "a live run is active"

- **Failure mode:** the campaign's flagship deliverable (Deep-recovers-Fast
  paired evidence) cannot complete on the current execution environment; each
  failure consumes a session, a new amendment, and a new sealed root, while the
  hypothesis stays exactly as unvalidated as it was two weeks ago.
- **Evidence:** `docs/build-log/artifacts/p1-t6/` contains 12 `run-*` roots, 5
  `live-*` probes, 2 diagnostics, 3 `no-model-*` roots — all dated 07-20/07-21 —
  plus 14 pre-execution amendments. Three distinct failure classes so far:
  stream identity mismatch (fixed `2e5c9bcd`), provider `parallel_tool_calls`
  default (fixed `69ab5a54`), and now `run-65981e4f/0002-19367bc7-sonnet/failure.json`:
  `/v1/recall returned HTTP 503 backend_unavailable` because the local Postgres
  Docker container ceased to exist mid-run (`docker ps -a` → zero containers;
  Amendment 14 root-cause, written 15:35 today). The only settled operational
  row in campaign history, `0001-19367bc7-fast`, scored
  `overall_full_set: 0.0` (`pct_unknown: 1.0`). Rows 3–24 never opened. Zero
  Deep dispatches have ever bound (`observed_deep_config_hash: null`).
  NEXT-SESSION-PROMPT.md still instructs the next session to babysit a process
  that is already dead (no matching PID as of this audit).
- **Cheapest early test:** one overnight, zero-cost, full n=12×24-row rehearsal
  with the stub/no-model provider through the *identical* controller, Docker,
  and scratch-DB path. If the environment cannot hold 24 unpaid rows for a few
  hours, it cannot hold paid ones — and every paid root until then is a coin
  flip against Docker Desktop.
- **Cut/change:** no further paid dispatch until the unpaid rehearsal passes.
  Give the run a Postgres it owns (controller-supervised `pg_ctl` on a unique
  port, or an in-run container the coordinator starts and monitors) instead of
  a shared Docker Desktop container that concurrent agent sessions can kill.

### 2. Deep mode attacks the wrong binding constraint for the chat lane — the repo's own ledgers say the reader/packing, not retrieval depth, is what's binding

- **Failure mode:** $0.30/query capped, 100s+ agentic recall (observed reader
  dispatch 103.788 s; Deep cap 300,000 micros = 125× the settled Fast reader
  cost of 2,421 micros) is being built as the headline lever while the ledgers
  repeatedly identify evidence *utilization* as the bottleneck.
- **Evidence:** STATUS reader-lattice calibration: no-memory 0.067 / MemPhant
  0.584 / **oracle 0.916** — "+0.331 oracle gap makes evidence utilization the
  next binding target" (the ledger's own words). Rung 7: the 8192 pack drops
  2–6 of the top-10 items per question — "a named suspect in the QA gap."
  Runtime-chunks campaign: 19/36 weak-stratum failures were pack drops, 0 were
  judge artifacts. All five agent-memory levers (more/better retrieval) were
  rejected 2026-07-13. LME-V2's 72.5-vs-48.5 transfer claim is also shaky: the
  72.5% system is AgentRunbook-C, a *coding agent over runbooks* — a different
  agent stack, not evidence that MemPhant's snapshot file-agent recovers chat
  Fast-misses.
- **Cheapest early test:** free, offline, today: classify the ~74 Fast-miss
  development questions (178 × miss rate) by whether gold evidence is already
  present in the k=64 recall pool / dropped by packing / genuinely absent —
  the retrieval traces already exist. If a majority are present-but-unused or
  pack-dropped, Deep's extra *search* cannot be the recovery mechanism and the
  paired gate is measuring the wrong lever.
- **Cut/change:** run that classification before authorizing another paid Deep
  dispatch. If absence dominates, proceed; if utilization dominates, redirect
  the T6 budget to packing-ordering/abstention (rung 7, still open) and reader
  prompting — which improve every query, not the explicit 100s path.

### 3. The evidence ceremony has inverted: it now guards $0.03 of settled spend and actively blocked the reliability fix for the exact failure that killed the last root

- **Failure mode:** process integrity outranks progress. The apparatus built to
  prevent fabricated evidence (a real 2026-07-03 failure) has metastasized into
  micro-dollar liability accounting whose upkeep consumes the scarce resource
  (owner sessions) it was meant to protect.
- **Evidence:** Amendment 14: corrected total settled campaign spend =
  **30,771 micros ($0.031)** under a $6.25 ceiling, tracked through 14
  amendments, 4+ dispatch authorizations, sha-sealed reservation/release
  ledgers, and per-row settlement proofs. Same amendment records that a
  database-liveness guard was implemented and then **reverted** because it
  would change the frozen `adapter_sha256` and invalidate construction proofs —
  i.e., the hash-freeze ceremony rejected the fix for the failure that had just
  burned the root, deferring it as a "non-blocking carry item." Contrast: the
  R1 docs gate spent $40–55 under far lighter rules and produced the campaign's
  most decision-relevant table.
- **What actually prevents the 2026-07-03 failure:** promotion-provenance rule
  (packaged runtime + pinned corpora + executed reader/judge), pre-registration,
  paired two-seed CIs, immutable failed roots. None of that requires
  hand-maintained micro-dollar reservation math or an amendment per infra fault.
- **Cheapest early test:** time-audit the last three sessions: minutes spent on
  ledger/amendment upkeep vs minutes on levers. Also: Amendment 14 itself proves
  reservation release is mechanically decidable (`observed_deep_config_hash:
  null` → never charged) — so the human ceremony is redundant with the artifact.
- **Cut/change:** leanest credible replacement — (a) keep the provenance rule,
  pre-registration, and append-only run log; (b) enforce the budget with a
  provider-side spend-limited OpenRouter key instead of hand accounting;
  (c) standing rule: an infrastructure fault with zero bound billable calls
  authorizes a fresh root *without* a new amendment. One protocol doc per
  campaign, amendments only for contract changes.

### 4. The docs "win" is a 14× volume artifact, and cutting Syndai over on deep-recall configs breaks its latency budget

- **Failure mode:** cutover on the only config that beats Syndai would ship
  38.5k chars/question to the reader in surfaces built around a ~1 s retrieval
  budget, multiplying reader cost ~5× and blowing interactive latency; cutover
  at Syndai-comparable volume *loses*.
- **Evidence:** R1 (`2026-07-12-r1-docs-gate.md`): flip only at k=50/8192,
  median ~38.5k chars vs Syndai's 2.8k (~14×); at k=10 comparable volume,
  pooled **−0.100 [−0.175,−0.025]**. R1.5: best comparable-volume arm +0.083
  with CI floor exactly 0.000 — "R6 replacement NOT unlocked: rule held." The
  largest QA lever (server-side cross-encoder, +0.158) is latency-RETIRED at
  12.9–13.6 s/query vs the pre-registered 1.5 s ceiling (9× breach). MemPhant's
  own hot-path SLO (fast p50<200 ms / p95<500 ms) is still an **unchecked**
  standing bar (STATUS §6). Syndai's incumbent measured candidate p95:
  0.877–1.015 s. The corrected-corpus 2026-07-13 rerank admission (R@10
  0.283/0.417) authorizes reader scoring only — hierarchy parity, holdout,
  live restraint, and cross-lane chat non-regression are all still open.
- **Cheapest early test:** one secret-free afternoon: paired end-to-end
  p50/p95 of `POST /v1/recall` (k=50/8192 and the Voyage-top-8 config) vs
  `search_knowledge_detached` on the pinned 4,870-section corpus, on the same
  machine. No paid reader needed — this is the missing SLO proof and the
  go/no-go latency fact for cutover.
- **Cut/change:** drop docs-RAG from the near-term cutover path. First cutover
  target should be the agent file-memory surface (WS-F — already built,
  trace-compared, low-risk, and it retires part of the 21,790-LOC memory
  incumbent), while docs waits for a comparable-volume win.

### 5. "One substrate for three use-cases" is currently one schema plus three divergent retrieval products — and the cross-lane non-regression rule taxes every promotion 3×

- **Failure mode:** the slogan hides that each lane has its own embedder,
  depth, packing, rerank, and binding constraint, so every lane needs its own
  promotion campaign; requiring cross-lane non-regression multiplies eval cost
  while the code lane free-rides on 40 questions of evidence.
- **Evidence:** chat default bge-small/session granularity ("third replication
  of chat-not-embedder-bound"); docs winner modernbert + k=50 or Voyage
  rerank-2.5 top-8; code lane's total corpus: "3 code arms on a 40Q mined
  sample — no API case at sample scale" (R0) plus an inventory doc; the CaaS
  gate requires ≥40 prospective validator-backed tasks that do not exist. Chat
  is reader-bound (oracle gap +0.331), docs is ordering-bound ("ordering, not
  depth, is the docs-lane bottleneck" — R1.5), code is unknown. Rung 8 is
  still held open partly on "cross-lane chat non-regression." Even the Syndai
  inventory concedes the point: "must not force the three lanes into one
  undifferentiated vector index or one global read policy."
- **Cheapest early test:** enumerate lane-conditional defaults in config. If
  embedder, k, budget, granularity, and rerank all fork by lane (they do), the
  shared substrate is storage + verbs + traces + tenancy — which is fine, but
  then per-lane promotions should not owe full paired campaigns to the other
  lanes.
- **Cut/change:** rename the claim internally to "one store, three tuned
  profiles." Scope promotions per-profile with a cheap fixed smoke for the
  other lanes instead of full paired non-regression. Explicitly deprioritize
  the code lane until a CaaS gate corpus exists — it is a roadmap word, not a
  tested lane.

### 6. The SOTA targets are mindshare-mispriced: LME-V2's empty leaderboard has no audience, SWE-ContextBench has zero built machinery, and full-scale runs don't fit the current infrastructure

- **Failure mode:** months of gate-laddering toward headline claims nobody is
  positioned to see, on an execution environment that loses a Docker container
  in 2.5 minutes.
- **Evidence:** verified landscape (2026-07-18): LME-V2 leaderboard EMPTY,
  submission via Google Form — "first-mover slot" is equally "zero watchers";
  its best-known number (72.5%) comes from a coding-agent architecture, and if
  ranking weights latency (as the campaign's own framing fears), a 100s Deep
  mode is structurally penalized. `grep -rl SWE-ContextBench` over the repo
  returns only a plan, a handoff, and a checkpoint JSON — no adapter, no
  harness, no pinned dataset, while beating 30.3% requires a coding-agent
  harness MemPhant has never run. Scale math from today's root: one case bank
  = 670 resources, 177,790,905 bytes, ~40 min construction (ledger 11:49 →
  construction proof 12:31); ~451 LME-V2 questions ≈ 13 serial days of
  construction on a laptop whose containers vanish. Meanwhile the landscape
  memo's own "de facto evidence bar" (uniform configs, standardized judge,
  faithful ingestion, third-party verification) is met at least as well by a
  real production cutover as by a Google-Form submission.
- **Cheapest early test:** one hour of web research: count external citations,
  submissions, or vendor mentions of LME-V2 and SWE-ContextBench since April
  2026. If both are still ~zero-footprint, first-mover value ≈ zero-audience
  value and the ordering (benchmarks before cutover) is backwards.
- **Cut/change:** invert the business goal: make "Syndai runs on MemPhant in
  production" the headline claim and demote benchmark runs to supporting
  evidence. Keep LME-S full-500 as the internal SOTA-language unlock only if
  the n=12→n=100 ladder ever completes; build the SWE adapter only after the
  code lane has a real corpus (see item 5).

### 7. Storage sprawl: half the storage matrix has zero users and zero evidence — explicitly defer it

- **Failure mode:** "pgvector + flat .md files + Supabase Storage + relational
  across substrates" quietly multiplies provider lint, bootstrap profiles, and
  design surface for consumers that do not exist, in a pre-production product
  whose owner priorities say KISS.
- **Evidence:** STATUS database-isolation note: "Accessible Supabase projects
  contain no deployed `memphant` objects and received no writes." The flat-file
  memory surface exists only behind the WS-F dogfood gate that was reopened
  2026-07-09 and is still unchecked. The dormancy ledger already lists
  TypeScript SDK, SQLite, CRDT, Helm, cache cluster, multi-region as DORMANT.
  Three bootstrap profiles (plain PG / Supabase / Neon) are maintained and
  linted with no external consumer of the latter two.
- **Cheapest early test:** grep for any non-test consumer of the Supabase
  Storage or flat-file write paths — if none, deferral is free.
- **Cut/change:** record in the decision register: v1 storage = one Postgres
  (pgvector + relational). Supabase Storage and flat-file stores DEFERRED
  until a named consumer exists. Keep the provider lint only if it stays
  zero-maintenance.

### 8. Roadmap ordering buries the everyday-UX lever (T1) behind the flakiest benchmark lever (T6)

- **Failure mode:** the bound execution order (T6 n=12 → T6 n≈100-300 → T1 →
  SWE Lite → full-500 → LME-V2) serializes everything behind the gate that has
  now consumed ~5 days, 22 artifact roots, and 14 amendments for one row of
  signal — while the chat lane has promoted zero levers since rung 4 and the
  owner's stated priority includes best UX.
- **Evidence:** NEXT-SESSION-PROMPT "Bound execution order" (T1 explicitly
  gated on T6 n=12 passing plus a fresh amendment); T6 failure history in item
  1; STATUS: levers 1–5 all rejected, rungs 5/7/9/10/11/13/15 all open; T1 is
  described in the handoff itself as "improves everyday chat UX."
- **Cheapest early test:** T1's n=12 prep is secret-free and touches none of
  T6's frozen hashes — prepare and dry-run it during T6's rehearsal downtime;
  if it conflicts with nothing (it shouldn't), the serialization was never
  necessary.
- **Cut/change:** unbind T1 from T6. Run T1 preparation in parallel this week.

---

## The single biggest cut this week (question 7)

**Stop the paid T6 live campaign in its current form — both the shared-Docker
execution environment and the amendment-per-failure ceremony — until two free
things complete:**

1. the Fast-miss pool classification (item 2, ~zero cost, uses existing
   traces) that determines whether Deep is even aimed at the binding
   constraint; and
2. one full unpaid n=12×24-row stub rehearsal on a run-owned Postgres (item 1)
   that proves the environment can survive its own gate.

Freed capacity goes to: the packing-ordering/abstention lever (rung 7 — the
ledger's own named suspect, benefits every lane and every query), the
secret-free recall-latency SLO proof (item 4 — the missing go/no-go fact for
any cutover), and T1 prep (item 8). This costs zero evidence integrity: every
existing proof, root, and rule survives; only the ceremony-per-retry and the
coin-flip infrastructure are removed.

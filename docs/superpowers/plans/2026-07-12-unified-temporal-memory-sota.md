# Unified Temporal Memory SOTA Campaign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox syntax for tracking.

**Goal:** Build and prove one temporally correct MemPhant substrate that is frontier-quality for personal agents, document/knowledge RAG, and codebase experience; then replace each Syndai memory or knowledge surface only after a paired gate proves MemPhant is better.

**Architecture:** Keep one Postgres evidence ledger, one MemoryService, and the existing public verbs. Share identity, scope, provenance, correction, forgetting, validity time, transaction time, traces, and outcome feedback across all lanes. Specialize query transformation, retrieval, reranking, packing, and answer composition by lane because conversational recall, knowledge retrieval, and procedural execution do not have the same optimal policy.

**Tech Stack:** Rust workspace, Postgres/pgvector in the memphant schema, SQLx, fastembed 5.17.x, optional winner-only hosted reranker, Python benchmark harnesses, public REST/MCP/CLI/Python SDK, and Syndai backend adapters.

## Global Constraints

- Priority for this campaign: **accuracy/correctness first, UX second, cost third, then non-user-facing throughput**. User-visible latency is UX, so the 1.5-second synchronous recall ceiling remains binding. Offline compilation throughput is measured but cannot veto an accuracy winner.
- Cost is measured but is not a promotion blocker until the SOTA system is frozen. Cost compression is a later non-inferiority exercise.
- Security is not traded away: server-derived tenancy, correction correctness, deletion, provenance, and scope isolation are correctness floors.
- Pre-production: no backwards compatibility. Delete losing flags, models, scripts, and dead paths after adjudication.
- Current code outranks this plan; this plan outranks older campaign reports and handoffs.
- STATUS.md remains the only state ledger. Checkboxes in this file are acceptance
  requirements, not a second progress tracker; build logs own evidence.
- This plan supersedes the active R2-R6 ordering in the 2026-07-11 campaign report and the 2026-07-12 handoff. Historical proof remains historical.
- No worktrees. Preserve unrelated dirty work and use explicit-path commits.
- No new graph database, vector database, profile database, cache cluster, generic provider framework, or direct web/mobile MemPhant client.
- A unified substrate does **not** mean one universal ranker. It means one truth/governance model with measured lane-specific read policies.
- Verbatim source material remains canonical. Derived profile, summary, procedure, and file views are cited projections.
- Synthetic fixtures gate regressions only. Promotions require packaged Postgres runtime, immutable corpora, complete executed scorers, and archived proof.
- A benchmark failure consumes that holdout. Do not inspect its labels and call the next run held out; obtain a new sealed set.

## Why One Benchmark Is Not Enough

LongMemEval-S measures five conversational-memory abilities, but it does not prove:

- months-long mutation and forgetting;
- implicit invalidation and stale-premise resistance;
- appropriate restraint when memory is irrelevant;
- document/knowledge replacement quality in Syndai;
- repository exploration or validator-backed coding success;
- multi-agent scope isolation;
- product latency at the winning quality point.

The campaign therefore uses a portfolio. A feature may be promoted for one lane without becoming a global default. A shared-core change promotes only when it preserves every lane's contract.

Research supports this separation:

- EvoMemBench finds no single memory form wins across knowledge and execution tasks: https://arxiv.org/abs/2605.18421
- LongMemEval-V2 evaluates compact evidence from long agent trajectories, including workflows and gotchas: https://github.com/xiaowu0162/LongMemEval-V2
- Memora/FAMA measures weeks-to-months mutation and penalizes obsolete-memory use: https://arxiv.org/abs/2604.20006
- STATE-Bench separates state capture, state maintenance, and state use in agent memory: https://opensource.microsoft.com/blog/2026/05/19/introducing-state-bench-a-benchmark-for-ai-agent-memory/
- SWE-Explore tests repository exploration under an explicit context budget: https://arxiv.org/abs/2606.07297
- Repository Memory improves localization by using commits, linked issues, and evolving-area summaries: https://www.microsoft.com/en-us/research/publication/improving-code-localization-with-repository-memory/
- Experience-following research shows bad or misaligned memories propagate errors, so task outcomes must grade memory: https://aclanthology.org/2026.acl-long.27/
- July 2026 evaluator research shows judge upgrades can move scores without changing candidate answers, so judge versions and audit logs are part of the measurement: https://arxiv.org/abs/2607.08535
- July 2026 LongEval-RAG evidence favors deterministic passage units plus late sentence selection and multi-metric evaluation over more elaborate chunking: https://arxiv.org/abs/2607.04008
- MemSyco-Bench directly tests stale-preference selection, scope control, memory/evidence conflict, objective facts, and valid personalization under an MIT-licensed public harness: https://arxiv.org/abs/2607.01071
- Re2Bench supplies a public temporal-conflict document gate before any learned recency policy is considered: https://aclanthology.org/2026.acl-long.1180/
- Mnemis reports the strongest identified published LongMemEval-S result (91.6)
  through hierarchical global selection: https://aclanthology.org/2026.acl-long.1096/
  Its full-500 LLM-judge protocol is not directly comparable to MemPhant's
  repaired 319 answer-only confirmation contract, so it must be rerun on the
  same IDs/protocol or reported separately rather than treated as a threshold.
- HiGMem supports selective hierarchical reading: retrieve an event anchor,
  then only its useful verbatim turns, rather than unconditional sibling
  expansion: https://aclanthology.org/2026.findings-acl.1690/

## SOTA Claim Contract

MemPhant may use the word SOTA only with a named scope. There is no honest universal SOTA score.

### Agent/personal-memory claim

Required:

- the 319-question cleaned LongMemEval-S confirmation set beats an independently rerun official baseline on the same IDs, using both the pinned official evaluator and the stricter internal evaluator;
- Memora/FAMA improves current-state accuracy without obsolete-memory reuse;
- STALE improves state resolution, premise resistance, and policy adaptation;
- restraint stays within the existing ceiling on a sealed internal suite and a pinned MemSyco-Bench run; OP-Bench and PS-Bench become additional named gates only when legally runnable official releases exist;
- a fixed-reader substrate score and a high-end product-reader score are both reported.
- the pinned Mnemis implementation is independently rerun on the same repaired
  319 protocol, or the claim is explicitly scoped away from published
  full-500 LongMemEval-S SOTA and reports Mnemis 91.6 alongside it.

### RAG/knowledge claim

Required:

- MemPhant beats the current Syndai RAG/KB stack on both existing private development sets and a new version-disjoint sealed private holdout with the same reader, judge, model, prompt, and corpus revision;
- the paired QA confidence interval lower bound is above zero on each development set, on the sealed holdout, and pooled;
- answer-bearing retrieval and citation support do not regress;
- packaged recall p95 is at most 1.5 seconds at the promoted point;
- a public zero-shot retrieval/answer set confirms that the change was not fitted only to Syndai;
- pinned Re2Bench confirms stale/conflicting document handling before any learned recency policy ships.

### Codebase-memory claim

Required:

- MemPhant beats no-memory and deterministic repo-file/search controls;
- it improves context-efficient localization on a pinned official SWE-Explore release once the release includes problem statements, base commits, and task mappings; until then, no official SWE-Explore outcome claim is permitted;
- it improves validator-backed task success on a time/repository-separated task-continuity suite;
- failed attempts reduce repeat failures rather than poison future executions;
- the same base coding agent, model, tools, and task environment are used across arms.

### Unified temporal-memory campaign exit

All three lane claims pass, plus:

- corrections stick;
- deleted or invalidated evidence does not resurrect;
- current and as-of queries resolve the correct state;
- every derived item has source provenance;
- cross-tenant and cross-scope isolation pass through the real non-owner database role;
- controlled, labeled Syndai dogfood replay shows no surface-level regression before cutover. Pre-production has no production traffic, so the campaign must not claim a production shadow.

## Benchmark Portfolio

| Lane | Development evidence | Sealed or external gate | Primary metric |
|---|---|---|---|
| Personal agent | 178 historically exposed cleaned LongMemEval-S question IDs, internal correction/preference goldens | 319 question-held-out and answer-bearing-session-disjoint cleaned LongMemEval-S questions; 3 linked questions are excluded | answer-only paired QA |
| Longitudinal temporal | internal mutation trajectories | Memora/FAMA and STALE | stale-penalized current-state accuracy |
| Restraint | targeted negative-personalization fixtures | sealed internal restraint suite plus pinned MemSyco-Bench; OP-Bench/PS-Bench only after complete licensed releases | irrelevance, repetition, sycophancy, harmful amplification |
| General agent experience | internal trajectory fixtures | STATE-Bench and LongMemEval-V2; EvoMemBench only after a licensed native harness exists | task success, accuracy, latency |
| RAG/KB | both existing Syndai docs golden sets, treated as exposed development evidence | independently curated version/content-disjoint private holdout, pinned LongEval-RAG evidence-selection confirmation, and pinned Re2Bench temporal-conflict confirmation | supported answer accuracy |
| Codebase | privacy-locked historical attempts split by time/repository | at least 40 prospective validator-backed tasks across at least two repositories; SWE-Explore only after a complete official release | localization coverage and task success |
| Governance | store/runtime contracts | GateMem after first honest reproduction | simultaneous utility, access control, forgetting |
| Scale/operations | checkpointed local soak | MemoryStress and BEAM as secondary stress tests | degradation curve, p95, storage growth |

MemoryStress is an engineering soak, not FAMA and not the primary longitudinal scientific claim.

## Model and Budget Policy

The campaign deliberately buys answer quality before optimizing price.

### Primary product lattice

- Current development reader: `openai/gpt-5.6-luna-pro`. The final contract
  screen admitted both Luna Pro and `google/gemini-3.5-flash`, but the one
  approved Flash Memora rung scored FAMA 19.54 versus the earlier, non-paired
  Luna pilot's 32.96. Flash is therefore rejected as the accuracy product tier;
  contract reliability alone is not answer quality.
- Terra Pro is a conditional accuracy challenger, not the next automatic run.
  Admit it only if Luna still misses questions whose corrected frozen evidence
  contains the answer, and compare both readers against that identical evidence
  bank. Grok and MiniMax have no local memory-quality evidence that justifies
  adding them now. Muse Spark would add a new provider surface and is deferred.
  Do not launch another full model run without explicit approval.
- Paid candidates run through the existing OpenRouter account with auto top-up.
  An Azure attribution means OpenRouter selected Azure as Luna's upstream; it
  is not free Azure credit and remains an OpenRouter-billed request.
- The rejected Flash arm was pinned to OpenRouter's Google AI Studio provider
  family for its paid smoke. Luna remains on OpenRouter's normal compatible-provider routing:
  the 2026-07-13 endpoint snapshot showed the same list price for OpenAI and
  Azure, Azure had higher 30-minute uptime, and no paired quality evidence
  justified sacrificing fallback coverage by pinning one upstream.
- Judge: the pinned official benchmark judge and canonical prompt. Reader output
  and judge output use independent, provider-enforced structured schemas.
- All candidate and baseline arms use the same model IDs, effort, prompt, and evidence budget.

Historical reader screens do not select the unified-memory model. They measured
answer composition on one exposed LongMemEval slice, not temporally ordered
structured extraction, state maintenance, FAMA accuracy, or provider
reliability. In particular, the earlier 100/178 Luna and 97/178 Flash result is
diagnostic only because Flash had 19 parse failures and neither arm passed the
current attempt-ledger, oracle, and provenance contracts. The final five-case
screen has now served its narrow admission purpose. It does not justify more
model calls. The next paid reader comparison must follow a corrected retrieval
mechanism and reuse one frozen extraction bank; the current Memora runner binds
the extractor and reader to the same model and auto-drops the database, so
another run would confound extraction, retrieval, and answer composition while
repaying all 163 extractions. End-to-end or 163-session runs require the
separate approval and promotion gates below.

### Model-selection rule

- Re-run the repaired reader calibration on development evidence.
- Freeze extractor model, compiler/schema identity, retained memory bank,
  retrieval configuration, evidence pack, prompt, and judge before comparing
  answer models. Reader-model selection must never silently change extraction.
- Validate Luna first on only the five corrected Memora reasoning questions.
  Add Terra only if Luna fails with answer-bearing evidence. A failed or
  unprovable arm receives no accuracy score; Flash is no longer a candidate.
- Model ability, route reliability, and cost completeness are separate axes.
  Recovered bounded transport/no-content retries preserve accuracy eligibility,
  but are reported; any unpriced attempt makes cost a lower bound and prevents a
  claim that the arm is cheaper.
- Record answer latency, input/output tokens, failures, and cost, but do not reject an accuracy winner for cost.
- Among statistically tied candidates that satisfy the UX latency ceiling, prefer lower total cost, then lower end-to-end p95.
- A model substitution creates a new lattice. Re-run both baseline and candidate; never compare across model changes.
- After the full campaign passes, test cheaper readers/rerankers against the frozen SOTA outputs with a non-inferiority margin. Until then, do not optimize model spend.

## Accuracy Improvement Loop

Every accuracy iteration follows the same closed loop:

1. Run the current frozen baseline on development evidence.
2. Classify every failure as retrieval miss, pack displacement, temporal-state error, reader/composition error, unsupported answer, stale-memory use, or judge ambiguity.
3. Choose the single highest-volume causal class.
4. Implement one lever that directly targets that class.
5. Run targeted regression and mechanism metrics before expensive QA.
6. Run paired QA with the high-end reader only if the mechanism moved.
7. Promote a winner, delete a loser, and update the trace schema only when needed for the next decision.
8. Combine only individually positive levers.
9. Attempt a sealed gate once, after all development and independent gates pass.

The 319 LongMemEval confirmation attempt is terminal for that release: the 178
development IDs, 319 confirmation IDs, and 3 exclusions exhaust all 500 rows.
If the 319 gate fails, no second tuning attempt may reuse it; obtain a genuinely
new independent holdout before another SOTA claim.

Do not build factorial matrices, retain losing feature flags, or tune against sealed question text.

## Authoritative Answers to the Open Calls

1. **Can the old LongMemEval-S or Syndai docs result choose the architecture?**
   No. Each is useful for causal diagnosis inside its lane, but neither covers
   mutable state, restraint, code-task outcomes, isolation, or production-shaped
   document hierarchy. Architectural promotion requires the portfolio and claim
   contracts above. A development benchmark may select the next experiment; it
   may not authorize a SOTA claim or a product replacement by itself.
2. **Should we spend more on the answer model now?** Yes when it buys measured
   accuracy, but not before the substrate supplies the answer. Keep Luna as the
   development reader, validate it on the five corrected reasoning packs, and
   spend on Terra only if those packs expose a composition failure. Flash is
   rejected; Grok, MiniMax, and Muse do not enter the lattice without a named
   residual failure or credible memory-specific evidence. Cost remains a
   tie-breaker until the SOTA quality point is frozen. Sol remains out of the
   current spend lattice by explicit user call.
3. **Is the current agent-memory bottleneck just retrieval?** No. The oracle gap
   and error decomposition show both evidence utilization and wrong-chunk
   packing. Evidence notes, temporal scoring/rendering, sibling expansion, and
   structured query rewrite all failed their predeclared development gates and
   remain default-off diagnostics where older benchmark contracts still expose
   them. There is no frozen agent candidate yet; the next lever must be
   chosen from a new causal failure decomposition rather than another prompt
   permutation.
4. **Is the current RAG reranker the root problem?** The local reranker is one
   root constraint: 128-token input truncation explains 29/47 observed
   answer-bearing candidate collapses, while longer local inputs breach the
   latency ceiling. Balanced vector/lexical admission plus Voyage rerank-2.5
   over the top eight is the only development arm that improved both exposed
   sets and met p95 (R@10 0.283/0.417 at 1.053/1.027 seconds). It is the frozen
   Task-3 candidate, not a replacement verdict.
5. **Should MemPhant replace Syndai/CaaS now?** No. Replacement remains blocked
   until MemPhant beats the incumbent on both exposed sets, a sealed
   version-disjoint holdout, and a public zero-shot set with identical reader,
   corpus, packer, and budget, while passing citation, correction, forgetting,
   restraint, isolation, and p95 gates. Run controlled labeled dogfood replay
   first; delete the incumbent only
   after the same adapter-boundary observation gate is clean.
6. **Does one unified system mean one global retrieval configuration?** No. One
   evidence ledger, identity model, temporal semantics, outcome loop, traces,
   and public service are canonical. Agent, document, and code lanes keep
   measured read policies because forcing one ranker across them would reduce
   accuracy.
7. **Should CaaS keep its own RAG engine?** No. Hosted code may own routing,
   billing, quotas, and operations only. Retrieval and temporal semantics remain
   in the public MemoryService; any CaaS-only engine is replayed under the same
   controlled labeled dogfood gate and then deleted
   after the same replacement gate.
8. **Can unavailable public benchmarks be replaced with local approximations?**
   No. As of 2026-07-13, OP-Bench has no runnable public scorer, PS-Bench lacks
   a usable license, SWE-Explore lacks the problem/base-commit/task mappings its
   evaluator requires, and EvoMemBench lacks a root license/native harness.
   Internal suites may drive engineering but must be labeled internal. Public
   claim scope expands only when complete legal official releases are pinned.
9. **Should we add a proactive inject/silence memory agent now?** No. Current
   abstention handles empty or contradictory packs and marks affect later
   ranking; neither supplies next-action counterfactuals. Add no selector,
   threshold, or prompt framework until a live action benchmark can compare
   baseline, always-inject, and selective-inject on identical prefixes with
   verifier-backed outcomes.
10. **What is the independent LongMemEval baseline?** Pin official LongMemEval
    commit `9e0b455f4ef0e2ab8f2e582289761153549043fc` and rerun the
    GPT-4o-2024-08-06 full-history/session JSON plus single-pass Chain-of-Note
    lane on the exact redacted 319 IDs with `topk_context=100`. Remove answer,
    answer-session, and `has_answer` fields from generation input. Score both
    baseline and MemPhant hypotheses through the official evaluator; report the
    stricter internal paired score separately. The published 0.606 full-500
   result is context, not the 319 threshold, and the 0.924 oracle is ineligible.
   The published Mnemis 0.916 full-500 result is the strongest identified
   external comparator, but it is not numerically interchangeable with the
   repaired 319 answer-only gate; rerun its pinned code on the same protocol or
   scope the claim explicitly.

---

### Task 0: Repair Evaluation Integrity

**Files:**

- Modify: scripts/run_reader.py
- Modify: scripts/gate_compare.py
- Modify: scripts/fetch_longmemeval.py
- Modify: benchmarks/manifests/longmemeval_s.lock.json
- Test: tests/test_run_reader_contract.py
- Create: tests/test_gate_compare.py

**Produces:**

- structured reader output with notes, answer, and abstain;
- answer-only grading;
- canonical task-specific LongMemEval judge prompts;
- exact abstention;
- fail-closed complete pairing;
- immutable dataset revision and split manifests.

- [x] Add failing regressions for gold in notes with wrong final answer, negated gold, mismatched final number, non-exact abstention, reader failure, judge failure, unequal paired IDs, and mismatched runtime/model hashes.
- [x] Run the focused tests and prove every new regression fails.
- [x] Parse structured reader output and submit only the answer field to containment/judging.
- [x] Count parse/runtime/judge failures as incorrect and fail promotion when any paired row is missing.
- [x] Pin the Hugging Face repository revision as well as file hashes.
- [x] Replace the obsolete pre-cleaned corpus with the official `longmemeval-cleaned` release and rerun the exposure audit.
- [x] Generate and lock the 178 exposed development IDs and 319 question-held-out plus answer-bearing-session-disjoint confirmation IDs without printing confirmation question contents. Record that all 500 questions share filler haystack sessions, so literal all-haystack-session disjointness is impossible rather than claiming it.
- [x] Run the official LongMemEval evaluator for leaderboard comparability and the repaired answer-only evaluator for promotion integrity; never substitute one for the other.
- [x] Re-score prior reader artifacts answer-only and label old published numbers historical where they change.
- [x] Run focused tests, then the Python suite.
- [x] Archive a build log and update STATUS only if a proof-bearing checkbox changes.

**Exit:** No partial or unequal run can produce a promotion result, and the historical false-positive examples score incorrect.

### Task 1: Calibrate the High-Accuracy Reader and Freeze Claim Baselines

**Files:**

- Modify: scripts/run_reader.py only if the existing model/effort plumbing is insufficient
- Create: benchmarks/manifests/reader_lattices.v1.json
- Create: docs/build-log/2026-07-13-reader-lattice-calibration.md
- Test: tests/test_run_reader_contract.py

- [x] Run the current medium-effort lattice and both high-accuracy lattices on exposed development questions only.
- [x] Run no-memory, MemPhant evidence, and oracle/full-context controls.
- [x] Select the reader by answer-only accuracy, then latency; ignore cost for selection.
- [x] Freeze reader, judge, effort, prompt hashes, evidence budget, and decoding settings.
- [x] Record the oracle gap so later work knows whether retrieval/packing or answer composition is binding.

**Exit:** One primary and one cross-family lattice are frozen. Feature comparisons never mix lattices.

### Task 2: Win Agent and Personal Memory on Development Evidence

**Files:**

- Modify: scripts/run_reader.py
- Modify: crates/memphant-core/src/lib.rs
- Modify: crates/memphant-core/src/service.rs
- Modify: crates/memphant-types/src/lib.rs
- Modify: crates/memphant-store-postgres/src/store.rs only for existing scope_block runtime access
- Test: tests/test_run_reader_contract.py
- Test: crates/memphant-core/tests/recall_trace_golden.rs
- Test: crates/memphant-store-postgres/tests/pg_store_contract.rs

**Ordered levers:**

1. same-session sibling expansion for the 26 observed right-session/wrong-chunk misses — rejected (−1/178, abstention regression);
2. relevance-gated active preference projection — rejected before reader QA (R@5 −0.0964 with CI excluding zero; R@10 −0.0361; preference R@10 −0.0909) and deleted;
3. implement pinned scope blocks only against a named user-authored constraint/restraint contract; the current LongMemEval corpus cannot validate this surface, so do not bundle it into another retrieval experiment;
4. bitemporal current-state resolution before recency scoring;
5. standalone conversational query rewrite — rejected before reader QA (proxy R@10 +7 but literal parent coverage −2 and one abstention regression);
6. HyDE only as a retrieval diagnostic — not run because the rewrite branch failed its mechanism gate;
7. cross-reranking only if the winning RAG reranker also improves chat retrieval.
8. selective anchor-to-child expansion: retrieve the episode/fact anchor, score
   only its existing verbatim child chunks with the query-aware reranker, and
   admit selected spans under the same total budget. This is distinct from the
   rejected unconditional sibling expansion and runs only against diagnosed
   right-session/wrong-chunk misses.

The global evidence-note prompt, broad temporal score boost, deterministic
date-relation rendering, sibling expansion, structured query rewrite, and
active-profile filtering have
failed their development promotion rules. They remain rejected evidence and
cannot become runtime defaults. Existing default-off benchmark routes are
diagnostic only. No agent-memory candidate is frozen.

- [x] Implement and test one lever at a time. Lever 4's independent bitemporal
  runtime is covered by focused core, PostgreSQL, REST, MCP, migration, and
  generated-artifact checks (`docs/build-log/2026-07-13-bitemporal-runtime.md`);
  no benchmark promotion is implied.
- [ ] Keep profile statements multi-source cited; never create an uncited synthetic memory row.
- [ ] Charge profile content to the existing total context budget.
- [ ] Require correction/preference improvement, overall non-inferiority, and restraint non-regression.
Decision rules: temporal boost and HyDE stay deleted unless a new causal trace
justifies a fresh experiment; run a combo only when at least two positive single
levers exist.

**Exit:** One agent-memory candidate is frozen without inspecting the 319 confirmation questions.

### Task 3: Win RAG/Knowledge Retrieval Before Syndai Replacement

**Files:**

- Modify: crates/memphant-core/src/lib.rs
- Modify: crates/memphant-runtime/src/embeddings.rs
- Modify: crates/memphant-runtime/src/lib.rs
- Modify: crates/memphant-types/src/lib.rs for reranker provenance only if current trace fields are insufficient
- Modify: scripts/gate_run_memphant.py
- Modify: scripts/gate_compare.py
- Create: benchmarks/manifests/syndai_docs_holdout.v1.json
- Create: benchmarks/manifests/longeval_rag.lock.json
- Test: tests/test_gate_r15_cross_rerank.py
- Create: tests/test_syndai_docs_gate.py
- Test: crates/memphant-runtime/src/embeddings.rs
- Test: crates/memphant-core/tests/recall_trace_golden.rs
- Test: crates/memphant-core/tests/recall_pool_depth.rs

**Accuracy-first retrieval decision:**

- The common full corpus is 4,870 sections. Any partial-corpus result is a
  diagnostic only.
- Local BGE base at top 32/max length 128 materially improves Recall@10 over
  the same small-embedder baseline, but both the base and reranked exhaustive
  paths miss the 1.5-second p95 ceiling. Do not spend reader calls on them.
- Fixed 32+32 admission did not materially improve recall and failed latency;
  it has been deleted. The runtime now uses balanced admission sized to the
  actual reranker head, with overlap-aware backfill.
- Evaluate production-style `fast` mode with release binaries. Report POST-only
  latency separately from trace-proof readback latency; neither debug-binary
  nor exhaustive-mode timings are product latency claims.
- Hosted Voyage rerank-2.5 top-8 is the frozen development candidate. It
  reached R@5/R@10 0.267/0.283 and 0.317/0.417 with 1.053/1.027-second
  end-to-end p95 and zero failures/fallbacks. Cohere's public terms do not
  permit this competitive benchmark; do not call it.
- The raw no-rerank and Voyage screens have different source identities. Their
  directional gap chooses the next candidate, but is not a paired promotion.
  Re-run the no-rerank baseline on the same final binary before reader scoring.
- The same-final-binary rerun is now complete. With identical corpus, binaries,
  source identity, evidence budget, and recall mode, no-rerank scored R@10
  0.050/0.100 and Voyage top-8 scored 0.283/0.417. Cluster-aware paired deltas
  were +0.233 (95% CI +0.133 to +0.333) and +0.317 (+0.204 to +0.435);
  candidate p95 was 1.015/0.877 seconds with zero
  failure/fallback/degraded rows. Voyage therefore passes the retrieval
  mechanism gate and earns supported-answer reader scoring; it still is not a
  Syndai replacement verdict.

FastEmbed applies pair-level tokenizer truncation. At max length 128, 29/47
observed oracle-to-top-10 collapses lost the answer span; max 256/512 preserve
more spans but cannot meet the local CPU latency ceiling. Do not add manual
slicing without a new causal trace.

- [x] Add exact reranker provenance: model, provider, candidate count, max length, batch size, input-length percentiles, latency, and failure state.
- [x] Prevent the worker from constructing a reranker.
- [x] Regenerate and lock one common corpus manifest, then make both arms index every pinned section. Mining eligibility may select questions but may never filter either retrieval haystack.
- [x] Freeze the actual incumbent production query/rerank configuration. Any fallback, skipped file, degraded search, or ingest error makes the replacement gate invalid rather than silently changing the baseline.
- [x] Apply one shared deterministic evidence packer and token counter after both engines return ranked items; enforce the same maximum evidence budget and record actual packed tokens and truncation.
- [x] Verify answer-bearing candidate coverage before reducing depth.
- [x] Add a hash-locked negative/abstention slice; the 60-question sets remain positive-only semantic stress tests, and candidate restraint still requires a complete paid reader run. Proof: `docs/build-log/2026-07-13-rag-retrieval-admission.md`.
- [ ] Preserve production document hierarchy in the incumbent arm; per-leaf KnowledgeSource ingestion is diagnostic and cannot authorize replacement.
- [x] Run latency/retrieval screening before reader calls.
- [x] Re-run no-rerank and Voyage top-8 from the same final binaries and source identity; validate every non-reranker runtime field as invariant.
- [x] Add the docs-only `rag-supported-v1` judge: strict answer correctness plus full evidence support, cited evidence ranks, raw output, fail-closed parsing, no fallback, and swapped A/B adjudication for correctness flips.
- [ ] Run the paired no-rerank and Voyage arms on both exposed Syndai docs development sets with the valid Flash/Luna winner and the pinned judge.
- [ ] Freeze a new version-disjoint private corpus and question set before reading its questions; store only IDs, corpus revision, and hashes in the public manifest.
- [ ] Have an independent curator freeze a new version-disjoint private holdout from a future corpus revision. Do not self-author and then call it blind.
- [ ] Run baseline and exactly one candidate once on that sealed private holdout, then run the same candidate zero-shot on pinned LongEval-RAG.
- [ ] Run pinned Re2Bench as a temporal-conflict confirmation before enabling any learned recency policy; do not treat its authors' Re3 result as independently reproduced.
- [x] If a hosted arm wins, implement only that provider behind the existing CrossReranker seam using the workspace ureq client and the existing blocking boundary. Do not add an HTTP dependency or create a generic provider framework.
Decision rules: LongEval-RAG remains a narrow 47-query evidence-selection
confirmation; choose the highest-QA arm inside the 1.5-second p95 ceiling. If no
synchronous arm passes, keep the deep-recall quality point. Query-aware
excerpting requires a causal retained-token trace first.

**Exit:** MemPhant beats Syndai on both exposed development sets and the sealed holdout over the identical full corpus and evidence budget, confirms direction on LongEval-RAG, has zero degraded/fallback rows, and meets the UX p95 ceiling; otherwise replacement remains blocked.

### Task 4: Prove Temporal State, Forgetting, and Restraint

**Files:**

- Modify: crates/memphant-core/src/lib.rs
- Modify: crates/memphant-core/src/service.rs
- Modify: crates/memphant-eval/src/lib.rs
- Create: scripts/run_memora_fama.py
- Create: scripts/run_stale.py
- Create: scripts/run_restraint_bench.py
- Create: benchmarks/manifests/memora.lock.json
- Create: benchmarks/manifests/stale.lock.json
- Create: benchmarks/manifests/op_bench.lock.json
- Create: benchmarks/manifests/ps_bench.lock.json
- Create: tests/test_temporal_benchmark_contract.py
- Test: crates/memphant-core/tests/store_contract.rs
- Test: crates/memphant-store-postgres/tests/pg_store_contract.rs

- [x] Run a minimum viable identical oracle screen first: only the food, steps,
  goal, deletion, and long-context sessions needed to exercise the contracts,
  with Flash and Luna Pro. Require deterministic sampling, exact projection,
  zero terminal semantic failures, and complete cost accounting. Add Terra Pro
  only if those two arms fail or materially disagree. A quick screen selects a
  candidate; it cannot support a SOTA or replacement claim.
- [x] Run the 163-session `weekly/software_engineer` reasoning slice only after
  explicit approval; do not infer permission from the quick-screen result.
- [x] Run the five corrected official reasoning packs through Luna as a
  reader-only composition gate. Luna returned every exact total and both goal
  statuses in five priced calls for $0.034025. This proves answer composition
  from correct evidence, so Terra/Grok/MiniMax/Muse remain out of the lattice;
  it does not prove extraction or end-to-end retrieval.
- [x] Separate extractor-bank construction from retrieval and reader replay.
  Persist an ignored, content-addressed Postgres bank snapshot plus extractor
  ledger/compiler identity, restore it into a fresh scratch database for each
  retrieval arm using a matching PostgreSQL archive-tool major, and prove bank
  hashes match. Do not compare answer models while
  `--model` changes both extractor and reader. The 163-session Luna bank and
  ledger are frozen, and the final 15-question zero-cost control restored it
  into two distinct scratch databases with matching identities and no provider
  calls (`docs/build-log/2026-07-14-extraction-bank-boundary.md`).
- [x] Re-run only the five failed reasoning questions with Luna against the
  corrected frozen bank. Add Terra against the exact same evidence only if Luna
  still fails. Re-run the complete `weekly/software_engineer` split (15 questions/
  71 subquestions) only after that mechanism rung improves. Expand to the full
  600-question/6,415-subquestion Memora release only after that independent rung
  improves the official FAMA score. Do not pay for multiple models at every rung.
  Luna answered the corrected five 5/5 and scored reasoning FAMA 100, so Terra
  was not run. The complete split then improved official FAMA 32.96 to 53.49;
  raw accuracy remained 43/71 and stale remembering/recommending stayed open,
  so the 600-question expansion remains deferred.
- [x] Report extraction correctness, provider reliability/retries, reader
  correctness, stale penalties, latency, and cost as separate axes. Never turn
  a gateway failure into a model-quality score or call a lower-bound cost cheap.
- [ ] Run STALE for state resolution, stale-premise resistance, and implicit policy adaptation against the official pinned 400-scenario/1,200-query Hugging Face release (`STALEproj/STALE`, CC BY 4.0), never a regenerated substitute.
- [ ] Run the sealed internal restraint suite and pinned MemSyco-Bench. Run OP-Bench/PS-Bench only when complete licensed official releases are available; otherwise record them as external release blockers, not failed MemPhant gates.
- [ ] Compare current chain-head/supersedence behavior against deterministic demotion.
- [ ] Report bank-maintenance correctness, retrieval state/role correctness, and answer-time resolution separately so temporal failures remain causal rather than one aggregate score.
- [ ] Keep originals immutable and require every derived node to cite sources.

Decision rule: add consolidation only for a named residual failure, and reject
any feature that trades static QA for stale use or over-personalization.

**Exit:** The agent candidate passes static, mutable, and restraint gates. MemoryStress remains a later soak.

### Task 5: Build and Prove Codebase Experience Memory

**Files:**

- Modify: scripts/code_lane_extract.py
- Modify: scripts/code_lane_mine.py
- Modify: scripts/code_lane_run_memphant.py
- Modify: crates/memphant-core/src/service.rs only when an existing retain/mark seam is insufficient
- Reuse: existing mark outcomes and review_event storage
- Create: scripts/run_swe_explore.py
- Create: benchmarks/manifests/swe_explore.lock.json
- Create: benchmarks/manifests/code_lane_split.v1.json
- Test: tests/test_code_lane_extract.py
- Test: tests/test_code_lane_gate_contract.py
- Test: tests/test_code_lane_run_memphant.py

- [ ] Mine commits, linked issues, successful attempts, failed attempts, corrections, and environmental gotchas.
- [ ] Keep raw evidence canonical; do not generate a free-form repository wiki as truth.
- [ ] Split by whole attempt, repository, and time so related events cannot cross the gate.
- [ ] Compare index off, deterministic grep/file search, verbatim MemPhant recall, and MemPhant plus mark outcomes with three seeds and identical agents/models/tools.
- [ ] Add one repo-scoped, fixed-budget flat-file memory control over the identical training history; keep it a control rather than a second source of truth.
- [ ] Run SWE-Explore localization with a fixed line budget only after the official release includes problem statements, base commits, and mappings. The current 848-row public file is not executable and the adapter must continue to fail closed.
- [ ] Run at least 40 prospective validator-backed tasks across at least two repositories, frozen at base commits, and report localization plus downstream resolve rate.
- [ ] Keep structural repository evidence separate from historical experience; evaluate experience chronologically on future changes.
- [ ] Promote procedural recall only if task success rises and repeated known failures fall.
- [ ] Keep failure memories typed as failures; never compile them into successful recipes.
- [ ] Reuse success, failure, corrected, and ignored. Do not add another outcome API.

**Exit:** MemPhant improves public localization and real task continuity, not merely retrieval QA.

### Task 6: Prove General Agent Experience Without Collapsing Lane Policies

**Files:**

- Modify: crates/memphant-eval/src/lib.rs only for a thin backend adapter
- Create: scripts/run_state_bench.py
- Create: scripts/run_longmemeval_v2.py
- Create: scripts/run_evomembench.py
- Create: benchmarks/manifests/state_bench.lock.json
- Create: benchmarks/manifests/longmemeval_v2.lock.json
- Create: benchmarks/manifests/evomembench.lock.json
- Create: tests/test_public_benchmark_adapters.py
- Reuse: public MemoryService/REST contracts

- [ ] Run STATE-Bench v0.8.0 (`e2c8d7af51ef48fbbea51bb2ce1fb859af36b423`) with paired no-memory and MemPhant arms under its official Agent Learning Track protocol. Track newer main separately; do not call it v0.8.0.
- [ ] Run LongMemEval-V2 (`be15ea6e995462f3391c1a610892df3f67dfa7bd`) against its official harness, fixed reader/protocol, and memory-context budget; report accuracy plus latency. Sol product-reader results are supplemental, not official-comparability scores.
- [ ] Run EvoMemBench across knowledge/execution and in-episode/cross-episode cells only after a licensed native harness is available; do not label an internal substitute official.
- [ ] Preserve lane-specific policies when the benchmark demonstrates different winners.
- [ ] Report a Pareto point rather than forcing one global configuration.

**Exit:** MemPhant has independently useful knowledge and execution policies over the same temporal evidence substrate.

### Task 7: Attempt Sealed SOTA Confirmation

**Files:**

- No feature implementation is allowed in this task.
- Create: docs/build-log/2026-07-12-unified-sota-confirmation.md
- Create: docs/build-log/artifacts/unified-sota-confirmation/manifest.json

- [ ] Freeze commit, binaries, models, prompts, judge, benchmark revisions, manifests, configs, seeds, commands, raw judge outputs, parse/fallback counts, and answer-order randomization.
- [ ] Blindly rerun the official GPT-4o-2024-08-06 full-history/session JSON + Chain-of-Note baseline on the same redacted 319 IDs; score both arms with the pinned official evaluator.
- [ ] Independently rerun pinned Mnemis on the same repaired 319 IDs/protocol,
  or explicitly narrow the claim and report its published full-500 91.6 result
  as non-comparable external context.
- [ ] Run baseline and exactly one candidate on all 319 cleaned LongMemEval-S confirmation questions.
- [ ] Run the primary portfolio gates with their official held-out splits.
- [ ] Adjudicate all candidate/baseline answer flips blind to arm with both A/B orders; position disagreement blocks promotion.
- [ ] Run the cross-family lattice for sign agreement.
- [ ] Publish per-category accuracy, paired confidence intervals, answer support, stale use, restraint, p50/p95, tokens, and cost.
- [ ] Record exactly one terminal outcome: on failure mark the 319 exposed and require a new independent holdout; on success update STATUS with the named, scoped claim and proof artifact.

**Exit:** A scoped SOTA claim is evidence-backed, or the campaign continues without laundering an exposed set.

### Task 8: Replace Syndai RAG/KB and Memory Surfaces

**Public-repo boundary:** implementation details and private paths remain in porting.md and the private Syndai plan. This public plan records contracts, not a local checkout path.

**Order:**

1. file-memory provenance and correction parity;
2. document/knowledge retrieval;
3. episodic memory;
4. relevant profile context;
5. behavioral/procedural memory.

- [ ] Freeze stable tenant, principal, scope, actor, trace, citation, and memory-ID mappings.
- [ ] Replay identical, controlled, labeled production-shaped dogfood requests through MemPhant and the incumbent; do not call this production shadow traffic while the app has no users.
- [ ] Require the same paired quality gate used in the benchmark, plus no citation/correct/forget regression.
- [ ] Route one backend surface at a time through MemPhant.
- [ ] Keep rollback at the backend adapter boundary until the observation window passes.
- [ ] Remove the incumbent implementation only after the MemPhant path wins and the observation window is clean.
- [ ] Keep web and mobile on Syndai backend contracts; no direct MemPhant dependency.

**Exit:** Syndai uses MemPhant as the canonical agent, RAG/KB, and code-experience substrate, and replaced code is deleted rather than maintained in parallel.

### Task 9: Make CaaS the Same Canonical Runtime and Enforce Database Isolation

**Files:**

- Replace/squash: memphant_migrations/versions/20260703_001_wsa_bootstrap.sql
- Delete: memphant_migrations/versions/20260709_002_runtime_reconciliation.sql
- Modify: crates/memphant-store-postgres/src/store.rs
- Modify: crates/memphant-runtime/src/lib.rs
- Create: crates/memphant-store-postgres/tests/rls_runtime_contract.rs

- [x] Because no accessible Supabase project contains `memphant` objects and this repo has no users or compatibility contract, ship one correct bootstrap rather than preserving 001/002 mistakes with an additive 003. If an undiscovered installation exists, generate its upgrade separately; do not make that path the canonical design.
- [x] Separate NOLOGIN capability roles for owner, runtime app, worker, authn, readonly, and provisioner; separately provision LOGIN credentials. Runtime roles are non-owner and `NOBYPASSRLS`.
- [x] Inventory CaaS retrieval entry points. Reuse Syndai's existing `memphant_dogfood_adapter.py`, cut document retrieval at the existing `search_detached.py` convergence seam, and map `CaaSTenantContext` to the existing tenant/actor/scope IDs. Do not create a second adapter or harness. Delete any CaaS-only engine only after the same paired gate wins. Proof: `docs/build-log/2026-07-13-syndai-retrieval-inventory.md`.
- [ ] Keep hosted-only code limited to routing, billing, quotas, and operations. Do not fork retrieval, temporal semantics, or ranking from the public runtime.
- [x] Run tenant operations as a non-owner role.
- [x] Force RLS on every tenant table. Bind tenant identity with transaction-local `set_config(..., true)` when the transaction begins; reject payloads for a different tenant and never query tenant data directly through an unbound pool.
- [x] Resolve API keys through a narrowly privileged pre-tenant function.
- [x] Claim global jobs through a narrowly privileged function, then process under the job tenant.
- [x] Revoke function execution from `PUBLIC`, establish exact default privileges, and keep Supabase browser/service roles outside the `memphant` schema.
- [x] Use explicit migration, runtime, worker, auth, provisioner, and readonly database URLs. Persistent server/worker processes prefer direct connections, with session pooler fallback; transaction-pool mode disables prepared-statement caching and remains a tested deployment option.
- [x] Prove missing/wrong tenant denial, concurrent isolation, pool reuse, and no owner bypass.
- [x] Validate direct/session/transaction-pool provider behavior for plain Postgres, Supabase, and Neon.
- [ ] Add quotas and metering after isolation, not before.

**Exit:** CaaS serves the same public MemoryService with no duplicate retrieval engine, and hosted MemPhant has a real database-enforced tenant boundary.

### Task 10: Optimize Cost Only After SOTA

**Files:**

- Modify: scripts/run_reader.py for cheaper-reader experiments
- Modify: crates/memphant-runtime/src/embeddings.rs for cheaper-reranker experiments
- Modify: crates/memphant-core/src/lib.rs only if candidate-depth experiments require a runtime setting
- Test: tests/test_run_reader_contract.py
- Test: tests/test_gate_r15_cross_rerank.py
- Test: crates/memphant-core/tests/recall_pool_depth.rs

- [ ] Freeze the SOTA answer, evidence, trace, and latency artifacts.
- [ ] Test cheaper answer models, rerankers, smaller candidate depths, caching, and batch APIs one lever at a time.
- [ ] Require accuracy non-inferiority and no stale/restraint regression.
- [ ] Promote savings only after two-lattice confirmation.

Decision rule: delete the expensive path only after a cheaper path passes the
non-inferiority gate; otherwise retain the quality tier.

**Exit:** Cost falls without surrendering the SOTA quality point.

## Standing Promotion Rules

- Accuracy decisions use paired complete rows and confidence intervals, not point estimates.
- Resample by the benchmark's independent unit: connected shared-session components for conversational memory and source-document components for docs. Apply Holm correction across screened arms before promotion.
- Mechanism metrics must move before an expensive end-to-end call is justified.
- Performance is end-to-end p50/p95, including network/provider time.
- Cost is always recorded. Before Task 10 it breaks statistical accuracy/UX ties among candidates inside the hard latency ceiling; it never vetoes a real accuracy winner.
- Retrieval-only wins do not imply answer-quality wins.
- Reader-only wins are labeled product orchestration, not substrate advances.
- A private Syndai win permits replacement; it does not by itself permit a public SOTA claim.
- A public benchmark win permits a scoped claim; it does not by itself permit Syndai replacement.
- One lane's winning policy may not become another lane's default without that lane's gate.

## Final Verification

Run the narrowest checks per task, then the full repository gate before any workstream exit:

    python3 -m pytest tests/ spikes/python-retain/test_spike.py -q
    python3 scripts/check_spec_drift.py
    cargo fmt --check
    cargo clippy --all-targets --all-features -- -D warnings
    cargo test --all-targets --all-features
    cargo test --doc
    bash scripts/with_scratch_db.sh postgres://memphant:memphant@localhost:5432/memphant MEMPHANT_TEST_DATABASE_URL cargo test -p memphant-store-postgres -p memphant-worker -- --ignored --test-threads=1
    cargo run -p memphant-cli -- db lint --provider plain-postgres
    cargo run -p memphant-cli -- db lint --provider supabase
    cargo run -p memphant-cli -- db lint --provider neon
    python3 scripts/apply_memphant_migrations.py --database-url postgres://memphant.invalid/memphant --dry-run
    DATABASE_URL=postgres://memphant:memphant@localhost:5432/memphant bash scripts/e2e_probe.sh

## Deliberately Not Built

- one universal retrieval policy;
- a new physical hot/warm/cold/file architecture;
- a generic reranker/provider plugin framework;
- manual 512-token truncation;
- async reranking for synchronous agent turns;
- a new profile table or preference memory kind;
- a second outcome API;
- speculative LLM consolidation;
- direct MemPhant web/mobile clients;
- cost optimization before the SOTA point exists.

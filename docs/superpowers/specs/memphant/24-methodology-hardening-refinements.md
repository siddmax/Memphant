# MemPhant - Methodology Hardening Refinements

## 0. Purpose

This doc records hardening decisions that are real and must be represented in the first public architecture. Some run only when data exists, but their schema, events, and gates are not left as future rearchitecture.

The rule:

```text
freeze schema/interface now if retrofitting is painful
ship SOTA-critical modes now
activate learned/data-dependent methods only when their evidence floor is met
```

## 1. Freeze Now

Schema/interface hooks to include early:

- `engine_version`
- `compiler_version`
- `trace_schema_version`
- `methodology_version`
- `embedding_profile_id` + `index_strategy`
- `memory_kind`
- `trust_level`
- `source_kind`
- `subject_key` (drives contradiction detection)
- `retention_tier`, `dedup_key`, `observation_count` (episode lifecycle)
- `feature_flags`
- `retrieval_mode`
- `evaluation_tier`
- `forget_generation`
- `citation_refs`
- `filter_selectivity`, `consolidation_lag` (trace honesty fields)

## 2. Method Activation Decisions

| Method | First public decision |
|---|---|
| DSR parameter fitting | fields/events/fixed priors ship now; learned fitting activates only with enough MemPhant reinforcement traces |
| graph DB traversal | rejected; relational edge expansion ships now |
| query decomposition | ships in balanced/deep and benchmark modes |
| HyDE | rejected for v1 because it creates synthetic evidence with weak provenance |
| cross-encoder rerank | bounded rerank interface ships now; provider-backed rerank runs only in balanced/deep/benchmark modes |
| L4 sandbox recall | ships as Deep/benchmark mode |
| procedure compiler | rejected; validated procedural memory ships without compiler |
| adaptive memory budgets | ships as deterministic budget policy plus trace fields; learned budgets require eval evidence |

## 2.2 Refinement Register (R-series)

The numbered, cited, supersession-linked audit trail of every hardening decision, so methodology drift is traceable to the round that caught it (mirrors EvalRank's `22` R-series). Decision classes: **[freeze]** (schema/interface now), **[accuracy]** (factual correction), **[data-gated]** (activate on evidence floor), **[reject]** (YAGNI), **[pointer]** (future, not v1). Round 1 = the 6-team 2026 audit (context7 / web / GitHub / codebase / tests / experimental).

| R | Class | Decision | Evidence (2026) | Supersedes | Lands in |
|---|---|---|---|---|---|
| R1 | accuracy+freeze | pgvector HNSW caps index dims by type — **`vector` 2,000, `halfvec` 4,000** (corrected by R51; MemPhant stores `halfvec` so `hnsw_full` is valid ≤4,000); >cap needs `hnsw_subvector`/`hnsw_binary` per `embedding_profile.index_strategy` | pgvector README (vector 2,000 / halfvec 4,000) | "HNSW per embedding profile" implying any dimension | `02` §2.1a, `03` §5.1, `25` §4 |
| R2 | accuracy | `halfvec` + `binary_quantize` two-phase (fast bit first-pass → full-vec rerank) is the scale lever, not exotic | pgvector bitvec workflow docs | "binary quantization … benchmarked features, not default" framing | `02` §2.1a, `05` §6 |
| R3 | accuracy | iterative scan is an **opt-in per-query GUC** (`hnsw.iterative_scan`); post-filter ANN silently under-recalls without it | pgvector 0.8 configuration | implicit "iterative scans" with no GUC named | `02` §2.1a, `05` §1 |
| R4 | accuracy+freeze | tenant-filtered HNSW is pgvector's worst case (small/new tenant → silent recall collapse) → partial per-tenant indexes + `filter_selectivity` trace + small-tenant-recall benchmark | 2026 filtered-vector-search analyses | "HNSW … with tenant prefilter" stated as if safe | `02` §2.1b, `05` §5 |
| R5 | accuracy | FSRS-6 = **21-weight global fitter** from review traces; per-unit state is `(stability, difficulty)`, retrievability computed; wrap `fsrs-rs` | ts-fsrs / py-fsrs / fsrs-rs (21-param) | `retrievability` as stored field; "FSRS parameter fitter" as per-unit | `04` §8, `03` §3 |
| R6 | accuracy | BEAM primary cite = **arxiv 2510.27246**; `agentmemorybenchmark.ai` is a Vectorize vendor leaderboard → `source_status: vendor_reported` | Tavakoli et al., "Beyond a Million Tokens" | the leaderboard URL cited as canonical source | `00-MAIN` §1, `12`, `27` |
| R7 | accuracy+scope | AgentDojo is **near-saturated** and tests tool-call-time injection, **not persistent memory poisoning** → lean on OWASP Agent Memory Guard fixtures + the corroboration-farming suite | AgentDojo (NeurIPS 2024) + 2026 saturation | AgentDojo presented as the memory-poisoning benchmark | `05` §7, `06` |
| R8 | freeze | contradiction **detection** contract (subject-key + embedding proximity + valid-time overlap); detection precision/recall reported separately from resolution; golden must not pre-annotate the conflict | Mem0 issue #4896 (ADD-only ≠ semantic resolution); Hendrickson 2026 | invariant #6 left detection as a background-job side effect | `04` §3.1, `05` §4.2/§5 |
| R9 | freeze+cost | raw-episode **retention tiers** (hot/warm/cold); invariant #1 restated "recoverable ground truth"; storage-growth SLI | BEAM 10M economics; Hindsight #1 via token-efficiency | invariant #1 "never replaced" implying always-hot, unbounded | `00-MAIN` inv #1, `04` §2.4 |
| R10 | freeze | episodic **near-dedup** (`dedup_key`/`observation_count`) protects storage *and* the DSR `reinforcement_count` signal | agent-episode near-duplication (retries) | content-addressing assumed sufficient | `04` §2.3 |
| R11 | security+freeze | corroboration requires **source independence** (distinct `actor_id` AND `source_kind`), not count ≥N | OWASP ASI06 + Sybil literature | "corroboration" as count-based in invariant #3 | `04` §5, `06` §9 |
| R12 | contract | `reflect` is a public verb → gets a stage/trace/cost/security contract parallel to `recall` | — (spec gap) | `reflect` listed as a tool with no contract | `04` §9, `08` §4.2 |
| R13 | resilience | write-path **backpressure** + `consolidation_lag` degraded-mode (declare, don't silently miss) | "async mode as default" named a 2026 production footgun | "eventual completion under normal load" with no abnormal-load behavior | `02` §3.1, `08` §2.1 |
| R14 | security | **RLS is the default** on hosted multi-tenant, not BYOC-only | filtered-HNSW leak surface (R4) | RLS as a BYOC-only conditional | `06` §6.1, `03` §5 |
| R15 | security | resource-fetch **SSRF floor** (resolve-and-reject private/loopback/metadata; reject IPv4-mapped-IPv6) | Syndai `ios_namespace` proven pattern | no SSRF posture on the resource-pointer fetch surface | `06` §3.1 |
| R16 | accuracy | Rust justification = **deployment + eval-replay + isolation**; extraction latency is **provider-bound, not Rust-bound** | extraction named the 2026 production bottleneck | Decision #2 over-claiming Rust on the accuracy/latency path | `03` §0.1 |
| R17 | target | **STATE-Bench is the primary neutral SOTA target** (greenfield, memory-agnostic); SOTA bar = beat a *reproduced* baseline or publish a Pareto win, never a vendor blog number | Microsoft STATE-Bench (May 2026) | `27` targets framed against vendor-reported numbers | `27` §1, `05` §8 |
| R18 | freeze+contract (was pointer) | retroactive validity-window correction = case (b) of the append-only generation model (R59, `04` §7.3a) — mechanically FREE under the 4 temporal fields (append a new generation with corrected past `valid_*`, never in-place mutate); surfaced via optional `correct` `valid_from`/`valid_to` args; rung-5 gates *marketing*, not mechanism | Fowler bitemporal-history; SQL:2011; XTDB | the pointer framing implying the 4-field model can't do temporal correction | `04` §3.2/§7.3a, `08` §4.2 |
| R19 | accuracy | `rmcp` derives `inputSchema` but **not** `outputSchema` from `#[tool]` alone; attach output schemas explicitly with `Tool::with_output_schema<T>()`; SSE is not a MemPhant launch transport | rmcp docs | MCP tool contract assumed TS-SDK parity | `02` §7, `08` §5 |
| R20 | contract | CI discipline: **bootstrap CIs ≥1,000 resamples**, paired comparisons for lever promotion, declared stability threshold | scoring-reproducibility practice | "report confidence intervals" with no method | `05` §8 |
| R21 | integrity | for *memory* benchmarks, contamination ≠ saturation; the integrity state machine needs **transition triggers** (ACTIVE→FLAGGED→DOWN-WEIGHTED→FROZEN→RETIRED) + per-transition evidence | EvalRank `22` R9/R10 analog | `12` §7 lists states but no transitions | `12` §7 |

**Change manifest.** R1–R7, R19 are accuracy corrections (apply immediately). R8–R12, R15 freeze write-path/security schema (unrecoverable if not captured at ingestion). R13–R14, R20–R21 are contract/discipline. R16–R18 are framing/pointers. Every R carries its evidence above; re-verify external figures at ingestion (invariant: vendor numbers are `vendor_reported`).

### Round 2 — 2026 SOTA cross-check (R22–R28)

Two external "MemPhant vs SOTA" reports were fact-checked by 6 adversarial teams against primary-source arXiv fetches (2026-06-25). The reports were ~90% accurate (no fabricated papers); these are the **validated** refinements. Each is `vendor_reported`/analogical until re-measured on a MemPhant target — they enter as **ablation hypotheses**, not settled levers.

| R | Class | Decision | Evidence (verified) | Lands in |
|---|---|---|---|---|
| R22 | accuracy | retrieval >> write dominance grounds the cheap-durable-write decision (retrieval ~20pt vs write 3–8pt swing on LoCoMo) | Yuan/Su/Yao arXiv:2603.02473 (CONFIRMED verbatim) | `03` §0.1, `05` §1.4 |
| R23 | ablation hooks | four retrieval-stage micro-levers as named ablation arms + trace fields: depth-tuning, context-formatting, query-prefix, source-kind rebalance | MemMachine arXiv:2604.04853 (all 5 deltas CONFIRMED verbatim) | `05` §1.4 |
| R24 | contract | cheap LLM judge on the *ambiguous residual* of contradiction detection (embedding cosine is polarity-blind → proximity over-flags; SOTA = embedding/temporal candidate + LLM decide) | Zep 2501.13956 + Mem0 2504.19413 + SparseCL 2406.10746 + biomedical 2110.15708 (CONFIRMED) | `04` §3.1 |
| R25 | ablation | fuzzy (MinHash/embedding) near-dedup second pass after exact-hash | arXiv:2605.09611 (5.81% vs 31.32%, CONFIRMED; agent-memory effect unmeasured) | `04` §2.3 |
| R26 | benchmark | add `MemoryStress` (longitudinal, 1,000-session) as the FSRS-decay ablation target + chained-contradiction eval; FSRS-for-agent-memory remains unvalidated (survey 2603.07670 §9.8) | OMEGA MemoryStress + survey 2603.07670 (CONFIRMED) | `04` §8, `12`, `27` |
| R27 | pointer | optional L0/L1/L2 multi-resolution summaries for the resource kind (filesystem-tiering) | VikingMem arXiv:2605.29640, VLDB 2026 (concept CONFIRMED) | `04` §6.1 |
| R28 | data-gated | RRF→tuned convex-combination once ~40 labeled queries exist | Bruch arXiv:2210.11934 + Elastic replication (CONFIRMED) | `05` §1.2 |

**Caveats the cross-check caught (do NOT propagate):** the source reports cited several wrong numbers — LegalWiz F1s were fabricated (real 52.6/21.4, 79.0/49.4, 87.7/64.9; cite the *pattern*, not numbers); Hindsight LongMemEval is 91.4% not 94.6%; OpenViking's "35.65→52.08% / 80%" is fabricated (concept only); the Zep 58.44% reproduction was authored by a competitor, not Zep; BEAM's Mem0 numbers are vendor self-report not in the paper. None of these wrong numbers entered the specs. "Adversarial replay" (`04` §4.2) is not a unique novelty (canary/stress-before-promote is established) — keep it for safety, drop any novelty claim. This Round-2 audit *itself* is the evidence for invariant: external figures are `vendor_reported` until independently reproduced.

### Round 3 — June-2026 gap-check cross-check (R29–R36)

A second external gap-check report, fact-checked by 6 adversarial teams (primary-source arXiv fetches, one via `gh`). Again ~90% accurate, zero fabricated papers. Validated refinements (ablation-hypothesis until re-measured):

| R | Class | Decision | Evidence (verified) | Lands in |
|---|---|---|---|---|
| R29 | contract | **calibrated restraint**: a relevance gate prunes off-query memory before injection — over-retrieval is a *measured* harm | OP-Bench 2601.13722 (26.2–61.1% drops, Self-ReCheck −29%, CONFIRMED) + A-MAC 2603.04549 (admission control F1 0.583>0.541) + EverMemOS 2601.02163 (accuracy>recall) | `05` §1.5, `27` |
| R30 | threat | intent-legitimation: benign memory can legitimize a harmful query | PS-Bench 2601.17887 (attack success +15.8–243.7%, CONFIRMED) | `06` §9 |
| R31 | methodology | choose the embedding *model* on a memory benchmark (LMEB), not MTEB rank (anti-correlated on dialogue) | LMEB 2603.12572 (Pearson −0.115/−0.496, CONFIRMED) + HiNS 2601.14857 | `02` §2.1a, `12` |
| R32 | lever | rerank = a *memory-tuned* cross-encoder reordering only the protected top-k (recall-preserving); a *generic* learned reranker does not beat off-the-shelf | MemReranker 2605.06132 + ConvMemory v1/v2 2605.28062/2606.10842 (CONFIRMED; single-author preprints) | `05` §1.4 |
| R33 | ablation | **no-edges + filesystem control baselines** — edges must beat them or be cut | Mem0 paper 2504.19413 (graph loses single+multi-hop) + Mem0 v3 PR #4805 removed graph (gh-verified) + Letta filesystem 74%>68.5% + Continua harness-noise; counter: Hindsight MPFP 2512.12818 (edges pay at 10M scale) | `05` §9.1, `27` rung 6 |
| R34 | ablation/contract | proactive pre-fetch arm (−28% hallucination, gated on not raising over-personalization); query-complexity-gated reflection | ProAct 2605.25971 + HyMem 2602.13933 (CONFIRMED) | `05` §1.4/§9.2 |
| R35 | contract | expose typed contradiction/causal edges to the caller (substrate exposes signals, agent reasons) | ActMem 2603.00026 (CONFIRMED, safety example) | `06` §10, `08` |
| R36 | portfolio | SOTA = a profile across axes; add EMemBench (interactive episodic) + the restraint axis | EMemBench 2601.16690 + the 9-axis portfolio (CONFIRMED real) | `12` §2.0 |

**Watchlist (do NOT adopt):** RL-trained memory management (Memory-R1 2508.19828, Mem-α 2509.25911, HAGE 2605.09942) — real and stronger than the report claimed (Memory-R1 is +48%/+37%, not +28%/+30%), but **pre-production**; the case against adoption rests on *maturity/cost*, not weak results. Track Memory-R1's `ADD/UPDATE/DELETE/NOOP` op-set as a design reference. SkillOS-style frozen-executor + trainable-curator (2605.06614) — reference for procedural memory.

**Caveats the cross-check caught (NOT propagated):** OP-Bench "attend 2x more than queries" was FABRICATED (search snippet, not paper); GAM 2604.12285 "38.94 vs 38.72 / E-mem 49.15" fabricated (real 35.88 vs 32.78; E-mem is a *separate* paper); Memory-R1 "+28%/+30%" understates the real +48%/+37%; **SimpleMem 30× does NOT conflict with ground-truth preservation** — it is *inference-context* reduction (orthogonal, +26.4% F1), the same insight MemPhant already has in budgeted assembly + depth-tuning (so keep raw episodes uncompressed); the spec never pinned Voyage-3-large (provider-agnostic), so the "suboptimal embeddings" critique attacked a strawman — only the selection discipline (R31) is real.

### Round 4 — full-lifecycle cross-check (R37–R44)

A full-lifecycle report (cold-start → years of hot memory), fact-checked by 6 adversarial teams against primary sources (GitHub issues via `gh`, arXiv, news). ~85-90% accurate; all issues + papers real. Validated refinements (the dominant theme: **memory rot and silent degradation over time** — for the mature user, cleaning + honesty beat accumulation):

| R | Class | Decision | Evidence (verified, corrected numbers) | Lands in |
|---|---|---|---|---|
| R37 | edge cases | mature/long-horizon edge cases (memory-rot, staleness, silent recall decay, deletion-at-scale, migration, creepiness) — the mirror of cold-start | the report's lifecycle framing; Mem0 #4573 audit | `01` §0.3 |
| R38 | contract | the dedup/corroboration/contradiction gates are a **write-time quality gate at ingest**, not a later sweep (over-storage is the dominant failure) | Mem0 #4573 (97.8% junk, one fact copied **808×** not 668) + Harvard/D³ 2505.16067 (add-all ≤ no-memory) | `04` §9.2 |
| R39 | freeze/contract | **active freshness** (re-confirm/down-weight high-churn fact types) — bitemporal is reactive | Mem0 "memory staleness" open problem | `04` §8.1 |
| R40 | contract | **consolidation invariants**: linear-in-NEW-memories, bounded batches, bisection retry 8→4→2→1, hierarchical retrieval (no full-scan), async; + extraction **schema-validate-or-quarantine** so one malformed item never aborts a batch | Hindsight (verbatim) + Graphiti #879/#796/#760/#875 + v0.21.0 note | `04` §9.3, `02` §5.2 |
| R41 | contract | corpus-size-aware recall + **continuous recall SLI** (HNSW degrades *silently* — latency stays flat); filtered-island effect is engine-dependent, measure own stack | TDS HNSW-at-scale (~10pt 50k→200k); ACORN/Cardinal vs Weaviate counter | `02` §2.1b, `22` |
| R42 | contract | deletion: **cross-store saga + read-back** (orphaning) + tombstone compaction + **crypto-shred** for forget-user-X | Mem0 #3245 + GDPR Art.17 + MemTrust 2601.07004 | `06` §6.2 |
| R43 | contract | embedding upgrade: **second-profile vector cutover + model-version tag on every vector**; Drift-Adapter bridge (95-99% @ 1M, NOT 99.7%@100K — that's Schift) | Optivulnix ($12k/180M) + Drift-Adapter 2509.23471 | `14` §10 |
| R44 | contract | intra-scope concurrency: **per-memory-kind isolation levels** + recency supersession; read-back-confirmed critical writes; provenance + one-click per-fact correction; never surface fabricated as fact | MAST 2503.13657 (36.9% IAM) + UCSD 2603.10062 + Governed-Shared-Memory 2606.24535 + Letta #689 + uncanny-valley fMRI/2508.18563 | `04` §11.2, `08`, `19` |

**Caveats the cross-check caught (NOT propagated):** "668 copies" → **808**; Mem0 "66.9 vs 52.9 / ECAI 2025" not in paper (use 26%/91%/90%); Harvard "10% boost" unverified (gains agent-dependent); "2-3x latency restores recall" false (lever = raise `ef_search`); Drift-Adapter "99.7%@100K" → 95-99%@1M (Schift's number); Governed-Shared-Memory "~2s / source-rank+human" → ~1s / recency-supersession (source-rank is *our* design option, not prior art); "Dreaming 2025" → June 2026; "quiet confidence of the false answer" has no source; Devin "memory pruning" conflated with Claude Code; Letta #689 "lost forever" quote fabricated (use the symptom — corrected fact reverts).

### Round 5 — eval/ablation methodology cross-check (R45–R49)

A methodology-review report (single-lever vs interaction effects, oracle validity, statistical rigor) fact-checked by 3 adversarial teams against primary-source fetches + 1 spec-coverage pass (2026-06-26). The report's *recommendations* were sound, but it carried more fabricated specifics than prior rounds — several real papers were cited for claims they do not make. Only the validated, not-already-covered gaps land:

| R | Class | Decision | Evidence (verified) | Lands in |
|---|---|---|---|---|
| R45 | discipline | **clustered/grouped bootstrap SEs** — resample by `session_id`/`corpus_id` cluster, not by case; correlated cases (multi-turn sessions, one corpus) make per-case SEs understate the interval by **>3×** | Evan Miller "Adding Error Bars to Evals" arXiv:2411.00640 (CONFIRMED, ">3X" verbatim, Table 4) | `05` §8, `22` §2 (`cluster_key`) |
| R46 | discipline | **multiple-comparison correction** on multi-lever promotion: Holm-Bonferroni (small set) → Benjamini-Hochberg/FDR (large) — many micro-levers (`05` §1.4) + the ablation matrix run on shared cases, inflating false promotions at a per-test α | textbook FWER/FDR control (CONFIRMED) | `05` §8, `27` §8 |
| R47 | methodology | **interaction-effect guard**: per-lever paired-CI is necessary-not-sufficient → **leave-one-out** validation of the promoted stack at release cadence (no full 2^n factorial) | JMIR Med Inform 2026 e94241 (context η² 49.0% vs 47.6% model choice; model×corpus + model×query-format interactions significant — CONFIRMED) | `05` §9.3 |
| R48 | contract | **oracle-rot guard**: hand-authored `answer_bearing_ids` rot → standing whole-corpus `verify-golden` + two-author confirm of the minimal set for new golden families (a single assessor's relevance labels agree less than two) | Sormunen 2002 single-assessor agreement (CONFIRMED principle) | `05` §4.0, `22` §3.1 |
| R49 | ablation | **contradiction-detection method** as a named ablation arm (embedding+temporal candidate / +NLI verifier / +cheap LLM-judge) — no public NLI-vs-judge-vs-embedding head-to-head exists inside an agent-memory store (field-first; the embedding+temporal+LLM commitment is R24) | CSMAD / Amazon Science NLI-verifier (CONFIRMED) | `04` §3.1, `05` §9 |

**Caveats the cross-check caught (do NOT propagate):** the report mis-attributed methodology to real papers more than prior rounds.
- **SkillFlow (arXiv:2604.17308) "applies Holm-Bonferroni within each benchmark"** — FABRICATED; SkillFlow contains *no* significance testing at all. **"SkillFlow + AgentSocialBench both use paired bootstrap with CI-excludes-zero"** — half FABRICATED; only AgentSocialBench (arXiv:2604.01487) uses paired bootstrap, and it states no CI-excludes-zero rule. So R46 cites the **textbook** correction, not a fabricated precedent.
- **arXiv:2603.02473 "200 human-labeled answers / 92% accuracy / Cohen's κ=0.82"** — FABRICATED; the retrieval≫utilization *thesis* is real (already R22), but those validation numbers are not in the paper.
- **CRYSTAL (arXiv:2603.13099) "19 of 20 models"** — FABRICATED; the paper says cherry-picking is "universal." Anti-cherry-picking is already covered by axis pre-registration (`27` §1), so nothing new was added from this claim.
- **SRAG (arXiv:2603.26670) interaction-effect quotes** ("improvements arise from interactions…", "should not be interpreted as evidence…") — both FABRICATED; the real abstract reports a 30% LLM-judge gain (p=2e-13), no ablation-interpretation sentence. R47's interaction evidence is grounded on JMIR e94241 instead.
- **TREC podcast re-assessment (arXiv:2601.05603) "perfect-relevance grades not reproducible by anyone but the topic creator / system orderings volatile"** — FABRICATED framing; the real finding is that human experts agree with LLMs more than with the original single assessor. R48 is grounded on the underlying single-assessor-agreement result, not the fabricated quote.
- **VISTA (arXiv:2510.27052) "contradiction detection is hard with substantial variance across models"** — misattributed; VISTA is a turn-based factuality/hallucination scorer, not a contradiction-difficulty study. Not cited for R49.
- **FadeMem (arXiv:2601.18642) "stretched-exponential decay"** — it is plain *adaptive exponential*; does not change R26.
- **ZenBrain (arXiv:2604.23878) "decay negligible overall"** — negligible only as the 14-day-LoCoMo *cost* of forgetting (|d|=0.015); the same mechanism is ~93% critical under stress, and it ablates decay on/off, **not** FSRS-vs-exponential — so the FSRS-vs-exponential gap (R26) stands, with ZenBrain as the closest near-miss.
- **Du et al. (arXiv:2510.05381)** 13.9–85% degradation + EMNLP 2025 Findings CONFIRMED; the exact 5-model roster was unverifiable from the abstract → not pinned. **Survey arXiv:2603.07670** lists "learned forgetting" (already R26), **not** "standardized evaluation" → not attributed. **MemoryAgentBench**'s real ID is arXiv:2507.05257 (not a 26xx ID).

This Round-5 audit is itself fresh evidence for the standing invariant: external figures are `vendor_reported` until independently reproduced — and a real arXiv ID does not make the sentence citing it true.

### Round 6 — architecture deep-research, the hard-to-reverse layers (R50–R56)

Unlike Rounds 2–5 (validating *external reports*), Round 6 is **our own** 5-agent deep-research on the load-bearing, hard-to-reverse decisions the prior rounds never touched — physical data layout and write-path distributed correctness — each grounded in fetched primary sources. It found **bugs, not just gaps**: three items below correct something the spec asserted *wrongly*.

| R | Class | Decision | Evidence (primary-source) | Lands in |
|---|---|---|---|---|
| R50 | freeze | **`PARTITION BY HASH(tenant_id)`** on episode/memory_unit/memory_edge/embedding + event ledgers; `tenant_id` in every PK; modulus set-once-immutable (64 hosted / 4–8 BYOC / 1 tiny); no pg_partman (HASH unsupported), no Citus (AGPL) | PostgreSQL partitioning docs + pgvector #479 ("Using PARTITION solved [filtered-recall]"); planner "up to a few thousand partitions" | `04` §7.0, `02` §2.1b, `00` §2, `26` §1 |
| R51 | **accuracy (spec was WRONG)** | `halfvec` HNSW caps at **4,000 dims, not 2,000** (2,000 is `vector`) → 3,072-dim models use `hnsw_full` directly; `vec` column is dimensionless; every active profile owns a `WHERE embedding_profile_id=…` **partial** index (the vector query MUST carry the predicate or it silently seq-scans) | pgvector README (vector 2,000 / halfvec 4,000 / storage 16,000); the §5.2 SQL omitted the profile predicate | `02` §2.1a, `03` §5.1/§5.2, `04` §7 |
| R52 | freeze | no-CLA **`schema_compat_revision` boot-floor** (Synapse `SCHEMA_COMPAT_VERSION`) + additive-vs-breaking taxonomy + forward-compat read contract (runtime `query_as` + explicit columns + TEXT-fallback enums on frozen tables) | Matrix/Synapse `storage/schema/__init__.py`; sqlx `FromRow`/`query_as!` decode semantics; parallel-change (Fowler/Sato) | `25` §11b/§11c/§12, `09` §10, `08` §7, `00` §2/§4 |
| R53 | contract | two-store GC marks from the **Postgres reference set + `blob_ledger`, never `object_store.list()`**; **`MIN_AGE` grace** closes the blob-PUT→row-commit race (proof = `max_txn ≪ MIN_AGE`); reject refcount (drifts under crash) | git-gc `--prune` grace; CNCF registry-GC read-only race; S3/GCS list-after-write consistency | `02` §2.3/§3.0, `03` §5.1, `14` §4, `06` §6.2 |
| R54 | contract | **`MemoryStore` transaction seam** (opaque `Txn` GAT + `begin()/commit()` + `&mut Txn`) so the atomic `{episode, units, reflect-enqueue}` commit and the forget saga are expressible without leaking SQL across the core boundary | the trait's standalone `async fn`s could not express `02` §3.0's required atomic commit | `03` §4, `02` §3.0 |
| R55 | **accuracy (spec was WRONG)** | `02` §6.1's "races are safe no-op upserts, workers stay parallel" is true for edge-uniqueness, **false for accumulators** (confidence `c←c+α(1−c)`, FSRS stability RMW, supersession `valid_to`) → per-`subject_key` `pg_try_advisory_xact_lock` lease + **event-sourced recompute** over deduped `belief_observation`/`review_event` ledgers (removes the RMW) + stage-checkpoint + DLQ park | PostgreSQL SKIP LOCKED / advisory-lock docs (xact- vs session-scope); pgmq/Temporal at-least-once; fsrs-rs `next_states()` RMW | `02` §6.2/§5.3, `04` §3.4/§5.1a/§8.2/§9.4, `14` §11 |
| R56 | contract | BYOC **`maintenance_work_mem` build-headroom preflight** (warn→raise/binary/exact) + re-embed **double-index peak budget** (≈2× index + headroom; CONCURRENTLY only) | pgvector 0.7+ parallel build (AWS Aurora: 5M/1536 ~21min, 10M/768 ~28min, binary ~3×, halfvec index 19GB); CREATE INDEX CONCURRENTLY lock semantics | `25` §11a, `14` §10.1 |

**Flagged unverified (do NOT present as primary-source fact):** the HNSW memory heuristic `N·D·4·2` and "10–50× disk-build slowdown" are community writeups, not pgvector-official (the build *times*/sizes above are AWS-cited and real). HNSW-index-propagation onto a partitioned parent is PostgreSQL-core behavior, not a pgvector-asserted guarantee — **validate on the target PG version at rung-0** (one real propagation-failure report exists under *Citus* specifically — another reason Citus is rejected). `MIN_AGE=1h` is a deliberately conservative default (≫ `statement_timeout`), tunable. **Reaffirms the standing invariant the whole pass embodies: a real source does not make the sentence citing it true** — R51/R55 are spec self-corrections caught by going to the primary source.

### Round 7 — untouched/irreversible access-path + infra surfaces (R57–R65)

Triage (3 read-only agents, irreversibility lens) found the prior rounds nailed storage/methodology but left the **access-path** (scope/trust/evidence) + a few infra surfaces thin; trust algebra, citation ledger, subject_key, request-admission were confirmed ADEQUATE (agents declined to manufacture gaps). 5 deep-research agents (primary-source) resolved the real Tier-1 surfaces — including **two more spec self-corrections**.

| R | Class | Decision | Evidence (primary-source) | Lands in |
|---|---|---|---|---|
| R57 | freeze | **scope tree = adjacency (`parent_scope_id`) + cached `materialized_path ltree`** (GiST `@>` ancestor walk, no hot-path recursion), depth ≤ 32; `scope`/`scope_policy`/`agent_node` UNPARTITIONED (tree, not memory — §7.0 carve-out). **Inheritance-policy is a typed `scope_policy` table** `(scope×kind×min_level, direction inherit\|grant)`, deny-by-default; a grant is an explicit row (`CHECK (direction='grant')=(grantee IS NOT NULL)`), NEVER a `memory_edge` — makes "no implicit sibling access" falsifiable | PostgreSQL `ltree`/recursive-CTE docs; hierarchical-SQL re-parent tradeoffs | `04` §11.0/§11.1, `03` §5.1/§5.2, `02` §2.1, `00` §2, `06` §2.1, `26` |
| R58 | **accuracy (spec was WRONG)** | confirmed contradiction: `03` §5.1 keyed `embedding` on a `resource_chunk_id` that **no table defines**; resolved — a chunk IS a `kind='resource'` `memory_unit`, `embedding` keys on `memory_unit_id` (ghost deleted, no frozen-PK re-key). `resource.acl` typed `{scopes?,trust_floor?,protected?}`, **in-stage** narrowing gate (never post-ANN — the RAG authz anti-pattern); fixes the Stage-3 vector SQL that carried NO scope predicate (a leak) | production-RAG authz-before-ANN consensus | `03` §5.1/§5.2, `04` §6/§6.1/§7, `05` §1.3, `06` §4, `26` |
| R59 | **accuracy (spec was WRONG)** | bitemporal transaction-time is **append-only**: `correct`/supersede/invalidate close the open generation (`transaction_to=now`) + INSERT a new one, NEVER in-place `valid_*` mutate — fixes §3.4's in-place `valid_to` write that broke audit replay; recall accepts independent `transaction_as_of` + `valid_at` axes rather than an ambiguous single timestamp plus clock selector; R18 retroactive correction now FREE; current-generation partial index (`transaction_to IS NULL`) + cold history index | Fowler bitemporal-history; SQL:2011 system-versioning; XTDB | `04` §3.2/§3.4/§7.3a/§7, `08` §3.1/§4.2, `00` §2, `26` |
| R60 | contract | cross-store **PITR is Postgres-authoritative**; bucket reconciled by *presence* (content-addressing → present-or-absent), never rolled back; **object-store retention ≥ PITR window** (fail-closed bootstrap-check); post-restore sweep: quiesce → restore PG → suspend GC → reconcile presence → integrity gate → resume GC → accept; crypto-shred correct across all restore points | PostgreSQL PITR; S3/GCS versioning + lifecycle; Litestream | `02` §2.3, `14` §4.2, `06` §6.2, `25` §7a, `26` |
| R61 | contract | encryption = **3-tier envelope** (per-user **DEK** ← per-tenant **KEK** ← KMS/TEE root KEK); encrypt `body`/blobs ONLY, vectors **plaintext** (HNSW can't index ciphertext); `exact`-profile opt-in to encrypt; keys never in Postgres (wrapped DEKs in `key_custody`); BYOC holds own KEK; crypto-shred destroys DEK (one user)/KEK (tenant), complements tombstone+compaction (order = DEK→saga) — resolves per-tenant-vs-per-user + encrypted-vector-vs-HNSW contradictions | AWS/GCP KMS envelope; crypto-shredding (GDPR); arXiv:2508.10373 (ANN-over-ciphertext infeasible) | `06` §6.1.1/§6.2, `02` §2.1a, `25` §7a, `26` |
| R62 | contract/pins | access-path pins: `agent_node.level`/`agent_node.id` are **server-derived from the key**, a client-supplied value is advisory+validated → `scope_denied` on mismatch (a child can't claim `level:0`); cross-tree invariant (child `agent_node` scope ⊆ parent's, level ≥ parent's); `subject_key` canonicalizer-*logic* change re-keys via `compiler_version` + offline rebuild from raw episodes | TREC single-assessor agreement (Sormunen 2002, R48 lineage) | `08` §3.0, `04` §3.3/§11 |
| R63 | freeze+contract | deployment posture = OSS library + closed managed service; multi-region residency = **cell-per-region** (open core single-region with immutable `tenant.region`; hosted = N single-region cells + no-PII tenant→region directory + Fly `fly-replay` router; migration = export→import) — keeps the OSS binary region-agnostic, residency is a closed-layer composition | Supabase regional projects + Fly multi-region/`fly-replay` + Temporal regional namespaces; cell-based architecture | `25` §7b, `03` §5.1, `09` §7, `00` §2, `26` |
| R64 | accuracy/contract | **`actor` and `agent_node` are orthogonal, NOT redundant** (confirmed by schema: `actor`=provenance/source+trust+independence-gate, `agent_node`=access-tree+level; episode has `actor_id NOT NULL` + nullable `agent_node_id`; retain carries actor, recall carries agent_node) — doc-only clarity fix, no collapse. **Partitioning is opt-in** (modulus 1 = plain table, no `PARTITION BY`) so single-tenant self-host pays zero partition overhead, but the `tenant_id` isolation key stays (2026-standard: Pinecone/Qdrant/Weaviate/Milvus + Letta/Cognee all bake an isolation primitive). **Billing/finance structural gaps filled**: metered units (`21` §1a), quota+overage+`billing_status` (`21` §3a/`15`), BYOC-vs-hosted split (`21` §3b), per-cell/per-tier COGS + gross-margin-per-tenant (`21` §2a), Syndai-as-payer + compliance-as-product (`21` §7) | tenancy/billing web research (Pinecone RU, Mem0, Zep, Weaviate/Qdrant pricing; Letta/Cognee tenant-in-core; AWS pool/RLS) | `00` §2, `03` §5.1, `04` §7.0/§11, `21`, `15`, `20`, `26` |
| R65 | contract | MCP/SDK frozen-forever contract audited vs the live 2026 MCP spec (2025-11-25 stable + 2026-07-28 stateless RC) — **mostly correct already** (SSE excluded from MemPhant launch transports, custom `memphant://` URI, verb-as-tool, explicit `outputSchema` attach all right). Fixed the real freeze-risks: **`Idempotency-Key` TTL (24h) + `(tenant,verb,key)` scope pinned**; `outputSchema` response shapes frozen **additive-only** + explicit `idempotentHint`/`destructiveHint` serialization (MCP defaults are wrong-way); **recall-never-enumerates** + the `GET /v1/scopes/{id}/memory` cursor list surface. Reserved (additive, NOT built): memory-event taxonomy, elicitation HITL, `subscriptions/listen` if the draft lands or reverse-DNS extension fallback, `reflect` `taskSupport`. **Subject identity merge/split pinned** (`subject_supersedes`, never in-place; `17` §2). **OTel stale claim corrected** (`gen_ai.memory.*` is a live proposal, not a frozen-no-memory enum; `22` §1.1a). | 2026 MCP spec (modelcontextprotocol.io); OTel semconv #2664/genai#35; Letta subject-supersession analog | `08` §5.1/§8, `17` §2, `22` §1.1a |

**Confirmed ADEQUATE (no change — did not manufacture gaps):** trust lattice algebra (clamp-to-source-ceiling is a real bounded-meet that closes laundering by construction), citation/evidence ledger (physically modeled, proptest-gated whitelist), `subject_key` derivation (one-canonicalizer + golden gate), request/throughput admission. **Scoping decisions resolved with the user:** deployment posture = **OSS library + a closed managed hosted service** (R63); hosted multi-region data-residency is now **designed as cell-per-region** (`25` §7b — open core single-region + immutable `tenant.region`; hosted = N single-region cells + a no-PII tenant→region router), keeping the library region-agnostic. Tenant suspension/hard-offboard + storage-quota enforcement are encoded as contract fields (`tenant.status` column + RLS predicate) and activate with the billing/offboard workflows. **This round adds R58/R59 to the standing tally of spec self-corrections found by going to the primary source (cf. R51/R55).**

### Round 8 — adversarial architecture-review hardening (R66–R72)

Two Codex deep-research reports counter-evidence-tested all 12 load-bearing decisions; **8 fact-check agents verified every claim against primary sources** (the verdict: claims survive, but several specifics were fabricated and were EXCLUDED). The reports' cross-cutting lesson — *the danger is the missing **escape hatch**, not the primitive* — drove R72.

| R | Class | Decision | Evidence (verified) | Lands in |
|---|---|---|---|---|
| R66 | **accuracy (correctness)** | **`hnsw_binary` forbidden below ~1024-d** (raw bit recall collapses: Katz 960-d=0.00%, 128-d≈2.2-2.5%, 1536-d plateaus ~68% no-rerank; Qdrant "<1024 poorer"; arXiv:2603.23710 BQ+rerank can't hit 95% on 128-d, 0.75× QPS on openai5M); `iterative_scan=relaxed_order` **default** on filtered recall (post-filter: 10%+ef_search40→"4 rows", README; AWS 100× completeness); per-profile partial-index **explosion cap** (Pinecone yfcc 200k-trap) | pgvector README/Katz/Qdrant docs; arXiv:2603.23710 (CONFIRMED verbatim) | `02` §2.1a/§2.1b, `04` §7 |
| R67 | contract | **vector-engine-split escape hatch** (route a profile/whale to a dedicated engine on SLO breach — market converges on ACORN/Weaviate arXiv:2403.04871 or DiskANN/Cosmos arXiv:2505.05885 <20ms@10M/partition); **whale-promotion FIRST-CLASS** (ClickHouse-PG guide; Qdrant tiered+promotion ~1000-shard cap; Milvus ladder; Notion 480 divisible shards); modulus low-hundreds (postgres.ai 12ms@1000) | vendor docs (CONFIRMED) | `02` §2.1b, `04` §7.0, `25` §7b, `26` |
| R68 | **accuracy (correctness)** | **bitemporal tiebreak = DB-assigned `transaction_from` (DB clock/HLC), NEVER writer wall-clock** (skew → non-deterministic winner, retroactive correction loses to stale generation; DDIA/Lamport; XTDB single-writer); **contradiction = write-time typed contract with keyed audit of the adjudicating LLM judge** (TOKI arXiv:2606.06240: replay-inconsistent without keyed logging; WorldDB arXiv:2604.18478 on content-addressed correctness) | TOKI/WorldDB (CONFIRMED real); HLC canon | `04` §3.1/§3.4 |
| R69 | contract | GC adds a **generation-epoch/mark-window fence** beyond {ref-absence, MIN_AGE} (registry GC deleted live layers mid-sweep #4461/#3254; git 2.weeks grace); restore: row-present/blob-absent = **hard quarantine never serve**; S3/GCS/Azure now strong read+LIST consistency (stale-LIST worry obsolete; no cross-store atomic) | CNCF registry issues + git-gc + cloud docs (CONFIRMED) | `02` §2.3, `14` §4.2 |
| R70 | contract | reflect/reembed: **cap worker concurrency (low hundreds/node)** + monitor `LWLock:MultiXactMemberSLRU`/`MultiXactOffsetSLRU` (EDB 2026-05-04 post-mortem); pgmq work queue; FK FOR-KEY-SHARE MultiXact caveat; no advisory lock in SELECT+LIMIT; **hot-subject escape** (hot-current vs audit-recompute split) | EDB/Microsoft post-mortem + PG advisory-lock docs (CONFIRMED; wait-event names corrected) | `02` §6.2 |
| R71 | **security (provenance insufficient)** | **provenance-only poisoning defense is necessary-NOT-sufficient** (MINJA arXiv:2503.03704 query-only self-generated, clean provenance, 98.2%/76.8% avg; Sybil/Douceur defeats independence gate; eTAMP arXiv:2604.02623 env-only) → add **MemAudit-style causal+structural anomaly layer** (arXiv:2605.23723, 70%→0% post-hoc) + Sybil-resistance assumption (attested/costly actor_id) + dual-guard + high-risk quorum/randomization | MINJA/MemAudit/eTAMP (CONFIRMED IDs+numbers) | `06` §3.2/§4.3 |
| R72 | **security + principle** | **crypto-shred MUST physically purge vectors from the index** (tombstone-filter ≠ erasure — plaintext embeddings invertible to PII, now cross-model/black-box/training-free: Vec2Text arXiv:2310.06816 92%/32-tok; arXiv:2004.00053 50-70%; ALGEN 2502.11308; Zero2Text 2602.01757) + GDPR hedge (key-shred = pseudonymisation per WP216, not settled anonymisation); **kind enum extensible-additive** (arXiv:2602.05665 degenerate-graph); **escape-hatch principle** (every frozen contract → promotion-to-specialized-lane) | inversion literature (CONFIRMED, one number re-attributed to Morris/Vec2Text) | `06` §6.2, `04` §7, `00` §2, `26` |

**Fabricated/overstated specifics the fact-check EXCLUDED (do NOT propagate):** the ">10× slower" HNSW-build multiplier (README says only "significantly more time"); a "~5% selectivity" IVFFlat threshold (arXiv:2602.11443 has no number, and Milvus is an explicit counterexample to "regardless of system"); a "50–100M hard ceiling" (vendor editorializing — TigerData's pgvectorscale actually *won* throughput at 50M, Qdrant won p99); the bare `LWLock:MultiXactSLRU` wait-event (real: `MultiXactMemberSLRU`/`MultiXactOffsetSLRU`); "SMSR" (no such paper — dropped); "OEP" (no locatable arXiv ID — not cited); the 92%/32-token number belongs to **Morris/Vec2Text (2310.06816)**, not Huang's transferable-inversion (2406.10280); GDPR "Guidelines 5/2019" + a verbatim "EDPB erasure" quote (both fabricated — replaced with the WP216-pseudonymisation hedge); "AgeMem" (nickname for Agentic-Memory 2601.01885); MemoryGraft is poison-via-executed-code (2512.16962), not environment-only (that's eTAMP). The Katz "64vCPU/512GB" figure is his **1M-row** test rig, not a 10M requirement. This Round-8 audit is itself fresh proof of the standing invariant: a real arXiv ID does not make the sentence citing it true.

### Round 9 — eight-agent team review + settled-decision relitigation (R73–R84)

Eight parallel agents (context7-docs, web-2026, github-oss, codebase, tests/eval, experimental, devil's-advocate, **fresh-eyes blind redesign**) with explicit permission to relitigate settled decisions. Headline: a blind redesign converged on **~80% of load-bearing decisions** (store, raw-truth invariant, bitemporal supersession, hybrid+RRF, provenance trust, scope tree, license split, eval-first) — the strongest robustness evidence the suite has; the docs-verifier found **zero wrong library claims**; the codebase-checker found **zero behavioral drift**. Re-ratified with fresh eyes: Postgres+pgvector single store; day-one hash partitioning; five kinds; append-only bitemporal; SQLite-rejection. **This is the first round to defer/delete scope, not only add** — the prior rounds' additive-only bias (R1–R72 contain zero scope deletions) is itself diagnosed here.

| R | Class | Decision | Evidence (verified) | Lands in |
|---|---|---|---|---|
| R73 | **scope (overturn)** | **V1 build-scope contradiction resolved to the ladder**: `00-MAIN` §5 "ship the methods from the first build" / `05` §1 "implements all stages" / `27` §2 "rungs 0–12 first-architecture" contradicted `29` §1 doctrine item 4 "store fields, activate behind gates". Resolution: freeze ALL interfaces; **build = rungs 0–3 spine + citations + `correct`/`forget` + REST/MCP/Python SDK**; rung-4+ *behavior* built at its rung activation. V1 cut line + soft calendar envelope own by `29` §2a | devil's-advocate + fresh-eyes independently converged on the same contradiction (both quoting the three passages) | `00-MAIN` §5, `05` §1, `27` §2, `29` §2a, `01`, `10` |
| R74 | accuracy | Library freshness: **pgvector pinned ≥ 0.8.4** (0.8.3/0.8.4, June 2026, fixed HNSW vacuum corruption + maintenance errors — delete-heavy `forget` = direct blast radius); **rmcp 2.x** (2.0.0 2026-06-29; `Json<T>` return wrapper auto-derives `outputSchema` from the canonical type = stated default, `with_output_schema` fallback); **PG 17/18** (native `uuidv7()`, AIO, B-tree skip scan); **pgmq partitioned queues are opt-in** (`create_partitioned` + pg_partman — the "partitioned" adjective corrected); `ltree` GiST `siglen` explicit (default 8 bytes); sqlx `after_release` min-connections caveat | pgvector CHANGELOG / crates.io / PG18 docs / pgmq docs (all fetched; **zero WRONG verdicts** across 20+ claims checked) | `02` §1.3/§2/§2.1a/§6.2/§7, `03` §3, `04` §11.0, `08`, `25` |
| R75 | accuracy | Competitive refresh (GitHub-API + primary-source verified 2026-07-02): **Cognee 1.0 pivoted onto the positioning** (single-Postgres graph+pgvector recommended default, Rust/TS SDKs, remember/recall/improve/forget verbs, COGX export; +4.5k★/wk) = the sharpest collision now; **memvid stalled** (0 commits >5wk) — demoted; **Hindsight is Python-core + already Postgres-backed** (Rust = CLI only; LongMemEval-anchored w/ independent VT+WaPo reproduction; 0.8.2 shipped scrubbing/reversible-curation parity pressure); **Mem0 v3 = ADD-only, graph removed, regression publicly admitted**; **MemPalace (56,868★, MIT) was missing entirely** — with its independent debunking (issue #125: BEAM-100K 49% end-to-end vs 96.6% R@5 headline; arXiv:2604.21284) = the canonical retrieval-metric≠answer-quality case study; **OpenViking (26k★) + honcho = AGPL landmines**; **Letta bifurcation** (learned memory-manager RL direction vs governed substrate — MemPhant = the substrate a learned manager reads/writes through). Rust-substrate lane confirmed OPEN (no Rust competitor >250★) | GitHub API (batched GraphQL) + vendor blogs (labeled) + arXiv | `13`, `12`, `26` §8, `01`, `11`, `16` |
| R76 | security | June-2026 poisoning wave: **SMSR** (arXiv:2606.12703 — certified defense: HMAC-signed writes 93–100%→0% unsigned injection; ablation+voting 8.0% CI[5.8,10.9]; single-author caveat. **Corrects Round 8**: "SMSR (no such paper)" was right *for that report's ID-less citation*; the paper exists at this ID) + **MPBench** (arXiv:2606.04329 — 4 write channels / 9 vulnerabilities / 6 attack classes; aggressive-writes↑exploitability ⇒ restraint is also a security control) + **VMG survey** (arXiv:2604.16548 — "cannot be retrofitted at retrieval or execution time alone… storage-time provenance, versioning, policy-aware retention from the outset"). New issue-verified failure classes: filter/selector injection (cross-tenant bypass — Mem0 #5977/#5976), NaN/Inf embedding dedup poisoning (Graphiti #1505), bitemporal backfill (Graphiti #1489), singleton-entity recall (Graphiti #1627), partial-batch silent loss (Mem0 #5245), silent-empty-extraction (Mem0 #5903), ack-without-persist (Graphiti #1574) | arXiv abstracts fetched by lead (fabrication-history claims re-verified); GitHub issues | `06`, `12`, `14`, `05` §10, `02` §5.2, `08` |
| R77 | **freeze** | **Outcome Ledger**: new public verb **`mark`** `{trace_id, used_ids[], outcome: success\|failure\|corrected\|ignored}` — defines the until-now producer-less `outcome_label` trace field; feeds `review_event` **grades** and per-unit utility. **`review_event` rows are captured from day one** (append-only inserts — outcome labels cannot be backfilled, every unlabeled day is training data destroyed); only the FOLD/decay engine is rung-11 (R82). Convergence: the experimentalist ranked this #1 and the tests-auditor independently flagged `outcome_label` as a dangling concept | two-agent independent convergence; rung-13/FSRS data floors all queue behind it | `08`, `05` §3.1, `04` §8.2, `20`, `22`, `06`, `12`, `27` |
| R78 | freeze | **Consolidation event taxonomy reserved-with-shape** (`memory.promoted\|superseded\|contradiction_detected\|quarantined`, `reflect.completed`) + transactional-outbox delivery design; **poll-cursor first, webhooks later, build post-v1** — upgrades pass-12's name-only reservation because integrators need push-shape typing before SDKs calcify (fresh-eyes gap: the suite was pull-only) | fresh-eyes MISSING #1; pass-12 additive backlog | `08`, `20` |
| R79 | contract | **File-memory compatibility adapter**: project the typed store as a `memory_20250818`-compatible virtual filesystem (Anthropic's six file commands, GA; OpenAI Agents SDK converged on the same file metaphor) — **one adapter to a platform convention, not a framework matrix** (the `26` non-goal is a *wide* matrix; this is the single de-facto interface). Answers the local-first wedge without a second store | Anthropic/OpenAI docs fetched (CONFIRMED) | `08`, `26`, `01`, `13` |
| R80 | data-gated | **Delta recall** (`recall(delta_since: trace_id)` — bitemporal diff; count-only absence so forget can never resurrect), **demand-paged re-extraction** (`reextract_on_miss` keyed `(episode_id, query_features_hash, compiler_version)` — invariant #1's largest un-cashed payoff), **retrievability probe** (post-promotion synthetic findability check — catches canonicalizer drift) — all as **rung-gated levers**: freeze the flags/trace fields/job rows now, build behavior at rung. Five further candidate levers (counterfactual replay, churn hazard, procedure canaries, co-recall edges, open-question units, actor reliability, evolution recall, trace-to-golden) recorded in §2.3 below — retrofittable, NOT specced | experimentalist top-3 + codex plan-review surface-reduction | `08`, `05`, `04`, `02` §5.1, `27` §4 |
| R81 | **accuracy (methodology)** | Eval-format executability: **`expect_units` symbolic→derived binding block** (golden `mem_*` names had NO resolution mechanism to runtime UUIDv7); `answer_bearing_ids` added to the YAML format; `verify-golden` failure criterion made mechanical; `cluster_key` source field; **record-replay extraction mode for PR goldens** (recorded per `(fixture_version, compiler_version)` + pinned local embedder; live derivation nightly) — fixes the "~$0 PR gate" contradiction (derivation goldens invoke reflect's LLM) AND the golden-determinism claim; pass^5 = 5× in the `12` §11 cost model; whole-corpus verify-golden → nightly; PR-gate command convergence (`05` §4.1 = `29` §4); `27` §5 pyramid gains profile + verify-golden rows | tests-auditor (builder-blocking defects, anchors verified) | `05` §4, `03` §6.2, `12` §11, `27` §5, `22` |
| R82 | accuracy | **Rung-11 FSRS gate self-violation fixed**: the gate cited MemoryStress, which `12` itself labels vendor-authored — reworded to an **internally-run MemoryStress-style longitudinal suite in MemPhant's own harness** (running the corpus ourselves = internally measured; anchoring to the vendor's leaderboard number stays forbidden). V1 decay scoring = plain recency/exponential (the shipped-field baseline); FSRS fields stay frozen; ledger capture day-one (R77); fold at rung 11. fsrs crate v6 note: native per-card desired retention | devil's-advocate exhibit 3 (internal contradiction); fsrs-rs v6.6.1 | `27` rung 11, `04` §8 |
| R83 | contract | **Rust-first RETAINED with recorded preconditions**: (a) WS-0 adds a **two-language spike** exit criterion (retain + golden-runner in both; measure wall-clock to change an extraction policy end-to-end); (b) **iteration-loop rule** — no accuracy-critical iteration (prompts, fusion weights, thresholds, judge policies) may require a Rust recompile (all are data/config behind `compiler_version`); (c) the team-Rust-fluency assumption is now explicit. Both attackers agreed the four claimed Rust wins are real-but-modest while the velocity cost was unpriced | devil's-advocate A7 + fresh-eyes B1 (independent convergence) | `26`, `03` §0.1, `29` WS-0 |
| R84 | guards | **Validator coverage**: ~9 of 18 frozen interfaces had no required-snippet guard (eval-case `answer_bearing_ids`, append-only bitemporal, `scope_policy`, tenant residency, delete path, memory identity, resource chunk, evidence path, migration ledger) — guard tuples added with verified anchors; `PARTITION BY HASH` guard extended to the owning doc (`04` §7.0, not only `00`); cross-ref checker extended to alphanumeric sections (`§2a`); memphant gates made discoverable in root `TESTS.md` | tests-auditor rule audit (gates were green but half-blind) | `scripts/validate_docs.py`, `backend/tests/scripts/`, `TESTS.md` |

**Fabrication tripwires this round (do NOT propagate):** "pgvector 0.9" (SEO-blog invention — changelog shows 0.8.4 latest, 0.8.5 unreleased); the BEAM "64.1" **number collision** (Hindsight claims 64.1 @10M while Mem0 claims 64.1 @1M — same number, different scale/system; all circulating BEAM scores are vendor-self-reported); MemPalace's own "100% LoCoMo" (structurally guaranteed — top-k ≥ corpus size). **Honest negatives (searched, found nothing):** no FSRS-for-agent-memory validation or refutation appeared (the decay bet remains externally unproven — R26 stands); no evidence against pgvector at MemPhant's scale envelope; no new Rust-first competitor.

### Round 10 — "Unified Memory OS" external report + landscape blindness fix + hosted-runtime call (R85–R93)

Eight agents validated a second external report (~97% accurate on checkable claims — the most accurate input yet; author scraped live GitHub pages) + a consensus-attacking devil's advocate + an outside-voice synthesis review. Component verdict: spec stronger on 24/29 (the proposal has no transaction time, an in-place reconcile that is exactly the pre-R57 bug, farmable α/β counters, ACL-by-score, and specs its cold tier on PQ — which pgvector does not have). The round's real finding was **frame-level, not component-level**: the spec suite contained ZERO mentions of the mega-harness memory layer the pass verified (OpenClaw 381k★ builtin, Hermes 208k★ provider SPI, Claude Code auto-memory on-by-default) — the process had component rigor and landscape blindness.

| R | Class | Decision | Evidence (verified) | Lands in |
|---|---|---|---|---|
| R85 | **process (calibration)** | **The "implausible star count" fabrication heuristic is DEAD** — agent-tooling stars now run 100k–380k★ (OpenClaw 381,443★ API-verified); verify via API, never plausibility. **Landscape-completeness rule**: any ≥50k★ project verified in a review pass is listed in `13` or gets a recorded one-line exclusion | GitHub API 2026-07-02 (many counts matched EXACTLY — scraped, not invented) | `13` §1.1/§1.4 |
| R86 | contract | **`26` §7 gains a second reopen test: distribution evidence** ("the adoption target is unreachable through specced channels") — a distribution gap can never produce a benchmark trace, so the benchmark-only clause made adoption holes structurally unfixable | devil's-advocate structural finding (Exhibit A) | `26` §7 |
| R87 | contract | **Hermes memory-provider adapter SPECCED at an activation gate** (`08` §5.1b) — the second platform-convention adapter under the R79 rule (a named SPI with 8 extant providers, six of them our mapped competitors). **NOT frozen** (an unbuilt adapter over an SPI Hermes can drift at will has zero retrofit cost — freeze-only-what's-painful). Capture-reliability logic recorded: `08` §4.2's determinism principle applies to capture — auto-capture belongs below the tool layer; MCP-only is model-dependent where the spec demands structure. Direction distinction: storage SPIs *below* MemPhant stay rejected; provider adapters *above* are the R79 lane | NousResearch/hermes-agent verified (207,822★; plugins/memory/ = 8 providers) | `08` §5.1b, `26`, `03`, `09` |
| R88 | **adapt (overturned mid-pass)** | **ONE pinned block per scope** — content-editable, hard Stage-7 token sub-budget, never silently dropped (explicit labeled truncation), trust-capped (data only; never high-risk-eligible; never corroboration), append-only versioned + trust_event audited, scope-`forget` clears it, OP-Bench-gated. REPLACES the same-pass `scope_pin` (≤16 order-only refs) ruling: order-only pins can silently vanish (broken promise) and 16 guaranteed refs recreate the OP-Bench harm; production evidence (Syndai's bounded editable persona block) says ONE block is the real Letta-analog job. `materialize`-verb + sixth-kind rejections stand | fresh-eyes production grounding + devil's-advocate overturn | `04` §12, `05` §1.2, `08` §3–4, `07`, `19`, `28` |
| R89 | data-gated | **Composition gets a `27` rung** (reflect-stage abstraction inference emitting belief candidates, `derived_by: composition` additive `05` §3.1 trace field; advance = corroborated-promotion precision + no OP-Bench regression; disable = over-personalization signal) — an unrunged feature can never activate; Syndai ships the precedent in production (typed behavioral inference, guardrailed persona evolution) | backend grounding (23 live behavioral rows; `persona_evolution_auto` guardrails) | `27`, `24` §2.3, `05` §3.1, `07` |
| R90 | accuracy | **GateMem** (arXiv:2606.18829 — "no method simultaneously achieves strong utility, robust access control, and reliable forgetting") joins the `12` portfolio as the multi-agent-governance axis; its `27` launch-gate row is **conditional on first successful internal reproduction** (the Zep-58.44% lesson: a benchmark gates nothing until reproduced — especially a flattering one) | arXiv + repo verified | `12`, `27` |
| R91 | absorptions | From the proposal, kept: **`GET /v1/scopes/{id}/stats`** (read-only, gate-respecting; inspector/quota/integrator consumers); **procedural/how-to query-signal fusion row**; **Beta-posterior fold shape as an ablation arm** (over the deduped independence-collapsed ledger only — as raw counters it is farmable + double-applies); **default category/churn→`desired_retention` prior table**. CUT: the `task_context` advisory hint (an unfalsifiable knob `08` §3.0's own no-escape-hatch discipline refuses) | arch-comparator + outside-voice narrowing | `08` §2, `05` §1.2/§9, `04` §5.1a/§8.1, `03`, `09` |
| R92 | accuracy | Positioning/premise fixes: `01` §3's "free baseline = agent + flat files" is STALE (OpenClaw's builtin ships hybrid search + active recall + structured claims w/ contradiction tracking, free and default — the free baseline is climbing the wedge); plug-points named exactly (Claude file-tool shipped; Hermes SPI at R87; OpenClaw: none — recorded non-target); one positioning sentence ("one governed memory ledger, many surfaces"); Cognee 1.0 date fixed (v1.0.0 tag 2026-04-11; 2026-06-26 = announcement + v1.2.2); `07`/`28` cutover corrections (decay row names BOTH live Syndai mechanisms; trajectory/failure/persona layer dispositions; the ≤2,500-token cutover acceptance threshold) | claims-verifier + repo-verifier + backend grounding | `01` §3, `frame.md`, `13`, `07`, `28` |

| R93 | contract | **Hosted runtime = full backend on Fly Machines** (same static binary as self-host: `memphant-server` + `memphant-worker` process groups, bluegreen, doppler-run boot — Syndai-proven ops); **Supabase = Postgres + Storage only, never compute; Edge Functions REJECTED for core** (Deno isolates cannot run the Rust core, hold reflect advisory leases, run pgmq/Temporal workers or `spawn_blocking` pools, or serve stateful MCP sessions; an edge layer forks invariant #11's one-binary contract). The only edge component is the thin `fly-replay` router | architecture-structural (no external verification needed); ratifies what `25` §7b's cell definition already implied | `25` §7b, `26` |

**Answers-of-record (the report's six open questions — so Pass 17 never relitigates):** (1) truth model — bitemporal generations + deterministic contract, and the proposal's own schema (no transaction time, in-place reconcile) cannot build its stated ledger; (2) aging — event-sourced confidence/DSR + active freshness (the piece a decay formula cannot do); (3) importance — decomposition beats a stored scalar (farmable); consequence = protected categories + `arg_risk` + retention priors; (4) composition — the one real gap → R89; (5) multi-agent — `scope_policy` structural + GateMem as the public metric (R90); (6) six-month evaluation — `mark` day-one capture + STATE-Bench attribution + the new `memory_utility_trend` SLI (`22`). Executable memory: REJECTED (auto-execution turns poisoning into persistent RCE; the safe subset — procedures-with-preconditions, `trusted_user` preference facts, outbox-hooks-the-runtime-acts-on — is named in `04` §4). "Memory OS": positioning sentence adopted; multi-backend SPI + the name rejected.

### 2.3 Candidate-Lever Register (recorded, NOT specced — retrofittable, YAGNI until evidence)

One line each; every lever is fully additive post-launch (no frozen-interface exposure), so speccing now would be scope creep. Source: Round-9 experimental agent; each carries its falsifiable metric in the session archive.

- **Counterfactual pack replay** — re-run fusion/pack/gate variants offline against archived traces (stages 5–7 are pure CPU over trace data); promote only if replay deltas predict live deltas.
- **Learned churn hazard** — per-predicate supersession survival curves (Kaplan-Meier) set `freshness_due_at`; fleet prior from content-free statistics.
- **Procedure canaries** — validated procedures `depends_on`-link their precondition facts; run outcomes confirm/flag those facts (procedures as distributed staleness sensors).
- **Co-recall edges** — Hebbian `co_recalled` edge mined from positive-outcome packs; must beat the rung-6 no-edges AND filesystem controls like any edge.
- **Open-question units** — repeated abstentions on one subject mint a structured known-unknown the runtime may surface once.
- **Empirical actor reliability** — per-actor supersession/correction history recalibrates `actor_factor` within its class ceiling (Sybil-economics complement).
- **Belief-evolution recall** — a typed generation-history projection may return the chain narrative for "how did this change" queries; it must not overload the two-axis recall timestamps with a third clock-selector mode.
- **Trace-to-golden distillation** — production miss + later corrective signal drafts golden YAML (redacted, second-author confirmed).
- **Inferred-belief composition** (PROMOTED to a `27` rung by R89 — mechanism of record): reflect-stage abstraction inference mints `kind=belief` units `derived_from` ≥N source units; definitionally single-source (the inferring extractor is the actor) so `agent_output` trust ceiling applies; promotion to semantic requires an independent DIRECT observation of the inferred claim itself, never the sources' independence; **supersession back-walk** (the one new mechanism): when a source generation closes, dependent inferred units are flagged for re-derivation/expiry in the next reflect; composition reads only sources ≥ a trust floor (compose-innocuous-beliefs-into-harmful-preference is the new amplifier surface; belief-tier containment bounds it).

## 2.1 Research Watchlist

Track but do not blindly implement:

- FSRS/open-spaced-repetition updates
- MemCog-style proactive traversal
- ReasoningBank-style strategy distillation
- MemMachine-style episode contextualization
- Hindsight-style hybrid recall scorecards
- Graphiti/Zep-style temporal graph facts
- OWASP memory contract updates

Each item must map to a failing trace/eval before becoming implementation work.

## 3. Integrity Hardening

Implemented in the first public scorecard pipeline:

- benchmark contamination checks — **distinct from saturation.** Saturation (everyone scores high because the benchmark is easy/old) is self-evident in the score distribution; contamination (the benchmark leaked into training/eval data) is not, and must be probed externally (rephrase-gap, held-out private subset divergence). For a *memory* benchmark there is an extra contamination surface: MemPhant's own internal golden cases are derived from real Syndai memory, which can overlap public corpora — keep a private held-out split that never appears in published golden sets.
- held-out private subsets
- judge drift checks (versioned against a frozen anchor set)
- confidence intervals — bootstrap, ≥1,000 resamples; a sampled eval is "stable" only below a declared CI half-width (R20)
- paired tests across configs for every lever-promotion decision (paired-delta CI must exclude zero)
- exposure tracking for adaptive evals
- a benchmark-integrity **state machine** with explicit transition triggers (`12` §7): ACTIVE→FLAGGED (contamination/divergence probe fires) → DOWN-WEIGHTED (degraded-but-real) or FROZEN (validity broken) → RETIRED, always with a public call-out + recompute + disclosed delta, never a silent mutation. A benchmark is never silently removed from the scorecard.

## 4. Security Hardening

Implemented in the hosted security plan:

- per-user DEK ← per-tenant KEK envelope (crypto-shred granularity = user; `06` §6.1.1/§6.2)
- admin access approval
- signed trace exports
- tamper-evident audit chain
- enterprise policy packs

## 5. Product Hardening

Product additions are fixed as:

- file-based importers from competitor memory systems: yes
- framework adapters: no, use API/SDK/MCP/cookbooks
- visual trace diff: yes, in trace explorer
- hosted eval scheduler: yes, for hosted dashboard
- memory quality recommendations: yes, generated from trace/eval failures with human review

## 6. Methodology Change Gate

A methodology change can ship when:

- it has a failing case
- it improves sampled evals
- it does not weaken tenant/security tests
- cost/latency impact is measured
- docs and public claims are updated

## 7. What Levers To Pull

When a benchmark misses:

| Trace symptom | Pull |
|---|---|
| answer never candidates | chunking, lexical/vector/entity extraction |
| answer candidates but low rank | fusion weights, decay, rerank |
| answer in context but model fails | context packing, citation wording, downstream prompt |
| stale answer wins | bitemporal validity, supersession, recency |
| poisoning wins | trust classification, quarantine, high-risk suppression |
| latency high | candidate caps, index shape, skip rerank/L4 |
| cost high | batch extraction, reduce external calls, sampled eval |

This is why the trace schema is mandatory before expensive benchmark runs.

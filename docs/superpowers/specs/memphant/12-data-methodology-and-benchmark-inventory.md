# MemPhant - Data Methodology and Benchmark Inventory

## 0. Methodology Doctrine

MemPhant scores memory systems by evidence retrieval and downstream usefulness, not by vibes.

Memory evaluation is not long-context evaluation. A memory benchmark must exercise write, update, retrieval, correction, and persistence over time. A long prompt with a needle is useful for context-window pressure, but it is not sufficient evidence that an agent memory system works.

Every eval row records:

- dataset/version
- query/task ID
- memory corpus ID
- feature flags
- candidate trace
- citations returned
- answer/judge result where applicable
- p50/p95 latency
- token/context cost
- storage footprint
- poisoning/security outcome where applicable

## 1. Evidence Tiers

| Tier | Meaning |
|---|---|
| `unit` | Deterministic tests for policy/scoring math. |
| `golden-internal` | Hand-curated Syndai/MemPhant cases. |
| `sampled-public` | Small reproducible subset of a public benchmark. |
| `full-public` | Full public benchmark run. |
| `heldout-private` | Private eval corpus, aggregate results only. |
| `adversarial` | Poisoning, leakage, deletion, or isolation eval. |

## 2. Seed Benchmarks

Full inventory — measures, license/ToS feasibility, run cadence, and known issues per benchmark (re-verify all external figures at ingestion):

| Benchmark | Primary source | Measures | License / ingest | Cadence | Known issues |
|---|---|---|---|---|---|
| **STATE-Bench** *(primary target)* | Microsoft, opensource.microsoft.com (May 2026) | task completion, **pass^5** reliability, efficiency, 1–5 UX; memory-agnostic ("bring your own memory") | open, MIT-style | weekly/release | **no published memory-system SOTA yet** → MemPhant's best neutral *first* SOTA claim; leaderboard **live + empty** as of 2026-07-02 (submission columns: pass@1, pass^5, UX, Cost/Task) — the first credible submission takes the visible slot |
| **LongMemEval-V2** | arxiv 2605.12493 | 5 memory abilities over ≤500 trajectories / 115M tokens; accuracy + latency | open dataset + repo | weekly/release | hard; in-paper best ~72.5% (AgentRunbook-C); leaderboard **live + empty** as of 2026-07-02 ("entries coming soon" on both tiers; author baseline AgentRunbook-C 74.9% on Small; scoring = "LAFS Gain" vs the accuracy-latency frontier) — the first credible submission takes the visible slot |
| **BEAM** | **arxiv 2510.27246** ("Beyond a Million Tokens") | 100K/1M/10M tiers, 10 abilities, accuracy/speed/context | open paper; board is vendor-run | release (sampled nightly) | the `agentmemorybenchmark.ai` board is **Vectorize-operated** → `source_status: vendor_reported`, never an anchor; **no neutral leaderboard exists** — live vendor number collision (2026-07-02): Hindsight claims #1 on BEAM-**10M** at 64.1% (their Apr 2 post) while Mem0's July 1 report claims 64.1 on BEAM-**1M** (and 48.6 @10M) — same number, different scale and system; every circulating BEAM score is vendor-self-reported |
| **MemMachine / ReasoningBank** | arxiv 2604.04853 / 2509.25140 | ground-truth preservation; retrieval-stage micro-levers (`05` §1.4); strategy distillation (k=1 optimal retrieval) | open | comparability | architectural twins, track for comparability not SOTA |
| **MemoryStress** *(longitudinal)* | OMEGA, HF `singularityjason/memorystress` | 1,000 sessions / 10 sim-months / 40 contradiction chains; **decay-degradation curve, cold-start, contradiction-chain accuracy** | Apache-2.0 | nightly/weekly | the eval that exercises decay (`04` §8 FSRS ablation target) + chained contradiction detection (`04` §3.1); vendor-authored, so a self-designed benchmark — treat its scores as `vendor_reported` |
| **LoCoMo / LongMemEval-S / PersonaMem / LifeBench** | respective papers | conversational/persona recall | open | baseline | **saturating** on easy categories; classic LongMemEval near-contaminated — baseline only |
| **AgentDojo + OWASP Agent Memory Guard fixtures** | NeurIPS 2024 / OWASP incubator | tool-call injection (AgentDojo); memory-poisoning controls (AMG) | open | adversarial | AgentDojo **near-saturated** + tests tool-call-time injection, **not persistent memory poisoning**; pair with MemPhant's own corroboration-farming suite |
| **MPBench** *(memory poisoning)* | arXiv:2606.04329 ("From Untrusted Input to Trusted Memory") | four memory write channels × nine structural vulnerabilities; taxonomy of six memory-poisoning attack classes | open | adversarial | covers the persistent-memory-poisoning gap AgentDojo misses; key findings: prompt-injection defenses do **not** transfer to memory poisoning, and more aggressive write/retrieval policies increase exploitability — restraint/admission control is also a security control (`06` §9) |
| **Internal Syndai memory golden cases** | this repo | real scoping/forget/correction bugs | internal | every PR | must be generalized before entering public core; keep a private held-out split (contamination surface, §7) |

## 2.0 The Benchmark Portfolio (there is no single source of truth)

There is **no one leaderboard that settles "memory SOTA"** — every recent system stakes out a *different axis*, so SOTA is a **profile across axes**, not a number (the same "never a bare number" discipline as the scorecard, §4). LoCoMo/LongMemEval-v1 are the shared currency but cover only the conversational-QA slice and are saturating. MemPhant's portfolio: **one benchmark per axis**, and a claim must state which axes it leads.

| Axis (what it uniquely measures) | Benchmark | Why it's in the portfolio |
|---|---|---|
| **Outcome** — does memory make the agent reliably *better* at stateful work | **STATE-Bench** *(primary)* | the only neutral, outcome-based, empty-leaderboard target (§2; still empty as of 2026-07-02) |
| **Long-horizon accuracy + latency frontier** | **LongMemEval-V2** (arXiv:2605.12493, LAFS) | 115M-token corpora / 200K reader budget; near-zero no-retrieval baseline proves memory is load-bearing; scores efficiency, not just accuracy |
| **Scale** | **BEAM** (arXiv:2510.27246) | 100K→10M tokens; retrieval-quality-at-scale stress |
| **Longitudinal degradation** | **MemoryStress** | 1,000 sessions / decay curve / contradiction chains — exercises decay (`04` §8) |
| **Restraint / safety** | **OP-Bench** (2601.13722) + **PS-Bench** (2601.17887) | over-personalization (26.2–61.1% drops) + intent-legitimation attack surface — the axis recall-focused benchmarks *miss* (`05` §1.5, `06` §9) |
| **Multi-principal governance** — utility + access control + reliable forgetting under shared memory | **GateMem** (arXiv:2606.18829, "Benchmarking Memory Governance in Multi-Principal Shared-Memory Agents"; repo `rzhub/GateMem`, 132★, MIT) | verified finding: "no method simultaneously achieves strong utility, robust access control, and reliable forgetting" — the MemPhant wedge stated as an open problem by a neutral benchmark. **Reproduce-first:** gates nothing (and stays outside the §2.0a profile runner) until first successful internal reproduction in MemPhant's harness; `27` owns the conditional launch-gate row |
| **Interactive episodic formation** | **EMemBench** (arXiv:2601.16690) | memory built *during* live interaction (VLM game agents), not static logs — a genuine gap vs MemPhant's conversational focus |
| **Embedding selection** | **LMEB** (arXiv:2603.12572) | which embedding model actually serves memory retrieval (MTEB is anti-correlated, `02` §2.1a) |
| **Procedural / skill memory** | **SkillOS** (arXiv:2605.06614) / MUSE-Autoskill | curation/reuse/self-evolution of skills — orthogonal to factual recall (`04` §4) |
| **Systems cost / footprint** | VikingMem-style / TrueMemory's SQLite-only footprint | deployability, not just offline accuracy |
| **Coding-agent workload** | internal Syndai golden cases + `28` §4 cutover fixtures | the first paying workload; no neutral public benchmark covers it yet |

**Harness-comparability caveat (binding):** cross-system LoCoMo/LME scores are **not** comparable — swapping only the answer model moved one system 74.4%→84.5% (Continua "Fair Fight", ~10pt from the model alone). So MemPhant runs every baseline *in its own harness* with a pinned answer model; numbers read off another system's leaderboard are `vendor_reported`, never a comparison (`05` §8). **Open gap:** the restraint axis (OP-Bench/PS-Bench) is the one none of the accuracy benchmarks cover and the one most teams skip — MemPhant treats it as a launch gate, not an afterthought. **Cross-scenario variance is a reported metric** alongside the per-axis scores: AutoMEM (arXiv:2606.04315) found an agentic harness managing flat text files through tool calls outranked eight memory systems, and rankings flip across scenarios — a system that wins one axis and craters another is reported as exactly that, and the `filesystem` control baseline (§2.0a) stays in every profile. **Coding-agent caveat:** the public portfolio is conversational-heavy while the first paying workload (Syndai) is coding-agent memory — that axis runs at `golden-internal` tier, derived from the `28` §4 fixture families, until a neutral public coding-agent memory benchmark exists.

### 2.0a The Profile Runner (one script → the whole 9-axis profile)

The portfolio is run by a **single `memphant-eval` subcommand** — the orchestrator over the existing `run`/`ablate`/`compare --paired`/`security`/`verify-golden` lanes (`03` §6):

```bash
memphant-eval profile --config memphant.lock --compare-to baselines/release.yaml --archive-traces
# confirm a candidate component (the model-currency loop, 22 §4.3) — pin one swap, hold the rest fixed:
memphant-eval profile --config memphant.lock --swap embedding=<model-id> --compare-to <incumbent-id>
```

`--swap <kind>=<id>` pins a candidate `embedding`/`reranker` and holds every other harness input fixed, so the resulting delta isolates that one component (the confirm-via-our-eval step, `07` §10 / `22` §4.3).

It runs each axis above and emits **one artifact — the SOTA profile** (axis keys match the portfolio table; SOTA is the *profile shape*, never a single number):

```jsonc
{ "profile_version": "...", "config_hash": "...",
  "harness_pin": { "answer_model": "...", "embedding_profile": "...", "reranker": "..." },
  "axes": {
    "outcome":            { "benchmark": "state-bench", "metric": "pass^5", "score": 0.41,
                            "ci": [0.37, 0.45], "delta_vs_baseline": 0.03, "source_status": "independently_reproduced", "trace_ref": "..." },
    "long_horizon":       { "benchmark": "lme-v2", "metric": "LAFS_gain", "score": 0.12, "ci": [0.08, 0.16], ... },
    "scale":              { "benchmark": "beam-10m", "metric": "accuracy", ... },
    "longitudinal":       { "benchmark": "memorystress", "metric": "degradation_auc", ... },
    "restraint":          { "benchmark": "op-bench", "metric": "rel_drop_vs_memfree", "score": -0.08, "gate": "pass(<0.15)", ... },
    "interactive":        { "benchmark": "emembench", ... },
    "embedding_selection":{ "benchmark": "lmeb", "metric": "mean_n@10", ... },
    "procedural":         { "benchmark": "skillos", ... },
    "systems_cost":       { "metric": "p95_ms / $per_1k_recalls / index_bytes", ... } },
  "control_baselines": { "no_edges": { "delta_vs_full": ... }, "filesystem": { "delta_vs_full": ..., "context_tokens_delta_vs_full": ... } } }
```

The `filesystem` control arm additionally reports `context_tokens_delta_vs_full`: the naive-file arm already runs in every profile, so token-efficiency-vs-baseline is a reported, measured metric — report the measured delta, never a pre-committed percentage. Every axis is pinned to the same `harness_pin` (the binding caveat above), so the profile is internally comparable even though cross-system numbers are not. The profile feeds two consumers: the **SOTA ladder** (`27` — each rung's advance/disable reads `axes.<x>.delta_vs_baseline` + `ci`) and the **model-currency loop** (`22` §4.3 — swapping an embedding/reranker re-runs the affected axes and gates on the same profile).

## 2.1 Repo-Derived Eval Signals

| Source | Useful signal | Caveat |
|---|---|---|
| Graphiti evals | temporal graph and LongMemEval-style memory patterns | graph-heavy assumptions should not force MemPhant graph DB default |
| Hindsight benchmark/dev packages | hybrid recall and BEAM-style reporting | vendor-published numbers require reproduction |
| Cognee eval framework | graph/vector control-plane eval shapes | broad control-plane scope can overcomplicate MemPhant |
| gbrain fixtures/docs | local context attribution and MCP docs | local-first coding memory differs from hosted production memory |
| Syndai golden cases | real L0/L1+, citation, correction, forget regressions | must be generalized before entering public core |

## 3.0 Authoring Provenance and Anti-Triviality

Golden cases have three provenances, in trust order: **`mined`** (a real Syndai memory bug — already failed in production once; the spine; generalize + hold out the original, §7 contamination surface), **`derived`** (a public-benchmark failure category MemPhant missed, re-authored paraphrased + structurally distinct), **`synthetic`** (hand-templated to isolate one lever — must trace to a named lever, §6, never invented difficulty). A purely-synthetic family with no mined/derived sibling is a YAGNI smell — flag in review; a new `mined` case lands **failing first** (red baseline), then the fix flips it green, so it's never authored post-hoc to ratify whatever the code already does.

**Anti-triviality rule:** a case is rejected at authoring if the **FTS-only ablation arm (§5) solves it** — that proves the answer was a keyword match, not a memory-quality test. Every `answer_bearing` case must survive a **query paraphrase sharing no content word with the answer-bearing unit** and still resolve; `verify-golden` runs the FTS-arm and the no-shared-token paraphrase as authoring gates (on top of the answer-bearing-removal gate, `05` §4.0). Decoy units are **same-subject near-dups**, so the discriminator is recency/trust/edge state, never vocabulary; the seed carries ≥1 same-subject distractor per answer-bearing unit (the utilization golden's 8 decoys is the floor).

## 3. Internal Golden Case Families

- exact preference recall
- stale preference replacement
- cross-user leakage
- child-agent isolation
- project-scoped recall
- correction supersession
- delete/forget completeness
- low-trust web poisoning
- tool-output memory poisoning
- resource evidence recall
- failure-pattern recall
- procedure promotion replay
- longitudinal update-chain supersession (5+ generations, value-revisit dedup trap)
- coding continuity: arch-decision honored / compaction rehydrate / positive cross-agent transfer / task+semantic composite (`28` §4)
- mixed-corpus recall (≥4 source_kinds; no single-kind domination via source_kind_distribution_in_top_k)

This list is the index; the full family specs live in `05` §4.2 and `28` §4.

Each case must include:

- seed corpus
- query
- expected top-k memory IDs
- expected citations
- forbidden memory IDs/text
- scope/agent policy assertions
- trace assertions
- fixture version

Cases that only validate shape/count do not protect memory quality.

## 4. Scorecard Format

Do not publish a single magic score.

Publish:

```text
dataset
config
accuracy
recall@k
citation validity
p50/p95 latency
context tokens
estimated cost
storage size
poisoning ISR/ASR where relevant
```

## 5. Ablation Matrix

Minimum public ablations:

| Config | Purpose |
|---|---|
| FTS only | lexical baseline |
| vector only | semantic baseline |
| FTS + vector + RRF | default hybrid |
| default + contextual chunks | write-path value |
| default + rerank | reranker value |
| default + trust gates | security impact |
| default + L4 Deep | accuracy-latency frontier |

## 6. Data Handling Rules

- Do not ingest private benchmark corpora into the public repo.
- Do not publish held-out task text if it enables gaming.
- Publish enough config for independent reproduction on public datasets.
- Keep raw eval traces available internally for audit.
- Mark vendor-reported and self-reported scores as such.
- Version-fabrication tripwire: SEO blogs circulate "pgvector 0.9 (sparse improvements)" — the pgvector changelog's latest release is 0.8.4; 0.9 does not exist. Verify any dependency-version claim against the primary changelog before it enters a spec; the vendor-reported rule applies to version claims too.

## 7. Benchmark Integrity

A benchmark moves through an explicit **state machine** — each transition needs a named trigger and recorded evidence, never an editorial judgment:

| From → To | Trigger (evidence required) | Public action |
|---|---|---|
| `active → flagged` | a contamination/divergence probe fires (rephrase-gap, held-out-subset divergence, judge-drift) | flag visible; evidence attached |
| `flagged → down_weighted` | confirmed *degraded but real* (e.g. partial saturation) | reduce weight; recompute + disclose delta |
| `flagged → frozen` | validity broken (dataset leak, judge broken) | stop scoring; recompute without it; disclose delta |
| `down_weighted → retired` | superseded by a benchmark covering the same abilities (§10) | remove on a published schedule, ability coverage inherited first |
| any `→ active` | re-validation passes | restore with disclosed history |

Transition reasons (controlled vocabulary): `contamination`, `saturation`, `broken_judge`, `dataset_leak`, `no_longer_tests_memory`, `cost_latency_incomparable`.

**Contamination ≠ saturation** (the distinction the integrity engine must keep — `24` R21): saturation shows up in the score distribution and is self-evident; contamination does not and is caught only by external probes. For memory benchmarks there is an extra surface — internal golden cases derived from real Syndai memory can overlap public corpora, so the held-out split must never appear in any published golden set.

Never silently remove a benchmark from a public scorecard. Publish the delta, recompute, renormalize.

## 8. Public Scorecard Rules

A public scorecard row contains:

```text
benchmark_id
benchmark_version
memphant_version
engine_version
trace_schema_version
model
embedding_model
reranker
corpus_size
feature_flags
accuracy
confidence_interval
recall_at_k
citation_validity
latency_p50_ms
latency_p95_ms
context_tokens
cost_micros
storage_bytes
source_status
```

`source_status` is one of:

```text
independently_reproduced
vendor_reproduced
vendor_reported
self_reported
internal_only
```

Vendor/self-reported results can be displayed, but cannot anchor SOTA claims.

**Retrieval R@k ≠ answer quality — never headline R@k.** Canonical case study: MemPalace issue #125 (`13` §1.2) — vendor-reported LongMemEval 96.6% R@5, yet an independent end-to-end eval measured BEAM-100K **answer quality** at 49.0% using raw ChromaDB k=10, with every proprietary mode scoring **below** that raw baseline (26–28%), and a LoCoMo "100%" structurally guaranteed by top-k ≥ corpus size (the §12 `non_discriminating` footgun). A public row headlines an answer/judge metric; `recall_at_k` is reported alongside, never as the headline.

**Vendor-attribution traps (verified 2026-07-02).** The Zep-Cloud-vs-Graphiti-OSS trap: Zep's docs claim "94.7% LoCoMo @155ms / 90.2% LongMemEval @162ms" for the **managed cloud** (vendor_reported; zero harness/judge/hardware description), while the Graphiti OSS README claims only "typically sub-second" — never attribute a managed-cloud number to the OSS artifact. The digit trap: Mem0 LongMemEval 94.8 (README) and Zep DMR 94.8 (paper) are the **same number on different benchmarks by different vendors** — both correctly attributed; do not "fix" one into the other.

## 9. Benchmark Ladder Policy

| Gate | Runs | Promotion condition |
|---|---|---|
| PR | unit/property/golden retrieval | contract gates stay green |
| Nightly | sampled public + security suite + ablations | trend is stable |
| Weekly | larger sampled external benchmarks | subset predicts release delta |
| Release | full external + archived traces | public claim allowed if caveats satisfied |

Do not wait to build the whole system before testing. Build cheap fixtures immediately and use expensive benchmarks only after the trace ladder says a change is worth measuring.

The concrete ladder, baseline reproduction order, statistical rules, and trace archive contract live in `27-sota-ladder-and-validation.md`.

### 9.1 Sampled Subset Construction

A "cheap subset" is **stratified by ability (§10), not random** — uniform random over-samples the dominant ability and blinds the subset to a regression in a rare-but-primary one (abstention is a small LongMemEval slice but a primary MemPhant claim). Strata = §10 ability tags × the benchmark's difficulty/context tiers (BEAM's 128K/500K/1M/10M; LongMemEval's five abilities). Allocate ≥`n_min` per stratum (every ability gets a CI), then fill proportional to full stratum sizes. The subset estimates the full score via a **stratum-reweighted (Horvitz-Thompson) estimator**, not a raw mean (biased under uneven sampling); its CI is the `05` §8 bootstrap resampled **within strata**. Promotion to a full run requires the paired subset-delta CI to exclude zero **AND** per-stratum delta-sign consistency (no ability silently regressing under a positive aggregate). The subset seed is pinned per benchmark version (nightly comparability); re-drawing is a §7 version event, recorded.

## 10. Memory Ability Coverage

Each benchmark/case is tagged with abilities:

```text
single-hop recall
multi-hop recall
temporal update
stale suppression
procedural transfer
resource evidence
cross-scope isolation
forget/delete
poisoning resistance
latency/cost pressure
```

SOTA claims must state which abilities are covered and which are not.

**Coverage continuity:** retiring a benchmark (§7) requires a successor that covers the same abilities *before* the scorecard drops them — an ability never loses coverage silently because its only benchmark saturated. The BEAM "knowledge-update" / "contradiction-resolution" categories specifically map to MemPhant's contradiction-detection metric (`04` §3.1) and must stay covered.

## 11. Cost Model of Evaluation

The ladder (§9) exists because eval cost is dominated by **answer-model context tokens at the high context tiers**, not by MemPhant's own retrieval. A single BEAM 10M-tier pass over its full question set is the cost cliff — release-gated, never per-PR.

| Rung | What runs | Dominant cost | Lever |
|---|---|---|---|
| PR | unit + golden + **retrieval-only oracle** | **~$0 — the oracle is answer-model-free** (`05` §4.0), so the gate has no LLM-answer cost | keep the judge off this rung by construction |
| Nightly | stratified subset + ablations + security | answer-model tokens on the *subset only*, low context tiers | sample size (§9.1), context-tier cap, **Batch API (50% off, 24h)** |
| Release | full external + paired baseline | full 10M-tier passes × competitor baselines | the only rung allowed to spend here; gated on a subset that predicted a delta |

Levers that cut eval cost without moving the claim: keep the oracle gate model-independent (never "upgrade" it to a judged gate — realism lives one rung up); run the full ablation matrix nightly on the **subset only**; reproduce a competitor baseline **once per benchmark version** and archive its traces (don't re-run every release); pin the answer model in the scorecard row and treat a model change as a new baseline, not a free re-score (a model swap is a ~10pt *and* a cost move).

**pass^5 multiplier:** STATE-Bench's pass^5 metric is 5 runs per task — a **5× answer-model cost multiplier** on the outcome axis at sampled cadence; size the nightly/weekly STATE-Bench subset with that multiplier applied (it is the one lane whose cost scales with run count, not just context tier).

**Record-replay on the PR rung:** PR-cadence derivation goldens replay **recorded extraction outputs**, keyed by (fixture_version, compiler_version), plus a **pinned local embedder** — no live LLM derivation runs on PR; live derivation is a nightly lane. The determinism/record-replay contract is owned by `03` §6.2.

## 12. Cross-System Comparison Harness

To compare MemPhant vs Mem0/Zep/Hindsight, the harness controls everything *except the memory system* — "comparable budget" (`05` §8) is **defined, not asserted**. Held equal across systems: **answer model + version** (the largest confound, ~10pt; letting each system pick its own answer model is not a memory comparison), embedding model where the system allows (else recorded as an uncontrolled axis), **answer-context token budget** (the accuracy/token *pair* is the unit — winning accuracy at 5× tokens is a Pareto row, not a win), benchmark version/corpus/haystack size.

Fairness footguns the harness **rejects**: `top_k ≥ candidate-pool size` silently converts a retrieval test into reading-comprehension (zero retrieval discrimination — the LoCoMo `top_k=50` over ≤32-pool case); a comparison violating `top_k < pool_size` is labeled `non_discriminating`, not scored. Post-hoc per-case fixing then re-scoring on the same set ("teaching to the test") downgrades a competitor's number to `vendor_reported`. "Lossless/full-recall" claims that drop accuracy under the actual retrieval task → record the task-specific number, never the headline. A competitor MemPhant cannot reproduce under these controls is `vendor_reported` and cannot anchor SOTA; reproduce **once per benchmark version**, archive traces.

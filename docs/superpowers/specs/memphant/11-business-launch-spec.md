# MemPhant - Business and Launch Spec

## 0. Launch Thesis

MemPhant launches as the open memory substrate for agent builders who no longer trust "dump it in context" or "just use a vector DB."

The market is crowded on memory claims and weak on memory evidence. The launch wedge is:

```text
Apache-2.0 Rust core
+ MCP/SDK adoption path
+ poisoning defense by default
+ traceable evals
+ Syndai production dogfood
```

## 1. Positioning

One-liner:

> MemPhant is the Apache-2.0 memory substrate for long-running agents: ground-truth episodes, scoped recall, poisoning defense, and reproducible evals.

Developer promise:

> Add durable agent memory without giving unsafe memories control over your agent.

Enterprise promise:

> Keep agent memory tenant-scoped, cited, auditable, and deletable.

Research promise:

> Run memory ablations without guessing which subsystem helped.

## 2. Initial Audience

| Audience | Why they care | Launch artifact |
|---|---|---|
| Claude/Cursor/Codex users | MCP memory they can inspect. | `npx`/binary MCP quickstart. |
| Python agent builders | Existing memory libs are easy but underspecified. | `pip install memphant` quickstart. |
| Infra-minded teams | Rust/Postgres feels deployable. | Docker Compose + migration guide. |
| Security-conscious teams | Memory poisoning is becoming a known agent risk. | ASI06 writeup + red-team fixtures. |
| Eval builders | Need traceable ablations. | Eval harness examples. |

### 2.1 Displaced Segments (near-term switchers)

Two competitor cohorts are actively displaced right now — court them honestly, no gloating:

- **Mem0 graph-traversal users**: Mem0 v3 removed the OSS graph module, and their July 1 "State of AI Agent Memory 2026" report publicly concedes the removal is "a regression" for graph-traversal users. MemPhant's relational edge expansion on plain Postgres is the landing path.
- **MemPalace debunked-eval users**: MemPalace's headline retrieval numbers were independently contradicted (arXiv:2604.21284; MemPalace issue #125 measured every proprietary mode below a raw ChromaDB baseline on answer quality). Users who chose it on those numbers are re-evaluating; traceable, reproducible evals are the landing path.

## 3. Pricing Posture

The core repo is free Apache-2.0.

Commercial lanes:

- hosted MemPhant Cloud
- enterprise support
- private deployment automation
- private held-out benchmark packs
- compliance pack
- managed migration from existing memory systems

What hosted uniquely has is more than convenience: **fleet-level learned priors and calibrated defaults** (retrieval/decay/trust calibration learned across many tenants' traces — only a fleet operator can learn them), **compliance lanes** (residency, erasure-SLA, DPA), and **operational trust** (someone accountable when memory is wrong in production). Self-host gets the same core behavior; hosted sells what a single deployment cannot produce for itself.

Do not charge for rank, security basics, or poisoning defense in the core.

## 4. Launch Gates

Public launch requires:

- Apache-2.0 repo with clean README.
- Working local Postgres quickstart.
- Working MCP server.
- Python SDK.
- TypeScript SDK or generated client.
- Golden eval runner.
- Poisoning red-team examples.
- Syndai dogfood statement with bounded proof.
- Public roadmap with explicit non-goals.

Launch proof bundle:

- "State of Memory 2026" style technical post distinguishing memory from long context
- poisoning red-team fixture walkthrough
- benchmark scorecard with cost/latency/CIs
- Syndai dogfood case study with caveats
- public trace explorer demo
- quickstart videos for MCP and Python

Timing input (verified 2026-07-02): both target leaderboards are live and **empty** — STATE-Bench's leaderboard has formal submission columns (pass@1, pass^5, UX, Cost/Task) and zero entries; LongMemEval-V2 shows "entries coming soon" on both tiers. The first credible memory-system submission takes the visible slot; sequence the scorecard work so MemPhant is that submission.

## 5. Kill / Narrow Gates

Narrow the launch if:

- recall quality does not pass internal Syndai contract fixtures
- setup takes longer than 15 minutes for a motivated developer
- poisoning defense cannot block obvious injected-memory fixtures
- trace explorer cannot explain failures

Fallback launch wedges:

- "memory poisoning guard for agent apps"
- "agent memory eval harness"
- "Rust MCP memory server"

## 6. Risk Register

| Risk | Response |
|---|---|
| Mem0/Hindsight/Zep copy the claim | Compete on Apache-2.0 Rust core + poisoning/eval rigor, not slogans. |
| Benchmarks are noisy | Publish caveats and trace configs. |
| Rust hurts Python adoption | Ship Python SDK and wheels early. |
| Security claims invite scrutiny | Good. Ship fixtures and threat model. |
| Hosted business lags | Core adoption and Syndai dogfood still create strategic value. |
| Rigor is not distribution | Ablation rigor wins engineers, not procurement; the distribution risk is real and is mitigated only by the hosted/enterprise motion, not by more benchmark wins. |

## 7. Messaging Guardrails

Say:

- "Rust-first memory substrate"
- "traceable recall"
- "ground-truth episodes"
- "trust-aware retrieval"
- "FSRS-inspired decay hooks"

Do not say:

- "Rust makes memory SOTA"
- "brain-like AI memory proven by neuroscience"
- "poisoning-proof"
- "drop-in replacement for every agent runtime"
- "best on benchmarks" without reproduced evidence

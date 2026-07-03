# MemPhant - Financial Model and Open Source Spec

## 0. Business Model

Open core creates adoption. Hosted service and enterprise support create revenue.

## 1. Revenue Lines

| Line | Description |
|---|---|
| Cloud usage | hosted storage, recalls, traces, eval runs |
| Team plan | shared projects, retention, higher quotas |
| Enterprise | BYOC/VPC, SSO, compliance, support |
| Eval packs | private/held-out benchmark packs |
| Import support | moving from existing memory stacks |

## 1a. Metered Units (what the hosted service bills)

COGS attribution (§6) measures *cost*; these are the customer-facing **billed** dimensions, each mapped 1:1 to a `20` event + a `22` §1.4 cost class so price and cost reconcile. Define the units now; prices wait for measured COGS (§3). Precedents (structure, not our prices): Pinecone bills write-units + **read-units** + storage-GB; Mem0 bills **stored-memories + retrieval-calls**; Zep bills write-volume (episode bytes).

| Unit | Meters off | Note |
|---|---|---|
| `recall_unit` | `recall_called` / `cost_micros` | the "memory read unit" (Pinecone-RU analog) |
| `storage_gb_month` **per `retention_tier`** | `episode.retention_tier` + `object_store_bytes` | hot(PG)/warm/cold priced differently — cold→object-store is the margin lever (§2a) |
| `retain_unit` | `retain_called` | write volume |
| `reflect_unit` + **embedding/LLM passthrough** | `reflect.extract` `gen_ai.usage.*_tokens` | the real COGS driver (`22` §1.4); bill embedding/LLM as cost-plus passthrough |
| `egress_gb`, `trace_storage_gb_month` | object-store + trace bytes | |

Billing events are a **separate plane** from adoption analytics (`20` §3 keeps them apart): `visibility: 'billing'`, tenant-attributed, immutable.

## 2. COGS Drivers

- Postgres compute
- Postgres/vector storage
- object storage
- embeddings
- reranker calls
- LLM extraction/consolidation
- trace storage
- eval runs

Primary COGS levers:

- novelty-gated extraction
- adaptive retrieval cascade
- skip rerank unless ambiguous
- background batch embedding
- trace retention tiers
- L4 deliberate recall only on paid/exhaustive modes

## 2a. COGS Realism (the locked deployment model's cost axes)

The cell-per-region + retention-tier model (`25` §7b, `04` §2.4) adds three cost axes the §2 list predates:

- **Per-cell fixed floor × N cells.** Each region is a full stack (Supabase project + regional bucket + regional Temporal + Fly app), so COGS multiplies by **cell count**, not tenant count — a low-volume EU cell still carries a full fixed floor. Add `cell_id`/`region` to the §6 attribution buckets.
- **Per-retention-tier $/GB.** hot (PG-resident, expensive) vs cold (object-store, cheap) differ by ~an order of magnitude — demoting cold-tier memory to the object store is the **primary margin lever** (industry object-store vector tiering claims up to ~90% storage savings). Model $/GB per tier, never a blended rate.
- **Re-embed 2× index peak** (`14` §10.1) is a periodic COGS event, not steady-state.

**Gross margin per tenant** = Σ(metered-unit revenue, §1a) − Σ(attributed COGS incl. the tenant's share of its cell's fixed floor). This is what makes a low-volume-EU-cell tenant's true margin visible.

## 3. Pricing Sketch

| Tier | Price | Notes |
|---|---:|---|
| OSS | $0 | self-host |
| Cloud Free | $0 | small quotas |
| Pro | $29-$99/mo | solo/dev projects |
| Team | $299-$999/mo | shared traces, retention, support |
| Enterprise | custom | BYOC/VPC/compliance |

Exact prices wait for measured COGS. Do not commit before usage data.

## 3a. Quotas, Overage & Billing Status

Tiers map to **numeric quotas on the §1a units** (even placeholder: "Pro = N recall-units + M GB hot; overage $X/unit"). Policy per dimension (2026 usage-based norm): **overage-bill** recall/retain (soft), **degrade** throughput (the existing `02` §3.1 backpressure), **warn-then-hard-cap** storage. Storage quota enforcement is billing-load-bearing, so it is owned here and implemented with the billing surface.

A tenant carries `billing_status ∈ {active, past_due, suspended}` (distinct from the `15` §3 security `revoked_at`). **Suspend revokes premium features but preserves core data visibility + the always-free export (`15`)** — never deletes, never breaks the anti-lock-in guarantee.

## 3b. BYOC vs Hosted Billing (opposite COGS profiles)

- **Hosted** — usage-metered on the §1a units; **MemPhant pays the infra** (cells, embeddings, LLM). Margin per §2a.
- **BYOC** — the customer runs their own Postgres + bucket + compute, so per-recall metering is structurally wrong: MemPhant's marginal infra cost is ~$0. BYOC pays a **flat control-plane + support/license fee** (for the closed router/directory/billing/autoscale/SSO/compliance tooling), **not** usage. The Enterprise line (§1) splits into these two distinct sub-lines.

## 4. Open / Closed Split

Open:

- core
- server
- MCP
- SDKs
- local evals
- golden fixtures
- poisoning fixtures

Closed/hosted:

- billing
- account management
- private eval corpora
- enterprise deployment automation
- hosted trace retention at scale
- regional cells + tenant→region router + the tenant→region directory (multi-region residency, `25` §7b)

Apache core must still include enough to:

- run a useful server
- store/retrieve memory
- run local evals
- inspect traces
- test poisoning fixtures
- self-host with Postgres

The hosted business should sell convenience, scale, compliance, and managed operations. It should not sell the basic safety contract back to open-source users.

## 5. Margin Rule

Hosted plans must pair accuracy with cost. If a feature only improves benchmark score by spending unbounded LLM/sandbox time, it belongs behind explicit exhaustive mode or not at all.

## 6. Cost Attribution

Every hosted recall/eval run should attribute cost by bucket:

```text
db_ms
embedding_calls
reranker_calls
llm_extraction_calls
object_store_bytes
trace_storage_bytes
eval_runner_seconds
```

This keeps pricing honest and prevents hidden SOTA spend. Attribution is also **per `cell_id`/`region`** (§2a) so a tenant's share of its cell's fixed floor is visible.

## 7. Economic Neutrality + Compliance-as-Product

- **Syndai is paying customer #1.** Its hosted tenant carries a real internal-billing line on the **same** metered units + quota path as an external Team tenant (`09` §9.1) — never modeled as free internal infra. Economic neutrality matches the already-enforced technical neutrality.
- **The free-vs-paid compliance line, drawn explicitly.** **Data-portability export stays free at every tier** (anti-lock-in — `15`, `17`; never a paid gate). **Residency (in-region/EU cell, `25` §7b), the crypto-shred erasure SLA (`06` §6.2), the DPA, and the compliance pack are Enterprise-billable** with stated SLAs — the one place residency becomes a *product*, not just a posture.

# MemPhant - Open Source and Governance Spec

## 0. License

MemPhant core is Apache-2.0 from day one.

Reason:

- It is real open source.
- It has enterprise-friendly patent language.
- It avoids source-available confusion.
- It makes external adoption easier.

Hosted service code, deployment automation, enterprise support tooling, or proprietary evaluation datasets may live outside the Apache core.

### 0.1 The Open/Closed Line (principled rule)

The split is decided by one rule, not revenue instinct:

> A capability is **Apache-2.0 public** if it is required to *store, retrieve, correct, forget, trace, or prove the quality of* memory on self-hosted Postgres. It may be **closed** only if it is operational convenience, multi-tenant scale machinery, or commercial packaging a competent self-hoster does not need for a correct, safe, inspectable substrate.

Litmus (yes to any → it stays public): does an external self-hoster need it to pass the golden + poisoning evals? does it change recall *correctness*, trust labeling, citation, or deletion completeness? is it on the path a benchmark reproduces? **Anti-open-washing gate:** the open core must independently reach the published accuracy + poisoning numbers; if a SOTA or safety claim depends on a closed component, the claim is invalid until that component is opened or the claim is narrowed (`13` §5). Closed is allowed for hosted control plane / billing / autoscale / SSO / private held-out corpora — **never** for a memory-kind, retrieval lever, trust/poisoning control, correction/deletion semantics, trace schema, or eval harness.

### 0.2 Relicensing Commitment (no rug pull)

The 2018–2025 pattern — MongoDB, Elastic, HashiCorp, Redis, Sentry relicensing permissive cores to source-available (SSPL/BSL/Elastic/FSL) once cloud vendors competed — is the exact trust failure MemPhant must not reproduce:

> **The MemPhant public core is Apache-2.0 forever. It will never be relicensed to a source-available, "fair-source", non-compete, or delayed-open (FSL/BUSL-style) license.**

Mechanically backstopped, not just promised: **no CLA, no copyright assignment** (§2) means copyright stays distributed across every contributor, so **no single entity — Syndai included — holds the rights to unilaterally relicense the accumulated core** (the same structure that makes the Linux kernel un-relicensable, and the real reason the CLA is rejected, not mere contributor friction). Apache-2.0's irrevocable grant means a fork can always continue from the last Apache commit. The escape hatch for revenue pressure is closing *operational* layers (§0.1), never the core — if that model fails commercially, the correct outcome is the core outlives the company under Apache-2.0. Closing a *previously-open* memory capability counts as a soft rug pull and is governed by the same §0.1 rule.

## 1. Public Repo Contents

Public:

- Rust core
- Postgres adapter
- REST server
- MCP server
- CLI
- SDKs
- golden eval fixtures
- poisoning red-team fixtures
- docs
- examples
- schema migrations

Required launch files:

```text
LICENSE
NOTICE
README.md
SECURITY.md
CONTRIBUTING.md
CODE_OF_CONDUCT.md
CHANGELOG.md
DCO  (or CONTRIBUTING.md "Sign your work" section)
TRADEMARK.md
docs/governance.md
docs/quickstart.md
docs/api.md
docs/mcp.md
docs/evals.md
docs/security.md
examples/
```

Not necessarily public:

- hosted cloud control plane
- billing
- enterprise deployment automation
- private held-out eval corpora
- customer integrations
- operational dashboards

## 2. Contribution Policy

Require:

- DCO with inbound=outbound Apache-2.0 contribution terms.
- Security policy.
- Code of conduct.
- Contributor guide.
- Clear benchmark disclosure rules.

No CLA at launch. Keep governance boring until contributors exist. No foundation cosplay.

### 2.1 Contribution Boundaries

Accepted early contributions:

- bug fixes
- docs
- golden eval cases
- database/provider fixes
- SDK ergonomics
- MCP client examples
- security red-team fixtures

Require maintainer design review:

- new memory kinds
- new DB backends
- new graph/rerank engines
- benchmark scorecard changes
- public API changes
- trust/security policy changes

Decline until core quality is proven:

- broad framework adapter PRs
- hosted-cloud-only features in core
- vendor-specific benchmark claims
- changes that weaken trace/citation/deletion contracts

Decision tree (resolves the hard cases against `00-MAIN` §5 + YAGNI):

| Proposed PR | Default | Rule |
|---|---|---|
| bug fix / docs / golden or poisoning fixture | **accept** | strengthens the contract |
| new **store backend** (SQLite/Mongo/graph DB) | **decline** | Postgres+pgvector by decision (`26` §1); reopen only with traces showing Postgres can't hit the target |
| new **memory kind** | **design review, default no** | the kind enum is a frozen interface — a schema+policy+eval burden, not a drive-by |
| new **retrieval/rerank engine** | **design review** | must arrive with ablation traces (invariant #9); no score-changing default without paired evals |
| **framework adapter** (LangChain/LlamaIndex/…) | **decline into core; welcome external** | the adapter surface *is* the public API/SDK/MCP; adapters live in contributor repos |
| new **provider** behind an existing trait | **accept** | pluggable by design; no new public surface |
| trust/deletion/citation contract change | **maintainer-only, highest scrutiny** | strengthening needs security evals archived; weakening is a decline |

Principle: contributions that **fill in** a frozen interface (a provider, a fixture, a doc) are cheap to accept; those that **add a new frozen interface** (a kind, a backend, a default lever) are expensive and gated — the cost is the forever-migration, not the patch.

## 3. Benchmark Honesty

Public scorecards must include:

- benchmark version
- MemPhant version
- config
- model/provider versions
- cost estimate
- p50/p95 latency
- trace archive pointer
- known caveats

No cherry-picked "SOTA" without cost and latency.

## 4. Brand Boundary

MemPhant is not Syndai-branded infrastructure.

README language:

```text
MemPhant is an Apache-2.0 memory substrate for long-running agents.
Syndai is the first production dogfood user.
```

Do not say:

```text
MemPhant is Syndai memory extracted.
```

That framing makes it feel like internal leftovers.

## 5. Security Disclosure

Security reports go to a dedicated address and template.

Initial sensitive areas:

- tenant isolation
- poisoning bypass
- deletion bypass
- auth bypass
- MCP prompt/tool injection
- trace leaks
- resource pointer leaks

Process (coordinated disclosure via GitHub Security Advisories): private intake to `security@`/a GHSA draft (never a public issue), ack ≤72h; **severity by a memory-substrate rubric** — *Critical* = cross-tenant read/write, deletion/forget bypass leaving recoverable data, auth bypass, persistent-poisoning that promotes attacker text to high-trust semantic memory without corroboration; *High* = cross-tenant trace/citation leak, MCP injection executing high-risk actions; fix under embargo (tenant-isolation + deletion-bypass classes default to the shortest practical embargo because data is at rest); **request a CVE** for any Critical/High in a released version (GitHub as CNA), publish a GHSA, and **add the vuln as a permanent red-team fixture** to the public security suite so the class cannot silently regress. No silent security fixes; scope honesty mirrors `17` §4 ("reduces poisoning risk", never "poisoning-proof"). Bug-bounty/safe-harbor not at launch; good-faith research on a local self-hosted instance is welcome, testing the hosted service requires written authorization.

## 6. Release Rules

Every release tags:

- Rust crate versions
- SDK versions
- schema version
- trace schema version
- eval harness version

Release notes include migrations and benchmark deltas when relevant.

## 7. Public/Private Split

| Layer | License/posture |
|---|---|
| core Rust crates | Apache-2.0 |
| Postgres adapter/migrations | Apache-2.0 |
| MCP server | Apache-2.0 |
| CLI | Apache-2.0 |
| Python/TS SDKs | Apache-2.0 |
| harness provider adapters (file-tool `08` §5.1a; Hermes `08` §5.1b, activation-gated) + the scope stats/block surfaces | Apache-2.0 (public API surface — no hidden Syndai-only behavior) |
| golden eval harness and synthetic fixtures | Apache-2.0 unless data license forbids |
| hosted cloud control plane | private hosted layer |
| regional cells + tenant→region router + the tenant→region directory | private hosted layer (multi-region residency is a closed-layer composition of single-region open-core cells; `25` §7b) |
| billing, metering, enterprise SSO | private hosted layer |
| private held-out benchmark corpora | private aggregate-only |
| Syndai adapter code | remains in Syndai until generalized into product-neutral code |

Open core must be sufficient for a third party to self-host useful memory without Syndai.

## 8. Release Checklist

Before a public release:

- `cargo fmt --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo nextest run --all-features`
- `cargo test --doc`
- Python wheel build for supported platforms
- TypeScript SDK build/typecheck
- OpenAPI diff reviewed
- MCP schema snapshot reviewed
- DB bootstrap checks on plain Postgres, Neon, Supabase BYOC
- golden evals archived
- security evals archived
- changelog includes migrations, breaking changes, and benchmark caveats

## 9. Governance Firewalls

Syndai is allowed to be the first customer. It is not allowed to silently privilege itself.

Rules:

- Syndai-specific features enter public core only when phrased as generic memory requirements.
- Public benchmarks disclose when Syndai traces/cases are used.
- Hosted MemPhant can have paid features, but rank/eval claims cannot depend on paid access.
- Public issue discussions for benchmark disputes remain open unless they include private data or security details.

### 9.1 Dogfood-Neutrality Is Tested, Not Promised

Invariant 11 ("Syndai calls the public contract, not hidden internals") is enforced, not trusted:

- **No private surface area.** One server build. Syndai's adapter (`28`) links only published REST/SDK/MCP operations + the public crate API — no `syndai-only` feature flag, endpoint, env var, or crate feature in the public build.
- **CI gate `neutrality_check`** (public repo): fails if any public crate exposes a symbol gated on a Syndai identifier, if any handler branches on a Syndai tenant/key, or if the OpenAPI/MCP schema snapshot contains an operation not in `docs/api.md`. The Syndai adapter repo runs the inverse (imports only published surface, no `memphant_core::internal::*`).
- **Same auth tier:** Syndai's hosted tenant uses the same key type, quota path, and eval access an external Pro/Team tenant uses; rank/eval claims cannot depend on a tier Syndai alone holds.
- **Disclosed, not privileged:** Syndai may be *first* (early access, first to file issues, first to fund a feature); it may not be *secret* (no unlisted operation, no pre-merged private patch in the running service).

The test of neutrality: an external customer who reads the public repo can stand up a service byte-identical in capability to the one Syndai runs. The only legitimate differences are operational scale and the closed hosted plane (§0.1), never memory behavior.

## 10. Frozen-Interface Change Governance

`08` §7 owns the version *taxonomy*; this owns the *change process* for the `00-MAIN` §5 frozen interfaces (kind enum, write-path columns, citation ledger, trace schema, API/MCP schemas, scope tree):

- **Public semver** post-launch: additive = minor, breaking = new versioned path + note; `cargo-semver-checks` + the OpenAPI/MCP schema-snapshot diff gate accidental breakage.
- **A frozen-interface change is an RFC, not a PR** — written rationale + migration + eval-delta (invariant #9) + maintainer sign-off; drive-by changes to frozen interfaces are rejected at review.
- **Deprecation window:** breaking a public contract = deprecate-then-remove across two majors (minor marks deprecated with migration guidance + a deprecation signal; removal waits for the next major). No same-release rug-out of a public field.
- **Dogfood gets no private migration path:** Syndai migrates over the *same* window + the *same* `memphant verify` lock-drift gate (`08`) an external customer uses; a breaking change is "done" only when the public window elapses.
- **Pre-launch exception:** before the first public tag, contract-simplifying breaks are free (`08` §7) — use that window to freeze hard so post-launch governance is rarely triggered.
- **Mechanized, not just reviewed:** the deprecation/contract steps are enforced by the `schema_compat_revision` boot-floor (`25` §11b) + the migration-class classifier + floor-monotonicity check (`25` §12) + `memphant verify`, not reviewer discipline alone. The expand→migrate→contract sequence (expand minor → migrate window → contract at next major with the floor bump) is the parallel-change pattern, and because the contract travels *in the data* (the floor in the ledger) it needs no central coordinator — what survives no-CLA forks. The pre-launch exception stands: land the *mechanism* now and freeze hard; the *window* activates only at the first public tag.

## 11. Trademark and Fork Posture

Apache-2.0 licenses the *code*, not the *name* — posture is fork-friendly, name-protective: anyone may fork the core, but a fork that diverges in behavior ships under a different name/mark (the LibreOffice/Valkey convention) so "MemPhant" keeps meaning *this* tested substrate with *these* published evals. Compatibility use ("works with MemPhant", "MemPhant-compatible") is fine (nominative use); naming a fork/competing service "MemPhant-<x>" is not. **No naked licensing** — the mark applies only to builds passing the public golden + poisoning evals, so the name stays a quality signal. A lightweight `TRADEMARK.md` states this; no foundation, no mark-licensing bureaucracy at launch.

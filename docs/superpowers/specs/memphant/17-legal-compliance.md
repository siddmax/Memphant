# MemPhant - Legal and Compliance Spec

> Engineering guidance, not legal advice. Counsel reviews before public launch.

## 0. License

Core repo: Apache-2.0.

Third-party code:

- track license in dependency audit
- no GPL/AGPL in core unless explicitly accepted
- separate non-default integrations when license risk is non-trivial

## 1. Data Privacy

MemPhant may store sensitive user/enterprise memory. Required:

- tenant isolation
- deletion/forget API
- export API
- retention policy
- audit log for admin access
- no training on customer data without explicit opt-in

## 2. GDPR / Deletion

`forget` and export must support:

- subject-scoped export
- subject-scoped deletion/invalidation
- derived memory invalidation
- object-store purge according to retention class
- deletion audit entry
- **subject identity reconciliation (merge/split)** — two `subject` rows found to be the same person (merge), or one row conflating two (split). A merge is a typed **`subject_supersedes`** record that re-points recall onto the surviving `subject_id` — **never an in-place identity rewrite**, so citations/edges/bitemporal generations stay stable (the §`04` §7.3a append-only discipline applied to identity). `subject_id` is a stable FK that merge re-points; post-merge GDPR erasure crypto-shreds the merged identity's per-user DEK (`06` §6.2). Pinned now because retrofitting merge onto a one-row-per-person assumption is a tenant-wide backfill (the `04` §7-class cost).

## 3. Benchmark Legal Posture

Public benchmark runs:

- obey dataset licenses
- cite benchmark source
- do not redistribute private datasets
- do not publish held-out private task text
- distinguish public reproduction from private/internal evals

Competitor claims:

- cite source
- label self-reported/vendor-reported numbers
- avoid implying endorsement
- re-verify before launch pages
- keep raw private traces out of public artifacts

## 4. Security Claims

Allowed:

- "designed to reduce memory poisoning risk"
- "ships poisoning red-team fixtures"
- "trust-aware retrieval"

Avoid:

- "poisoning-proof"
- "guaranteed secure"
- "compliant by default"

## 5. Export Controls

Memory infra is generally software, but enterprise deployments may touch regulated data. Hosted terms should reserve the right to block prohibited uses.

## 6. Terms / Policies Needed

Before hosted public launch:

- Terms of service
- Privacy policy
- Security policy
- DPA template for enterprise
- Acceptable use policy
- Vulnerability disclosure policy

Open-source repo can launch with LICENSE, SECURITY.md, CONTRIBUTING.md first.

## 7. Data Processing Notes

Hosted terms and DPA should explicitly describe:

- memory retention classes
- deletion/invalidation semantics
- subprocessors for embeddings/LLM extraction if used
- whether raw memory leaves the region/provider
- trace retention and redaction policy
- customer export format

## 8. OSS Licensing Hygiene

Before public repo launch:

- run dependency license audit
- generate NOTICE file
- document non-default dependency licenses
- keep AGPL/GPL out of default core unless deliberately accepted
- keep copied benchmark fixtures license-compatible

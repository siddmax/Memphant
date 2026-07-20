# Syndai retrieval and CaaS convergence inventory

Date: 2026-07-13

## Decision

Syndai has three incumbent projections over related evidence, not one retrieval
engine:

1. document KB/RAG under `backend/src/features/knowledge`;
2. agent prompt memory at `MemoryContextLoader`; and
3. CaaS structural code intelligence under
   `backend/src/features/coding/code_intel`.

MemPhant should unify evidence identity, tenant/scope/actor mapping, temporal
validity, outcomes, citations, and retrieval traces. It must not force the
three lanes into one undifferentiated vector index or one global read policy.

## Existing convergence seams

- Document retrieval: replace behind
  `backend/src/features/knowledge/search_detached.py::search_knowledge_detached`.
  Keep controllers, the canonical `knowledge_search` tool, source lifecycle,
  response DTOs, citation validation, billing, web, and mobile contracts.
- Agent memory: extend the existing public REST-only
  `memphant_dogfood_adapter.py` used by
  `backend/src/features/memory/context_loader.py`. Do not add another adapter
  or import MemPhant internals.
- CaaS code intelligence: keep the governed code-intel capability boundary and
  its exact repo/base-SHA identity. Replace the tree-sitter projection only
  after a separate prospective code-task gate wins.

The document incumbent is hybrid halfvec plus Postgres FTS/RRF with source,
version, hierarchy, project, agent, user, filter, and snapshot constraints. It
has one adaptive rewrite/HyDE retry, optional Jina reranking, bounded structural
expansion, deterministic packing, and exact source/version/chunk/page/section
citations. Provider failures and fallbacks are observable gate failures even
where production currently fails open.

The code incumbent is a content-addressed tree-sitter graph keyed by
repository, base commit, and extractor fingerprint. Its honesty constraints
are part of parity: ambiguous cross-file names do not mint semantic edges,
neighborhood is one hop, results are bounded and reversible, and stale or
missing bundles fall back to language-server grounding.

## Replacement gates

Document RAG must preserve the production-shaped hierarchy and exact wire
contract, then improve supported-answer accuracy on both exposed sets and an
independently curated version-disjoint holdout under identical corpus, budget,
reader, judge, and latency conditions. Add negatives for unrelated queries,
lexical collisions, plausible-but-absent answers, wrong tenant/user/project/
agent, post-snapshot-only evidence, stale-only evidence, and unsupported
answerable questions. Any source skip, hierarchy flattening, citation mismatch,
fallback, parse error, missing pair, or degraded row invalidates the run.

CaaS is a separate paired gate over at least 40 prospective validator-backed
tasks with identical repository/base SHA, executor, model, tools, and turn
budget. Primary outcomes are localization under a fixed line budget and actual
task resolution. It must also preserve ambiguous-name honesty, freshness,
determinism, truncation, not-found recovery, and language-server fail-open.

Only after those independent gates and controlled dogfood observation windows
pass should the incumbent retrieval implementations be deleted. Source
management, canonical tools, citations, billing, and client contracts stay in
Syndai because they are product surfaces, not duplicate memory engines.

No code, live database, paid API, or user-facing path changed during this
inventory.

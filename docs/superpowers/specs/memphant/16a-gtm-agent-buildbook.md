# MemPhant - GTM Agent Buildbook

## 0. Rule

Use agents for repeatable GTM chores. Do not let them publish unsupervised.

## 1. Launch Research Agent

Job:

- monitor memory benchmarks
- monitor competitor releases
- collect new poisoning examples
- propose docs updates

Output:

- weekly markdown digest
- source links
- "needs human review" flags

## 2. Example Generator Agent

Job:

- turn real failure traces into minimal examples
- produce Python/TS/MCP snippets
- run examples before PR

Gate:

- example must run locally
- no secrets
- no private Syndai data

## 3. Content Draft Agent

Job:

- draft comparison pages
- draft benchmark notes
- draft security explanations

Gate:

- factual claims require source URLs
- competitor claims require caveats
- no "SOTA" without scorecard

## 4. Support Triage Agent

Job:

- group GitHub issues
- identify setup failures
- propose FAQ updates

Gate:

- never close issues automatically
- never request secrets

## 5. Eval Regression Agent

Job:

- run nightly sampled eval
- compare against previous release
- open draft issue/PR on regression

Gate:

- no auto-release
- no auto-change of benchmark claims

## 6. Ownership

All agents output draft artifacts only. Humans publish.


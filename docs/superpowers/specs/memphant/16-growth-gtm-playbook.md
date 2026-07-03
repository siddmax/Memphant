# MemPhant - Growth and GTM Playbook

## 0. Launch Asset

The launch asset is not a generic landing page. It is a technical proof:

> We ran memory systems through long-horizon recall and poisoning cases. Here is what broke, what MemPhant does, and the traces.

## 1. Channel Order

1. GitHub README and examples.
2. MCP quickstart.
3. Technical launch blog.
4. Hacker News / Reddit / X with benchmark traces.
5. Agent framework examples.
6. Security article on memory poisoning.
7. Syndai dogfood case study.
8. Conference/podcast outreach only after repeatable scorecard.

## 2. Launch Content

Ship:

- "Why agent memory needs provenance"
- "Memory poisoning is not prompt injection"
- "How MemPhant traces a recall failure"
- "Rust + Postgres is enough for v1 agent memory"
- "Syndai dogfood: scoped child-agent recall"

### 2.1 Displaced-Segment Angles

- Mem0 v3 removed the OSS graph module and publicly calls it "a regression" for graph-traversal users — target them with relational edge expansion on plain Postgres.
- MemPalace's vendor eval numbers were independently contradicted (arXiv:2604.21284; issue #125) — target its users with reproducible, trace-backed evals; factual and caveated, never gloating.

## 3. SEO / AEO Pages

Pages:

- `/`
- `/docs`
- `/docs/quickstart`
- `/docs/mcp`
- `/docs/python`
- `/docs/typescript`
- `/docs/security`
- `/docs/evals`
- `/compare/mem0`
- `/compare/zep-graphiti`
- `/compare/hindsight`
- `/benchmarks/longmemeval-v2`
- `/benchmarks/beam`
- `/security/memory-poisoning`

Comparison pages must be factual and caveated.

## 4. Adoption Loops

- MCP users create traces.
- Traces become reproducible bugs/evals.
- Evals improve defaults.
- Better defaults improve benchmark + dogfood claims.
- Claims drive more MCP/SDK users.

## 5. Community Loop

Encourage:

- memory failure reports
- benchmark adapters
- poisoning fixtures
- importers
- framework examples

Do not encourage:

- unverifiable leaderboard submissions
- private benchmark leakage
- vendor trash talk

## 6. Launch Metrics

Launch week targets:

- GitHub stars
- MCP installs
- Python package downloads
- docs quickstart completion
- Discord/GitHub issues quality
- reproducible eval runs by external users
- first non-Syndai production pilot


# MemPhant - Governance and Neutrality Charter

## 0. Charter

MemPhant is an open memory substrate. Its public claims must be reproducible, caveated, and not rigged to favor Syndai.

## 1. Conflict Disclosure

Syndai is:

- creator/backer
- first production dogfood user
- potential hosted-service operator

Therefore:

- public benchmarks must state when Syndai data/cases are used
- Syndai-only private evals are labeled internal
- competitor comparisons use published configs or explicit caveats

## 2. Benchmark Governance

Benchmark changes require:

- version bump
- changelog
- reason
- expected score impact
- rerun or sampled comparison

No silent benchmark swaps.

## 3. Methodology Changes

Changes that affect public scores require:

- config diff
- trace schema version note
- release note
- old vs new sampled run where practical

## 4. Community Contributions

Accepted contribution types:

- storage adapter
- benchmark adapter
- poisoning fixture
- docs/example
- SDK improvement
- bug fix

Higher scrutiny:

- score-changing retrieval defaults
- trust policy changes
- deletion/privacy changes
- benchmark claim changes

## 5. Advisory Board

Not needed at launch. Add only if:

- public leaderboard becomes material
- vendors dispute public scores
- enterprise buyers require independent review

Until then, public methodology + reproducible configs are enough.

## 6. Public Dispute Process

If a vendor/user disputes a public result:

1. acknowledge and link the challenged scorecard row
2. freeze the disputed row's methodology version
3. rerun sampled cases when reproducible
4. publish old/new deltas
5. correct the score if the dispute is valid
6. preserve audit history

Never silently edit a public benchmark claim.

## 7. Neutrality Boundary

Syndai-derived cases are useful for regression protection. They must be labeled:

```text
internal-syndai-golden
```

They can protect regressions and dogfood. They cannot be the sole basis for public SOTA claims.

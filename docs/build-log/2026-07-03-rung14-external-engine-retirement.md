# Rung 14 External Engine Retirement

## Scope

Rung 14 evaluated the external graph DB / dedicated vector engine escape hatch. The gate requires relational edge or pgvector traces to prove a bottleneck and a specialized engine to improve a target axis beyond its cost/ops penalty. That gate is not met by the archived MemPhant profiles through Rung 13, so the correct Rung 14 action is retirement for the current public architecture, not another unused backend.

## Current Evidence Checked

- Owner contract: `27` rung 14 says to implement a graph adapter only if SQL edge traces prove a bottleneck; disable when there is no material win over relational edges.
- Activation gate: `29` §8 requires relational edge/pgvector traces proving a bottleneck and specialized-engine improvement beyond cost/ops penalty.
- Existing proof: Rung 6 relational edge expansion beat no-edges and filesystem controls, and later Rung 12/Rung 13 profiles still record no Postgres or pgvector bottleneck.
- Context7 `/pgvector/pgvector`: pgvector supports exact search plus approximate HNSW indexing; HNSW supports `vector` indexes up to 2,000 dimensions and `halfvec` indexes up to 4,000 dimensions, matching the local index-strategy contract in `27`.
- Web check: pgvector primary docs confirm exact nearest-neighbor search and HNSW approximate indexes remain the current default Postgres path.

## Proof Artifacts

- Retirement profile: `docs/build-log/artifacts/rung14-external-engine-retirement-profile.json`
- Relational-edge promotion evidence: `docs/build-log/artifacts/rung6-edge-expansion-profile.json`
- No-edges control evidence: `docs/build-log/artifacts/rung6-no-edges-sampled-traces.json`
- No-bottleneck latest profile evidence: `docs/build-log/artifacts/rung13-learned-rerank-profile.json`

## Focused Verification

```text
cargo test -p memphant-eval --test profile_contract rung14_profile_archives_external_engine_retirement
cargo test -p memphant-eval --test profile_contract rung14_retirement_requires_relational_edge_control_and_pgvector_evidence
cargo run -p memphant-eval -- profile examples/evals/rung14-external-engine-retirement-profile.yaml --compare-to rungs-0-13-baseline --archive docs/build-log/artifacts/rung14-external-engine-retirement-profile.json
```

Result:

```text
profile=pass id=rung14_external_engine_retirement_profile_001 compare_to=rungs-0-13-baseline activated=4 dormant=10 retired=1 archive=docs/build-log/artifacts/rung14-external-engine-retirement-profile.json
```

## Full Verification

```text
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
cargo test --doc
python3 -m pytest tests -q
cargo run -p memphant-eval -- verify-golden examples/evals/golden.yaml
cargo run -p memphant-eval -- security examples/evals/security-smoke.yaml
python3 scripts/check_spec_drift.py
```

Results:

```text
cargo fmt --check: pass
cargo clippy --all-targets --all-features -- -D warnings: pass
cargo test --all-targets --all-features: pass
cargo test --doc: pass
python3 -m pytest tests -q: 25 passed
verify_golden=pass cases=13
security=pass lanes=poisoning,query_filter_injection,high_risk_action_suppression,tenant_leakage,deletion_completeness deletion_completeness=pass
spec_drift=clean public=/Users/sidsharma/Memphant/docs/superpowers/specs/memphant private=/Users/sidsharma/Syndai/.wt/codex-memphant-cross-repo/docs/superpowers/specs/memphant
```

## Decision

Retire `External graph DB / dedicated vector engine` for the current public architecture. A future specialized engine requires a new decision-register entry backed by fresh traces showing Postgres relational edges or pgvector breach the target SLO and that the specialized engine improves the target axis after cost and operational penalty.

# Rung 15 — Inferred-Belief Composition

## Scope

- Added conservative reflect-stage preference composition: two trusted, non-risky preference observations can mint a belief-tier abstraction with `source_kind=composition`.
- Added `derived_by` to recall candidate traces and packed context items so composed units are labeled as `composition`; default extracted units remain `extraction`.
- Preserved restraint boundaries: composed units are `belief` + `candidate` when minted by reflect, carry `agent_output` trust, and promote to semantic only after a direct observation of the inferred claim.
- Added supersession back-walk: correcting a source unit expires dependent composed beliefs; forgetting a source deletes dependent composed beliefs.

## Evidence

- Owner contract: `docs/superpowers/specs/memphant/27-sota-ladder-and-validation.md` Rung 15.
- Mechanism of record: `docs/superpowers/specs/memphant/24-methodology-hardening-refinements.md` R89 / §2.3.
- Restraint reference: OP-Bench arXiv 2601.13722, over-personalization taxonomy and Self-ReCheck relevance mitigation.
- Agent memory context reference: OpenAI Agents SDK session-memory docs via Context7; current agent sessions prepend stored context automatically, so MemPhant keeps composed memory as advisory evidence with explicit provenance.

## Artifacts

- `examples/evals/golden/inferred_belief_composition.yaml`
- `examples/evals/golden/inferred_belief_composition_control.yaml`
- `benchmarks/rung15-inferred-belief-sampled.yaml`
- `benchmarks/rung15-baseline-sampled.yaml`
- `examples/evals/rung15-inferred-belief-composition-profile.yaml`
- `docs/build-log/artifacts/rung15-inferred-belief-sampled-traces.json`
- `docs/build-log/artifacts/rung15-baseline-sampled-traces.json`
- `docs/build-log/artifacts/rung15-inferred-belief-composition-profile.json`

## Verification

```text
cargo test -p memphant-core --test write_compiler_golden -- --nocapture
result: pass, 7 passed

cargo run -p memphant-eval -- run benchmarks/rung15-inferred-belief-sampled.yaml --archive-traces --archive-dir docs/build-log/artifacts
result: eval=pass id=rung15-inferred-belief-sampled passed=1/1 archive=docs/build-log/artifacts/rung15-inferred-belief-sampled-traces.json

cargo run -p memphant-eval -- run benchmarks/rung15-baseline-sampled.yaml --archive-traces --archive-dir docs/build-log/artifacts
result: expected control failure, eval=fail id=rung15-baseline-sampled passed=0/1

cargo run -p memphant-eval -- profile examples/evals/rung15-inferred-belief-composition-profile.yaml --compare-to rungs-0-14-baseline --archive docs/build-log/artifacts/rung15-inferred-belief-composition-profile.json
result: profile=pass id=rung15_inferred_belief_composition_profile_001 compare_to=rungs-0-14-baseline activated=4 dormant=10 retired=1 archive=docs/build-log/artifacts/rung15-inferred-belief-composition-profile.json

cargo fmt --check && cargo clippy --all-targets --all-features -- -D warnings && cargo test --all-targets --all-features && cargo test --doc && python3 -m pytest tests -q && cargo run -p memphant-eval -- verify-golden examples/evals/golden.yaml && cargo run -p memphant-eval -- security examples/evals/security-smoke.yaml && python3 scripts/check_spec_drift.py
result: pass; golden cases=14; pytest=25 passed; security lanes=poisoning,query_filter_injection,high_risk_action_suppression,tenant_leakage,deletion_completeness; spec_drift=clean
```

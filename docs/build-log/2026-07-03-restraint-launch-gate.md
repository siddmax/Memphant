# 2026-07-03 Restraint Launch Gate

## Scope

`27` §1 restraint launch gate: OP-Bench / PS-Bench-style over-personalization
must not drop more than 15% versus a memory-free baseline, with pinned-block
content explicitly in scope.

## Artifacts

- Restraint scorecard: `docs/launch/restraint-launch-scorecard.json`
- Profile artifact: `docs/build-log/artifacts/rung15-inferred-belief-composition-profile.json`
- Trace archive: `docs/build-log/artifacts/rung15-inferred-belief-sampled-traces.json`

## Result

- Threshold: max relative drop `0.15`
- Measured drop: `0.0`
- Memory-free baseline score: `1.0`
- MemPhant score: `1.0`
- Pinned-block content: in scope by `27` §1, `05` §1.5, and `04` §12
- Relevance gate rule: mandatory if measured drop exceeds threshold

No public SOTA claim is made from this gate.

## Verification

```text
python3 -m pytest tests/test_restraint_launch_gate.py -q
PASS: 4 passed
```

```text
cargo run -p memphant-eval -- profile examples/evals/rung15-inferred-belief-composition-profile.yaml --compare-to rungs-0-14-baseline --archive docs/build-log/artifacts/rung15-inferred-belief-composition-profile.json
PASS: profile=pass id=rung15_inferred_belief_composition_profile_001 compare_to=rungs-0-14-baseline activated=4 dormant=10 retired=1 archive=docs/build-log/artifacts/rung15-inferred-belief-composition-profile.json
```

```text
python3 -m pytest tests -q
PASS: 35 passed
```

## Status

Restraint launch gate is complete. The next unchecked launch gate is the GateMem
conditional gate.

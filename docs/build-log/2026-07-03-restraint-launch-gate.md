# 2026-07-03 Restraint Launch Gate

## Scope

`27` §1 restraint launch gate: OP-Bench / PS-Bench-style over-personalization
must not drop more than 15% versus a memory-free baseline, with pinned-block
content explicitly in scope.

## Artifacts

- Restraint scorecard: `docs/launch/restraint-launch-scorecard.json`
- Profile artifact: `docs/build-log/artifacts/real-launch-evidence-20260704-v1/sota-profile.json`
- Trace archive: `docs/build-log/artifacts/real-launch-evidence-20260704-v1/restraint-ps-bench-sampled-traces.json`

## Result

- Status: `pass`
- Threshold: max relative drop `0.15`
- Measured drop: `0.0`
- Memory-free baseline score: `1.0`
- MemPhant score: `1.0`
- Sample count: `50`
- Pinned-block content: in scope by `27` §1, `05` §1.5, and `04` §12
- Relevance gate rule: mandatory if measured drop exceeds threshold

No public SOTA claim is made from this gate.

## Verification

```text
python3 scripts/ingest_public_bench.py --sample-count 50
PASS: wrote docs/build-log/artifacts/real-launch-evidence-20260704-v1/sample-manifest.json
```

```text
python3 -m pytest tests/test_restraint_launch_gate.py tests/test_launch_evidence_contract.py -q
PASS
```

## Status

Restraint launch gate is complete.

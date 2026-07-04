# 2026-07-03 Public Launch Gate

## Scope

`29` §7 public launch gate for the public repository boundary. This gate does
not make a public SOTA claim.

## Artifacts

- Public launch scorecard: `docs/launch/public-launch-scorecard.json`
- Profile artifact: `docs/build-log/artifacts/real-launch-evidence-20260704-v1/sota-profile.json`
- Sample manifest: `docs/build-log/artifacts/real-launch-evidence-20260704-v1/sample-manifest.json`
- LongMemEval-V2 sampled trace: `docs/build-log/artifacts/real-launch-evidence-20260704-v1/public-real-sampled-traces.json`
- PS-Bench restraint sampled trace: `docs/build-log/artifacts/real-launch-evidence-20260704-v1/restraint-ps-bench-sampled-traces.json`

## Result

- Status: `pass`
- LongMemEval-V2 sampled profile: `50/50`
- PS-Bench restraint profile: `50/50`
- Measured recall p95: `5.717ms`
- Public SOTA claim: none

## Gate Mapping

| `29` §7 requirement | Proof |
|---|---|
| public API, SDK, MCP, CLI, docs, and examples | `openapi/memphant.v1.json`, `bindings/python/`, `mcp/memphant.tools.v1.json`, `crates/memphant-cli/`, `docs/deployment/`, `examples/evals/`, `web/public/` |
| self-host Docker/Compose path | `Dockerfile`, `compose.yaml`, `docs/deployment/self-host.md`, `docker compose config` |
| security policy and release process | `SECURITY.md`, `CONTRIBUTING.md`, `docs/release-process.md`, `.github/workflows/ci.yml` |
| golden, security, sampled benchmark, and deletion completeness gates green | `verify-golden`, `security-smoke` with `deletion_completeness=pass`, and `nightly-sampled` all passed |
| one reproduced public benchmark profile with cost/latency/config/traces | `docs/build-log/artifacts/real-launch-evidence-20260704-v1/sota-profile.json` |
| no critical Supabase/provider/advisor warning for hosted DB exposure | provider bootstrap checks passed for plain Postgres, Supabase, and Neon; scorecard records `critical_findings: []`; Supabase profile keeps `memphant` out of exposed schemas and requires warning-level advisor/lint failure |
| no hidden Syndai-only API field or behavior | scorecard test scans public API/MCP/SDK/server/types/web surfaces and rejects `syndai`/`dogfood` strings |
| public SOTA claim, if any, says exactly which axis it wins | no public SOTA claim is made; release policy blocks bare claims without exact axis/baseline/trace/cost/latency/security/deletion evidence |

## Verification

```text
python3 scripts/ingest_public_bench.py --sample-count 50
PASS: wrote docs/build-log/artifacts/real-launch-evidence-20260704-v1/sample-manifest.json
```

```text
python3 -m pytest tests/test_public_launch_gate.py tests/test_restraint_launch_gate.py tests/test_launch_evidence_contract.py -q
PASS
```

## Status

Public launch gate is complete.

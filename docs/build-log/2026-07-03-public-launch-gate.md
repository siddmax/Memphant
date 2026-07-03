# 2026-07-03 Public Launch Gate

## Scope

`29` §7 public launch gate. This is a launch-candidate proof for the public
repository boundary; it does not claim external SOTA.

## Artifacts

- Public launch scorecard: `docs/launch/public-launch-scorecard.json`
- Release process: `docs/release-process.md`
- Public CI workflow: `.github/workflows/ci.yml`
- Reproduced public benchmark profile: `docs/build-log/artifacts/rung15-inferred-belief-composition-profile.json`
- Sampled trace archive: `docs/build-log/artifacts/nightly-sampled-traces.json`

## Gate Mapping

| `29` §7 requirement | Proof |
|---|---|
| public API, SDK, MCP, CLI, docs, and examples | `openapi/memphant.v1.json`, `bindings/python/`, `mcp/memphant.tools.v1.json`, `crates/memphant-cli/`, `docs/deployment/`, `examples/evals/`, `web/public/` |
| self-host Docker/Compose path | `Dockerfile`, `compose.yaml`, `docs/deployment/self-host.md`, `docker compose config` |
| security policy and release process | `SECURITY.md`, `CONTRIBUTING.md`, `docs/release-process.md`, `.github/workflows/ci.yml` |
| golden, security, sampled benchmark, and deletion completeness gates green | `verify-golden`, `security-smoke` with `deletion_completeness=pass`, and `nightly-sampled` all passed |
| one reproduced public benchmark profile with cost/latency/config/traces | `rung15_inferred_belief_composition_profile_001` has `harness_pin`, sampled-public-style trace refs, p95/cost fields, and pass security/deletion decisions |
| no critical Supabase/provider/advisor warning for hosted DB exposure | provider bootstrap checks passed for plain Postgres, Supabase, and Neon; scorecard records `critical_findings: []`; Supabase profile keeps `memphant` out of exposed schemas and requires warning-level advisor/lint failure |
| no hidden Syndai-only API field or behavior | scorecard test scans public API/MCP/SDK/server/types/web surfaces and rejects `syndai`/`dogfood` strings |
| public SOTA claim, if any, says exactly which axis it wins | no public SOTA claim is made; release policy blocks bare claims without exact axis/baseline/trace/cost/latency/security/deletion evidence |

## Verification

```text
cargo fmt --check
PASS
```

```text
cargo clippy --all-targets --all-features -- -D warnings
PASS
```

```text
cargo test --all-targets --all-features
PASS
```

```text
cargo test --doc
PASS
```

```text
python3 -m pytest tests -q
PASS: 31 passed
```

```text
npm test
PASS: 6 passed
```

```text
cargo run -p memphant-eval -- verify-golden examples/evals/golden.yaml
PASS: verify_golden=pass cases=14
```

```text
cargo run -p memphant-eval -- run benchmarks/nightly-sampled.yaml --archive-traces --archive-dir docs/build-log/artifacts
PASS: eval=pass id=nightly-sampled passed=2/2 archive=docs/build-log/artifacts/nightly-sampled-traces.json
```

```text
cargo run -p memphant-eval -- security examples/evals/security-smoke.yaml
PASS: security=pass lanes=poisoning,query_filter_injection,high_risk_action_suppression,tenant_leakage,deletion_completeness deletion_completeness=pass
```

```text
cargo run -p memphant-eval -- profile examples/evals/rung15-inferred-belief-composition-profile.yaml --compare-to rungs-0-14-baseline --archive docs/build-log/artifacts/rung15-inferred-belief-composition-profile.json
PASS: profile=pass id=rung15_inferred_belief_composition_profile_001 compare_to=rungs-0-14-baseline activated=4 dormant=10 retired=1 archive=docs/build-log/artifacts/rung15-inferred-belief-composition-profile.json
```

```text
cargo run -p memphant-cli -- db bootstrap-check --provider plain-postgres
PASS: bootstrap_check=clean provider=plain-postgres profile=deploy/provider-profiles/plain-postgres.env.example
```

```text
cargo run -p memphant-cli -- db bootstrap-check --provider supabase
PASS: bootstrap_check=clean provider=supabase profile=deploy/provider-profiles/supabase.env.example
```

```text
cargo run -p memphant-cli -- db bootstrap-check --provider neon
PASS: bootstrap_check=clean provider=neon profile=deploy/provider-profiles/neon.env.example
```

```text
docker compose config
PASS
```

## Status

Public launch gate is complete as a launch-candidate public repository gate.
The next unchecked launch gate is the restraint launch gate.

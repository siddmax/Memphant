# 2026-07-03 Standing Quality Bars

## Scope

`STATUS.md` §6 release-standing bars:

- Hot-path SLO: `fast` p50 <200ms / p95 <500ms (`02` §4).
- Dogfood `memory_utility_trend` SLI wired through `mark` (`22` §1.3).
- Latest landscape-completeness review pass satisfied (`13` §1.4).

## Artifacts

- Standing scorecard: `docs/launch/standing-quality-bars.json`
- Hot-path executable guard: `crates/memphant-core/tests/hot_path_slo.rs`
- Utility lane proof: `docs/build-log/artifacts/syndai_agent_file_memory_001-trace-compare.json`
- Landscape owner doc: `docs/superpowers/specs/memphant/13-prior-art-and-competitive-spec.md`

## Landscape Review

GitHub metadata checked 2026-07-03 with GraphQL repository metadata for the
known threshold set. Every verified project at or above 50,000 stars appears in
`13` or has a recorded exclusion row. BMAD crossed the exact threshold since the
previous snapshot and was already listed in `13` §1.4.

```text
mem0ai/mem0: 60018 stars
openclaw/openclaw: 381583 stars
NousResearch/hermes-agent: 208477 stars
MemPalace/mempalace: 56914 stars
obra/superpowers: 245208 stars
bmad-code-org/BMAD-METHOD: 50039 stars
```

## Verification

```text
python3 -m pytest tests/test_standing_quality_bars.py -q
PASS: 4 passed
```

```text
cargo test -p memphant-core --test hot_path_slo
PASS: fast_mode_recall_holds_release_hot_path_slo
```

## Final Release Sweep

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
cargo run -p memphant-eval -- syndai-trace-compare examples/syndai/file-memory-trace-compare.yaml --archive-traces
PASS: syndai_trace_compare=pass id=syndai_agent_file_memory_001 surface=agent_file_memory recall=1 archive=docs/build-log/artifacts/syndai_agent_file_memory_001-trace-compare.json
```

```text
cargo run -p memphant-cli -- db bootstrap-check --provider plain-postgres
cargo run -p memphant-cli -- db bootstrap-check --provider supabase
cargo run -p memphant-cli -- db bootstrap-check --provider neon
PASS: all providers bootstrap_check=clean and migration_lint=clean
```

```text
cd web && npm test
PASS: 6 passed
```

```text
python3 scripts/check_spec_drift.py
PASS: spec_drift=clean
```

```text
python3 scripts/validate_docs.py  # Syndai mirror
PASS: Documentation consistency check passed
WARN: pre-existing provider verification staleness warnings remain
```

## Status

Standing quality bars are complete for this release pass. With all §1-§6
checkboxes checked, `STATUS.md` can move to the `COMPLETE` banner.

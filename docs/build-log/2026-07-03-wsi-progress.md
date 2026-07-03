# WS-I Progress - Advanced Lever Activation Audit

## Exit Packet

- Added `memphant-eval profile <profile.yaml> --compare-to <baseline> [--archive <path>]`.
- Added `examples/evals/wsi-profile.yaml` as the local SOTA profile/audit fixture.
- Archived the profile at `docs/build-log/artifacts/wsi-local-sota-profile.json`.
- The profile records:
  - rungs 0-3 built and covered by existing golden/security/Syndai trace evidence;
  - 0 advanced levers activated;
  - 15 advanced levers dormant because their promotion gates are not met;
  - public benchmark axes explicitly marked `not_run` instead of inferred.

## Verification

```bash
cargo run -p memphant-eval -- profile examples/evals/wsi-profile.yaml --compare-to rungs-0-3-baseline --archive docs/build-log/artifacts/wsi-local-sota-profile.json
# profile=pass id=wsi_local_gate_profile_001 compare_to=rungs-0-3-baseline activated=0 dormant=15 retired=0 archive=docs/build-log/artifacts/wsi-local-sota-profile.json

cargo test -p memphant-eval --test profile_contract
# 2 passed

cargo fmt --check
# pass

docker compose config
# pass

python3 -m pytest tests
# 25 passed

npm test
# 6 passed

cargo clippy --all-targets --all-features -- -D warnings
# pass

cargo test --all-targets --all-features
# pass

cargo test --doc
# pass

python3 scripts/check_spec_drift.py
# spec_drift=clean public=/Users/sidsharma/Memphant/docs/superpowers/specs/memphant private=/Users/sidsharma/Syndai/.wt/codex-memphant-cross-repo/docs/superpowers/specs/memphant
```

## Gate Notes

- Alpha gate is checked: the repo now has retain/reflect/recall/correct/forget, REST/MCP/Python SDK surfaces, DB exposure lint, security/deletion lanes, trace archive, and Syndai trace compare proof.
- Dogfood, public launch, restraint, GateMem, hot-path SLO, `memory_utility_trend`, and landscape-completeness stay unchecked because they require live or external benchmark/adoption evidence that this local repo cannot honestly manufacture.
- No public SOTA claim is made from this profile.

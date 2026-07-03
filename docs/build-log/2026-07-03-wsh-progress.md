# WS-H Progress - BYOC, Hosted Packaging, and Deployment

## Exit Packet

- Added `Dockerfile` for the Rust server, worker, and CLI in one non-root runtime image.
- Added `compose.yaml` with `pgvector/pgvector:0.8.4-pg17`, localhost-only ports, Postgres `pg_isready`, and `service_healthy` dependencies.
- Added provider profiles:
  - `deploy/provider-profiles/plain-postgres.env.example`
  - `deploy/provider-profiles/supabase.env.example`
  - `deploy/provider-profiles/neon.env.example`
- Added `memphant db bootstrap-check --provider <plain-postgres|supabase|neon> [--profile <env-file>]`.
- Added hosted hook contract at `deploy/hosted/control-plane-hooks.json`.
- Added deployment runbooks under `docs/deployment/`.
- Added WS-H regression coverage:
  - `crates/memphant-cli/tests/bootstrap_check.rs`
  - `tests/test_wsh_deployment_contract.py`

## Verification

```bash
cargo fmt --check
# pass

cargo run -p memphant-cli -- db bootstrap-check --provider plain-postgres
# bootstrap_check=clean provider=plain-postgres profile=deploy/provider-profiles/plain-postgres.env.example
# migration_lint=clean provider=plain-postgres

cargo run -p memphant-cli -- db bootstrap-check --provider supabase
# bootstrap_check=clean provider=supabase profile=deploy/provider-profiles/supabase.env.example
# migration_lint=clean provider=supabase

cargo run -p memphant-cli -- db bootstrap-check --provider neon
# bootstrap_check=clean provider=neon profile=deploy/provider-profiles/neon.env.example
# migration_lint=clean provider=neon

docker compose config
# pass

python3 -m pytest tests
# 25 passed

cargo clippy --all-targets --all-features -- -D warnings
# pass

cargo test --all-targets --all-features
# pass

cargo test --doc
# pass

python3 scripts/check_spec_drift.py
# spec_drift=clean public=/Users/sidsharma/Memphant/docs/superpowers/specs/memphant private=/Users/sidsharma/Syndai/.wt/codex-memphant-cross-repo/docs/superpowers/specs/memphant
```

## Notes

- The Supabase BYOC preflight is offline and fail-closed. It validates the profile boundary, then reuses the bundled migration lint that rejects `public.`/`syndai.` references, browser role grants, missing RLS, missing tenant policies, and missing tenant indexes.
- `docker compose config` validates the local stack shape here; no live customer Supabase/Neon database was touched.
- The restore runbook treats Postgres PITR as authoritative and blocks release on unexpected missing blobs.


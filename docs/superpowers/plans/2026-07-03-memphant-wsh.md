# WS-H Plan - BYOC, Hosted Packaging, and Deployment

## Scope

- Add Docker and Compose packaging for the Rust server/worker/CLI.
- Add provider bootstrap profiles for plain Postgres, Supabase BYOC, and Neon.
- Add an offline `memphant db bootstrap-check` gate for profile and migration invariants.
- Document Supabase BYOC, self-host, hosted control-plane hooks, and backup/PITR reconciliation.
- Prove the deployment files with focused tests plus the existing full gates.

## Exit Proof

- `cargo run -p memphant-cli -- db bootstrap-check --provider plain-postgres`
- `cargo run -p memphant-cli -- db bootstrap-check --provider supabase`
- `cargo run -p memphant-cli -- db bootstrap-check --provider neon`
- `python3 -m pytest tests`
- `cargo fmt --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --all-targets --all-features`
- `cargo test --doc`
- `python3 scripts/check_spec_drift.py`


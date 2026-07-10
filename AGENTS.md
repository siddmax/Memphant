# MemPhant Agent Instructions

MemPhant is the public Apache-2.0 memory substrate repo. Treat `docs/superpowers/specs/memphant/STATUS.md` as the live ledger and flip checkboxes only with the proof artifact named in the same change.

## Repo Boundaries

- Public product work lives in this repo: Rust crates, migrations, the Python SDK, public docs, public fixtures, provider lint, and the self-hostable runtime.
- Private Syndai integration and porting boundaries are described in `porting.md`; do not track a local Syndai worktree path in this repo.
- Keep mirrored MemPhant spec files drift-free when a private Syndai checkout is available.
- Never commit secrets. Use `.env.example` for local variable names only.

## Database Rules

- MemPhant-owned database objects must live in the `memphant` schema.
- Do not create or modify application objects in `public`.
- Tenant identity is derived server-side from API keys; every tenant-scoped read/write is tenant-bound (traces included).
- Keep provider lint green for `plain-postgres`, `supabase`, and `neon`.

## Working Rules

- Use current docs when touching libraries, providers, CLIs, or cloud services; prefer Context7 plus official web docs.
- Fix root causes and add tests or contract checks for regressions.
- Do not add compatibility shims, feature-flag rot, or temporary bypass paths in this pre-production repo.
- Preserve unrelated dirty work in this repo and in any private Syndai checkout.
- `openapi/memphant.v1.json` and `mcp/memphant.tools.v1.json` are generated artifacts — regenerate via the server/mcp binaries, never hand-edit.

## Verification

Run the narrowest meaningful checks while iterating, then the full gate before claiming a workstream exit:

```sh
python3 -m pytest tests/ spikes/python-retain/test_spike.py -q
python3 scripts/check_spec_drift.py
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
cargo test --doc
# Live-Postgres contract + worker-binary smoke tests: #[ignore]d by default,
# only run with MEMPHANT_TEST_DATABASE_URL set (never point this at a
# database another process is using — migrate a scratch database first).
MEMPHANT_TEST_DATABASE_URL=postgres://memphant:memphant@localhost:5432/memphant \
  cargo test -p memphant-store-postgres -p memphant-worker -- --ignored --test-threads=1
cargo run -p memphant-cli -- db lint --provider plain-postgres
cargo run -p memphant-cli -- db lint --provider supabase
cargo run -p memphant-cli -- db lint --provider neon
python3 scripts/apply_memphant_migrations.py --database-url postgres://memphant.invalid/memphant --dry-run
# Real binaries + real Postgres end-to-end probe; requires a running
# memphant-postgres-1 container (compose service `memphant-postgres`) on :5432.
DATABASE_URL=postgres://memphant:memphant@localhost:5432/memphant bash scripts/e2e_probe.sh
```

# MemPhant Agent Instructions

MemPhant is the public Apache-2.0 memory substrate repo. Treat `docs/superpowers/specs/memphant/STATUS.md` as the live ledger and flip checkboxes only with the proof artifact named in the same change.

## Repo Boundaries

- Public product work lives in this repo: Rust crates, migrations, SDKs, public docs, public fixtures, provider lint, and the self-hostable runtime.
- Private Syndai integration work lives in `/Users/sidsharma/Syndai/.wt/codex-memphant-cross-repo`; keep it linked through `.codex/linked-repos.json`.
- Keep mirrored MemPhant spec files drift-free between this repo and the linked Syndai worktree.
- Never commit secrets. Use `.env.example` for local variable names only.

## Database Rules

- MemPhant-owned database objects must live in the `memphant` schema.
- Do not create or modify application objects in `public`.
- Tenant and scope identifiers are part of every tenant-scoped API and storage boundary.
- Keep provider lint green for `plain-postgres`, `supabase`, and `neon`.

## Working Rules

- Use current docs when touching libraries, providers, CLIs, or cloud services; prefer Context7 plus official web docs.
- Fix root causes and add tests or contract checks for regressions.
- Do not add compatibility shims, feature-flag rot, or temporary bypass paths in this pre-production repo.
- Preserve unrelated dirty work in this repo and in the linked Syndai worktree.

## Verification

Run the narrowest meaningful checks while iterating, then the full gate before claiming a workstream exit:

```sh
python3 -m pytest tests/test_repo_contract.py tests/test_wsa_migration_contract.py spikes/python-retain/test_spike.py -q
python3 scripts/check_spec_drift.py
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
cargo test --doc
cargo run -p memphant-cli -- db lint --provider plain-postgres
cargo run -p memphant-cli -- db lint --provider supabase
cargo run -p memphant-cli -- db lint --provider neon
python3 scripts/apply_memphant_migrations.py --database-url postgres://memphant.invalid/memphant --dry-run
```

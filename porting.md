# MemPhant Porting Notes

MemPhant is the public product boundary. Syndai integration code stays in the
private Syndai repository until a surface is generalized enough to belong here.

## Repo Boundary

- Public MemPhant work lives here: Rust crates, migrations, SDKs, public docs,
  public fixtures, provider lint, and the self-hostable runtime.
- Private Syndai work lives in Syndai. Do not track a local Syndai worktree path
  or commit SHA in this repo.
- When mirrored specs need a drift check, run `python3 scripts/check_spec_drift.py`.
  The script compares this repo with `MEMPHANT_PRIVATE_SPEC_DIR` when set, then
  falls back to a sibling checkout at `../Syndai/docs/superpowers/specs/memphant`
  if it exists.

## Porting Rule

Port code from Syndai into MemPhant only when it is product-neutral and can be
tested through public MemPhant contracts. Keep Syndai tenant wiring, private
fixtures, hosted credentials, and app-specific adapters out of this repository.

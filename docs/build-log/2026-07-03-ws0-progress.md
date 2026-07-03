# WS-0 Progress

## Changed

- Added the public MemPhant repo skeleton on branch `codex/memphant-ws0`.
- Added the required governance docs: `LICENSE`, `README.md`, `SECURITY.md`, and `CONTRIBUTING.md`.
- Added repo hygiene files: `.gitignore`, `.gitattributes`, `.editorconfig`, `.env.example`, `rust-toolchain.toml`, `rustfmt.toml`, and checked-in `Cargo.lock`.
- Added `memphant.lock` with the WS-0 schema snapshot keys.
- Added `scripts/check_spec_drift.py` and `.codex/linked-repos.json` verification coverage so the public spec copy is checked against `/Users/sidsharma/Syndai/.wt/codex-memphant-cross-repo`.
- Added a minimal Rust workspace with the eight WS-0 crates named by `03-engineering-spec.md`; the R83 spike crate is explicitly excluded from the product workspace.
- Added shared spike fixtures plus Python and Rust retain/golden-runner spikes.
- Added `scripts/run_spike.py`; the Rust toolchain is pinned by `rust-toolchain.toml` to `1.96.1` with `rustfmt` and `clippy`.
- Recorded the WS-0 R83 spike result in `docs/build-log/artifacts/ws0-two-language-spike.json`, updated `STATUS.md`, and confirmed Decision #2 remains Rust-first in `26-decision-register.md`.

## Proof

- `python3 -m pytest tests/test_repo_contract.py spikes/python-retain/test_spike.py -q`
  - Result: `7 passed in 0.08s`
- `python3 scripts/check_spec_drift.py`
  - Result: `spec_drift=clean public=/Users/sidsharma/Documents/Memphant/docs/superpowers/specs/memphant private=/Users/sidsharma/Syndai/.wt/codex-memphant-cross-repo/docs/superpowers/specs/memphant`
- `~/.cargo/bin/rustc --version && ~/.cargo/bin/cargo fmt --check`
  - Result: `rustc 1.96.1 (31fca3adb 2026-06-26)` and formatting passed.
- `~/.cargo/bin/cargo metadata --format-version 1 --no-deps`
  - Result: `workspace_members=memphant-cli,memphant-core,memphant-eval,memphant-mcp,memphant-server,memphant-store-postgres,memphant-types,memphant-worker`
- `~/.cargo/bin/cargo clippy --all-targets --all-features -- -D warnings`
  - Result: passed.
- `~/.cargo/bin/cargo test --doc`
  - Result: passed doc tests for `memphant-core`, `memphant-eval`, `memphant-store-postgres`, and `memphant-types`.
- `python3 scripts/run_spike.py`
  - Result: `artifact=/Users/sidsharma/Documents/Memphant/docs/build-log/artifacts/ws0-two-language-spike.json`, `ratio=0.073`, `decision=rust_proceeds`.

## R83 Decision Result

The spike passed the R83 Rust-first threshold:

- Measurement mode: median of five warm no-recompile policy-runner invocations; Rust `cargo build` recorded separately and excluded from the policy-change ratio.
- Python policy-change median: `0.03419108397793025s`
- Rust policy-change median: `0.002484540920704603s`
- Rust/Python policy-change ratio: `0.0726663396313592`
- Decision: `rust_proceeds`

WS-0's exit packet is recorded. WS-A can begin after reconciling the remaining §1 preflight-history checkbox if current Syndai proof is required for the ledger.

## Next

- Start WS-A: schema, core types, and store seam.
- If the remaining §1 preflight-history checkbox needs live Syndai proof rather than the clean remote-main spec copy, run the Syndai preflight loop in the linked worktree and update the ledger with that proof.

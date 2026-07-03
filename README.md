# MemPhant

MemPhant is the Apache-2.0, Rust-first memory substrate for long-running agents. It stores recoverable episodes, compiles cited memory units, retrieves scoped evidence, and keeps poisoning, tenant isolation, correction, and forgetting inside one auditable contract.

This repository is the public product boundary. It owns the Rust crates, Postgres and pgvector schema, public API, MCP server, CLI, SDKs, eval harness, synthetic fixtures, docs, and self-host packaging. The hosted control plane, credentials, private corpora, and Syndai adapter stay outside this repository.

## Repository Split

- Public MemPhant: product code, public schemas, public tests, synthetic examples, provider bootstrap and lint code.
- Syndai adapter: private dogfood integration work in `/Users/sidsharma/Syndai/.wt/codex-memphant-cross-repo` until a surface is generalized.
- No hidden hosted behavior: hosted MemPhant must run the same public binary self-hosters run.

## Current State

Build state is tracked in `docs/superpowers/specs/memphant/STATUS.md`. WS-0 has an exit artifact, and the R83 Rust-vs-Python two-language spike kept the Rust-first posture: warm no-recompile Rust policy iteration measured at `0.073x` Python.

## Local Checks

```bash
python3 -m pytest tests/test_repo_contract.py -q
python3 scripts/check_spec_drift.py
~/.cargo/bin/cargo metadata --format-version 1 --no-deps
```

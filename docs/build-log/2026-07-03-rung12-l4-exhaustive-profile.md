# Rung 12 L4 Exhaustive Recall

## Changed
- Added explicit `mode=exhaustive` L4 raw-episode scan behavior in `memphant-core`; fast and balanced keep their existing scan depth and do not auto-escalate to L4.
- Added additive trace contract fields for L4 evidence: `RecallChannel::Exhaustive`, `l4_sandbox_id`, `l4_gathered_evidence_ids`, `l4_exhaustive_enabled`, and deeper `iterative_scan_depth`.
- Added `--disable-l4-exhaustive` / `EvalRunOptions::l4_exhaustive_enabled` for paired no-L4 sampled controls.
- Added the `l4_exhaustive_raw_episode_buried` golden, sampled Rung 12 suites, profile fixture, and profile validator requirements.

## Proof
- command: `cargo test -p memphant-core --test recall_trace_golden exhaustive_mode_gathers_buried_raw_episode_evidence_without_changing_fast_mode`
- command: `cargo test -p memphant-eval --test eval_contract rung12_l4_exhaustive_suite_proves_raw_episode_delta`
- command: `cargo test -p memphant-eval --test profile_contract rung12_promotion_requires_l4_sample_and_no_l4_control`
- command: `cargo run -p memphant-eval -- run examples/evals/golden.yaml --archive-traces --archive-dir docs/build-log/artifacts`
- command: `cargo run -p memphant-eval -- run benchmarks/rung12-baseline-sampled.yaml --disable-l4-exhaustive --archive-traces --archive-dir docs/build-log/artifacts` (expected control failure: `passed=0/1`)
- command: `cargo run -p memphant-eval -- run benchmarks/rung12-l4-exhaustive-sampled.yaml --archive-traces --archive-dir docs/build-log/artifacts`
- command: `cargo run -p memphant-eval -- profile examples/evals/rung12-l4-exhaustive-profile.yaml --compare-to rungs-0-11-baseline --archive docs/build-log/artifacts/rung12-l4-exhaustive-profile.json`
- command: `cargo fmt --check`
- command: `cargo clippy --all-targets --all-features -- -D warnings`
- command: `cargo test --all-targets --all-features`
- command: `cargo test --doc`
- command: `python3 -m pytest tests -q`
- command: `cargo run -p memphant-eval -- verify-golden examples/evals/golden.yaml`
- command: `cargo run -p memphant-eval -- security examples/evals/security-smoke.yaml`
- command: `cargo run -p memphant-cli -- db lint --provider plain-postgres`
- command: `cargo run -p memphant-cli -- db lint --provider supabase`
- command: `cargo run -p memphant-cli -- db lint --provider neon`
- command: `python3 scripts/apply_memphant_migrations.py --database-url postgres://memphant.invalid/memphant --dry-run`
- command: `cargo run -p memphant-cli -- db bootstrap-check --provider plain-postgres`
- command: `cargo run -p memphant-cli -- db bootstrap-check --provider supabase`
- command: `cargo run -p memphant-cli -- db bootstrap-check --provider neon`
- command: `python3 scripts/check_spec_drift.py`
- artifact: `docs/build-log/artifacts/pr-golden-traces.json`
- artifact: `docs/build-log/artifacts/rung12-baseline-sampled-traces.json`
- artifact: `docs/build-log/artifacts/rung12-l4-exhaustive-sampled-traces.json`
- artifact: `docs/build-log/artifacts/rung12-l4-exhaustive-profile.json`

## Not Built
- No auto-escalation from fast/balanced to exhaustive; the contract remains explicit opt-in or benchmark mode only.
- No learned reranker, learned DSR fitter, ablation voting, external graph/vector engine, or inferred-belief composition; those remain gated by later rungs.
- No real sandbox/file walker provider. This rung adds the deterministic in-memory L4 raw-episode scan and trace contract needed to prove the mode before provider-backed expansion.

## Next
- Rung 13: learned rerank/DSR only if archived traces show fixed deterministic rules leave a paired held-out win on the table.

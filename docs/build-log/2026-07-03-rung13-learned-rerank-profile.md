# Rung 13 Learned Rerank Profile

## Scope

Rung 13 promoted the learned reranker side of `learned rerank/DSR` with an archived memory-tuned linear rerank profile over the existing protected top-k. The implementation does not add retrieval fanout or a model runtime; it accepts a traceable `LearnedRerankProfile` and scores only the already bounded rerank input set.

The learned DSR/FSRS fitter remains dormant. Current `fsrs-rs` training docs expose `compute_parameters(ComputeParametersInput { train_set: Vec<FSRSItem>, .. })` and state that good fitting needs review histories from many cards. The Rung 13 public profile is a rank-sensitive rerank proof, not a many-card MemPhant-native review-history floor.

## External Evidence Checked

- Context7 `/open-spaced-repetition/fsrs-rs`: confirmed the optimizer API uses `FSRSItem { reviews: Vec<FSRSReview> }` and needs many-card review histories for parameter fitting.
- Web/arXiv: MemReranker and ConvMemory evidence supports memory-tuned protected-top-k reranking, while generic learned rerankers are not enough to justify adoption without a MemPhant paired delta.

## Implementation

- Added `LearnedRerankProfile` to `memphant-types`.
- Added `RecallRequest.learned_rerank_profile` and `RetrievalTrace.learned_rerank_training_set_id`.
- Preserved `deterministic-local-v1` as the default reranker and `weight_vector_id=default` when no learned profile is supplied.
- Added learned-profile validation for non-empty IDs and finite weights.
- Added eval support for fixture-scoped learned profiles and `--disable-learned-rerank` paired controls.
- Added `learned_rerank_memory_tuned_runbook` golden fixture and Rung 13 sampled suites.
- Added Rung 13 SOTA profile validation requiring the learned sample, no-learned-rerank control, archived training-set ref, and delta CI above the 0.03 gate.

## Proof Artifacts

- Learned profile pass: `docs/build-log/artifacts/rung13-learned-rerank-sampled-traces.json`
- No-learned-rerank control: `docs/build-log/artifacts/rung13-baseline-sampled-traces.json`
- SOTA profile archive: `docs/build-log/artifacts/rung13-learned-rerank-profile.json`

## Focused Verification

```text
cargo test -p memphant-core --test recall_trace_golden learned_rerank_profile_reorders_protected_topk_and_traces_training_set
cargo test -p memphant-eval --test eval_contract rung13_state_style_suite_proves_learned_rerank_delta
cargo test -p memphant-eval --test profile_contract rung13_profile_archives_learned_rerank_promotion
cargo test -p memphant-eval --test profile_contract rung13_promotion_requires_learned_sample_control_and_training_set
cargo test -p memphant-eval --test eval_contract trace_schema_snapshot_is_current
cargo run -p memphant-eval -- run benchmarks/rung13-learned-rerank-sampled.yaml --archive-traces --archive-dir docs/build-log/artifacts
cargo run -p memphant-eval -- run benchmarks/rung13-baseline-sampled.yaml --disable-learned-rerank --archive-traces --archive-dir docs/build-log/artifacts
cargo run -p memphant-eval -- profile examples/evals/rung13-learned-rerank-profile.yaml --compare-to rungs-0-12-baseline --archive docs/build-log/artifacts/rung13-learned-rerank-profile.json
```

Expected control result:

```text
eval=fail id=rung13-baseline-sampled passed=0/1
case=learned_rerank_memory_tuned_runbook error=None
```

## Full Verification

```text
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
cargo test --doc
python3 -m pytest tests -q
cargo run -p memphant-eval -- verify-golden examples/evals/golden.yaml
cargo run -p memphant-eval -- security examples/evals/security-smoke.yaml
python3 scripts/check_spec_drift.py
```

Results:

```text
cargo fmt --check: pass
cargo clippy --all-targets --all-features -- -D warnings: pass
cargo test --all-targets --all-features: pass
cargo test --doc: pass
python3 -m pytest tests -q: 25 passed
verify_golden=pass cases=13
security=pass lanes=poisoning,query_filter_injection,high_risk_action_suppression,tenant_leakage,deletion_completeness deletion_completeness=pass
spec_drift=clean public=/Users/sidsharma/Memphant/docs/superpowers/specs/memphant private=/Users/sidsharma/Syndai/.wt/codex-memphant-cross-repo/docs/superpowers/specs/memphant
```

## Promotion Decision

Promote learned reranker behavior for profiled modes: `rung13_learned_rerank_profile_001` recovered the atlas rollback runbook while the same fixture with learned rerank disabled returned the lexical decoy. Keep learned DSR/FSRS fitting dormant until enough MemPhant-native review histories exist to train and hold out a real parameter fit.

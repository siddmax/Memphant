# Temporal benchmark adapter: first durable slice

Date: 2026-07-13

This slice adds the smallest benchmark boundary that can use an upstream native
metric unchanged. It does not claim a STALE score and does not mark Task 4
complete.

## Selection

STALE was selected first because all three promotion-critical pieces are public:

- official code at `icedreamc/STALE`, pinned to
  `ea7d391103a151927cd29d2f01d87597a782bdcb`;
- the official 400-record dataset at `STALEproj/STALE`, pinned to repository
  revision `617c51dc200b5ab09970834144c7e51c77959af0` and file SHA-256
  `5f3ec375179e20e2e94469e018189188f34e2e7e5f21cbecbd99fcfa648c1876`;
- the upstream all-in-one judge, which scores State Resolution, Premise
  Resistance, and Implicit Policy Adaptation in one native call per record.

The adapter in `scripts/run_stale.py` deliberately does not reproduce STALE's
judge prompt or accuracy calculation. It verifies the immutable checkout,
dataset, and exact answer pairing, then invokes upstream
`STALE/Evaluation/full_eval_performance.py` as a subprocess. Missing rows,
duplicate rows, empty dimension answers, source drift, dataset drift, missing
result rows, and upstream judge errors all fail the run. A successful run emits
the untouched native result plus a proof sidecar containing input and result
hashes.

## Other named benchmarks

- **Memora/FAMA:** publicly runnable. `geniesinc/Memora` main was
  `a6493188efc836d6511ed5e4163fe3ba87da30ff`. It ships all 30
  period/persona datasets and its native multi-judge FAMA evaluator. It is a
  larger adapter because a memory system must ingest roughly 27,000 session
  files, answer questions with a fixed reader, and then run three judges per
  criterion. It should follow STALE rather than be reduced to a locally
  reimplemented FAMA formula.
- **PS-Bench:** code, data, and the native Longformer response classifier are
  public at `MuyuenLP/PS-Bench` revision
  `210e72ea8352a1700141476bfde1f153a3a826e4`. The classifier model is public at
  `LibrAI/longformer-action-ro` revision
  `bb1f0a07dcb55ae0e9af5c5431ea8075f9a92c92`. The GitHub repository has no
  declared license, so raw benchmark data or code must not be copied into
  MemPhant. A later adapter may execute a user-supplied checkout in place after
  legal provenance is resolved.
- **OP-Bench:** not publicly runnable as of this check. The official paper
  specifies 1,700 reviewed instances and its irrelevance, sycophancy, and
  repetition metrics, but exposes no official dataset, scorer, code repository,
  or immutable release. No substitute dataset or locally reconstructed scorer
  should be labeled OP-Bench.

Primary sources:

- https://github.com/icedreamc/STALE
- https://huggingface.co/datasets/STALEproj/STALE
- https://arxiv.org/abs/2605.06527
- https://github.com/geniesinc/Memora
- https://arxiv.org/abs/2604.20006
- https://github.com/MuyuenLP/PS-Bench
- https://arxiv.org/abs/2601.17887
- https://arxiv.org/abs/2601.13722

## Verification

Red phase:

```text
python3 -m pytest tests/test_temporal_benchmark_contract.py -q
4 failed
```

Green phase and full immutable-input smoke:

```text
python3 -m pytest tests/test_temporal_benchmark_contract.py -q
4 passed

python3 scripts/run_stale.py \
  --official-repo /tmp/memphant-task4-research/stale \
  --dataset /tmp/memphant-task4-research/T1_T2_400_FULL.json \
  --answers /tmp/memphant-task4-research/stale-placeholder-answers.json \
  --out /tmp/memphant-task4-research/not-run.json \
  --verify-only
# exit 0
```

The downloaded official dataset was independently checked as 305,908,212
bytes, SHA-256
`5f3ec375179e20e2e94469e018189188f34e2e7e5f21cbecbd99fcfa648c1876`,
and 400 records.

## Remaining blocker

The scorer boundary is runnable, but MemPhant has not yet produced the 400
complete answer rows. The next slice must ingest each STALE history once,
issue all three probes independently against the same frozen memory state, and
write the exact upstream answer schema. Only then should the paid native judge
run. Placeholder answers above were used only for `--verify-only`; no score was
computed and no model response was judged.

## Generation adapter added

`scripts/generate_stale_memphant_answers.py` now supplies the previously
missing answer-generation boundary without running the paid 400-record job.
For every pending UID it:

1. creates a distinct tenant plus deterministic scope and actor;
2. whitelists only `haystack_session`, `timestamps`, and `probing_queries`;
3. ingests one episode per session in timestamp order through the real server;
4. performs one global worker drain after all pending histories are ingested;
5. issues SR, PR, and IPA independently against the unchanged post-drain state;
6. uses the frozen Sol Pro high reader and the frozen exhaustive top-10,
   8,192-token evidence budget;
7. writes upstream-compatible `target_model_responses`, per-dimension trace and
   evidence hashes, exact runtime binary hashes, and resumable UID checkpoints.

The generation contract is pinned in
`benchmarks/manifests/stale_generation.v1.json`. Prompt, structured-output,
dataset-lock, reader-lattice, OpenAPI, reader-runtime, and executable hashes
are fail-closed. Existing answer/proof rows resume only when their complete UID
sets, generation fingerprint, answer hashes, and three trace records agree.
The request uses the canonical snapshot slug
`openai/gpt-5.6-sol-pro-20260709` directly rather than relying on the movable
`openai/gpt-5.6-sol-pro` alias.

The synthetic fixture dry-run passed without Postgres, runtime binaries, or an
OpenRouter key:

```text
python3 scripts/generate_stale_memphant_answers.py \
  --dataset tests/fixtures/stale_generation_small.json \
  --out /tmp/stale-generation-dry-run.json \
  --cache-dir /tmp/stale-generation-cache --fixture --dry-run
# record_count=1 session_count=2 source_status=dry_run_no_answers
```

The same no-model dry-run validated all official rows:

```text
python3 scripts/generate_stale_memphant_answers.py \
  --dataset /tmp/memphant-task4-research/T1_T2_400_FULL.json \
  --out /tmp/stale-generation-official-dry-run.json \
  --cache-dir /tmp/stale-generation-cache --dry-run
# record_count=400 session_count=20000 source_status=dry_run_no_answers
# body_sha256=37ce8036fe277e939401dd828802b8ab948fb13d2b03c0f280d71d4e5eb36108
# query_sha256=ca91de73a3798c0faa0b49d7a2d2252d54037b8198130f558a0d5b746324026f
```

Focused contract verification after this addition: `9 passed`. The paid
1,200-response generation and 400-call native judge were intentionally not
launched.

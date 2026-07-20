# Temporal model admission and rollup root cause

The paired five-contract screen ran the same food, steps, goal, deletion, and
long-context cases through the packaged OpenRouter structured-state path.
Flash and Luna Pro each passed 5/5 on the final schema with zero retries or
rejections and complete provider cost proof. Flash used Google AI Studio and
cost $0.044466; Luna used OpenAI and cost $0.0492662. The screen proved contract
admission only; it did not predict temporal answer quality.

The one approved full rung used Flash on the pinned Memora
`weekly/software_engineer` group. Extraction completed 163/163 episodes with
zero retries, decode errors, rejected operations, fallbacks, or unpriced calls;
161 operations were accepted. Extraction cost $1.981869 and the 15 answer calls
cost $0.1294365. The official three-judge scorer completed all 71 subquestions:

- FAMA: 19.539682539682538
- overall: 31/71
- memory presence: 11/44
- forgetting absence: 20/27
- reasoning: 0/9, FAMA 0

This rejects Flash as the accuracy product tier. The prior Luna pilot remains
higher at FAMA 32.96, but the runs have different compiler/schema identities
and are not a paired promotion.

No Terra, Grok, MiniMax, or Muse run is warranted yet. The current generator's
single `--model` value controls both structured extraction and answer reading,
and its scratch database is dropped at exit. Re-running another model would
therefore repay 163 extractions and conflate writer, retrieval, and reader
quality. The next harness prerequisite is a content-addressed extracted-bank
snapshot that can be restored into isolated scratch databases, with extractor
model/compiler/ledger hashes fixed independently of the reader model. Luna is
the retained development reader; Terra becomes eligible only if Luna misses a
corrected answer-bearing pack. Flash remains rejected, while Grok and MiniMax
lack local memory-specific evidence and Muse would add an unneeded provider.

The minimum reader-only causal check is now complete. Five corrected packs were
derived from the pinned official activity rows and goal provenance, without a
new extraction or paid judge. Luna returned all exact targets and statuses in
five priced calls for $0.034025:

- total food spending: $258.23
- total steps: 51,268
- coffee spending: $79.49
- coffee goal: no, $79.49 exceeds $30
- daily step goal: no, 7,324 is below 8,000

This proves Luna can compose the missing answers once the substrate supplies
them, so Terra is not run. It remains reader/oracle mechanism evidence, not an
end-to-end benchmark promotion. The first attempted request also exposed a
shared routing bug: the generic OpenRouter reader sent `temperature=0` to Luna,
whose current endpoints reject that parameter under `require_parameters=true`.
The reader now omits temperature for Luna, matches the already-correct extractor
path, and has a focused regression. The failed 404 attempt was unpriced and is
preserved separately; the successful five-call ledger is exact.

The reasoning traces identify a substrate failure, not an answer-model failure:
the exact, provenance-carrying quantity rollup was generated but ordinary
fusion buried it at rank 80, outside the ten-item pack. The runtime now treats
query/window-specific deterministic rollups as authoritative pack items. A
regression with 100 stronger lexical distractors fails before the fix and passes
after it; all ten quantity-rollup contracts pass. No new full model run is
authorized by this local mechanism proof.

Proof hashes:

- Flash screen: `1349a7c7727b4458542e689e29fb7208adcf4f472c94cf2952d2813f809cfdfe`
- Luna screen: `1f0956f73c845e512f11b276037b1069918d048c5bd1e6abf09b61e24cc3aeb0`
- Flash generation proof: `9eed44a54abd7e68fc60b7eccfd41c7e9755ad30dc89e539e1a91bfc472cd53c`
- Flash official FAMA: `1c88327a7c84ba4807f63c534009650b765dce0f1842da6e82d07bd0a1cac5ea`
- Luna corrected-pack report: `284c4a79a9ac5eca9a3c3eb142b262e5274a0fd8b860c4039635301bde8c6939`
- Luna corrected-pack ledger: `b318ebef793ea1a5097e850823e5238d7e1d56747488d38aee3e71cab5af3683`

Focused verification:

```text
python3 -m pytest tests/test_run_reader_contract.py tests/test_memora_benchmark_contract.py -q
72 passed

cargo test -p memphant-runtime structured_state_openrouter::tests --lib -- --skip live_structured_state_smoke
21 passed

cargo test -p memphant-core --test quantity_rollup
10 passed

python3 -m pytest tests/test_run_reader_contract.py tests/test_memora_benchmark_contract.py tests/test_gate_runtime.py tests/test_validate_memora_reasoning_proof.py -q
118 passed

python3 -m pytest tests/test_run_reader_contract.py tests/test_validate_memora_reasoning_proof.py -q
61 passed
```

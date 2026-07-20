# STALE wiring smoke: four-call rung passed

## Outcome

The authorized one-scenario STALE smoke stopped after its first Luna reader
attempt. No retry and no official judge call ran. The smoke remains
promotion-ineligible and proves no memory-quality result.

The completed attempt used `openai/gpt-5.6-luna-pro`, returned 52,266 prompt
tokens and 375 completion tokens, cost $0.03522315, and recorded response ID
`gen-1784087092-F8lWBGE5JJLT98vY402J` with provider `OpenAI`, retry index
zero, and a validated response parse.

## Root cause and correction

The immediate OpenRouter completion identified its served model with the
mutable alias `openai/gpt-5.6-luna-pro`. The authoritative generation record
for the same response ID identified
`openai/gpt-5.6-luna-pro-20260709`, which is the pinned canonical snapshot.
The direct reader trusted the immediate alias and therefore failed the pin
before it could write an answer.

The shared SDK meter and direct reader now always prefer the authoritative
generation-record model when a generation lookup is available. Existing
request/result hashes, immediate usage, provider identity, and retry behavior
remain unchanged.

## Verification

- The regression failed before the correction on both direct-reader and
  SDK-metered paths, then passed after it.
- `python3 -m pytest tests/test_run_reader_contract.py
  tests/test_temporal_benchmark_contract.py
  tests/test_restraint_benchmark_contract.py -q` — 78 passed.
- The focused Python compilation gate passed.
- `git diff --check` passed.

## Artifacts and next gate

The partial attempt ledger, response cache, server log, dry-run, Python
dependency inventory, failure record, and SHA-256 manifest live under
`docs/build-log/artifacts/unified-sota-20260714/stale-wiring-smoke-20260715T034300Z/stale-prefix/`.

A fresh STALE attempt must use a new run directory and needs renewed paid
authorization. MemSyco, Memora, full STALE, STATUS movement, and product
cutover remain blocked.

## Retry hardening and no-cost preflight

Before any renewed spend, the smoke now fails closed at the real provider-call
boundary. Each STALE dimension permits exactly one OpenRouter completion
attempt; a transport failure records one terminal error row and cannot enter
the reader's internal retry loop. If the paid completion succeeds but its
generation-stat lookup fails, the direct reader and sync/async SDK meters retain
the immediate response ID, model/provider, usage/cost, latency, retry index,
parse status, and request/result hashes in that terminal error row. No cache or
successful result is written.

Promotion-ineligible STALE smokes also reject existing generation outputs,
proofs, checkpoints, attempt ledgers, server logs, non-empty reader caches,
native results, judge ledgers, and result proofs before scratch-runtime or judge
launch. Full-run resume behavior is unchanged. Generation and judge proofs now
fingerprint the shared attempt meter and STALE bootstrap.

No-cost evidence on 2026-07-15:

- The focused provenance gate passes: 88 tests, including request-shape and eventual-generation regressions.
- Python compilation, spec mirror drift, and `git diff --check` passed.
- PostgreSQL 17.10 was already running and healthy on `127.0.0.1:5432`.
- The pinned STALE revision/source hashes and 305,908,212-byte dataset hash
  verified.
- OpenRouter's live catalog contained the pinned Luna reader and Gemini judge;
  Doppler exposed the key without printing it.
- The three packaged binaries rebuilt successfully: server
  `c87f5a16cac4a47ec9c37ea633824cf496171d2819b3dea6b1924bbd8f433f72`,
  worker `76784d8aecd32cb0d696a257ce3a57ebd3a5260474a9889cf0c284a25b3e28c2`,
  and CLI `f3cabb07019ed3afe47fbd578d58660577c855c644ffed115c331be9685b6262`.
- A clean Python 3.10.19 environment resolved the 23 official dependencies.
- The pinned no-model dry-run produced one prefix UID, 50 sessions, and both
  promotion-ineligible smoke flags.
- The prior failed run's `SHA256SUMS` still verifies unchanged.

No completion or judge call was made during this hardening. The next action is
still a separately authorized fresh three-reader/one-judge smoke.

## Second authorized attempt: zero-spend API-contract stop

The fresh run at `stale-wiring-smoke-20260715T050534Z` stopped during its
first episode ingest, before any Luna reader or Gemini judge call. The live
packaged server returned HTTP 422 because the STALE adapter still supplied
`tenant_id` in the `/v1/episodes` body. The current public API intentionally
derives tenant ownership from the bearer key and rejects that field. Inspection
also found that both the retain and recall payloads omitted the canonical
`subject_id`, `agent_node_id`, and `subject_generation` context, so deleting
only the first rejected field would merely expose the next contract failure.

The adapter now derives deterministic subject, scope, actor, and agent-node IDs
per official UID, sends generation zero on both request paths, and never sends
tenant identity in either public request body. A regression exercises the
exact retain and recall payloads. The post-fix no-cost gate passed 87 focused
tests, Python compilation, spec-mirror drift, and `git diff --check`.

This attempt made zero provider calls, recorded zero reader/judge attempts, and
cost $0. Its dry-run, empty attempt ledger, server log, Python inventory,
failure record, and hash manifest are preserved under
`docs/build-log/artifacts/unified-sota-20260714/stale-wiring-smoke-20260715T050534Z/stale-prefix/`.
The scratch database was dropped and PostgreSQL remains in its observed
pre-run running state. Per the sealed stop contract, another fresh run still
requires a new directory and renewed explicit paid authorization.

## Third authorized attempt: zero-spend context-binding stop

The fresh run at `stale-wiring-smoke-20260715T051759Z` passed strict JSON
validation but stopped on the first episode with HTTP 403 `scope_denied`, again
before any reader or judge call. The second correction had invented stable
context UUIDs; the packaged runtime requires those identities to be resolved
and registered under the authenticated tenant through
`PUT /v1/context-bindings/{client_ref}`.

STALE now creates one replay-safe binding per official UID and uses the
returned subject, scope, actor, agent-node, and current subject generation for
both retain and recall. The shared benchmark HTTP client gained only the
missing one-line `PUT` method. The request regression now covers binding plus
both downstream payloads, and the complete no-cost gate remains green at 87
tests with compilation, spec drift, and diff checks passing.
A no-model packaged-server preflight then provisioned a scratch tenant and key,
resolved the binding, and retained one episode successfully; the scratch
database was dropped afterward.

This attempt made zero provider calls, recorded zero reader/judge attempts, and
cost $0. Its partial artifacts and failure proof are sealed under
`docs/build-log/artifacts/unified-sota-20260714/stale-wiring-smoke-20260715T051759Z/stale-prefix/`.
The scratch database was dropped and PostgreSQL remains in its original running
state. Another fresh run requires renewed explicit paid authorization.

## Final retries and passing rung

The `stale-wiring-smoke-20260715T053026Z` retry stopped before provider
activity because a concurrently rebuilt Postgres store had made the packaged
server stale. After the background verification completed, all three binaries
were rebuilt from stable inputs and the exact official UID binding/retain
preflight passed. The immutable zero-cost failure is sealed in that directory.

The `stale-wiring-smoke-20260715T053645Z` retry made one Luna completion and
then stopped when OpenRouter's immediate generation-stat lookup returned 404.
The same response ID resolved moments later with authoritative provider, model,
usage, and cost, proving eventual consistency. The existing shared lookup now
retries only free HTTP 404 metadata reads with bounded 1/2/4/8/16-second waits;
it never resends a paid completion. The regression raised the focused gate to
88 passing tests. That failed call cost $0.03615310 and remains separate from
the passing rung.

The fresh run at `stale-wiring-smoke-20260715T054021Z` passed:

- one official prefix UID and 50 sessions, extractor off;
- three non-degraded traces and three Luna reader attempts;
- one unchanged official Gemini judge attempt after verify-only passed;
- four unique response IDs, retry index zero, positive tokens/cost, validated
  parses and request/result hashes;
- canonical reader snapshot `openai/gpt-5.6-luna-pro-20260709` and judge served
  snapshot `google/gemini-3.1-flash-lite-preview-20260303`;
- `smoke_only=true`, `promotion_ineligible=true`, and a verified `SHA256SUMS`.

Reader usage was 146,821 prompt plus 3,073 completion tokens, costing
$0.10852960. Judge usage was 1,003 prompt plus 189 completion tokens, costing
$0.00053425. Total passing-rung cost was $0.10906385. Native smoke accuracy was
0/3; this is wiring evidence, not a quality or SOTA result, and moves no STATUS
checkbox. PostgreSQL remains in its observed running state and no scratch
database remains.

Reader response IDs were
`gen-1784094070-HskkHBZq2DR0go2t9gaf`,
`gen-1784094092-WdhWrkTJTQF9MNcVmuQL`, and
`gen-1784094109-zN010EMKc3T5xmeMJdTa`, all served by OpenAI. The judge response
ID was `gen-1784094226-8z5gc8SVVMesBRWQnqE3`, served by Google AI Studio.

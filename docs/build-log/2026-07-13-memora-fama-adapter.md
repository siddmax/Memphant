# Memora/FAMA immutable adapter audit

Date: 2026-07-13

## Outcome

Memora is now an independently pinned, gold-sealed benchmark input, real
packaged-runtime answer executor, and native scoring boundary. No paid answer
or judge call was launched in this slice, and FAMA was not reimplemented.

The official source is `geniesinc/Memora` at commit
`a6493188efc836d6511ed5e4163fe3ba87da30ff` (2026-06-03). That commit is
important: it fixes Track-2's silent single-judge fallback and makes the
published three-judge protocol available in strict mode. The repo and released
dataset are Apache-2.0.

The complete checked-in release contains:

- 27,614 dialogue sessions across 30 isolated period/persona groups;
- 600 questions: 150 weekly, 150 monthly, and 300 quarterly;
- 6,415 FAMA judge subquestions;
- 27,645 files / 114,657,782 bytes under `data/` (including the data README);
- an immutable framed tree digest of
  `12d63b7d86d8d1751ab4da1f282c5a0729a93ef87bd5ef720ccee85cffd47d58`.

## Information boundary

Only the observable dialogue is eligible for MemPhant ingestion:

- session: `session_id`, `date`, `persona`;
- turn: `turn`, `speaker`, `message`;
- query: `period`, `persona`, `question_id`, `question`, `question_date`,
  derived task type.

The adapter excludes `session_type`, `operation`, `operation_details`, and
`share_memory`. These are generation annotations, not dialogue presented to the
evaluated agent. It also excludes `memory_evidence`, `forgetting_evidence`, and
the entire `evaluation` rubric from MemPhant and the answer reader. Those fields
are loaded only inside the native scorer after answer generation.

Each period/persona pair is a separate tenant/scope/actor identity. The executor
retains sessions in `(date, session_id)` order, drains reflection before any
query, recalls independently with the frozen 10-item/8,192-token exhaustive
budget, and requires a non-degraded trace plus an evidence digest on every
answer. It checkpoints answers and proof atomically after each question and
resumes only when the complete generation fingerprint and every answer/proof
pair still match.

Memora is a final-state benchmark, not a historical snapshot benchmark. Its
official evaluator passes `session_date` as semantic context and explicitly does
not use it for date filtering. Accordingly, session dates determine ingest order
and remain visible in the retained dialogue text; `question_date` is reader
context only; recall runs against the current snapshot after the complete group
has been ingested and drained. The executor does not invent `valid_from` values
for whole dialogue sessions or pass `valid_at` for these final-state questions.

## Native scoring boundary

`scripts/run_memora_fama.py` verifies the official revision, scorer sources,
license, complete dataset tree, dataset counts, and exact answer/question
pairing. In scoring mode it imports the official
`evals/agent_eval/memory_to_answer.py`, injects only precomputed MemPhant
evidence and answers, and calls the official question evaluator and report
builder unchanged. The wrapper adds one stricter invariant: every one of the
6,415 subquestions must have valid results from exactly the published judges:

- `openai/gpt-4.1`;
- `anthropic/claude-haiku-4.5`;
- `google/gemini-2.5-flash`.

Any partial judge result fails the entire run. There is no single-judge fallback
and no locally copied FAMA formula or judge prompt.

## Benchmark decision

Memora/FAMA is strong evidence for personalized temporal memory—especially
updates, deletion/forgetting, remembering, reasoning, and recommendation over
week/month/quarter horizons. It is not sufficient by itself to select a unified
memory system for agents, RAG/KB, and codebases. It does not measure repository
exploration, provenance-sensitive knowledge retrieval, tool-loop behavior, or
production replacement risk. It must therefore remain one independent cell in
the broader benchmark ladder, not a promotion gate by itself.

The development answer reader is frozen to the current reader-lattice winner,
`openai/gpt-5.6-luna-pro` with high reasoning. Sol Pro remains the authority and
finalist reader elsewhere in the evaluation plan; it is not silently substituted
into this development run. Reader choice remains fixed across compared memory
arms so gains are attributable to the memory system rather than a stronger
answer model.

Before either packaged binary starts, the executor removes every inherited
`MEMPHANT_*` variable and installs one explicit behavior environment: local
`small` embeddings, 64-candidate recall, resource chunks off, cross-reranking
off, and frozen dormant reranker defaults. Doppler or shell state therefore
cannot silently change an arm. The effective environment is included in the
generation fingerprint.

The proof sidecar fingerprints the official source revision and dataset tree,
both manifests, reader lattice, packaged binaries, OpenAPI, runtime config,
prompt, selected model, and harness sources. It records per-question trace,
evidence hash, elapsed time, cache/fresh-call status, actual served model,
selected OpenRouter provider, token usage, response-reported cost, errors, and a
hard zero fallback count. Those reader facts have their own resume-validated
hash. Provider attempts—including failed HTTP/provider attempts—are cumulative
across checkpoint resumes in an atomic run-scoped ledger keyed by generation
fingerprint and reader cache key, and bounded by the fingerprinted CLI budget.
The transport writes attempt start before network I/O and writes response/error
before returning to the cache/checkpoint path. A crash after a paid response is
cached but before the answer checkpoint therefore resumes from cache without
losing or double-counting that original attempt. Cached answers retain their
original provenance but add zero new cost to the resumed run.
On resume, the checkpoint's embedded attempt history must be an exact prefix of
the current durable ledger; only a longer ledger is accepted as the crash-window
case. Deleted, truncated, or divergent ledgers fail closed. Absolute output and
ledger paths are part of the generation fingerprint, so switching either path
cannot masquerade as the same run. Reader cache entries use temp-file plus atomic
replace, preventing a killed write from leaving a corrupt reusable cache file.
Attempts without parseable response usage are counted separately as unpriced;
they are never silently treated as known-zero cost. `usage.cost=null` is a valid
OpenRouter response and is recorded as one unpriced attempt.

The response proof follows OpenRouter's live 2026-07-13 OpenAPI contract:
`ChatResult.model`, `ChatResult.usage.cost`, and the selected endpoint provider
under `openrouter_metadata.endpoints.available`. The inspected
`https://openrouter.ai/openapi.json` bytes hashed to
`abaf90acc89dc3a2b4cd8824afcbf87734c8d0a5f4429ea85dca0d9eb02e353b`.
Every request opts into this metadata with
`X-OpenRouter-Metadata: enabled`; the transport regression asserts the header.

## Proof run

```text
python3 -m pytest tests/test_memora_benchmark_contract.py -q
6 passed

python3 scripts/run_memora_fama.py \
  --official-repo /tmp/memphant-task4-memora --verify-only
{"paid_calls": 0, "questions": 600, "sessions": 27614, "subquestions": 6415}

python3 scripts/generate_memora_memphant_answers.py \
  --official-repo /tmp/memphant-task4-memora \
  --out /tmp/memora-weekly-software-engineer-plan.json \
  --cache-dir /tmp/memora-reader-cache \
  --group weekly/software_engineer \
  --dry-run
{"groups": 1, "model": "openai/gpt-5.6-luna-pro", "queries": 15, "sessions": 163, "source_status": "dry_run_no_answers"}

python3 -m pytest tests/test_memora_benchmark_contract.py -q
15 passed
```

The exact one-group paid pilot command was launched after the runtime and
contract regressions below were fixed:

```sh
cargo build -p memphant-server -p memphant-worker -p memphant-cli
doppler run --project syndai --config dev -- python3 scripts/generate_memora_memphant_answers.py \
  --official-repo /tmp/memphant-task4-memora \
  --out docs/build-log/artifacts/unified-sota-20260713/task4-memora/weekly-software-engineer.answers.json \
  --proof docs/build-log/artifacts/unified-sota-20260713/task4-memora/weekly-software-engineer.proof.json \
  --checkpoint docs/build-log/artifacts/unified-sota-20260713/task4-memora/weekly-software-engineer.checkpoint.json \
  --attempt-ledger docs/build-log/artifacts/unified-sota-20260713/task4-memora/weekly-software-engineer.attempts.json \
  --cache-dir docs/build-log/artifacts/unified-sota-20260713/task4-memora/reader-cache \
  --group weekly/software_engineer \
  --max-provider-attempts 60
```

## Executed one-group pilot

The first no-model execution found that the adapter's `memora-dialogue` origin
was outside the database's episode-source contract. The durable fix rejects
invalid episode source kinds at the service boundary with `422 invalid_request`
and maps observable Memora dialogue to `user`. A second no-model execution
found that the shared packaged-runtime client lacked trace GET; the shared
client now owns both retrying POST and GET, covering Memora and STALE.

The first real Luna pass then exposed two evaluator-integrity issues. A reader
abstention aborted the benchmark instead of being scored, so abstention is now
preserved as the explicit non-empty response `I cannot answer from the
retrieved memory.` and marked in reader proof. Runtime episode UUIDs also made
identical replay prompts miss cache. Reader evidence now replaces those UUIDs
with stable per-pack `episode-N` labels; six already-paid responses were
mechanically migrated to their canonical keys. The cumulative attempt ledger
retains duplicate and interrupted attempts rather than erasing their cost.

Native scoring gained a fail-closed `--group PERIOD/PERSONA` selection. It
still verifies the complete official 600-question release first, then requires
the selected answer identities to match that official group exactly. The
weekly/software_engineer pilot contains 15 questions, 163 sessions, and 71
FAMA subquestions. All 213 strict judge results were valid across GPT-4.1,
Claude Haiku 4.5, and Gemini 2.5 Flash.

```text
answer generation: 15/15 complete, zero errors, zero fallbacks
reader: Luna Pro high; 6 cache hits; 9 fresh calls
attempt ledger: 18 attempts; 17 priced; 1 interrupted/unpriced
reported answer-reader cost: $0.32080335
explicit abstentions: 3/15

native FAMA: 32.96296296296297
overall accuracy: 44/71 = 0.6197183098591549
memory presence: 20/44 = 0.45454545454545453
forgetting absence: 24/27 = 0.8888888888888888
Remembering FAMA: 53.8888888888889
Recommending FAMA: 38.33333333333333
Reasoning FAMA: 6.666666666666667
```

Artifacts live under
`docs/build-log/artifacts/unified-sota-20260713/task4-memora/` with answer hash
`d81986c2218a9e64fca70e821b7ae570ea614d5988d8f257285027d77b5893a5`
and native result hash
`e4f5b9c35531669124b8affd17ef59cd950796a17975e30c6b87981310b89b25`.

This pilot is valid and decisively not SOTA. The low memory-presence and
reasoning scores make a 600-question run wasteful until retrieval coverage and
aggregate/current-state resolution improve on this exposed group.

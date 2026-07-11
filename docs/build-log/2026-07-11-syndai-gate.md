# 2026-07-11 — Syndai replacement gate: first engine-vs-engine run (W10)

## Verdict

**HOLD. MemPhant does not beat Syndai's knowledge stack on the docs golden set —
replacement work must not proceed.** (Finding, not failure, per the binding rule
in the 2026-07-09 plan addendum: MemPhant must beat Syndai with the paired QA CI
excluding zero before any replacement.)

| metric (n=60, paired) | MemPhant | Syndai | Δ (Mem−Syn) | 95% CI | excl. 0 |
|---|---|---|---|---|---|
| QA accuracy (binding) | **0.050** | **0.217** | −0.167 | [−0.267, −0.083] | yes |
| provenance hit@10 | 0.067 | 0.200 | −0.133 | [−0.250, −0.033] | yes |
| provenance hit@5 | 0.033 | 0.200 | — | — | — |

Both engines are weak in absolute terms (best 0.217 QA): the golden set is
paraphrase-heavy by construction (median question↔span lexical overlap 0.0), so
it is close to a pure semantic-matching benchmark. Huge headroom on both sides.

## Setup (all artifacts in docs/build-log/artifacts/syndai-gate/)

- **Corpus**: 108 Syndai product-doc markdown files (docs/** minus
  docs/superpowers/ process scaffolding), pinned by per-file sha256 in
  `benchmarks/manifests/syndai_docs_gate.lock.json` (Syndai commit 6945f429a81d,
  working tree; the sha256s are the true pin).
- **Golden set**: 60 questions (48 single-hop + 12 multi-hop), mined by
  google/gemini-3.1-pro-preview via OpenRouter (neither reader nor judge — no
  self-grading). Every gold answer is a verbatim corpus span with file +
  heading path + char offsets; `corpus[start:end] == span` is contract-tested.
  Deterministic: seeded stratified sampling + sha256 reply cache (rerun from
  warm cache: 0 fresh calls, byte-identical output, sha256 c424b08f0260).
- **Syndai half** (`scripts/gate_run_syndai.py`): their REAL pipeline —
  `prepare_content_chunks` (production sectionizer + 500tok/75 chunker) →
  `embed_and_store_chunks` (text-embedding-3-small@1536, heading-path context
  prefixes) → `search_knowledge_detached` (HNSW + BM25 + RRF K=60), top_k=10,
  min_score=0, adaptive-query LLM retry patched off (as their own eval does).
  LOCAL dev Postgres only (127.0.0.1:55432 syndai_local; Supabase refused at
  startup). 108/108 sources, 9887 chunks. Zero Syndai repo changes.
- **MemPhant half** (`scripts/gate_run_memphant.py`): packaged
  memphant-server + worker + cli against a dedicated `memphant_gate` database
  (never the shared campaign DB). Corpus pre-split into 3254 markdown sections
  (resources don't auto-chunk), each ingested as a `kind=document` resource;
  worker drained (3254 compiled, fastembed bge-small-en-v1.5 embeddings for
  all units); `/v1/recall` k=10, mode=exhaustive, budget_tokens=8192 so the
  full top-10 is returned rather than the 512-token pack default.
- **Scoring**: `scripts/run_reader.py` UNCHANGED, identical for both engines —
  engine=openrouter, reader openai/gpt-5.6-terra, judge anthropic/claude-sonnet-5,
  prompt v1. Provenance graded identically for both by shared
  `gate_common.provenance_hit` (normalized word-boundary span containment over
  top-k bodies; multi-hop needs bridge AND answer spans covered).

## Honesty notes

1. **Latent Syndai ingest bug found and steelmanned away.** Their
   `_add_sections_and_edges` (processing_chunks.py) inserts
   `KnowledgeSectionEdge` rows referencing `section.id` before the sections are
   flushed → NotNullViolation on any doc whose sectionizer emits
   cross-reference edges. 11/108 real product docs fail (their production KB is
   empty; nobody had ever ingested these). Their own eval fixture sidesteps it
   with explicit uuid4 ids. The gate runner retries those files chunks-only
   (their storage path with `sectionized_document=None`; chunks keep heading
   hierarchy + context prefixes; only structure expansion is lost for those 11
   files — listed in `syndai_provenance.json.chunks_only_fallback_files`).
   Without the steelman Syndai scored 0.200 QA on the 97-file partial corpus;
   with it, 0.217 on the full corpus. The verdict is unchanged either way.
   Filed as a fix task against the Syndai repo.
2. **Jina rerank was enabled but rate-limited** (provider circuit open) during
   the searches, so Syndai fell back to RRF order per its production behavior.
   A Syndai with working Jina rerank is plausibly stronger — which reinforces
   the HOLD, it cannot flip it.
3. **Syndai returned exactly 5 items per query at top_k=10**: its own
   `result_limit_for_first_stage_evidence` caps vector-only result sets (no
   BM25 support in the visible window) at 5, and the paraphrase-heavy questions
   almost never fire BM25. Its hit@10 therefore equals hit@5 by construction —
   measured as shipped, not worked around.
4. **Provenance is span-level only** (item body contains the gold span). The
   plan sketch allowed span-or-section credit; section-level credit would need
   engine-specific provenance mapping and can only help Syndai's chunked items.
   Span containment is identical for both engines and is the answer-bearing
   criterion; QA is the binding metric regardless.
5. **Ingest granularity differs by necessity and is recorded**: Syndai ingests
   whole files (its chunker splits them); MemPhant ingests the pre-split
   sections (its resource channel stores one unit per resource). Both index the
   full corpus; both are graded by the same span containment on returned
   bodies. Every gold section is in MemPhant's haystack (asserted at startup),
   so a perfect engine could hit @1.

## Why MemPhant lost (diagnosis for the next wave)

- **Embedding gap is the prime suspect**: bge-small-en-v1.5@384 (shipped
  default) vs text-embedding-3-small@1536, on a golden set that is nearly pure
  semantic matching. W8's bge-base profile arm + W2's profile machinery exist
  to measure exactly this swap; a large-model profile arm is now clearly
  motivated.
- **No ancestor-context at embed time**: Syndai prepends the full heading-path
  breadcrumb ("Section path: A > B > C") to every chunk before embedding;
  MemPhant embeds the raw section body (leaf heading only). The rung-4
  contextual-chunks lane is the natural carrier for this.
- **Fusion looks lexically dominated at this scale**: MemPhant's top ranks on
  missed questions were heading/keyword matches ("architecture",
  "integration") rather than semantic neighbors; with rerank off (default) and
  a weak vector channel, the lexical families win the RRF vote. Multi-hop:
  0/12 provenance hits (Syndai 1/12), 0/12 QA (Syndai 3/12).
- Reproduce: commands at the top of each script; full artifact set (evidence
  packs and miner cache are gitignored — they embed raw doc text; scored
  reports + compare verdict are committed).

## Gates

- `python3 -m pytest tests/ spikes/python-retain/test_spike.py -q` — 90 passed
  (includes the new golden-lock/evidence-shape/verbatim-span contract tests).
- `python3 -m py_compile` clean on all five gate scripts.
- Determinism: miner rerun from cache = 0 fresh calls, identical sha256.

# P1 Retrieval-Pipeline Benchmark Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rigorously benchmark each retrieval-pipeline stage — embedders, retrieval algorithms, rerankers — on ONE shared hard-adversarial LME-S test set (embed ~100 docs → retrieve top-48 → rerank to top-5 → gold-in-top-5), across accuracy / cost / latency, and land chunk-granularity reranking in production code.

**Architecture:** One committed Python builder makes a fixed adversarial pool JSON (regenerable from LME-S; corpus text uncommitted). Two new thin `memphant-eval` subcommands (`embed-pool`, `rerank-pool`) run every local/API model through the PRODUCTION seams (`embedder_from_id`, `build_cross_reranker`). A committed Python harness does retrieval variants (BM25/RRF/convex fusion/MaxSim), scoring, paired stats, cost & latency tables over disk-cached vectors/scores — embed once per (doc,embedder), reuse everywhere. Separately, `cross_rerank_candidates` in memphant-core gains a `chunk` granularity (rerank `contextual_chunks`, max-pool to unit) behind a default-OFF env flag.

**Tech Stack:** Rust (memphant-eval/runtime/core), Python 3 stdlib (no new deps), OpenRouter for LLM verify, existing fastembed/ONNX local models.

## Global Constraints

- API keys via ENV ONLY (Doppler `syndai/dev` or owner-pasted in env). Never print, persist, or commit a key value. Verify no key leaks into any committed file/artifact before each commit.
- Doppler-wrap ONLY secret-consuming commands (AGENTS.md:14-18). Local arms secret-free.
- Corpus text (LME-S sessions) must NOT be committed. Builders/harness/scorers committed; pool JSONs + vector caches live in `docs/build-log/artifacts/p1-retrieval-bench/` but are gitignored (same pattern as reranker-spike `rr_pools*.json`).
- Live-PG work (none planned here — harness is fixed-pool, no DB) would use `scripts/with_scratch_db.sh`.
- Owner priorities: accuracy/SOTA > cost > speed. KISS/DRY/YAGNI. Paired comparisons, same seed, honest CIs.
- Commit gate per AGENTS.md Verification: `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test --all-targets --all-features` (narrowest checks while iterating). Commits end with `Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>`. Do NOT push.
- Binding accuracy gate for any DEFAULT change stays paid reader-QA (n≥100); this benchmark is the retrieval-frontier screen.
- Seed for the adversarial set: **20260722**. Fixed everywhere.

## Pre-registered decisions (from research, locked before runs)

- **Piece-1 embedder arms** (each scored with identical retrieval = prod-mirror RRF hybrid, and identical reranker = MiniLM-L6-int8 chunk-rerank):
  - Local (free): `small` (bge-small anchor), `modernbert` (R0 docs winner), `gemma`; `qwen3` and every local arm gated on PROJECTED TOTAL wall time: 200-chunk smoke × (unique_chunks/200) must project <2h for the arm, else retire it citing the projection (a raw 10-min smoke gate would pass runs projecting to days — R0 retired qwen3 for exactly this).
  - Paid (keys in Doppler): `openai-text-embedding-3-small` (control), `gemini-embedding-001`, **NEW `gemini-embedding-2`** (probe-verified served), **NEW `jina-v5-small`** (`jina-embeddings-v5-text-small`, 1024-d, probe-verified served).
  - Paid (owner key pending): `voyage-4`, `voyage-context-4` (skip gracefully if no `VOYAGE_API_KEY`).
  - Not runnable here (state honestly in results): Nemotron-3-Embed-8B/1B (GPU), Harrier-27B (GPU), KaLM-12B (GPU + custom license). Jina v5 local weights CC-BY-NC (API use OK).
- **Piece-2 retrieval variants** (fixed best embedder from piece 1; metric = recall@48 + gold-rank; then e2e recall@5 with MiniLM rerank for the finalists):
  - V0 dense-only (chunk max-pool), V1 BM25-only (Okapi chunk max-pool), V2 RRF k=60 (prod mirror), V3 convex combination (min-max normalized, α tuned on even-index questions, reported on odd — no tuning-on-test), V4 instruction-prompted queries, V5 context-prepended chunks (session date + session head, contextual-retrieval-lite), V6 late-interaction full-pool MaxSim (Jina ColBERT API; skip gracefully on API failure), V7 MMR λ=0.7 guarded (report gold-evictions).
  - Pre-registered SKIPs (cite research): HyDE (demotion confirmed arXiv:2309.08541), query-decomposition-by-default (multi-hop only), SPLADE (needs local model plumbing; BM25 here is real Okapi not ts_rank), MUVERA/PLAID (billion-scale tools), listwise-LLM rerank (not a retrieval stage).
- **Piece-3 reranker arms** (fixed retrieved-48 from best embedder+retrieval; every arm scores IDENTICAL candidates): none (baseline order), MiniLM-L6-int8 chunk+max-pool (local, via `rerank-pool`), bge-reranker-base chunk+max-pool (parity control), Cohere v4.0-fast, Cohere v4.0-pro (full-doc native; owner key), zerank-2 (owner key), zerank-1-small local IF a CPU ONNX exists on HF (check; else note), Voyage rerank-2.5 (if key).
- **Metrics:** recall@5 (any gold in top-5, primary), MRR of first gold, recall@48 (retrieval screen — reported BOTH dense-only and hybrid for piece 1, to isolate the embedder signal from the shared BM25 channel), gold-coverage@5/@48 (secondary, multi-gold questions), per-question gold ranks (paired), abstention max-score separation (reported, not gated), cost $/query (billed units or token-price math), latency decomposed per-query e2e = query-embed + retrieve + rerank at p50/p95 (warm; cold noted) with doc-embedding reported separately as $/1K-docs indexing cost.
- **Stats:** exact binomial sign test on paired recall@5 discordant pairs (report the actual two-sided p, not a flip heuristic — 6-0 discordant gives p=0.031 but 10-4 gives p≈0.18); bootstrap (1000 resamples, seed 20260722) CI on MRR deltas. n=72 scored questions; deltas needing <~8 one-sided flips stay directional — state this in every table.
- **Freeze rules (pre-registered):** piece-1 winner = highest mean MRR of first gold over the full pool ranking (recall@48 is near-ceiling on 100-doc pools — a random order scores ~0.48 — so it must NOT be the decision metric; report it anyway); tie → recall@16, then cheaper per query, then local over API. Same metric for piece-2 variant freeze.
- **Cost budget:** ~$10-20 embeddings total (all API arms at n=80), <$2 rerank APIs, ~$2 LLM verify. V6 ColBERT: project token cost AND multi-vector cache size BEFORE running; if projected >$10, run on a 24-question paired subset and say so. Abort-and-ask threshold: any single arm projecting >$20.
- **Framing (owner-directed scope):** rung-7/A1 evidence says packing, not retrieval ordering, dominates current-default misses; this benchmark is the owner-ordered SOTA retrieval-frontier screen that pre-positions for post-packing headroom — RESULTS.md must carry this sentence so nobody reads a retrieval win as an end-metric claim.

## File Structure

- `docs/build-log/artifacts/p1-retrieval-bench/build_adversarial_set.py` — committed builder (T1)
- `docs/build-log/artifacts/p1-retrieval-bench/harness.py` — committed harness: retrieval variants, API rerankers, scoring, tables (T5)
- `docs/build-log/artifacts/p1-retrieval-bench/.gitignore` — `pool*.json`, `cache/`, `out/` (T1)
- `docs/build-log/artifacts/p1-retrieval-bench/RESULTS.md` — committed results tables (T8)
- `crates/memphant-eval/src/pool_tools.rs` — `embed-pool` + `rerank-pool` subcommands (T3, T4)
- `crates/memphant-eval/src/main.rs` — dispatch for the two subcommands (T3, T4)
- `crates/memphant-runtime/src/api_embeddings.rs` — `JinaEmbedding`, `GeminiEmbedding2` arms (T3)
- `crates/memphant-runtime/src/lib.rs` — `embedder_from_id` new ids; `MEMPHANT_RERANK_GRANULARITY` env (T2, T3)
- `crates/memphant-core/src/lib.rs` — `CrossRerankGranularity` + chunk-granularity path in `cross_rerank_candidates` (T2)
- `crates/memphant-core/tests/cross_reranker.rs` — TDD tests for chunk granularity (T2)
- `crates/memphant-eval/src/bench_lme.rs` + `main.rs` — `--rerank-granularity` flag (T2)
- `docs/build-log/2026-07-22-p1-retrieval-pipeline-bench.md` — build-log narrative (T8)

---

### Task T1: Adversarial test set builder (ONE set, reused by all 3 pieces)

**Files:**
- Create: `docs/build-log/artifacts/p1-retrieval-bench/build_adversarial_set.py`
- Create: `docs/build-log/artifacts/p1-retrieval-bench/.gitignore` (`pool*.json`, `cache/`, `out/`, `vectors*/`)

**Interfaces:**
- Produces `pool.json` (uncommitted):
```json
{"meta": {"seed": 20260722, "corpus_sha256": "<sha of longmemeval_s.json>", "chunk_chars": 1200,
          "n_scored": 36, "n_abstention": 6},
 "questions": [{
   "qid": "gpt4_2655b836", "qtype": "multi-session", "abstention": false,
   "question": "...", "answer": "...", "question_date": "2023/05/20",
   "gold_doc_ids": ["session_a", "session_b"],
   "gold_verified": "string|llm",
   "docs": [{"doc_id": "session_a", "date": "2023/04/10", "is_gold": true,
             "source": "haystack", "answer_char_pos": 5210,
             "text": "<full session text>", "chunks": ["<1200-char windows>"]}]
 }]}
```
- Selection (seeded 20260722): 24 multi-session, 24 temporal-reasoning, 16 knowledge-update, 8 single-session-* with answer buried >2000 chars, 8 abstention (`_abs`) = 80 (72 scored). Eligibility: every `answer_session_ids` present in haystack; prefer deep `answer_char_pos`.
- Pool = all own-haystack sessions (the same-user topical near-dups, ~40-56) + MIXED-mined hard negatives from OTHER questions' haystacks, topped up to 100 docs/question: half mined by BM25 similarity to (question + gold), half by embedding cosine (openai-text-embedding-3-small, ~$0.05 one-time) — one-sided mining would adversarially handicap the matching retrieval arm; residual bias documented in RESULTS.md either way. BM25 lives in `harness.py`; the builder imports it (DRY).
- **False-negative guards** (all enforced in-builder):
  1. exclude any mined session whose id ∈ this question's `answer_session_ids` (LME-S reuses sessions across questions);
  2. exclude mined sessions containing the exact normalized answer string;
  3. drop mined near-clones of gold (token-set Jaccard > 0.8 vs any gold session).
- **Gold verification:** string-locate normalized answer in a gold session → `gold_verified: "string"`; else ONE OpenRouter call per gold session (model `anthropic/claude-sonnet-5`, prompt: session text + question + "Does this session contain information that CONTRIBUTES to answering this question? Partial evidence counts — for multi-session questions no single session needs to suffice. YES/NO") → every labeled gold session must be YES; drop the question otherwise and pull the next candidate from the same stratum. (Verbatim string-match alone would mass-attrit temporal/multi-session strata whose answers are computed, e.g. "3 days".) Abstention questions: answer-string check is vacuous (answers are meta-statements); instead LLM spot-check the top-5 most-similar pool docs per abstention question with the same CONTRIBUTES prompt — expect NO for all; replace violators.
- `--verify` mode re-runs all guards on an existing pool.json and prints the audit table.

**Steps:**

- [ ] **S1: Write builder with self-check** (ponytail: assert-based `--verify`, no pytest). Core selection:
```python
STRATA = {"multi-session": 12, "temporal-reasoning": 12, "knowledge-update": 8, "single-session": 4}
N_ABST, POOL_DOCS, CHUNK = 6, 100, 1200
rng = random.Random(20260722)
def norm(s): return re.sub(r"\s+", " ", str(s).lower()).strip()
def sess_text(sess): return " ".join(t.get("content", "") for t in sess)
def chunks(text): return [text[i:i+CHUNK] for i in range(0, len(text), CHUNK)]
def answer_pos(text, ans):
    a = norm(ans); t = norm(text)
    return t.find(a[:40]) if a else -1
# BM25 (Okapi, k1=1.5 b=0.75) over session token counters for hard-negative mining,
# query = question + gold session text (mine what *looks like* the gold).
```
- [ ] **S2: Run builder against LME-S** — `python3 build_adversarial_set.py benchmarks/data/longmemeval_s.json --out pool.json` (LLM verify wrapped in Doppler for OPENROUTER_API_KEY). Expected: `42 questions (36 scored + 6 abstention), gold-verified 36/36, guards: 0 violations` printout.
- [ ] **S3: Run `--verify`** on the emitted pool — audit table clean; spot-read 3 questions by hand (gold really answers, distractors really near-dup).
- [ ] **S4: Commit** builder + .gitignore (no pool text): `bench(p1): adversarial fixed-pool builder — 42q hard LME-S set`.

### Task T2: Chunk-granularity cross-rerank in production core (TDD)

**Files:**
- Modify: `crates/memphant-core/src/lib.rs` (~7131 `cross_rerank_candidates`, service builder)
- Modify: `crates/memphant-core/src/service.rs` (builder threading)
- Test: `crates/memphant-core/tests/cross_reranker.rs`
- Modify: `crates/memphant-runtime/src/lib.rs` (env `MEMPHANT_RERANK_GRANULARITY`)
- Modify: `crates/memphant-eval/src/main.rs` + `bench_lme.rs` (`--rerank-granularity body|chunk`)

**Interfaces:**
- Produces: `pub enum CrossRerankGranularity { UnitBody, ContextualChunks }` (default `UnitBody`), `MemoryService::with_cross_rerank_granularity(CrossRerankGranularity)`, env `MEMPHANT_RERANK_GRANULARITY=body|chunk`, bench flag `--rerank-granularity`.
- Behavior under `ContextualChunks`: for the same head selection as today, docs fed to `CrossReranker::rerank` = flattened `candidate.unit.contextual_chunks[*].body` (fallback: `unit.body` when a candidate has no chunks); candidate score = MAX over its chunk scores; identical validation/fail-open and reorder semantics; `candidate_limit` still counts CANDIDATES, not chunks; trace records `granularity` + `docs_scored`.

**Steps:**

- [ ] **S1: Failing test** in `tests/cross_reranker.rs`:
```rust
// Reranker that scores each doc by whether it contains the marker "NEEDLE".
// Candidate A: body WITHOUT needle, one contextual chunk WITH needle (buried-chunk case).
// Candidate B: body ranks first under UnitBody; under ContextualChunks A must outrank B.
#[test]
fn chunk_granularity_max_pools_contextual_chunks() { /* build 2 candidates via existing helpers,
    recall with granularity=ContextualChunks, assert A before B, assert trace.docs_scored == total_chunks */ }
#[test]
fn chunk_granularity_falls_back_to_body_when_no_chunks() { /* candidate with empty chunks still scored */ }
#[test]
fn chunk_granularity_mixed_pool_scatter_mapping() { /* 3 candidates: 3-chunk, 0-chunk (body fallback),
    2-chunk. Reranker scores by exact text match. Assert each candidate gets the max of ITS OWN
    chunk scores (no off-by-one bleed across the flattened chunk list) and ordering is correct. */ }
```
- [ ] **S2: Run** `cargo test -p memphant-core --test cross_reranker` → new tests FAIL (enum/method missing).
- [ ] **S3: Implement** enum + builder + the docs-construction/max-pool change inside `cross_rerank_candidates` (keep fail-open contract: wrong count/non-finite → no reorder).
- [ ] **S4: Run** same tests → PASS; run existing cross_reranker tests → all PASS (regression: UnitBody path byte-identical).
- [ ] **S5: Wire runtime env + bench flag**; `cargo clippy --all-targets --all-features -- -D warnings` + `cargo fmt`.
- [ ] **S6: Live bench-lme A/B on REAL contextual_chunks** (the unit tests use synthetic chunks; prod chunks are adaptive 4-turn windows — `adaptive_chunk_window`, service.rs:2302-2323 — that can exceed 512 tokens on long docs, so the truncation-recovery premise must be checked on the shipped shape): scratch-PG `bench-lme --sample 20 --seed 3 --cross-rerank` body vs chunk granularity (local MiniLM byo), compare recall@10 + first_answer_rank + wall latency, and log the prod chunk char-length distribution during the run. If many chunks exceed ~2000 chars, record a follow-up (rerank-time chunk split/cap) in RESULTS — do not silently ship the premise.
- [ ] **S7: Commit**: `feat(rerank): chunk-granularity cross-rerank (contextual_chunks + max-pool) behind MEMPHANT_RERANK_GRANULARITY`.

### Task T3: `embed-pool` subcommand + new embedder arms (jina-v5-small, gemini-embedding-2)

**Files:**
- Create: `crates/memphant-eval/src/pool_tools.rs`
- Modify: `crates/memphant-eval/src/main.rs` (dispatch `embed-pool`)
- Modify: `crates/memphant-runtime/src/api_embeddings.rs` (`JinaEmbedding` — mirror `OpenAiEmbedding` at :669-722; endpoint `https://api.jina.ai/v1/embeddings`, model `jina-embeddings-v5-text-small`, key `JINA_API_KEY`, dims 1024, batch 128, task `retrieval.passage`/`retrieval.query`; `GeminiEmbedding2` — mirror `GeminiEmbedding` at :584-661 with model `gemini-embedding-2`, dims 3072)
- Modify: `crates/memphant-runtime/src/lib.rs` `embedder_from_id` (+ `"jina-v5-small"`, `"gemini-embedding-2"`)
- Test: unit tests beside existing api_embeddings tests (request-shape serialization only; no live calls in CI)

**Interfaces:**
- CLI: `memphant-eval embed-pool --pool pool.json --embed-model <id> --out out/vectors-<id>.jsonl [--queries] [--query-prefix "<instruction>"]`
- Reads pool.json (T1 schema). Embeds every UNIQUE chunk (sha256 dedup) via `embedder.embed`, and with `--queries` every question via `embed_query` (with optional prefix; per-query wall time recorded — this is the query-latency component). Appends JSONL `{"hash": "<sha256 of text>", "vec": [f32...]}`; **resume-safe**: pre-loads hashes already in `--out`, skipping them, and tolerates a truncated final line (crash mid-append) by ignoring the malformed tail. Prints `{"embedded": n, "skipped": m, "elapsed_ms": t, "approx_tokens": chars/4}`.

**Steps:**

- [ ] **S1: Failing serialization tests** for `JinaEmbedding`/`GeminiEmbedding2` request bodies (mirror existing api_embeddings test style). Run → FAIL.
- [ ] **S2: Implement arms** (mirror lines cited above; shared `ApiHttp::post_json` retry). Tests PASS.
- [ ] **S3: Implement `embed-pool`** (~120 lines; serde structs for pool.json; reuse `embedder_from_id`). Smoke: tiny 2-doc fixture JSON in-repo → run with `--embed-model small` → JSONL rows appear; re-run → `skipped==all`.
- [ ] **S4: Live probe (1 call each, Doppler)**: `embed-pool` over the 2-doc fixture with `jina-v5-small` and `gemini-embedding-2` → dims 1024/3072 confirmed.
- [ ] **S5: fmt/clippy; Commit**: `feat(eval): embed-pool subcommand + jina-v5-small/gemini-embedding-2 arms`.

### Task T4: `rerank-pool` subcommand (local + API rerankers through prod seam)

**Files:**
- Modify: `crates/memphant-eval/src/pool_tools.rs`, `crates/memphant-eval/src/main.rs`

**Interfaces:**
- CLI: `MEMPHANT_RERANKER=<byo|fastembed|cohere-rerank-3.5|voyage-rerank-2.5> memphant-eval rerank-pool --candidates out/cands-<config>.json --granularity <chunk|doc> --out out/rr-<arm>.json`
- `--candidates` schema (produced by harness `make-candidates`): `[{"qid": "...", "question": "...", "docs": [{"doc_id": "...", "text": "...", "chunks": ["..."]}]}]` (docs = the retrieved top-48, order = retrieval rank).
- Per question: `granularity=chunk` → rerank flattened chunks, doc score = max (mirror of `rerank_chunked_pool_accuracy` embeddings.rs:1247-1252); `granularity=doc` → rerank `text` directly. Uses `build_cross_reranker()` so construction is byte-identical to serving. Emits `[{"qid": "...", "scores": {"<doc_id>": f32}, "elapsed_ms": t}]`. `MEMPHANT_RERANK_TIMEOUT_MS=0` documented for offline runs.

**Steps:**

- [ ] **S1: Implement + smoke on fixture** (2 questions × 3 docs, `MEMPHANT_RERANKER=byo` with MiniLM dir): doc with needle-chunk outranks others under `chunk`; ranks differ from `doc` granularity on the buried case.
- [ ] **S2: fmt/clippy; Commit**: `feat(eval): rerank-pool subcommand — fixed-pool reranking through the production seam`.

### Task T5: Harness (retrieval variants, API rerank arms, scoring) + Piece 1 embedder runs

**Files:**
- Create: `docs/build-log/artifacts/p1-retrieval-bench/harness.py` (stdlib only)

**Interfaces:** subcommands:
- `retrieve --pool pool.json --vectors out/vectors-<id>.jsonl --variant <v0|v1|v2|v3|v4|v5|v6|v7> --out out/retr-<id>-<v>.json` → per-q ranked doc_ids (chunk-level cosine, doc = max chunk; BM25 Okapi k1=1.5 b=0.75 same shape; RRF k=60; convex α; MaxSim over Jina ColBERT API vectors cached; MMR λ=0.7 over dense).
- `make-candidates --retr out/retr-....json --pool pool.json --k 48 --out out/cands-....json`
- `rerank-api --arm <zerank-2|zerank-1-small-api|none> --cands ... --out out/rr-....json` (ZeroEntropy is NOT in prod code, so Python is its only path; disk cache `cache/<sha256(arm+qid+dochash)>.json`; keys from env; billed units recorded; failures never cached). **Cohere and Voyage arms run through `rerank-pool` (the prod seam) instead** — one path per arm, so a winning hosted number is servable-by-construction. Requires a one-line runtime change in T4 scope: `MEMPHANT_COHERE_MAX_TOKENS_PER_DOC` env (default 4096) because the hard-coded 4096 truncates 22KB sessions and cuts deep-buried golds; benchmark runs set 8192 and RESULTS states the prod default.
- `score --pool pool.json --retr ... [--rr ...] --out out/score-....json` → recall@48, recall@5, MRR, per-type rows, per-q gold ranks, abstention max-score table, latency p50/p95, $ totals; `compare A B` → paired sign test + bootstrap CI.

**Steps:**

- [ ] **S1: Write harness core + self-check** (`python3 harness.py selftest`: synthetic 3-doc pool where gold is engineered top-1 under dense; BM25 tie-break case; RRF merge case; sign-test known table; assert all).
- [ ] **S2: Build pool embeddings for all runnable piece-1 arms** (local: small, modernbert, gemma; API via Doppler: openai-3-small, gemini-001, gemini-2, jina-v5-small; voyage-* only if key present). Cache = the JSONL files.
- [ ] **S3: Run piece 1**: per arm → `retrieve --variant v2` (prod-mirror RRF) → recall@48 table; then `make-candidates --k 48` → `rerank-pool` (MiniLM chunk) → `score` → recall@5/MRR/cost/latency table. qwen3 gated on the 10-min smoke.
- [ ] **S4: Write piece-1 table into RESULTS.md** (draft), note per-question ranks artifact paths.
- [ ] **S5: Commit harness + RESULTS draft**: `bench(p1): fixed-pool harness + piece-1 embedder screen`.

### Task T6: Piece 2 — retrieval variants on the best embedder

- [ ] **S1: Freeze best piece-1 embedder** (pre-registered rule: highest recall@48; tie → cheaper per query; local beats API on ties per owner privacy/cost priority).
- [ ] **S2: Run V0-V7** (+ V4/V5 use extra `embed-pool` runs with `--query-prefix` / context-prepended chunk texts — new vector files, cached). Jina ColBERT arm via API with disk cache; on failure record and skip.
- [ ] **S3: e2e check for the top-2 variants** (MiniLM chunk rerank → recall@5) to confirm retrieval wins survive reranking.
- [ ] **S4: Update RESULTS.md piece-2 table + variant verdicts vs pre-registered expectations. Commit**: `bench(p1): piece-2 retrieval-variant screen`.

### Task T7: Piece 3 — reranker head-to-head on frozen retrieved-48

**Key gate (explicit):** the hosted arms most likely to define the accuracy frontier (Cohere v4, zerank-2, Voyage) are BLOCKED until the owner pastes COHERE_API_KEY / ZEROENTROPY_API_KEY (and optionally VOYAGE_API_KEY). Until then piece 3 ships a PARTIAL verdict (local arms only, labeled as such in RESULTS.md); the hosted re-run + RESULTS revision is mandatory when keys arrive, not optional.

- [ ] **S1: Freeze best embedder+variant; emit ONE `cands-final.json`** (identical candidates for every reranker arm).
- [ ] **S2: Local arms** via `rerank-pool`: none-baseline, MiniLM chunk, bge-base chunk (parity), + zerank-1-small if CPU ONNX found on HF (`zeroentropy/zerank-1-small` — check; else record absence).
- [ ] **S3: Hosted arms once keys arrive** — cohere-v4.0-fast, cohere-v4.0-pro (via `rerank-pool` seam + `MEMPHANT_COHERE_MODEL`), voyage-rerank-2.5 (seam, if key), zerank-2 (via `rerank-api`, not in prod code). Warm-up call excluded from latency; every response cached.
- [ ] **S4: Score + paired stats; RESULTS.md piece-3 table (PARTIAL if keys missing) + winning e2e config. Commit**: `bench(p1): piece-3 reranker head-to-head`.

### Task T8: Results, build-log, memory, STATUS

- [ ] **S1: Finalize `RESULTS.md`** (three tables + winning config + n/CI caveats + $ spent per arm + reproduce commands).
- [ ] **S2: Build-log** `docs/build-log/2026-07-22-p1-retrieval-pipeline-bench.md` (narrative, decisions, incidents, honest limits).
- [ ] **S3: Update memory** (`memphant-reranker-landscape-2026-07` + new `memphant-p1-retrieval-bench-verdict`), flip relevant `docs/superpowers/specs/memphant/STATUS.md` boxes with proof-artifact links.
- [ ] **S4: Key-leak sweep** — grep the staged diff for secret VALUE patterns, not env-var names (name-greps fire on every legitimate `*_API_KEY` reference and train you to ignore the gate): `git diff --cached -U0 | grep -nE '(sk-[A-Za-z0-9_-]{20,}|ze_[A-Za-z0-9]{20,}|AIza[A-Za-z0-9_-]{30,}|jina_[A-Za-z0-9_-]{20,})'` must return nothing. Then final gate (`fmt/clippy/test`), final commit.

## Self-Review notes

- Spec coverage: shared adversarial set (T1), embedders incl. new-arm research+wiring (T3, T5), retrieval algorithms built-vs-new enumeration + flagged variants (T6 + pre-registered list), rerankers incl. zerank-1-small check (T7), chunk-granularity production fix (T2), deliverables (T8). Abstention cases: built in T1, scored (reported separately) in harness `score`.
- Efficiency: embeddings cached per (doc-hash, embedder); pools fixed; API rerank responses disk-cached; one seed; n=80.
- Honesty: pre-registered arms/skips; α tuned on split; exact sign-test p reported; vendor self-run numbers flagged in research citations.
- Keys: Cohere/ZeroEntropy/Voyage owner-pasted → T7 hosted arms explicitly gated (PARTIAL verdict until keys); everything else proceeds.

## NOT in scope

- SPLADE/learned-sparse arm — needs local model plumbing; harness BM25 is already real Okapi (research verdict: test only after real BM25).
- HyDE, query-decomposition-by-default, MMR-as-default, MUVERA/PLAID, listwise-LLM rerank — pre-registered skips with citations.
- GPU-only embedders (Nemotron-3-Embed, Harrier-27B, KaLM-12B) — no GPU in this eval environment; named honestly in RESULTS.
- Flipping any production default — binding gate is paid reader-QA (n≥100), separate campaign.
- Anthropic-style per-chunk LLM context generation — V5 tests the cheap header-prepend variant; full LLM contextualization only if V5 shows signal (follow-up).
- Rerank-time chunk split/cap for oversized prod chunks — only if T2-S6 measurement shows prod chunks materially exceed the 512-token window.

## What already exists (reused, not rebuilt)

- `embedder_from_id` + api_embeddings.rs retry/batching (T3 mirrors, no new HTTP stack); `build_cross_reranker` + byo/Cohere/Voyage arms (T4 reuses byte-identically); `rerank_chunked_pool_accuracy` max-pool pattern (T2/T4 mirror it); reranker-spike builders + rr_api_score.py patterns (T1/harness generalize them); bench-lme (T2-S6 live A/B); with_scratch_db.sh (T2-S6); prior-session facts (512-token wall, MiniLM winner) carried, not re-measured.

## Failure modes (new codepaths)

- Builder LLM-verify outage → retry w/ backoff; fallback: keep string-verified questions only, record attrition (visible in meta counts, not silent).
- embed-pool crash mid-run → JSONL resume + truncated-tail tolerance (tested T3-S3).
- API 429/5xx storms → shared ApiHttp retry (Rust) / cached-never-on-failure (Python); arm marked FAILED in RESULTS, never zero-filled.
- Chunk/candidate scatter off-by-one (T2) → mixed-pool mapping test (the regression-class risk).
- Cohere truncation silently cutting deep golds → `MEMPHANT_COHERE_MAX_TOKENS_PER_DOC` + RESULTS states both settings.
- Wrong-count/non-finite reranker output → existing fail-open contract preserved (no reorder, trace records failure).

## Parallelization

Lanes after T1: Lane A = T2 (core+runtime+bench flag), Lane B = T3→T4 (eval subcommands + arms), Lane C = harness.py skeleton (T5-S1). A/B/C are module-disjoint (core vs eval vs artifacts). T5-S2+ needs T1+T3; T6 needs T5; T7 needs T6 (+keys); T8 last. Conflict flag: T2 and T4 both touch `crates/memphant-eval/src/main.rs` (flag + subcommand dispatch) — sequence those two edits.

## GSTACK REVIEW REPORT

| Review | Trigger | Why | Runs | Status | Findings |
|--------|---------|-----|------|--------|----------|
| CEO Review | `/plan-ceo-review` | Scope & strategy | 0 | — | owner task spec is the scope authority for this run |
| Codex Review | `/codex review` | Independent 2nd opinion | 1 | NO_OUTPUT | Codex ran (209K tokens) but repo hooks swallowed its final message; Claude-subagent outside voice substituted |
| Eng Review | `/plan-eng-review` | Architecture & tests (required) | 1 | CLEAR | 8 findings (self-review sections), all folded into plan |
| Outside Voice | Claude subagent | Cross-model challenge | 1 | ISSUES_FOLDED | 14 findings: 11 adopted into plan, 3 recorded as caveats/limitations |
| Design Review | `/plan-design-review` | UI/UX gaps | 0 | — | no UI surface |

- **CROSS-MODEL:** Outside voice challenged strategic calibration (finding 8: packing, not retrieval, is the measured bottleneck). Kept owner-ordered scope; adopted as a mandatory framing sentence in RESULTS.md. All other material findings adopted (n=80, MRR freeze rule, live bench-lme A/B for T2, contribution-semantics gold verify, seam-path Cohere, key gate, projection-scaled local-arm gate, value-pattern key sweep).
- **VERDICT:** ENG CLEARED (autonomous mode: every finding auto-resolved to the recommended option; owner may override any line above).
- **Auto-resolutions logged for owner review:** scope-gate target = this plan file (passed as skill argument); complexity-check proceed-as-is (owner spec pre-decides 3-piece scope); all 22 findings resolved to recommended options without live confirmation because the session is autonomous.

**UNRESOLVED DECISIONS:**
- Whether to add a VOYAGE_API_KEY (owner said "confirm whether to add" — piece-1 voyage arms + piece-3 voyage-rerank-2.5 stay skipped until provided)
- Cohere + ZeroEntropy key re-paste timing (T7 hosted arms + PARTIAL→FULL piece-3 verdict wait on it)

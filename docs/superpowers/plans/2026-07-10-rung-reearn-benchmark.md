# Rung Re-Earn + Real Benchmark Plan (2026-07-10)

**Goal:** Re-adjudicate rungs 4–15 with evidence produced by the packaged Postgres-backed runtime under the promotion-provenance rule, executing ONE real public benchmark; every rung row ends either promoted-with-real-proof or honestly open with a stated reason.

**The benchmark call (authoritative):** **LongMemEval-S, retrieval-only, stratified sample.**
Why: (1) open dataset with labeled `answer_session_ids` → scoring needs NO answer model, so marginal cost ≈ $0 (fastembed runs locally); (2) the rung advance-when conditions are top-k/recall conditions, which retrieval-only measures directly; (3) doc 12 already mandates stratified-by-ability sampling; (4) the 2026 "Benchmark Theatre" climate makes an honest retrieval-only number strictly better than a judge-inflated QA number we can't afford to reproduce. We report `retrieval_only: true` everywhere and never claim QA accuracy or SOTA — the full reader-scored campaign stays open on the ledger.

**Runtime rule:** all ingestion/recall runs through `MemoryService<PgStore>` (the packaged runtime's exact code path) against live pgvector Postgres with fastembed embeddings enabled. Dataset pinned by sha256 in a committed lock manifest; raw data itself gitignored.

## Tasks
- **T1 fetch:** `scripts/fetch_longmemeval.py` — download LongMemEval-S (Hugging Face `xiaowu0162/longmemeval`, oracle+S files as available), record sha256+size+url in `benchmarks/manifests/longmemeval_s.lock.json` (committed), data → `benchmarks/data/` (gitignored).
- **T2 lane:** `memphant-eval bench-lme --database-url … --sample N --seed S [--disable <flag>]` — per sampled question: fresh tenant, ingest haystack sessions chronologically (session id in subject metadata so units map back to sessions), reflect via worker path, then recall with the question; score Recall@5/@10 = answer-bearing session represented in top-k citations/derived units. Stratified by question_type, pinned seed. `--disable` toggles one retrieval stage for paired ablations. Emits profile JSON: metrics per stratum, paired deltas + bootstrap CI, dataset hash, runtime=postgres, retrieval_only=true.
- **T3 run:** baseline + paired ablations for edge_expansion(rung6), rerank(rung8), query_decomposition(rung9), contextual-chunk path(rung4), exhaustive mode(rung12), vector-vs-lexical (embedding value). Also execute the internal golden/security/ops suites (`memphant-eval run/security/ops`) against the PG runtime for rungs whose gates cite golden/internal evidence (5,7,10,11,15).
- **T4 adjudicate:** flip ONLY rungs whose advance-when is satisfied by this evidence class; every other rung row gets an honest note (what evidence exists, what's still missing — e.g., rung 11 needs a longitudinal suite; rung 13 needs archived-trace training data; rung 14 stays RETIRED). Build-log entry + artifacts under `docs/build-log/artifacts/real-retrieval-20260710/`, STATUS + meta-tests updated, mirror synced, full gate green.

**Out of scope:** reader-scored LME accuracy, STATE-Bench/GateMem/PS-Bench executions, launch gates (stay open), dogfood flag flip.

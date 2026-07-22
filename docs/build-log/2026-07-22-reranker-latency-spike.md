# Reranker latency-cut spike — 2026-07-22

The cross-encoder rerank seam is the campaign's largest single QA lever (+0.158,
`2026-07-12-r15-rank-compression.md`) but was **latency-retired at 12.9–13.6 s/query**
(9× the pre-registered 1.5 s p95 ceiling), so it ships flag-gated, default OFF.
This spike asked: can the latency be cut 4–8× to fit the ceiling? Owner priority
**accuracy > cost > speed**, self-host, Apache/MIT.

## Verdict

**Yes — decisively, via a model swap, not tuning.** The 13 s is not a fundamental
limit; it is the wrong model. `bge-reranker-base` (278M params) is overkill.
Swapping to **`ms-marco-MiniLM-L-6-v2` int8 ONNX (22M params, Apache-2.0)** reranks
the **full 64-candidate pool at 512 tokens in 449 ms** on this CPU — **~13× faster**,
under the ceiling with 3× headroom, no candidate cut, no truncation — at comparable
BEIR accuracy (MiniLM-L12 52.0 vs bge-base 51.6; L6 ~1 pt below L12).

## Web research (2026 landscape, verified)

Deep-research (`memphant-reranker-landscape-2026-07` memory) + targeted searches +
the owner's `awesome-rerankers`/FlashRank pointers converged:

- **Voyage `rerank-2.5` is still the latest** (no rerank-3); our `api_reranking.rs`
  is current. `rerank-2.5-lite` is the latency variant.
- **The fast, Apache/MIT, CPU-friendly cross-encoders**: `ms-marco-MiniLM-L6/L12`
  (22M/33M, Apache-2.0), FlashRank's `ms-marco-TinyBERT-L-2` (~4MB), `mxbai-rerank-
  base-v2` (0.5B). Jina rerankers are the accuracy leaders but **CC-BY-NC (non-
  commercial → disqualified)**. `bge-reranker-v2-m3` (568M) is *bigger* than
  bge-base, not faster.
- **INT8 ONNX ≈ 1.75–3× on CPU**; Sentence-Transformers v4.1 ONNX backend ≈ 2–3×.
- Newer efficiency architectures (CROSS-JEM "4× lower latency", Set-Encoder) exist
  but lack drop-in ONNX; MiniLM-L6-int8 is the pragmatic pick today.

## fastembed / ort knob map (5.17.2 / ort 2.0.0-rc.12)

- fastembed's `RerankerModel` enum has only 4 models (bge-base, bge-v2-m3, two
  Jina) — **no MiniLM, no quantized reranker**. All hardcode full-precision
  `model.onnx`.
- **The escape hatch: `TextRerank::try_new_from_user_defined(OnnxSource::File, TokenizerFiles)`**
  loads ANY local ONNX + tokenizer *without* an ort dependency and without an enum
  entry. This is how the MiniLM int8 arm is wired.
- fastembed hardcodes ONNX graph-opt Level3 (already near-max) and passes through
  only `execution_providers` + `intra_threads`. Inter-op threads / CoreML / CUDA
  need dropping to ort directly (CoreML is Mac-dev-only; CI is Linux) — not pursued.

## Measured latency (this Mac, cached models, synthetic 64×~1.5 KB docs)

`crates/memphant-runtime/src/embeddings.rs` — `rerank_real_model_latency_matrix`
(bge-base) + `rerank_byo_model_latency_matrix` (BYO), both `#[ignore]`d,
`MEMPHANT_RERANK_SMOKE=1` / `MEMPHANT_RERANK_BYO_DIR`.

| candidates × max_len | bge-reranker-base | ms-marco-MiniLM-L6 int8 | speedup |
|---|---|---|---|
| **64 × 512** (prod default) | **5954 ms** | **449 ms** | **13.3×** |
| 32 × 512 | 3024 ms | — | |
| 24 × 512 | 2166 ms | — | |
| 16 × 512 | 1420 ms | — | |
| 32 × 256 | 2041 ms | 188 ms | 10.9× |
| 24 × 256 | 1556 ms | 130 ms | 12.0× |
| 16 × 256 | 1039 ms | 89 ms | 11.7× |
| 32 × 128 | 1146 ms | — | |
| 24 × 128 | 857 ms | — | |

Notes: the local bge-base baseline (5954 ms) is faster than the July 12.9–13.6 s
figure — shorter synthetic docs / a faster machine — but the *relative* levers hold.
**Tuning bge-base alone** can reach the ceiling (16×512 = 1420 ms, 32×128 = 1146 ms)
but each option sacrifices either candidate depth or context; **the model swap needs
neither** and keeps full 64×512.

## Retrieval accuracy (free, LME-S, n=20 seed=3, recall@10)

Paired `bench-lme --cross-rerank` on ephemeral scratch PG, same sample/seed, local
`small` embedder:

| Arm | recall@10 | approx latency/query |
|---|---|---|
| no-rerank (baseline) | 0.474 | — |
| bge-reranker-base 24×256 | 0.474 | ~1556 ms |
| **ms-marco-MiniLM-L6 int8 24×256** | **0.526** | ~130 ms |
| **ms-marco-MiniLM-L6 int8 64×512** (full pool) | **0.526** | ~449 ms |

**MiniLM is at least parity — directionally better here.** At the *identical* 24×256
config it beat bge-base (0.526 vs 0.474, +0.053) at ~12× lower latency; at full
64×512 pool it held 0.526 in 449 ms (a config bge-base can't afford at ~5.9 s). The
no-rerank and bge-base arms tied at 0.474, so on this sample **rerank quality — not
just presence — is what moved retrieval, and MiniLM's speed is what makes full-pool
reranking affordable.**

**Caveats (honest):** n=20 is small — one question ≈ 0.053, so the +0.053 edge is a
single-question flip, directional not promotion-grade. This is the **retrieval** axis
only (free). Reader-QA — the binding adoption gate for flipping rerank default-on —
is a separate paid step, still gated per the plan; run it at n≥100 with CIs before
any default flip. recall@10 is also a weak discriminator when gold is already
in-pool (rung-7), so a rank-sensitive metric (first-answer-rank / recall@5) at
larger n is the better next screen.

## Cohere & Contextual Reranker v2 (owner-requested comparison)

Both fall outside the CPU-self-host-commercial constraint:
- **Cohere Rerank**: no `COHERE_API_KEY` in env or Doppler `syndai/dev`; API-only,
  not self-hostable. Would be a cost-arm like Voyage, not a self-host option.
- **Contextual AI Reranker v2** (`ctxl-rerank-v2-instruct-multilingual-1b/2b/6b`,
  Aug 2025 — the "multilingual instruction-following reranker"): **CC-BY-NC-SA-4.0
  (non-commercial)** AND GPU-only (1B+ causal LM, BF16/vLLM/NVFP4; CPU fallback
  "impractical for production"). Doubly disqualified for our path — an accuracy-
  frontier/GPU option, not a latency-fit self-host one.

Available API keys for optional accuracy/cost *reference* arms (not self-hostable):
`VOYAGE` (rerank-2.5), `JINA` (jina-v2, CC-BY-NC for self-host but API-testable).

## Wiring landed (spike → usable seam)

- `FastEmbedCrossReranker::from_user_defined(dir, onnx_name, config)` — loads a
  local ONNX + tokenizer through fastembed's user-defined path
  (`crates/memphant-runtime/src/embeddings.rs`).
- `MEMPHANT_RERANKER=byo` dispatch (`crates/memphant-runtime/src/lib.rs`), reading
  `MEMPHANT_RERANK_BYO_DIR` + optional `MEMPHANT_RERANK_BYO_ONNX`
  (default `model_quantized.onnx`), respecting the existing
  `MEMPHANT_RERANK_CANDIDATE_LIMIT`/`_MAX_LENGTH` knobs. The `--cross-rerank` bench
  arm and served recall both route through `build_cross_reranker`, so a bench and a
  served recall install byte-identical construction.
- Two `#[ignore]`d latency-matrix tests as reusable guards.

The model files are NOT committed (23 MB int8 ONNX + tokenizer; download from
`Xenova/ms-marco-MiniLM-L-6-v2`). The seam is data-agnostic.

## Recommendation

1. **Adopt ms-marco-MiniLM-L6 int8 as the cross-reranker model** behind the existing
   default-OFF flag — it removes the *latency* blocker entirely (13× headroom).
2. Keep bge-reranker-base as a selectable higher-accuracy arm (`MEMPHANT_RERANKER=fastembed`)
   for when latency is not a constraint (async knowledge panels).
3. **Gate the default-ON flip on paid reader-QA** (unchanged): confirm MiniLM's
   reader-QA is non-inferior to bge-base before flipping. The retrieval-parity check
   above is the free pre-screen.

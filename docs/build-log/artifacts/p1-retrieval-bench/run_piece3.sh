#!/usr/bin/env bash
# Piece 3 (rerankers): every arm reranks the SAME frozen retrieved-48 candidates
# (cands-final.json) → top-5. Local arms + Cohere/Voyage via the prod rerank-pool
# seam; ZeroEntropy via the harness rerank-api (not in prod code). Keys via env.
#
# Usage: source keys.env; BIN=... MINILM_DIR=... ./run_piece3.sh
set -uo pipefail
ART="$(cd "$(dirname "$0")" && pwd)"
POOL="$ART/pool.json"
OUT="$ART/out"
CANDS="$OUT/cands-final.json"
RETR="$OUT/retr-small-v2.json"   # frozen embedder+retrieval (small + RRF hybrid)
BIN="${BIN:?set BIN}"
MINILM_DIR="${MINILM_DIR:-$HOME/.cache/memphant-byo-minilm}"
BGE_DIR="${BGE_DIR:-}"           # optional bge-reranker-base byo dir (parity control)

# Freeze ONE candidate set (top-48 by the frozen retrieval) for every arm.
python3 "$ART/harness.py" make-candidates --pool "$POOL" --retr "$RETR" --k 48 --out "$CANDS"

score() { # arm-label rr-json
  python3 "$ART/harness.py" score --pool "$POOL" --retr "$RETR" --rr "$2" \
      --out "$OUT/score-rr-$1.json"
}

echo "=== none (retrieval-only baseline) ==="
python3 "$ART/harness.py" score --pool "$POOL" --retr "$RETR" --out "$OUT/score-rr-none.json"

echo "=== MiniLM-L6-int8 chunk (local, prod seam) ==="
MEMPHANT_RERANKER=byo MEMPHANT_RERANK_BYO_DIR="$MINILM_DIR" MEMPHANT_RERANK_TIMEOUT_MS=0 \
    "$BIN" rerank-pool --candidates "$CANDS" --granularity chunk --out "$OUT/rr-minilm.json" \
    && score minilm-chunk "$OUT/rr-minilm.json"

if [ -n "$BGE_DIR" ]; then
  echo "=== bge-reranker-base chunk (local parity) ==="
  MEMPHANT_RERANKER=byo MEMPHANT_RERANK_BYO_DIR="$BGE_DIR" MEMPHANT_RERANK_BYO_ONNX=model.onnx \
      MEMPHANT_RERANK_TIMEOUT_MS=0 "$BIN" rerank-pool --candidates "$CANDS" --granularity chunk \
      --out "$OUT/rr-bge.json" && score bge-chunk "$OUT/rr-bge.json"
fi

echo "=== Cohere v3.5 (prod seam, doc granularity — hosted handles long ctx) ==="
MEMPHANT_RERANKER=cohere-rerank-3.5 MEMPHANT_RERANK_TIMEOUT_MS=0 \
    "$BIN" rerank-pool --candidates "$CANDS" --granularity doc --out "$OUT/rr-cohere35.json" \
    && score cohere-v3.5 "$OUT/rr-cohere35.json"

echo "=== Cohere v4.0-fast ==="
MEMPHANT_RERANKER=cohere-rerank-3.5 MEMPHANT_COHERE_MODEL=rerank-v4.0-fast MEMPHANT_RERANK_TIMEOUT_MS=0 \
    "$BIN" rerank-pool --candidates "$CANDS" --granularity doc --out "$OUT/rr-cohere4fast.json" \
    && score cohere-v4.0-fast "$OUT/rr-cohere4fast.json"

echo "=== Cohere v4.0-pro ==="
MEMPHANT_RERANKER=cohere-rerank-3.5 MEMPHANT_COHERE_MODEL=rerank-v4.0-pro MEMPHANT_RERANK_TIMEOUT_MS=0 \
    "$BIN" rerank-pool --candidates "$CANDS" --granularity doc --out "$OUT/rr-cohere4pro.json" \
    && score cohere-v4.0-pro "$OUT/rr-cohere4pro.json"

echo "=== Voyage rerank-2.5 (prod seam) ==="
MEMPHANT_RERANKER=voyage-rerank-2.5 MEMPHANT_RERANK_TIMEOUT_MS=0 \
    "$BIN" rerank-pool --candidates "$CANDS" --granularity doc --out "$OUT/rr-voyage.json" \
    && score voyage-2.5 "$OUT/rr-voyage.json"

echo "=== ZeroEntropy zerank-2 (harness rerank-api, not in prod code) ==="
python3 "$ART/harness.py" rerank-api --arm zerank-2 --cands "$CANDS" --out "$OUT/rr-zerank2.json" \
    && score zerank-2 "$OUT/rr-zerank2.json"

echo "piece3 done"

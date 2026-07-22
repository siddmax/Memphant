#!/usr/bin/env bash
# Piece 1 (embedders): embed the fixed pool with each arm through the prod seam,
# retrieve top-48 with the prod-mirror RRF hybrid (v2), rerank to top-5 with
# MiniLM chunk-rerank, score recall@5 / MRR / recall@48. Local arms are free;
# API arms (openai/gemini/jina) MUST be wrapped in Doppler by the caller.
#
# Usage (local arms, no Doppler):
#   ./run_piece1.sh small modernbert gemma
# Usage (API arms, Doppler-wrapped):
#   doppler run -p syndai -c dev -- ./run_piece1.sh openai-text-embedding-3-small \
#       gemini-embedding-001 gemini-embedding-2 jina-v5-small
#
# Env: BIN (memphant-eval binary, built --features fastembed),
#      MINILM_DIR (~/.cache/memphant-byo-minilm).
set -euo pipefail
ART="$(cd "$(dirname "$0")" && pwd)"
POOL="$ART/pool.json"
OUT="$ART/out"
mkdir -p "$OUT"
BIN="${BIN:?set BIN to the memphant-eval binary}"
MINILM_DIR="${MINILM_DIR:-$HOME/.cache/memphant-byo-minilm}"

for ARM in "$@"; do
  echo "=== piece1 arm: $ARM ==="
  VEC="$OUT/vec-$ARM.jsonl"
  # 1. embed pool + queries (resume-safe; API arms Doppler-wrapped by caller)
  "$BIN" embed-pool --pool "$POOL" --embed-model "$ARM" --out "$VEC" --queries
  # 2. retrieve top-48, prod-mirror RRF hybrid
  python3 "$ART/harness.py" retrieve --pool "$POOL" --vectors "$VEC" \
      --variant v2 --out "$OUT/retr-$ARM-v2.json"
  # 3. candidates -> MiniLM chunk-rerank (prod seam) -> score
  python3 "$ART/harness.py" make-candidates --pool "$POOL" \
      --retr "$OUT/retr-$ARM-v2.json" --k 48 --out "$OUT/cands-$ARM.json"
  MEMPHANT_RERANKER=byo MEMPHANT_RERANK_BYO_DIR="$MINILM_DIR" MEMPHANT_RERANK_TIMEOUT_MS=0 \
      "$BIN" rerank-pool --candidates "$OUT/cands-$ARM.json" --granularity chunk \
      --out "$OUT/rr-$ARM-minilm.json"
  python3 "$ART/harness.py" score --pool "$POOL" \
      --retr "$OUT/retr-$ARM-v2.json" --rr "$OUT/rr-$ARM-minilm.json" \
      --out "$OUT/score-$ARM.json"
done
echo "piece1 arms done: $*"

#!/usr/bin/env python3
"""Cost-vs-accuracy analysis for the P1 benchmark. Recomputes $/query and the
Pareto/recommendation from committed score files + measured token counts.
Run: python3 cost_analysis.py"""
import json, os

CORPUS_TOK = 9_417_873   # measured: voyage-4 embed_pool approx_tokens over 33,224 chunks
ZE_TOK_PER_Q = 6_499_032 / 72  # measured: zerank-2 total_tokens / 72 queries (~90k, 48 full docs)

def r(p):
    return json.load(open(p))["overall"] if os.path.exists(p) else None

EMB = [("small (bge, local)", "scores/score-p1r-small.json", 0.0),
       ("openai-3-small", "scores/score-p1r-openai-text-embedding-3-small.json", 0.02),
       ("voyage-4", "scores/score-p1r-voyage-4.json", 0.06),
       ("voyage-context-4", "scores/score-p1r-voyage-context-4.json", 0.12),
       ("gemini-001", "scores/score-p1r-gemini-embedding-001.json", 0.15),
       ("gemini-embedding-2", "scores/score-p1r-gemini-embedding-2.json", 0.20)]

RR = [("none", "scores/score-rr-none.json", 0.0),
      ("MiniLM local", "scores/score-rr-minilm-chunk.json", 0.0),
      ("Cohere v3.5", "scores/score-rr-cohere-v3.5.json", 0.001),
      ("Cohere v4-fast", "scores/score-rr-cohere-v4.0-fast.json", 0.002),
      ("Cohere v4-pro", "scores/score-rr-cohere-v4.0-pro.json", 0.0025),
      ("zerank-2", "scores/score-rr-zerank-2.json", ZE_TOK_PER_Q / 1e6 * 0.025),
      ("Voyage 2.5", "scores/score-rr-voyage-2.5.json", ZE_TOK_PER_Q / 1e6 * 0.05)]

print("EMBEDDER: one-time corpus index cost (33,224 chunks); query embed ~$0")
for name, p, price in EMB:
    o = r(p)
    if o:
        print(f"  {name:22s} R@5={o['recall@5']:.3f} MRR={o['MRR']:.3f}  index=${CORPUS_TOK/1e6*price:.2f}")
print("\nRERANKER: recurring cost, billed per query (48 candidates each)")
for name, p, price in RR:
    o = r(p)
    if o:
        tag = "" if price else " ($0)"
        print(f"  {name:16s} R@5={o['recall@5']:.3f} MRR={o['MRR']:.3f} cov={o['gold_cov@5']:.3f} "
              f"${price*1000:.2f}/1k-q{tag}")

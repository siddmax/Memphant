#!/usr/bin/env python3
"""Fixed-pool reranker micro-benchmark — API arms.
Reranks each pool (gold at index 0) with each hosted model, computes rank of gold,
MRR@10 / recall@1/5/10, latency, and cost. Keys read from env; never printed."""
import os, json, time, urllib.request, urllib.error

POOLS = json.load(open(os.path.join(os.path.dirname(__file__), "rr_pools.json")))
COHERE_KEY = os.environ.get("COHERE_API_KEY", "")
VOYAGE_KEY = os.environ.get("VOYAGE_API_KEY", "")
ZE_KEY = os.environ.get("ZEROENTROPY_API_KEY", "")


def post(url, headers, body, timeout=60):
    req = urllib.request.Request(url, data=json.dumps(body).encode(), headers=headers)
    t = time.time()
    with urllib.request.urlopen(req, timeout=timeout) as r:
        d = json.loads(r.read())
    return d, (time.time() - t) * 1000.0


def cohere(model, query, docs):
    d, ms = post(
        "https://api.cohere.com/v2/rerank",
        {"Authorization": f"Bearer {COHERE_KEY}", "Content-Type": "application/json"},
        {"query": query, "documents": docs, "model": model, "top_n": len(docs), "max_tokens_per_doc": 4096},
    )
    order = [x["index"] for x in sorted(d["results"], key=lambda x: -x["relevance_score"])]
    billed = d.get("meta", {}).get("billed_units", {})
    return order, ms, billed


def voyage(model, query, docs):
    d, ms = post(
        "https://api.voyageai.com/v1/rerank",
        {"Authorization": f"Bearer {VOYAGE_KEY}", "Content-Type": "application/json"},
        {"query": query, "documents": docs, "model": model, "return_documents": False, "truncation": True},
    )
    order = [x["index"] for x in sorted(d["data"], key=lambda x: -x["relevance_score"])]
    billed = d.get("usage", {})
    return order, ms, billed


def zeroentropy(model, query, docs):
    d, ms = post(
        "https://api.zeroentropy.dev/v1/models/rerank",
        {"Authorization": f"Bearer {ZE_KEY}", "Content-Type": "application/json"},
        {"query": query, "documents": docs, "model": model, "top_n": len(docs)},
    )
    results = d.get("results", d.get("data", []))
    order = [x["index"] for x in sorted(results, key=lambda x: -x.get("relevance_score", x.get("score", 0)))]
    return order, ms, {"total_tokens": d.get("total_tokens", 0)}


def eval_model(name, fn, gap=0.4):
    ranks, lats, tokens = [], [], 0
    err = None
    for p in POOLS:
        try:
            order, ms, billed = fn(p["question"], p["docs"])
            rank = order.index(p["gold_index"]) + 1  # 1-based rank of gold
            ranks.append(rank); lats.append(ms)
            if isinstance(billed, dict):
                tokens += billed.get("total_tokens", 0) or billed.get("total_neurons", 0) or 0
        except urllib.error.HTTPError as e:
            err = f"HTTP {e.code}: {e.read()[:120]!r}"; break
        except Exception as e:
            err = str(e)[:150]; break
        time.sleep(gap)
    if err:
        print(f"{name:24s} FAILED: {err}"); return
    n = len(ranks)
    mrr = sum(1.0 / r for r in ranks) / n
    r1 = sum(1 for r in ranks if r <= 1) / n
    r5 = sum(1 for r in ranks if r <= 5) / n
    r10 = sum(1 for r in ranks if r <= 10) / n
    lats.sort()
    print(f"{name:24s} MRR={mrr:.3f} R@1={r1:.2f} R@5={r5:.2f} R@10={r10:.2f}  "
          f"lat p50={lats[n//2]:.0f}ms max={lats[-1]:.0f}ms  ranks={ranks}")


if __name__ == "__main__":
    import sys
    which = sys.argv[1] if len(sys.argv) > 1 else "all"
    if COHERE_KEY and which in ("all", "cohere"):
        eval_model("cohere-rerank-v3.5", lambda q, d: cohere("rerank-v3.5", q, d))
        eval_model("cohere-rerank-v4.0-fast", lambda q, d: cohere("rerank-v4.0-fast", q, d))
        eval_model("cohere-rerank-v4.0-pro", lambda q, d: cohere("rerank-v4.0-pro", q, d))
    if VOYAGE_KEY and which in ("all", "voyage"):
        eval_model("voyage-rerank-2.5", lambda q, d: voyage("rerank-2.5", q, d))
    if ZE_KEY and which in ("all", "zerank"):
        eval_model("zerank-2", lambda q, d: zeroentropy("zerank-2", q, d))

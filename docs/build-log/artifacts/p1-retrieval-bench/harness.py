#!/usr/bin/env python3
"""P1 retrieval-pipeline fixed-pool harness (stdlib only).

Shared math/scoring primitives for the adversarial benchmark: tokenize, Okapi
BM25, RRF and convex fusion, max-pool doc scoring, recall/MRR, exact sign test,
bootstrap CI. CLI subcommands (retrieve / make-candidates / rerank-api / score)
are added by later tasks; `selftest` runs the known-answer checks.

Run: python3 harness.py selftest
"""
import json
import math
import os
import re
import sys
import hashlib
import random
import time
import urllib.request
from collections import Counter

TOKEN_RE = re.compile(r"[a-z0-9']+")


def tokenize(text):
    return TOKEN_RE.findall(text.lower())


class BM25:
    """Okapi BM25 over a fixed doc list (k1=1.5, b=0.75)."""

    def __init__(self, docs_tokens, k1=1.5, b=0.75):
        self.k1, self.b = k1, b
        self.docs = [Counter(t) for t in docs_tokens]
        self.lens = [len(t) for t in docs_tokens]
        self.n = len(docs_tokens)
        self.avgdl = (sum(self.lens) / self.n) if self.n else 0.0
        df = Counter()
        for d in self.docs:
            df.update(d.keys())
        self.idf = {t: math.log((self.n - c + 0.5) / (c + 0.5) + 1.0) for t, c in df.items()}

    def score(self, query_tokens):
        scores = []
        for d, dl in zip(self.docs, self.lens):
            s = 0.0
            for t in query_tokens:
                tf = d.get(t)
                if not tf:
                    continue
                denom = tf + self.k1 * (1 - self.b + self.b * dl / (self.avgdl or 1.0))
                s += self.idf.get(t, 0.0) * tf * (self.k1 + 1) / denom
            scores.append(s)
        return scores


def cosine(a, b):
    dot = sum(x * y for x, y in zip(a, b))
    na = math.sqrt(sum(x * x for x in a))
    nb = math.sqrt(sum(y * y for y in b))
    return dot / (na * nb) if na and nb else 0.0


def max_pool_doc_scores(chunk_scores, chunk_doc_index, n_docs):
    doc_scores = [-math.inf] * n_docs
    for s, di in zip(chunk_scores, chunk_doc_index):
        if s > doc_scores[di]:
            doc_scores[di] = s
    return doc_scores


def rrf_fuse(rank_lists, k=60):
    fused = {}
    for ranks in rank_lists:
        for i, doc_id in enumerate(ranks):
            fused[doc_id] = fused.get(doc_id, 0.0) + 1.0 / (k + i + 1)
    return fused


def _minmax(scores):
    lo, hi = min(scores.values()), max(scores.values())
    span = (hi - lo) or 1.0
    return {d: (s - lo) / span for d, s in scores.items()}


def minmax_convex(dense, lexical, alpha):
    dn, ln = _minmax(dense), _minmax(lexical)
    return {d: alpha * dn.get(d, 0.0) + (1 - alpha) * ln.get(d, 0.0) for d in set(dn) | set(ln)}


def recall_at(ranked_ids, gold_ids, k):
    return 1.0 if any(d in gold_ids for d in ranked_ids[:k]) else 0.0


def mrr_first_gold(ranked_ids, gold_ids):
    for i, d in enumerate(ranked_ids):
        if d in gold_ids:
            return 1.0 / (i + 1)
    return 0.0


def sign_test_p(wins, losses):
    """Exact two-sided binomial sign test on discordant pairs."""
    n = wins + losses
    if n == 0:
        return 1.0
    tail = max(wins, losses)
    p_one = sum(math.comb(n, k) for k in range(tail, n + 1)) / 2.0 ** n
    return min(1.0, 2.0 * p_one)


def bootstrap_ci(deltas, seed=20260722, resamples=1000, level=0.95):
    rng = random.Random(seed)
    n = len(deltas)
    means = sorted(sum(rng.choice(deltas) for _ in range(n)) / n for _ in range(resamples))
    lo_i = int((1 - level) / 2 * resamples)
    hi_i = min(resamples - 1, int((1 + level) / 2 * resamples))
    return means[lo_i], means[hi_i]


def sha256_text(text):
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def load_vectors(path):
    """hash -> vec from an embed-pool JSONL; tolerates a truncated final line."""
    vecs = {}
    with open(path) as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            try:
                row = json.loads(line)
                vecs[row["hash"]] = row["vec"]
            except (json.JSONDecodeError, KeyError):
                continue  # crash-truncated tail
    return vecs


def query_hash(qid):
    """Key under which embed-pool stores a question's query embedding."""
    return sha256_text("q:" + qid)


def rank_docs_dense(pool_q, vecs, qvec):
    """Chunk-cosine max-pool per doc -> (ranked doc_ids, doc_id->score)."""
    scores = {}
    for d in pool_q["docs"]:
        best = -math.inf
        for c in d["chunks"]:
            v = vecs.get(sha256_text(c))
            if v is None:
                raise KeyError(f"missing vector for chunk of {d['doc_id']} (q {pool_q['qid']})")
            s = cosine(v, qvec)
            if s > best:
                best = s
        scores[d["doc_id"]] = best
    ranked = sorted(scores, key=lambda k: (-scores[k], k))
    return ranked, scores


def rank_docs_bm25(pool_q, query_tokens):
    """Okapi BM25 over chunks, max-pool per doc -> (ranked doc_ids, scores)."""
    chunk_tokens, chunk_doc = [], []
    for i, d in enumerate(pool_q["docs"]):
        for c in d["chunks"]:
            chunk_tokens.append(tokenize(c))
            chunk_doc.append(i)
    bm = BM25(chunk_tokens)
    chunk_scores = bm.score(query_tokens)
    doc_scores = max_pool_doc_scores(chunk_scores, chunk_doc, len(pool_q["docs"]))
    scores = {d["doc_id"]: s for d, s in zip(pool_q["docs"], doc_scores)}
    ranked = sorted(scores, key=lambda k: (-scores[k], k))
    return ranked, scores


def mmr_select(ranked_ids, rel_scores, doc_vecs, lam=0.7, k=48):
    """Greedy MMR over relevance-ranked docs using doc-vector similarity."""
    selected, remaining = [], list(ranked_ids)
    rel = _minmax(rel_scores) if rel_scores else {}
    while remaining and len(selected) < k:
        best_id, best_val = None, -math.inf
        for d in remaining:
            redundancy = max((cosine(doc_vecs[d], doc_vecs[s]) for s in selected), default=0.0)
            val = lam * rel.get(d, 0.0) - (1 - lam) * redundancy
            if val > best_val:
                best_id, best_val = d, val
        selected.append(best_id)
        remaining.remove(best_id)
    return selected


def contextualize_question(pool_q, date_field=True):
    """V5 contextual-retrieval-lite: prepend session date + session head to every
    chunk (embedding-side only; doc text and ids untouched)."""
    for d in pool_q["docs"]:
        head = re.sub(r"\s+", " ", d.get("text", d["chunks"][0] if d["chunks"] else ""))[:100]
        date = d.get("date", "") if date_field else ""
        prefix = f"[session {date}] [{head}] " if date else f"[{head}] "
        d["chunks"] = [prefix + c for c in d["chunks"]]
    return pool_q


def selftest():
    # tokenize
    assert tokenize("Hello, World! it's 42.") == ["hello", "world", "it's", "42"]

    # BM25: doc with the rare query term wins; absent term scores 0
    docs = [tokenize(t) for t in [
        "the cat sat on the mat",
        "the dog chased the tennis racket downtown",
        "a completely unrelated sentence about weather",
    ]]
    bm = BM25(docs)
    s = bm.score(tokenize("tennis racket"))
    assert s[1] > s[0] and s[1] > s[2], s
    assert s[0] == 0.0 and s[2] == 0.0, s

    # cosine
    assert abs(cosine([1, 0], [1, 0]) - 1.0) < 1e-9
    assert abs(cosine([1, 0], [0, 1])) < 1e-9

    # max-pool: doc 1 owns chunks 1,2 -> takes max of ITS chunks only
    doc_scores = max_pool_doc_scores([0.1, 0.9, 0.2, 0.5], [0, 1, 1, 2], 3)
    assert doc_scores == [0.1, 0.9, 0.5], doc_scores

    # RRF: consistent winner beats split ranks
    fused = rrf_fuse([["a", "b", "c"], ["a", "c", "b"]])
    assert max(fused, key=fused.get) == "a"

    # convex: alpha=1 follows dense, alpha=0 follows lexical
    dense = {"x": 0.9, "y": 0.5}
    lex = {"x": 1.0, "y": 3.0}
    assert max(minmax_convex(dense, lex, 1.0), key=minmax_convex(dense, lex, 1.0).get) == "x"
    assert max(minmax_convex(dense, lex, 0.0), key=minmax_convex(dense, lex, 0.0).get) == "y"

    # recall/MRR
    assert recall_at(["d1", "d2", "d3"], {"d3"}, 2) == 0.0
    assert recall_at(["d1", "d2", "d3"], {"d3"}, 3) == 1.0
    assert abs(mrr_first_gold(["d1", "d2", "d3"], {"d2", "d3"}) - 0.5) < 1e-9
    assert mrr_first_gold(["d1"], {"missing"}) == 0.0

    # exact two-sided sign test: 6-0 -> 0.03125; 10-4 -> ~0.1796 (NOT significant)
    assert abs(sign_test_p(6, 0) - 0.03125) < 1e-9
    assert abs(sign_test_p(10, 4) - 0.17956542968) < 1e-6
    assert sign_test_p(0, 0) == 1.0

    # bootstrap: deterministic under seed, CI brackets the mean for constant deltas
    lo, hi = bootstrap_ci([0.1] * 20)
    assert abs(lo - 0.1) < 1e-9 and abs(hi - 0.1) < 1e-9
    lo1, hi1 = bootstrap_ci([0.0, 0.1, 0.2, -0.1, 0.3] * 8)
    lo2, hi2 = bootstrap_ci([0.0, 0.1, 0.2, -0.1, 0.3] * 8)
    assert (lo1, hi1) == (lo2, hi2)
    assert lo1 < 0.1 < hi1

    # dense retrieval over a synthetic pool: gold doc has the best-matching chunk
    pool_q = {"qid": "q1", "question": "find the needle", "gold_doc_ids": ["g"],
              "abstention": False,
              "docs": [
                  {"doc_id": "g", "is_gold": True, "chunks": ["hay", "needle text"]},
                  {"doc_id": "d1", "is_gold": False, "chunks": ["hay hay"]},
                  {"doc_id": "d2", "is_gold": False, "chunks": ["straw", "more hay"]},
              ]}
    vecs = {sha256_text("hay"): [1, 0, 0], sha256_text("needle text"): [0, 1, 0],
            sha256_text("hay hay"): [1, 0, 0], sha256_text("straw"): [0.9, 0.1, 0],
            sha256_text("more hay"): [1, 0, 0], sha256_text("q:q1"): [0, 1, 0.1]}
    ranked, _scores = rank_docs_dense(pool_q, vecs, vecs[sha256_text("q:q1")])
    assert ranked[0] == "g", ranked

    # bm25 doc ranking on the same pool: query term only in gold's chunk
    ranked_bm, _ = rank_docs_bm25(pool_q, tokenize("needle"))
    assert ranked_bm[0] == "g", ranked_bm

    # MMR with lambda=1.0 degrades to pure relevance order
    ranked_mmr = mmr_select(["a", "b", "c"], {"a": 0.9, "b": 0.8, "c": 0.1},
                            {"a": [1, 0], "b": [1, 0], "c": [0, 1]}, lam=1.0, k=3)
    assert ranked_mmr == ["a", "b", "c"], ranked_mmr
    # MMR with low lambda promotes the diverse doc over the redundant one
    ranked_mmr2 = mmr_select(["a", "b", "c"], {"a": 0.9, "b": 0.8, "c": 0.7},
                             {"a": [1, 0], "b": [1, 0], "c": [0, 1]}, lam=0.3, k=3)
    assert ranked_mmr2[1] == "c", ranked_mmr2

    # context-prepend transformation: chunks gain prefix, text/doc_id untouched
    ctx_q = contextualize_question(json.loads(json.dumps(pool_q)), date_field=False)
    assert ctx_q["docs"][0]["chunks"][1].endswith("needle text")
    assert ctx_q["docs"][0]["chunks"][1] != "needle text"
    assert ctx_q["docs"][0].get("text") == pool_q["docs"][0].get("text")

    print("selftest OK")


def _load_pool(path):
    with open(path) as f:
        return json.load(f)


def _maxsim(query_tvecs, doc_tvecs):
    """ColBERT MaxSim: sum over query tokens of the max cosine to any doc token.
    Jina ColBERT vectors are L2-normalized, so cosine == dot product (skip the
    norm division — the pure-Python hot loop is the bottleneck without numpy)."""
    total = 0.0
    for q in query_tvecs:
        best = 0.0
        for d in doc_tvecs:
            s = 0.0
            for i in range(len(q)):
                s += q[i] * d[i]
            if s > best:
                best = s
        total += best
    return total


def _jina_colbert(texts, input_type, key):
    """Per-token (multi-vector) embeddings from Jina ColBERT v2. Cached per
    (input_type, text-hash) on disk so a re-run is free."""
    cache_dir = os.path.join(os.path.dirname(os.path.abspath(__file__)), "cache", "colbert")
    os.makedirs(cache_dir, exist_ok=True)
    out = [None] * len(texts)
    missing, missing_idx = [], []
    for i, t in enumerate(texts):
        p = os.path.join(cache_dir, sha256_text(input_type + "\x00" + t) + ".json")
        if os.path.exists(p):
            with open(p) as f:
                out[i] = json.load(f)
        else:
            missing.append(t)
            missing_idx.append(i)
    for b in range(0, len(missing), 32):
        batch = missing[b:b + 32]
        body = {"model": "jina-colbert-v2", "input": batch, "input_type": input_type}
        req = urllib.request.Request(
            "https://api.jina.ai/v1/multi-vector",
            data=json.dumps(body).encode(),
            headers={"Authorization": f"Bearer {key}", "Content-Type": "application/json",
                     # Jina sits behind Cloudflare, which 1010-blocks the default
                     # python-urllib UA; a normal UA clears it.
                     "User-Agent": "curl/8.4.0", "Accept": "application/json"})
        for attempt in range(4):
            try:
                with urllib.request.urlopen(req, timeout=120) as r:
                    d = json.loads(r.read())
                break
            except Exception as e:  # noqa: BLE001
                if attempt == 3:
                    raise
                time.sleep(2.0 * (attempt + 1))
        for j, item in enumerate(d["data"]):
            idx = missing_idx[b + j]
            out[idx] = item["embeddings"]
            with open(os.path.join(cache_dir,
                                   sha256_text(input_type + "\x00" + batch[j]) + ".json"), "w") as f:
                json.dump(item["embeddings"], f)
    return out


def cmd_retrieve_colbert(args):
    """V6: late-interaction MaxSim over the full per-question pool (no ANN — the
    pool is ~100 docs). Processes one question at a time so no giant multi-vector
    cache is held; token-vecs are disk-cached per text for free re-runs. Doc
    score = MAX MaxSim over its chunks (chunk-level late interaction)."""
    import time as _t
    key = os.environ.get("JINA_API_KEY", "")
    if not key:
        raise SystemExit("JINA_API_KEY missing (Doppler syndai/dev)")
    pool = _load_pool(args.pool)
    questions = pool["questions"]
    if args.subset:
        questions = questions[: args.subset]
    out = {"meta": {"variant": "v6", "model": "jina-colbert-v2",
                    "subset": args.subset or len(questions)}, "questions": []}
    for qi, q in enumerate(questions):
        t0 = _t.perf_counter()
        qvec = _jina_colbert([q["question"]], "query", key)[0]
        chunk_texts, chunk_doc = [], []
        for di, d in enumerate(q["docs"]):
            for c in d["chunks"]:
                chunk_texts.append(c)
                chunk_doc.append(di)
        cvecs = _jina_colbert(chunk_texts, "document", key)
        chunk_scores = [_maxsim(qvec, cv) for cv in cvecs]
        doc_scores = max_pool_doc_scores(chunk_scores, chunk_doc, len(q["docs"]))
        ranked = sorted(range(len(q["docs"])), key=lambda i: -doc_scores[i])
        out["questions"].append({
            "qid": q["qid"],
            "ranked": [q["docs"][i]["doc_id"] for i in ranked],
            "elapsed_ms": (_t.perf_counter() - t0) * 1000.0})
        print(f"  colbert {qi + 1}/{len(questions)} {q['qid']}", flush=True)
    with open(args.out, "w") as f:
        json.dump(out, f)
    print(f"retrieve v6 (colbert): {len(out['questions'])} questions -> {args.out}")


def cmd_retrieve(args):
    import time as _t
    pool = _load_pool(args.pool)
    vecs = load_vectors(args.vectors)
    out = {"meta": {"variant": args.variant, "vectors": os.path.basename(args.vectors),
                    "alpha": args.alpha, "mmr_lambda": args.mmr_lambda},
           "questions": []}
    for q in pool["questions"]:
        t0 = _t.perf_counter()
        qvec = vecs.get(query_hash(q["qid"]))
        if args.variant != "v1" and qvec is None:
            raise SystemExit(f"missing query vector for {q['qid']} in {args.vectors}")
        if args.variant == "v1":
            ranked, _ = rank_docs_bm25(q, tokenize(q["question"]))
        elif args.variant == "v0":
            ranked, _ = rank_docs_dense(q, vecs, qvec)
        elif args.variant == "v2":
            rd, _ = rank_docs_dense(q, vecs, qvec)
            rb, _ = rank_docs_bm25(q, tokenize(q["question"]))
            fused = rrf_fuse([rd, rb])
            ranked = sorted(fused, key=lambda k: (-fused[k], k))
        elif args.variant == "v3":
            _, sd = rank_docs_dense(q, vecs, qvec)
            _, sb = rank_docs_bm25(q, tokenize(q["question"]))
            fused = minmax_convex(sd, sb, args.alpha)
            ranked = sorted(fused, key=lambda k: (-fused[k], k))
        elif args.variant == "v7":
            rd, sd = rank_docs_dense(q, vecs, qvec)
            doc_vecs = {}
            for d in q["docs"]:
                best, bv = -math.inf, None
                for c in d["chunks"]:
                    v = vecs[sha256_text(c)]
                    s = cosine(v, qvec)
                    if s > best:
                        best, bv = s, v
                doc_vecs[d["doc_id"]] = bv
            ranked = mmr_select(rd, sd, doc_vecs, lam=args.mmr_lambda, k=len(rd))
        else:
            raise SystemExit(f"unknown variant {args.variant} (v4/v5 = same code on "
                             "prefix-vectors / context-pool; v6 has its own path)")
        out["questions"].append({"qid": q["qid"], "ranked": ranked,
                                 "elapsed_ms": (_t.perf_counter() - t0) * 1000.0})
    with open(args.out, "w") as f:
        json.dump(out, f)
    lat = sorted(x["elapsed_ms"] for x in out["questions"])
    print(f"retrieve {args.variant}: {len(out['questions'])} questions -> {args.out} "
          f"(retrieval p50 {lat[len(lat) // 2]:.1f}ms)")


def cmd_make_context_pool(args):
    pool = _load_pool(args.pool)
    for q in pool["questions"]:
        contextualize_question(q)
    pool["meta"]["contextualized"] = True
    with open(args.out, "w") as f:
        json.dump(pool, f)
    print(f"context pool -> {args.out}")


def cmd_make_candidates(args):
    pool = _load_pool(args.pool)
    with open(args.retr) as f:
        retr = json.load(f)
    ranked_by_qid = {x["qid"]: x["ranked"] for x in retr["questions"]}
    cands = []
    for q in pool["questions"]:
        by_id = {d["doc_id"]: d for d in q["docs"]}
        top = ranked_by_qid[q["qid"]][:args.k]
        cands.append({"qid": q["qid"], "question": q["question"],
                      "docs": [{"doc_id": i, "text": by_id[i]["text"],
                                "chunks": by_id[i]["chunks"]} for i in top]})
    with open(args.out, "w") as f:
        json.dump(cands, f)
    print(f"candidates k={args.k} -> {args.out}")


def _final_ranking(retr_ranked, rr_scores):
    """Reranked head (score desc, stable by retrieval rank) + untouched tail."""
    if not rr_scores:
        return retr_ranked
    head = [d for d in retr_ranked if d in rr_scores]
    tail = [d for d in retr_ranked if d not in rr_scores]
    pos = {d: i for i, d in enumerate(head)}
    head.sort(key=lambda d: (-rr_scores[d], pos[d]))
    return head + tail


def cmd_score(args):
    pool = _load_pool(args.pool)
    with open(args.retr) as f:
        retr = json.load(f)
    ranked_by_qid = {x["qid"]: x["ranked"] for x in retr["questions"]}
    retr_lat = {x["qid"]: x["elapsed_ms"] for x in retr["questions"]}
    rr_by_qid, rr_lat = {}, {}
    if args.rr:
        with open(args.rr) as f:
            for x in json.load(f):
                rr_by_qid[x["qid"]] = x["scores"]
                rr_lat[x["qid"]] = x.get("elapsed_ms", 0.0)
    rows, abst_rows = [], []
    for q in pool["questions"]:
        ranked = _final_ranking(ranked_by_qid[q["qid"]], rr_by_qid.get(q["qid"]))
        if q["abstention"]:
            top = rr_by_qid.get(q["qid"])
            abst_rows.append({"qid": q["qid"],
                              "max_rr_score": max(top.values()) if top else None})
            continue
        golds = set(q["gold_doc_ids"])
        cov5 = len(golds & set(ranked[:5])) / len(golds)
        cov48 = len(golds & set(ranked[:48])) / len(golds)
        rows.append({"qid": q["qid"], "qtype": q["qtype"],
                     "r5": recall_at(ranked, golds, 5),
                     "r16": recall_at(ranked, golds, 16),
                     "r48": recall_at(ranked, golds, 48),
                     "mrr": mrr_first_gold(ranked, golds),
                     "cov5": cov5, "cov48": cov48,
                     "first_rank": next((i + 1 for i, d in enumerate(ranked) if d in golds),
                                        None),
                     "elapsed_ms": retr_lat[q["qid"]] + rr_lat.get(q["qid"], 0.0)})
    n = len(rows)

    def agg(rs):
        m = len(rs)
        return {"n": m, "recall@5": sum(r["r5"] for r in rs) / m,
                "recall@16": sum(r["r16"] for r in rs) / m,
                "recall@48": sum(r["r48"] for r in rs) / m,
                "MRR": sum(r["mrr"] for r in rs) / m,
                "gold_cov@5": sum(r["cov5"] for r in rs) / m,
                "gold_cov@48": sum(r["cov48"] for r in rs) / m}

    lat = sorted(r["elapsed_ms"] for r in rows)
    report = {"config": {"retr": args.retr, "rr": args.rr},
              "overall": agg(rows),
              "latency_ms": {"p50": lat[n // 2], "p95": lat[int(n * 0.95)]},
              "by_type": {t: agg([r for r in rows if r["qtype"] == t])
                          for t in sorted({r["qtype"] for r in rows})},
              "abstention": abst_rows, "rows": rows}
    with open(args.out, "w") as f:
        json.dump(report, f, indent=1)
    o = report["overall"]
    print(f"score: n={n}  R@5={o['recall@5']:.3f}  MRR={o['MRR']:.3f}  "
          f"R@16={o['recall@16']:.3f}  R@48={o['recall@48']:.3f}  "
          f"cov@5={o['gold_cov@5']:.3f}  lat p50={report['latency_ms']['p50']:.0f}ms "
          f"-> {args.out}")


def cmd_compare(args):
    with open(args.a) as f:
        a = json.load(f)
    with open(args.b) as f:
        b = json.load(f)
    arows = {r["qid"]: r for r in a["rows"]}
    brows = {r["qid"]: r for r in b["rows"]}
    common = sorted(set(arows) & set(brows))
    wins = sum(1 for q in common if brows[q]["r5"] > arows[q]["r5"])
    losses = sum(1 for q in common if brows[q]["r5"] < arows[q]["r5"])
    deltas = [brows[q]["mrr"] - arows[q]["mrr"] for q in common]
    lo, hi = bootstrap_ci(deltas)
    p = sign_test_p(wins, losses)
    print(f"compare (B vs A, n={len(common)}): recall@5 flips +{wins}/-{losses} "
          f"(exact sign p={p:.4f}); dMRR={sum(deltas) / len(deltas):+.4f} "
          f"CI95=[{lo:+.4f},{hi:+.4f}]")
    flips = [q for q in common if brows[q]["r5"] != arows[q]["r5"]]
    if flips:
        print(f"  flipped qids: {flips}")


def cmd_rerank_api(args):
    """Hosted rerankers with no prod-seam arm (ZeroEntropy only; Cohere/Voyage go
    through `memphant-eval rerank-pool`)."""
    key = os.environ.get("ZEROENTROPY_API_KEY", "")
    if not key:
        raise SystemExit("ZEROENTROPY_API_KEY missing")
    with open(args.cands) as f:
        cands = json.load(f)
    cache_dir = os.path.join(os.path.dirname(os.path.abspath(__file__)), "cache", "rr-api")
    os.makedirs(cache_dir, exist_ok=True)
    results, total_tokens = [], 0
    for c in cands:
        texts = [d["text"] for d in c["docs"]]
        ck = sha256_text(args.arm + c["qid"] + sha256_text("".join(texts)))
        cpath = os.path.join(cache_dir, ck + ".json")
        if os.path.exists(cpath):
            with open(cpath) as f:
                row = json.load(f)
        else:
            t0 = time.perf_counter()
            body = {"query": c["question"], "documents": texts, "model": args.arm,
                    "top_n": len(texts)}
            # Retry 429/5xx with exponential backoff (ZeroEntropy rate-limits a
            # tight 72-question loop). Failures are NOT cached.
            d = None
            for attempt in range(6):
                req = urllib.request.Request(
                    "https://api.zeroentropy.dev/v1/models/rerank",
                    data=json.dumps(body).encode(),
                    headers={"Authorization": f"Bearer {key}",
                             "Content-Type": "application/json",
                             "User-Agent": "curl/8.4.0"})
                try:
                    with urllib.request.urlopen(req, timeout=120) as r:
                        d = json.loads(r.read())
                    break
                except urllib.error.HTTPError as e:
                    if e.code in (429, 500, 502, 503) and attempt < 5:
                        retry_after = e.headers.get("Retry-After")
                        wait = float(retry_after) if retry_after and retry_after.isdigit() \
                            else 2.0 * (2 ** attempt)
                        time.sleep(min(wait, 30.0))
                        continue
                    raise
            ms = (time.perf_counter() - t0) * 1000.0
            res = d.get("results", d.get("data", []))
            scores = {c["docs"][x["index"]]["doc_id"]:
                      x.get("relevance_score", x.get("score", 0.0)) for x in res}
            row = {"qid": c["qid"], "scores": scores, "elapsed_ms": ms,
                   "total_tokens": d.get("total_tokens", 0)}
            with open(cpath, "w") as f:
                json.dump(row, f)
            time.sleep(1.0)
        total_tokens += row.get("total_tokens", 0) or 0
        results.append(row)
    with open(args.out, "w") as f:
        json.dump(results, f)
    lats = sorted(r["elapsed_ms"] for r in results)
    print(f"rerank-api {args.arm}: {len(results)} queries, lat p50 "
          f"{lats[len(lats) // 2]:.0f}ms, total_tokens={total_tokens} -> {args.out}")


def main():
    import argparse
    ap = argparse.ArgumentParser(description=__doc__)
    sub = ap.add_subparsers(dest="cmd", required=True)
    sub.add_parser("selftest")
    r = sub.add_parser("retrieve")
    r.add_argument("--pool", required=True)
    r.add_argument("--vectors", required=True)
    r.add_argument("--variant", required=True)
    r.add_argument("--alpha", type=float, default=0.6)
    r.add_argument("--mmr-lambda", type=float, default=0.7)
    r.add_argument("--out", required=True)
    cp = sub.add_parser("make-context-pool")
    cp.add_argument("--pool", required=True)
    cp.add_argument("--out", required=True)
    mc = sub.add_parser("make-candidates")
    mc.add_argument("--pool", required=True)
    mc.add_argument("--retr", required=True)
    mc.add_argument("--k", type=int, default=48)
    mc.add_argument("--out", required=True)
    sc = sub.add_parser("score")
    sc.add_argument("--pool", required=True)
    sc.add_argument("--retr", required=True)
    sc.add_argument("--rr")
    sc.add_argument("--out", required=True)
    co = sub.add_parser("compare")
    co.add_argument("--a", required=True)
    co.add_argument("--b", required=True)
    ra = sub.add_parser("rerank-api")
    ra.add_argument("--arm", required=True)
    ra.add_argument("--cands", required=True)
    ra.add_argument("--out", required=True)
    rc = sub.add_parser("retrieve-colbert")
    rc.add_argument("--pool", required=True)
    rc.add_argument("--subset", type=int, default=0, help="0 = full pool")
    rc.add_argument("--out", required=True)
    args = ap.parse_args()
    if args.cmd == "selftest":
        selftest()
    elif args.cmd == "retrieve-colbert":
        cmd_retrieve_colbert(args)
    elif args.cmd == "retrieve":
        cmd_retrieve(args)
    elif args.cmd == "make-context-pool":
        cmd_make_context_pool(args)
    elif args.cmd == "make-candidates":
        cmd_make_candidates(args)
    elif args.cmd == "score":
        cmd_score(args)
    elif args.cmd == "compare":
        cmd_compare(args)
    elif args.cmd == "rerank-api":
        cmd_rerank_api(args)


if __name__ == "__main__":
    main()

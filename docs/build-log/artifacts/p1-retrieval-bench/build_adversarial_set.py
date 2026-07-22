#!/usr/bin/env python3
"""Build the P1 hard-adversarial fixed-pool test set from LongMemEval-S (ONE set,
reused by all three benchmark pieces). Plan: docs/superpowers/plans/
2026-07-22-p1-retrieval-pipeline-bench-plan.md (T1).

Selection (seed 20260722): 24 multi-session, 24 temporal-reasoning,
16 knowledge-update, 8 deep-buried single-session (answer >2000 chars in),
8 abstention = 80 questions (72 scored).

Pool per question = ~100 docs: the question's own haystack sessions (same-user
topical near-dups) + hard negatives mined from OTHER selected questions'
haystacks — half by BM25 similarity to (question + gold), half by embedding
cosine (text-embedding-3-small) to the gold centroid.

False-negative guards: mined docs never share an id with this question's
answer sessions, never contain the normalized answer string, never exceed
token-set Jaccard 0.8 vs any gold. Gold verification: string-locate, else one
LLM call per gold session asking whether it CONTRIBUTES to the answer (partial
evidence counts). Abstention pools: LLM spot-check that the top-5 most similar
docs do NOT contribute.

Keys via env ONLY (OPENAI_API_KEY for mining embeddings, OPENROUTER_API_KEY
for LLM verify) — wrap in Doppler. All API responses cached under cache/.
Corpus text is NOT committed; regenerate with this script.

Usage:
  python3 build_adversarial_set.py ../../../../benchmarks/data/longmemeval_s.json --out pool.json
  python3 build_adversarial_set.py --verify pool.json
"""
import argparse
import hashlib
import json
import math
import os
import random
import re
import sys
import time
import urllib.request
from concurrent.futures import ThreadPoolExecutor

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from harness import BM25, cosine, sha256_text, tokenize

SEED = 20260722
STRATA = {"multi-session": 24, "temporal-reasoning": 24, "knowledge-update": 16, "single-session": 8}
N_ABST = 8
POOL_DOCS = 100
CHUNK = 1200
DEEP_POS = 2000
JACCARD_CAP = 0.8
MINE_TRUNC = 8000
MINE_EMB_MODEL = "text-embedding-3-small"
VERIFY_MODEL = "anthropic/claude-sonnet-5"
CACHE_DIR = os.path.join(os.path.dirname(os.path.abspath(__file__)), "cache")

SINGLE_TYPES = ("single-session-user", "single-session-assistant", "single-session-preference")


def norm(s):
    return re.sub(r"\s+", " ", str(s).lower()).strip()


def sess_text(sess):
    return " ".join(t.get("content", "") for t in sess)


def chunk_text(text):
    return [text[i:i + CHUNK] for i in range(0, len(text), CHUNK)]


def locate_answer(text, answer):
    """Position of the normalized answer in normalized text, else a <=250-char
    window containing every answer word (short answers), else -1."""
    t, a = norm(text), norm(answer)
    if not a:
        return -1
    pos = t.find(a)
    if pos >= 0:
        return pos
    words = a.split()
    if 1 <= len(words) <= 6:
        first = words[0]
        start = t.find(first)
        while start >= 0:
            window = t[start:start + 250]
            if all(w in window for w in words):
                return start
            start = t.find(first, start + 1)
    return -1


def jaccard(tokens_a, tokens_b):
    sa, sb = set(tokens_a), set(tokens_b)
    if not sa or not sb:
        return 0.0
    return len(sa & sb) / len(sa | sb)


def cache_path(kind, key):
    d = os.path.join(CACHE_DIR, kind)
    os.makedirs(d, exist_ok=True)
    return os.path.join(d, key + ".json")


def cached(kind, key, fn):
    p = cache_path(kind, key)
    if os.path.exists(p):
        with open(p) as f:
            return json.load(f)
    v = fn()
    tmp = p + ".tmp"
    with open(tmp, "w") as f:
        json.dump(v, f)
    os.replace(tmp, p)
    return v


def post_json(url, headers, body, timeout=120, tries=4):
    err = None
    for i in range(tries):
        try:
            req = urllib.request.Request(url, data=json.dumps(body).encode(), headers=headers)
            with urllib.request.urlopen(req, timeout=timeout) as r:
                return json.loads(r.read())
        except Exception as e:  # noqa: BLE001 - retry any transport/HTTP error
            err = e
            time.sleep(2.0 * (i + 1))
    raise RuntimeError(f"POST {url} failed after {tries} tries: {err}")


def openai_embed(texts):
    """Embed texts (mining only), cached per text hash."""
    key = os.environ.get("OPENAI_API_KEY", "")
    if not key:
        raise SystemExit("OPENAI_API_KEY missing (wrap in doppler)")
    out, missing, missing_idx = [None] * len(texts), [], []
    for i, t in enumerate(texts):
        p = cache_path("mine-emb", sha256_text(t))
        if os.path.exists(p):
            with open(p) as f:
                out[i] = json.load(f)
        else:
            missing.append(t)
            missing_idx.append(i)
    for b in range(0, len(missing), 100):
        batch = missing[b:b + 100]
        d = post_json(
            "https://api.openai.com/v1/embeddings",
            {"Authorization": f"Bearer {key}", "Content-Type": "application/json"},
            {"model": MINE_EMB_MODEL, "input": batch},
        )
        for j, item in enumerate(d["data"]):
            idx = missing_idx[b + j]
            out[idx] = item["embedding"]
            with open(cache_path("mine-emb", sha256_text(batch[j])), "w") as f:
                json.dump(item["embedding"], f)
        print(f"  embedded {min(b + 100, len(missing))}/{len(missing)} new texts", flush=True)
    return out


def llm_contributes(question, session_text_str, cache_key):
    """YES/NO: does this session contribute evidence toward the answer?"""
    key = os.environ.get("OPENROUTER_API_KEY", "")
    if not key:
        raise SystemExit("OPENROUTER_API_KEY missing (wrap in doppler)")
    prompt = (
        "You are auditing a memory-retrieval benchmark.\n"
        f"QUESTION (asked at a later date): {question}\n\n"
        f"CHAT SESSION (recorded earlier):\n{session_text_str[:12000]}\n\n"
        "Does this session contain information that CONTRIBUTES to answering the "
        "question? Partial evidence counts - for multi-session questions no single "
        "session needs to suffice on its own. Answer with exactly YES or NO."
    )

    def call():
        # reasoning disabled: with it on, tiny max_tokens budgets go entirely to
        # reasoning and content comes back None.
        for attempt in range(3):
            d = post_json(
                "https://openrouter.ai/api/v1/chat/completions",
                {"Authorization": f"Bearer {key}", "Content-Type": "application/json"},
                {"model": VERIFY_MODEL, "temperature": 0, "max_tokens": 16,
                 "reasoning": {"enabled": False},
                 "messages": [{"role": "user", "content": prompt}]},
            )
            content = (d["choices"][0]["message"].get("content") or "").strip().upper()
            if content:
                return content
            time.sleep(1.0 + attempt)
        raise RuntimeError(f"empty verify response for {cache_key}")

    return cached("verify", cache_key, call).startswith("YES")


def llm_answers_alone(question, answer, session_text_str, cache_key):
    """Leak audit: could this session ALONE yield the labeled answer? Distinct
    from CONTRIBUTES — an old-value session in a knowledge-update question
    contributes topically but must NOT count as a duplicate gold."""
    key = os.environ.get("OPENROUTER_API_KEY", "")
    if not key:
        raise SystemExit("OPENROUTER_API_KEY missing (wrap in doppler)")
    prompt = (
        "You are auditing a memory-retrieval benchmark for unlabeled duplicate evidence.\n"
        f"QUESTION (asked at a later date): {question}\n"
        f"LABELED CORRECT ANSWER: {answer}\n\n"
        f"CHAT SESSION (recorded earlier):\n{session_text_str[:12000]}\n\n"
        "Using ONLY this session, could one correctly answer the question with the "
        "labeled answer above? Answer YES only if this session alone supports that "
        "exact answer; coincidental mentions or outdated values are NO. "
        "Answer with exactly YES or NO."
    )

    def call():
        for attempt in range(3):
            d = post_json(
                "https://openrouter.ai/api/v1/chat/completions",
                {"Authorization": f"Bearer {key}", "Content-Type": "application/json"},
                {"model": VERIFY_MODEL, "temperature": 0, "max_tokens": 16,
                 "reasoning": {"enabled": False},
                 "messages": [{"role": "user", "content": prompt}]},
            )
            content = (d["choices"][0]["message"].get("content") or "").strip().upper()
            if content:
                return content
            time.sleep(1.0 + attempt)
        raise RuntimeError(f"empty leak-audit response for {cache_key}")

    return cached("leak", cache_key, call).startswith("YES")


def stratum_of(q):
    qt = q["question_type"]
    return "single-session" if qt in SINGLE_TYPES else qt


class Corpus:
    """Unique sessions (by id) across the selected questions' haystacks."""

    def __init__(self):
        self.text = {}
        self.date = {}
        self.tokens = {}

    def add_question(self, q):
        for sid, date, sess in zip(q["haystack_session_ids"], q["haystack_dates"], q["haystack_sessions"]):
            if sid not in self.text:
                t = sess_text(sess)
                self.text[sid] = t
                self.date[sid] = date
                self.tokens[sid] = tokenize(t[:MINE_TRUNC])


def verify_question(q, verbose=True):
    """Return (ok, per-gold list of {sid, verified, answer_char_pos})."""
    sids = q["haystack_session_ids"]
    golds = []
    for aid in q.get("answer_session_ids") or []:
        if aid not in sids:
            return False, []
        text = sess_text(q["haystack_sessions"][sids.index(aid)])
        pos = locate_answer(text, q.get("answer", ""))
        if pos >= 0:
            golds.append({"sid": aid, "verified": "string", "answer_char_pos": pos})
        else:
            ok = llm_contributes(q["question"], text, f"{q['question_id']}-{aid}")
            if not ok:
                if verbose:
                    print(f"  DROP {q['question_id']}: gold {aid} does not contribute (LLM)")
                return False, []
            golds.append({"sid": aid, "verified": "llm", "answer_char_pos": -1})
    return bool(golds), golds


def build(args):
    t0 = time.time()
    print("loading LME-S...", flush=True)
    with open(args.data) as f:
        data = json.load(f)
    corpus_sha = hashlib.sha256(open(args.data, "rb").read()).hexdigest()
    rng = random.Random(SEED)

    by_stratum = {s: [] for s in STRATA}
    abst = []
    for q in data:
        if q["question_id"].endswith("_abs"):
            abst.append(q)
            continue
        s = stratum_of(q)
        if s in by_stratum:
            by_stratum[s].append(q)
    for s in by_stratum:
        rng.shuffle(by_stratum[s])
    rng.shuffle(abst)

    # Deep-burial requirement only for the single-session stratum.
    def eligible(q, s):
        aids = q.get("answer_session_ids") or []
        sids = q["haystack_session_ids"]
        if not aids or any(a not in sids for a in aids):
            return False
        if s == "single-session":
            text = sess_text(q["haystack_sessions"][sids.index(aids[0])])
            return locate_answer(text, q.get("answer", "")) > DEEP_POS
        return True

    print("selecting + verifying scored questions...", flush=True)
    selected, gold_info = [], {}
    for s, quota in STRATA.items():
        picked = 0
        for q in by_stratum[s]:
            if picked >= quota:
                break
            if not eligible(q, s):
                continue
            ok, golds = verify_question(q)
            if ok:
                selected.append(q)
                gold_info[q["question_id"]] = golds
                picked += 1
        if picked < quota:
            print(f"  WARNING: stratum {s} filled {picked}/{quota}")
    abst_selected = [q for q in abst[:N_ABST]]

    print(f"selected {len(selected)} scored + {len(abst_selected)} abstention "
          f"({time.time() - t0:.0f}s)", flush=True)

    corpus = Corpus()
    for q in selected + abst_selected:
        corpus.add_question(q)
    all_sids = sorted(corpus.text)
    print(f"mining corpus: {len(all_sids)} unique sessions", flush=True)

    print("embedding mining corpus (openai text-embedding-3-small, cached)...", flush=True)
    sid_vec = dict(zip(all_sids, openai_embed([corpus.text[s][:MINE_TRUNC] for s in all_sids])))

    bm25 = BM25([corpus.tokens[s] for s in all_sids])

    def mine(q, gold_sids, need, exclude, answer):
        """Top hard negatives: alternate BM25-mined and embedding-mined."""
        gold_texts = [corpus.text.get(g, "")[:2000] for g in gold_sids]
        query_toks = tokenize(q["question"]) + [t for g in gold_texts for t in tokenize(g)]
        bm_scores = bm25.score(query_toks)
        if gold_sids:
            vecs = [sid_vec[g] for g in gold_sids if g in sid_vec]
            centroid = [sum(c) / len(vecs) for c in zip(*vecs)] if vecs else None
        else:
            centroid = None
        qvec = centroid or openai_embed([q["question"]])[0]
        emb_ranked = sorted(all_sids, key=lambda s: -cosine(sid_vec[s], qvec))
        bm_ranked = [s for _, s in sorted(zip(bm_scores, all_sids), key=lambda x: -x[0])]
        na = norm(answer) if answer else ""
        gold_toks = [corpus.tokens.get(g, []) for g in gold_sids]

        def usable(sid):
            if sid in exclude:
                return False
            if na and locate_answer(corpus.text[sid], answer) >= 0:
                return False
            return not any(jaccard(corpus.tokens[sid], gt) > JACCARD_CAP for gt in gold_toks)

        mined, seen = [], set()
        streams = [(bm_ranked, "mined-bm25"), (emb_ranked, "mined-emb")]
        idx = [0, 0]
        while len(mined) < need and (idx[0] < len(all_sids) or idx[1] < len(all_sids)):
            for k, (ranked, label) in enumerate(streams):
                while idx[k] < len(ranked):
                    sid = ranked[idx[k]]
                    idx[k] += 1
                    if sid not in seen and usable(sid):
                        mined.append((sid, label))
                        seen.add(sid)
                        break
                if len(mined) >= need:
                    break
        return mined

    def build_docs(q, gold_map, abstention):
        sids = q["haystack_session_ids"]
        aids = set() if abstention else set(q.get("answer_session_ids") or [])
        docs, have = [], set()
        for sid, date, sess in zip(sids, q["haystack_dates"], q["haystack_sessions"]):
            if sid in have:
                continue
            have.add(sid)
            text = sess_text(sess)
            d = {"doc_id": sid, "date": date, "is_gold": sid in aids, "source": "haystack",
                 "text": text, "chunks": chunk_text(text)}
            if sid in aids:
                d["answer_char_pos"] = gold_map.get(sid, {}).get("answer_char_pos", -1)
            docs.append(d)
        exclude = have | set(q.get("answer_session_ids") or [])
        need = POOL_DOCS - len(docs)
        if need > 0:
            for sid, label in mine(q, sorted(aids), need, exclude,
                                   None if abstention else q.get("answer", "")):
                docs.append({"doc_id": sid, "date": corpus.date[sid], "is_gold": False,
                             "source": label, "text": corpus.text[sid],
                             "chunks": chunk_text(corpus.text[sid])})
        return docs

    print("building scored pools...", flush=True)
    questions = []
    for q in selected:
        golds = gold_info[q["question_id"]]
        gold_map = {g["sid"]: g for g in golds}
        docs = build_docs(q, gold_map, abstention=False)
        # Leak audit: own-haystack docs join the pool unguarded, and short
        # answers match filler chat coincidentally. Adjudicate every non-gold
        # doc containing the answer string: true duplicate -> remove, else waive.
        answer = q.get("answer", "")
        removed, waived = [], []
        for d in docs:
            if not d["is_gold"] and locate_answer(d["text"], answer) >= 0:
                if llm_answers_alone(q["question"], answer, d["text"],
                                     f"{q['question_id']}-leak-{d['doc_id']}"):
                    removed.append(d["doc_id"])
                else:
                    waived.append(d["doc_id"])
        if removed:
            print(f"  {q['question_id']}: removed {len(removed)} unlabeled duplicate(s): "
                  f"{removed}")
            docs = [d for d in docs if d["doc_id"] not in set(removed)]
        questions.append({
            "qid": q["question_id"], "qtype": q["question_type"], "abstention": False,
            "question": q["question"], "answer": str(q.get("answer", "")),
            "question_date": q.get("question_date", ""),
            "gold_doc_ids": [g["sid"] for g in golds],
            "gold_verified": "string" if all(g["verified"] == "string" for g in golds) else "llm",
            "leak_waived": waived,
            "docs": docs,
        })

    print("building abstention pools (adversarial spot-check)...", flush=True)
    for q in abst_selected:
        docs = build_docs(q, {}, abstention=True)
        # Spot-check the 5 docs most similar to the question: none may contribute.
        bm_local = BM25([tokenize(d["text"][:MINE_TRUNC]) for d in docs])
        scores = bm_local.score(tokenize(q["question"]))
        ranked = sorted(range(len(docs)), key=lambda i: -scores[i])
        removed = 0
        with ThreadPoolExecutor(8) as ex:
            checks = list(ex.map(
                lambda i: (i, llm_contributes(q["question"], docs[i]["text"],
                                              f"{q['question_id']}-abst-{docs[i]['doc_id']}")),
                ranked[:5]))
        for i, contributes in checks:
            if contributes:
                docs[i] = None
                removed += 1
        docs = [d for d in docs if d is not None]
        if removed:
            print(f"  {q['question_id']}: removed {removed} contributing docs from abstention pool")
        questions.append({
            "qid": q["question_id"], "qtype": q["question_type"], "abstention": True,
            "question": q["question"], "answer": str(q.get("answer", "")),
            "question_date": q.get("question_date", ""),
            "gold_doc_ids": [], "gold_verified": "n/a", "docs": docs,
        })

    n_scored = sum(1 for x in questions if not x["abstention"])
    pool = {"meta": {"seed": SEED, "corpus_sha256": corpus_sha, "chunk_chars": CHUNK,
                     "n_scored": n_scored, "n_abstention": len(questions) - n_scored,
                     "built_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())},
            "questions": questions}
    with open(args.out, "w") as f:
        json.dump(pool, f)
    print(f"wrote {args.out} ({os.path.getsize(args.out) // 1024 // 1024} MB, "
          f"{time.time() - t0:.0f}s total)")
    verify_pool(args.out)


def verify_pool(path):
    """Re-run every guard on an emitted pool; print the audit table."""
    with open(path) as f:
        pool = json.load(f)
    bad = 0
    strata = {}
    for x in pool["questions"]:
        docs = x["docs"]
        ids = [d["doc_id"] for d in docs]
        golds = set(x["gold_doc_ids"])
        errs = []
        if len(ids) != len(set(ids)):
            errs.append("dup doc ids")
        if x["abstention"]:
            if any(d["is_gold"] for d in docs):
                errs.append("gold in abstention pool")
        else:
            if not golds <= set(ids):
                errs.append("gold missing from pool")
            if x["gold_verified"] not in ("string", "llm"):
                errs.append("gold unverified")
            na = norm(x["answer"])
            waived = set(x.get("leak_waived", []))
            for d in docs:
                if (not d["is_gold"] and na and d["doc_id"] not in waived
                        and locate_answer(d["text"], x["answer"]) >= 0):
                    errs.append(f"answer leaked into non-gold {d['doc_id']} (unaudited)")
                    break
        if len(docs) < POOL_DOCS - 5:
            errs.append(f"pool only {len(docs)} docs")
        for d in docs:
            if "".join(d["chunks"]) != d["text"]:
                errs.append(f"chunks!=text for {d['doc_id']}")
                break
        key = ("abstention" if x["abstention"] else x["qtype"])
        s = strata.setdefault(key, [0, 0])
        s[0] += 1
        if errs:
            s[1] += 1
            bad += 1
            print(f"  FAIL {x['qid']}: {'; '.join(errs)}")
    print(f"verify: {len(pool['questions'])} questions "
          f"({pool['meta']['n_scored']} scored + {pool['meta']['n_abstention']} abstention), "
          f"{bad} with guard violations")
    for k, (n, b) in sorted(strata.items()):
        print(f"  {k}: {n} questions, {b} bad")
    gv = {}
    for x in pool["questions"]:
        gv[x["gold_verified"]] = gv.get(x["gold_verified"], 0) + 1
    print(f"  gold_verified: {gv}")
    src = {}
    for x in pool["questions"]:
        for d in x["docs"]:
            src[d["source"]] = src.get(d["source"], 0) + 1
    print(f"  doc sources: {src}")
    return bad == 0


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("data", nargs="?", help="path to longmemeval_s.json")
    ap.add_argument("--out", default="pool.json")
    ap.add_argument("--verify", metavar="POOL", help="re-run guards on an existing pool.json")
    args = ap.parse_args()
    if args.verify:
        sys.exit(0 if verify_pool(args.verify) else 1)
    if not args.data:
        ap.error("data path required (or --verify)")
    build(args)


if __name__ == "__main__":
    main()

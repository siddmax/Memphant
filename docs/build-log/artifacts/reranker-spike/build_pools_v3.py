#!/usr/bin/env python3
"""v3 fixed-pool test: 48 FULL (untruncated) LME-S sessions per pool.

GOLD = the full answer session (9-18 KB, genuinely contains the answer — verified),
DISTRACTORS = 47 random full OTHER sessions (same full-length distribution).
Tests how each reranker handles LONG documents at pool depth 48 (internal
truncation, long-context handling, latency on big inputs). Gold at index 0.

Corpus text is NOT committed; regenerate from benchmarks/data/longmemeval_s.json."""
import json, random, sys

POOL = 48
random.seed(3)
d = json.load(open(sys.argv[1] if len(sys.argv) > 1 else "benchmarks/data/longmemeval_s.json"))
render = lambda s: " ".join(t.get("content", "") for t in s)


def contains(text, answer):
    ans = str(answer).lower().strip()
    if ans and ans[:40] in text.lower():
        return True
    aw = ans.split()[0] if ans.split() else ""
    return bool(aw) and aw in text.lower()


pools, skipped = [], 0
for q in d:
    ans_ids = q.get("answer_session_ids") or []
    sids = q.get("haystack_session_ids") or []
    sess = q.get("haystack_sessions") or []
    if len(ans_ids) != 1 or ans_ids[0] not in sids:
        continue
    gi = sids.index(ans_ids[0])
    gold = render(sess[gi])
    if not contains(gold, q.get("answer", "")):
        skipped += 1
        continue
    others = [render(sess[i]) for i in range(len(sess)) if i != gi]
    if len(others) < POOL - 1:
        continue
    random.shuffle(others)
    docs = [gold] + others[: POOL - 1]
    pools.append({"question": q["question"], "answer": str(q.get("answer", "")),
                  "gold_index": 0, "docs": docs, "qid": q["question_id"]})
    if len(pools) >= 12:
        break

json.dump(pools, open("rr_pools_v3.json", "w"))
lens = [len(x) for p in pools for x in p["docs"]]
gold_lens = [len(p["docs"][0]) for p in pools]
ok = sum(1 for p in pools if contains(p["docs"][0], p["answer"]))
print(f"built {len(pools)} pools of {POOL} FULL sessions; gold-contains-answer {ok}/{len(pools)}; skipped {skipped}")
print(f"doc length: min={min(lens)} median={sorted(lens)[len(lens)//2]} max={max(lens)} chars")
print(f"gold length: min={min(gold_lens)} median={sorted(gold_lens)[len(gold_lens)//2]} max={max(gold_lens)} chars")

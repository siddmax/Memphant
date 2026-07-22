#!/usr/bin/env python3
"""Rebuild rr_pools.json from LME-S (seed 3): 8 pools, each = 1 gold session +
up to 63 real distractor sessions (gold at index 0). Corpus text is NOT committed;
this regenerates it from benchmarks/data/longmemeval_s.json (fetch_longmemeval.py)."""
import json, random, sys
d = json.load(open(sys.argv[1] if len(sys.argv) > 1 else "benchmarks/data/longmemeval_s.json"))
random.seed(3)
render = lambda sess: " ".join(t.get("content", "") for t in sess)[:1500]
pools = []
for q in d:
    ans, sids, sess = q.get("answer_session_ids") or [], q.get("haystack_session_ids") or [], q.get("haystack_sessions") or []
    if len(ans) != 1 or ans[0] not in sids:
        continue
    gi = sids.index(ans[0])
    others = [render(sess[i]) for i in range(len(sess)) if i != gi]
    random.shuffle(others)
    distractors = others[:63]
    if len(distractors) < 40:
        continue
    pools.append({"question": q["question"], "gold_index": 0, "docs": [render(sess[gi])] + distractors, "qid": q["question_id"]})
    if len(pools) >= 8:
        break
json.dump(pools, open("rr_pools.json", "w"))
print(f"built {len(pools)} pools -> rr_pools.json")

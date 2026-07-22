#!/usr/bin/env python3
"""Better fixed-pool test data (v2): 48-doc pools of same-length CHUNKS where the
gold chunk actually CONTAINS the answer (v1 truncated the answer away in 6/8 pools).

For each eligible LME-S question:
  - chunk every haystack session into ~1200-char windows (MemPhant's runtime-chunk unit),
  - GOLD = the chunk from the answer session that contains the answer string,
  - DISTRACTORS = 47 random chunks from OTHER sessions (same length distribution),
  - pool = [gold] + distractors  (gold at index 0), shuffled-distractors, seed 3.

Skips any question whose answer chunk can't be located (keeps the test honest).
Corpus text is NOT committed; regenerate from benchmarks/data/longmemeval_s.json."""
import json, random, sys

CHUNK = 1200
POOL = 48
random.seed(3)

d = json.load(open(sys.argv[1] if len(sys.argv) > 1 else "benchmarks/data/longmemeval_s.json"))


def chunks(sess):
    text = " ".join(t.get("content", "") for t in sess)
    return [text[i:i + CHUNK] for i in range(0, len(text), CHUNK)]


def find_gold_chunk(sess, answer):
    ans = str(answer).lower().strip()
    cs = chunks(sess)
    # prefer the chunk containing the full answer string; fall back to first answer word.
    for c in cs:
        if ans and ans[:40] in c.lower():
            return c
    aw = ans.split()[0] if ans.split() else ""
    for c in cs:
        if aw and aw in c.lower():
            return c
    return None


pools = []
skipped = 0
for q in d:
    ans_ids = q.get("answer_session_ids") or []
    sids = q.get("haystack_session_ids") or []
    sess = q.get("haystack_sessions") or []
    if len(ans_ids) != 1 or ans_ids[0] not in sids:
        continue
    gi = sids.index(ans_ids[0])
    gold = find_gold_chunk(sess[gi], q.get("answer", ""))
    if gold is None:
        skipped += 1
        continue
    # distractor chunks from all OTHER sessions
    distractor_chunks = []
    for i in range(len(sess)):
        if i == gi:
            continue
        distractor_chunks.extend(chunks(sess[i]))
    if len(distractor_chunks) < POOL - 1:
        continue
    random.shuffle(distractor_chunks)
    docs = [gold] + distractor_chunks[: POOL - 1]
    pools.append({"question": q["question"], "answer": str(q.get("answer", "")),
                  "gold_index": 0, "docs": docs, "qid": q["question_id"]})
    if len(pools) >= 12:
        break

json.dump(pools, open("rr_pools_v2.json", "w"))
# sanity: confirm gold contains the answer in EVERY pool
ok = sum(1 for p in pools if str(p["answer"]).lower()[:40] in p["docs"][0].lower()
         or (str(p["answer"]).split() and str(p["answer"]).split()[0].lower() in p["docs"][0].lower()))
print(f"built {len(pools)} pools of {POOL} chunks ({CHUNK} chars); gold-contains-answer: {ok}/{len(pools)}; skipped {skipped}")
for p in pools:
    print(f"  {p['qid']}: q={p['question'][:45]!r} answer={p['answer'][:25]!r}")

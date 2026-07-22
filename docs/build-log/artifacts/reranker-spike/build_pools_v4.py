#!/usr/bin/env python3
"""v4: the SAME 48 FULL-doc pools as v3, but each full doc is pre-CHUNKED into
~1200-char windows. Scored with max-pooling: a doc's score = the max score over
its chunks (the standard 'rerank chunks, aggregate to doc' pattern that a
512-token reranker needs to handle long docs). This mirrors MemPhant's
contextual-chunks design (default ON).

Output shape (rr_pools_chunked.json): a list of pools, each:
  {question, gold_doc:0, docs:[{chunks:[...]}, ...]}
The scorer reranks the flattened chunks per pool and max-pools back to docs."""
import json, sys

CHUNK = 1200
src = sys.argv[1] if len(sys.argv) > 1 else "rr_pools_v3.json"
v3 = json.load(open(src))


def chunk(text):
    return [text[i:i + CHUNK] for i in range(0, len(text), CHUNK)] or [""]


out = []
for p in v3:
    docs = [{"chunks": chunk(d)} for d in p["docs"]]
    out.append({"question": p["question"], "gold_index": p["gold_index"],
                "qid": p.get("qid", ""), "docs": docs})
json.dump(out, open("rr_pools_chunked.json", "w"))
total_chunks = sum(len(d["chunks"]) for p in out for d in p["docs"])
print(f"built {len(out)} chunked pools; {total_chunks} total chunks "
      f"(avg {total_chunks/sum(len(p['docs']) for p in out):.1f} chunks/doc)")

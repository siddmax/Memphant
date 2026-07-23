#!/usr/bin/env python3
"""Kill-gate (a) scorer: fraction of retired-run QA flips whose provenance
spans are fully contained in the top-16 FUSED (no-rerank) candidates.

Inputs: the flips extracted from r15-docs reader.json (killgate_a_flips.json)
and the k=16 no-rerank evidence JSONLs reproduced on the OLD pinned corpus.
A flip is "reproducible in top-16" iff gc.provenance_hit(golden, bodies, 16)
holds — multi-hop needs BOTH spans, mirroring the gate's grading exactly.
"""
import json
import sys
from pathlib import Path

sys.path.insert(0, "/Users/sidsharma/.codex/worktrees/Memphant/p1-deep-mode/scripts")
import gate_common as gc  # noqa: E402

SCRATCH = Path(__file__).resolve().parent
flips = json.loads((SCRATCH / "killgate_a_flips.json").read_text())["positive"]

goldens = {}
for name in ("syndai_docs_golden.jsonl", "syndai_docs_golden_v2.jsonl"):
    for row in gc.load_goldens(SCRATCH / "old-pins" / name):
        goldens[row["question_id"]] = row

evidence = {}
for name in ("ev-v1.jsonl", "ev-v2.jsonl"):
    for row in gc.load_goldens(SCRATCH / "killgate-a" / name):
        evidence[row["question_id"]] = [item["body"] for item in row["evidence"]]

reproducible, missing = [], []
for qid in flips:
    golden = goldens[qid]
    bodies = evidence[qid]
    assert len(bodies) <= 16
    if gc.provenance_hit(golden, bodies, 16):
        reproducible.append(qid)
    else:
        missing.append(qid)

frac = len(reproducible) / len(flips)
print(f"flips={len(flips)} reproducible_in_top16={len(reproducible)} fraction={frac:.3f}")
print(f"threshold=0.60 verdict={'PASS' if frac >= 0.60 else 'FAIL -> DROP C2'}")
print("reproducible:", *reproducible, sep="\n  ")
print("not_in_top16:", *missing, sep="\n  ")
json.dump(
    {
        "flips": len(flips),
        "reproducible_in_top16": len(reproducible),
        "fraction": frac,
        "threshold": 0.60,
        "pass": frac >= 0.60,
        "reproducible_qids": reproducible,
        "missing_qids": missing,
    },
    open(SCRATCH / "killgate-a" / "verdict.json", "w"),
    indent=1,
)

#!/usr/bin/env python3
"""Engine-vs-engine comparison for the Syndai replacement gate (W10).

Combines the two runners' provenance reports (R@5/R@10 retrieval) and the two
``run_reader.py`` QA reports (reader-scored answer accuracy) into a single
verdict, applying the binding gate rule from the plan addendum:

    MemPhant must BEAT Syndai's stack on the golden set — the paired QA-accuracy
    bootstrap CI (MemPhant - Syndai) must EXCLUDE ZERO and be positive — before
    any replacement work. If it does not beat it, that is a finding, not a
    failure.

Both a retrieval delta (provenance hit@10) and the QA delta are reported with a
paired bootstrap CI (``run_reader.bootstrap_ci``, reused so the CI method is
identical to the rest of the harness). Pairing is by question_id.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import gate_common as gc  # noqa: E402

bootstrap_ci = gc._RUN_READER.bootstrap_ci
BOOTSTRAP_RESAMPLES = gc._RUN_READER.BOOTSTRAP_RESAMPLES


def paired_deltas(
    memphant: dict[str, float], syndai: dict[str, float]
) -> list[float]:
    """MemPhant - Syndai per question_id present (and scored) in both."""
    return [
        memphant[qid] - syndai[qid]
        for qid in memphant
        if qid in syndai
    ]


def provenance_map(report: dict, field: str) -> dict[str, float]:
    return {
        row["question_id"]: float(row[field])
        for row in report["per_question"]
    }


def qa_map(report: dict) -> dict[str, float]:
    return {
        row["question_id"]: float(row["correct"])
        for row in report["per_question"]
        if row.get("correct") is not None
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--memphant-provenance", required=True)
    parser.add_argument("--syndai-provenance", required=True)
    parser.add_argument("--memphant-reader", help="run_reader QA report for MemPhant")
    parser.add_argument("--syndai-reader", help="run_reader QA report for Syndai")
    parser.add_argument("--out", required=True)
    parser.add_argument("--seed", type=int, default=20260711)
    args = parser.parse_args()

    mem_prov = json.loads(Path(args.memphant_provenance).read_text())
    syn_prov = json.loads(Path(args.syndai_provenance).read_text())

    result: dict = {
        "gate": "syndai_docs_engine_vs_engine",
        "seed": args.seed,
        "bootstrap_resamples": BOOTSTRAP_RESAMPLES,
        "retrieval": {},
        "qa": None,
        "verdict": None,
    }

    # Retrieval (provenance) comparison.
    mem_h10 = provenance_map(mem_prov, "hit_at_10")
    syn_h10 = provenance_map(syn_prov, "hit_at_10")
    prov_deltas = paired_deltas(mem_h10, syn_h10)
    result["retrieval"] = {
        "memphant": {
            "recall_at_5": mem_prov["recall_at_5"],
            "recall_at_10": mem_prov["recall_at_10"],
            "haystack_sections": mem_prov.get("haystack_sections"),
        },
        "syndai": {
            "recall_at_5": syn_prov["recall_at_5"],
            "recall_at_10": syn_prov["recall_at_10"],
            "haystack_sections": syn_prov.get("haystack_sections"),
        },
        "n_paired": len(prov_deltas),
        "delta_hit_at_10": bootstrap_ci(prov_deltas, BOOTSTRAP_RESAMPLES, args.seed),
    }

    # QA comparison (the binding verdict).
    if args.memphant_reader and args.syndai_reader:
        mem_reader = json.loads(Path(args.memphant_reader).read_text())
        syn_reader = json.loads(Path(args.syndai_reader).read_text())
        mem_qa = qa_map(mem_reader)
        syn_qa = qa_map(syn_reader)
        qa_deltas = paired_deltas(mem_qa, syn_qa)
        qa_ci = bootstrap_ci(qa_deltas, BOOTSTRAP_RESAMPLES, args.seed)
        beats = bool(qa_ci["ci_excludes_zero"] and qa_ci["mean"] > 0)
        result["qa"] = {
            "memphant_accuracy": mem_reader["overall"]["qa_accuracy"],
            "syndai_accuracy": syn_reader["overall"]["qa_accuracy"],
            "memphant_n_scored": mem_reader["overall"]["n_scored"],
            "syndai_n_scored": syn_reader["overall"]["n_scored"],
            "n_paired": len(qa_deltas),
            "delta_qa_accuracy": qa_ci,
            "reader_model": mem_reader.get("reader_model_id"),
            "judge_model": mem_reader.get("judge_model_id"),
        }
        result["verdict"] = {
            "rule": (
                "MemPhant beats Syndai iff the paired QA-accuracy bootstrap CI "
                "(MemPhant - Syndai) excludes zero and is positive"
            ),
            "memphant_beats_syndai": beats,
            "decision": (
                "PROCEED: MemPhant beats Syndai's stack on the golden set"
                if beats
                else "HOLD: MemPhant does not beat Syndai's stack (finding, not failure)"
            ),
        }
    else:
        result["verdict"] = {
            "rule": "QA reader reports required for the binding verdict",
            "memphant_beats_syndai": None,
            "decision": "INCOMPLETE: run run_reader.py on both engines' evidence",
        }

    Path(args.out).write_text(json.dumps(result, indent=2) + "\n")

    ret = result["retrieval"]
    print(
        f"RETRIEVAL  memphant R@5={ret['memphant']['recall_at_5']:.3f} "
        f"R@10={ret['memphant']['recall_at_10']:.3f} | "
        f"syndai R@5={ret['syndai']['recall_at_5']:.3f} "
        f"R@10={ret['syndai']['recall_at_10']:.3f} | "
        f"Δhit@10 mean={ret['delta_hit_at_10']['mean']:+.3f} "
        f"CI=[{ret['delta_hit_at_10']['ci95_low']:+.3f},"
        f"{ret['delta_hit_at_10']['ci95_high']:+.3f}]",
        file=sys.stderr,
    )
    if result["qa"]:
        qa = result["qa"]
        print(
            f"QA         memphant={qa['memphant_accuracy']} "
            f"syndai={qa['syndai_accuracy']} | "
            f"Δacc mean={qa['delta_qa_accuracy']['mean']:+.3f} "
            f"CI=[{qa['delta_qa_accuracy']['ci95_low']:+.3f},"
            f"{qa['delta_qa_accuracy']['ci95_high']:+.3f}] "
            f"excl_zero={qa['delta_qa_accuracy']['ci_excludes_zero']}",
            file=sys.stderr,
        )
        print(f"VERDICT    {result['verdict']['decision']}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

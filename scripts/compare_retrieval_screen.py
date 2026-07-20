#!/usr/bin/env python3
"""Fail-closed paired retrieval-screen comparison over pinned provenance.

Example (repeat all three positional pairs for v1/v2):

  python3 scripts/compare_retrieval_screen.py \
    --baseline-provenance small-v1.json --candidate-provenance modernbert-v1.json \
    --golden benchmarks/data/syndai_docs_golden.jsonl \
    --variant-key embed_model --out comparison.json

For a reranker arm, explicitly declare every runtime key intentionally changed,
for example ``--variant-key cross_rerank --variant-key cross_reranker
--variant-key requested_cross_reranker``. Every other runtime field is invariant.
This comparator reads artifacts only; it never runs a model or API.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import random
import statistics
import sys
from collections import defaultdict
from pathlib import Path
from typing import Any

sys.path.insert(0, str(Path(__file__).resolve().parent))
import gate_common as gc  # noqa: E402

DEFAULT_RESAMPLES = 10_000
DEFAULT_SEED = 20260713


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def fingerprint(value: Any) -> str:
    payload = json.dumps(value, sort_keys=True, separators=(",", ":")).encode()
    return hashlib.sha256(payload).hexdigest()


def _percentile(values: list[float], percentile: int) -> float:
    if not values:
        raise ValueError("cannot compute percentile of empty values")
    if len(values) == 1:
        return float(values[0])
    return float(
        statistics.quantiles(values, n=100, method="inclusive")[percentile - 1]
    )


def _cluster_bootstrap_ci(
    values_by_cluster: dict[str, list[float]], *, resamples: int, seed: int
) -> dict[str, float | int | bool]:
    if resamples < 1000:
        raise ValueError("cluster bootstrap requires at least 1000 resamples")
    cluster_ids = sorted(values_by_cluster)
    observed_values = [value for key in cluster_ids for value in values_by_cluster[key]]
    rng = random.Random(seed)
    samples = []
    for _ in range(resamples):
        selected = [rng.choice(cluster_ids) for _ in cluster_ids]
        values = [value for key in selected for value in values_by_cluster[key]]
        samples.append(sum(values) / len(values))
    samples.sort()
    low = samples[min(int(resamples * 0.025), resamples - 1)]
    high = samples[
        min(max(math.ceil(resamples * 0.975) - 1, 0), resamples - 1)
    ]
    return {
        "mean": sum(observed_values) / len(observed_values),
        "ci95_low": low,
        "ci95_high": high,
        "ci_excludes_zero": low > 0 or high < 0,
        "resamples": resamples,
        "seed": seed,
    }


def _arm_summary(rows: list[dict], hard_ceiling_ms: float) -> dict:
    latencies = [float(row["recall_e2e_ms"]) for row in rows]
    p95 = _percentile(latencies, 95)
    return {
        "recall_at_5": sum(row["hit_at_5"] for row in rows) / len(rows),
        "recall_at_10": sum(row["hit_at_10"] for row in rows) / len(rows),
        "recall_e2e_ms_p50": float(statistics.median(latencies)),
        "recall_e2e_ms_p95": p95,
        "p95_within_hard_ceiling": p95 <= hard_ceiling_ms,
    }


def compare_reports(
    report_sets: list[tuple[dict, dict, Path]],
    *,
    variant_keys: set[str],
    resamples: int = DEFAULT_RESAMPLES,
    seed: int = DEFAULT_SEED,
) -> dict:
    if not report_sets:
        raise ValueError("at least one report set is required")
    if not variant_keys:
        raise ValueError("at least one explicit variant key is required")

    all_baseline: list[dict] = []
    all_candidate: list[dict] = []
    deltas5: dict[str, list[float]] = defaultdict(list)
    deltas10: dict[str, list[float]] = defaultdict(list)
    set_summaries = []
    corpus_revision = None
    hard_ceiling = None
    seen_questions: set[tuple[str, str]] = set()
    baseline_arm_config = None
    candidate_arm_config = None

    for set_index, (baseline, candidate, golden_path) in enumerate(report_sets):
        golden_revision, golden_clusters = gc.golden_source_clusters(golden_path)
        baseline_rows = gc.validate_provenance_report(baseline)
        candidate_rows = gc.validate_provenance_report(candidate)
        if baseline["golden_revision"] != golden_revision or candidate["golden_revision"] != golden_revision:
            raise ValueError(f"set {set_index} golden revision does not match pinned golden")
        if baseline["corpus_revision"] != candidate["corpus_revision"]:
            raise ValueError(f"set {set_index} mixes corpus revisions")
        if corpus_revision is None:
            corpus_revision = baseline["corpus_revision"]
        elif baseline["corpus_revision"] != corpus_revision:
            raise ValueError("report sets mix corpus revisions")
        if set(baseline_rows) != set(candidate_rows):
            raise ValueError(f"set {set_index} question IDs are not exactly paired")
        if set(baseline_rows) != set(golden_clusters):
            raise ValueError(f"set {set_index} question IDs differ from pinned golden")

        baseline_runtime = dict(baseline["runtime_config"])
        candidate_runtime = dict(candidate["runtime_config"])
        for key in variant_keys:
            if key not in baseline_runtime or key not in candidate_runtime:
                raise ValueError(f"variant key {key!r} is absent from a runtime config")
            baseline_runtime.pop(key)
            candidate_runtime.pop(key)
        if baseline_runtime != candidate_runtime:
            raise ValueError(f"set {set_index} runtime invariant mismatch")
        normalized_baseline = dict(baseline["runtime_config"])
        normalized_candidate = dict(candidate["runtime_config"])
        normalized_baseline.pop("golden_revision", None)
        normalized_candidate.pop("golden_revision", None)
        if baseline_arm_config is None:
            baseline_arm_config = normalized_baseline
            candidate_arm_config = normalized_candidate
        else:
            if normalized_baseline != baseline_arm_config:
                raise ValueError("baseline arm configuration drift across golden sets")
            if normalized_candidate != candidate_arm_config:
                raise ValueError("candidate arm configuration drift across golden sets")

        ceilings = {
            float(baseline["recall_e2e_p95_ceiling_ms"]),
            float(candidate["recall_e2e_p95_ceiling_ms"]),
        }
        if len(ceilings) != 1:
            raise ValueError(f"set {set_index} mixes hard latency ceilings")
        ceiling = ceilings.pop()
        if hard_ceiling is None:
            hard_ceiling = ceiling
        elif ceiling != hard_ceiling:
            raise ValueError("report sets mix hard latency ceilings")

        for question_id in sorted(golden_clusters):
            identity = (golden_revision, question_id)
            if identity in seen_questions:
                raise ValueError(f"duplicate paired question {question_id!r}")
            seen_questions.add(identity)
            base_row = baseline_rows[question_id]
            cand_row = candidate_rows[question_id]
            # Namespace by corpus, but not candidate labels. Questions sourced
            # from the same pinned document across golden sets stay clustered.
            cluster = f"{corpus_revision}\x1e{golden_clusters[question_id]}"
            deltas5[cluster].append(float(cand_row["hit_at_5"] - base_row["hit_at_5"]))
            deltas10[cluster].append(float(cand_row["hit_at_10"] - base_row["hit_at_10"]))
            all_baseline.append(base_row)
            all_candidate.append(cand_row)
        set_summaries.append(
            {
                "golden_revision": golden_revision,
                "n_paired": len(golden_clusters),
                "baseline": _arm_summary(
                    [baseline_rows[qid] for qid in sorted(golden_clusters)], ceiling
                ),
                "candidate": _arm_summary(
                    [candidate_rows[qid] for qid in sorted(golden_clusters)], ceiling
                ),
            }
        )

    assert hard_ceiling is not None
    return {
        "comparison": "paired_retrieval_screen",
        "valid": True,
        "corpus_revision": corpus_revision,
        "variant_keys": sorted(variant_keys),
        "baseline_runtime_config_fingerprint": fingerprint(baseline_arm_config),
        "candidate_runtime_config_fingerprint": fingerprint(candidate_arm_config),
        "cluster_source": "pinned_golden.provenance[].file",
        "n_paired": len(all_baseline),
        "n_source_document_clusters": len(deltas5),
        "hard_ceiling_ms": hard_ceiling,
        "baseline": _arm_summary(all_baseline, hard_ceiling),
        "candidate": _arm_summary(all_candidate, hard_ceiling),
        "delta_recall_at_5": _cluster_bootstrap_ci(
            deltas5, resamples=resamples, seed=seed
        ),
        "delta_recall_at_10": _cluster_bootstrap_ci(
            deltas10, resamples=resamples, seed=seed + 1
        ),
        "sets": set_summaries,
    }


def _load_json(path: str) -> dict:
    value = json.loads(Path(path).read_text())
    if not isinstance(value, dict):
        raise ValueError(f"{path} is not a JSON object")
    return value


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline-provenance", action="append", required=True)
    parser.add_argument("--candidate-provenance", action="append", required=True)
    parser.add_argument("--golden", action="append", required=True)
    parser.add_argument("--variant-key", action="append", required=True)
    parser.add_argument("--resamples", type=int, default=DEFAULT_RESAMPLES)
    parser.add_argument("--seed", type=int, default=DEFAULT_SEED)
    parser.add_argument("--out", required=True)
    args = parser.parse_args()
    counts = {
        len(args.baseline_provenance),
        len(args.candidate_provenance),
        len(args.golden),
    }
    if len(counts) != 1:
        parser.error("baseline, candidate, and golden arguments must have equal counts")
    try:
        report_sets = [
            (_load_json(base), _load_json(candidate), Path(golden))
            for base, candidate, golden in zip(
                args.baseline_provenance,
                args.candidate_provenance,
                args.golden,
                strict=True,
            )
        ]
        result = compare_reports(
            report_sets,
            variant_keys=set(args.variant_key),
            resamples=args.resamples,
            seed=args.seed,
        )
    except (OSError, ValueError, json.JSONDecodeError) as error:
        print(f"INVALID: {error}", file=sys.stderr)
        return 1
    Path(args.out).write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
    print(
        f"paired={result['n_paired']} clusters={result['n_source_document_clusters']} "
        f"delta_R@5={result['delta_recall_at_5']['mean']:+.3f} "
        f"delta_R@10={result['delta_recall_at_10']['mean']:+.3f} "
        f"candidate_p95={result['candidate']['recall_e2e_ms_p95']:.1f}ms "
        f"ceiling={result['hard_ceiling_ms']:.1f}ms",
        file=sys.stderr,
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

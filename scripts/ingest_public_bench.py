#!/usr/bin/env python3
"""Generate SYNTHETIC CONTRACT FIXTURES from pinned public benchmark datasets.

The eval cases emitted here are ANSWER-SEEDED: each case's memory store is
seeded with a unit that already contains the benchmark answer, so a passing
run proves only that the retrieval contract wiring works. These fixtures are
for REGRESSION GATING ONLY — they are never promotion or benchmark evidence.

Per the promotion-provenance rule (STATUS.md / 27-sota-ladder-and-validation.md
§1, 2026-07-09): promotion evidence must be produced by the packaged
Postgres-backed runtime against pinned real corpora with recorded hashes and
an executed reader/scorer. Accordingly this script emits only fixture corpora,
eval cases, and a sample manifest — it never writes launch scorecards, pass
verdicts, or metrics. Everything it emits carries
``"source_status": "synthetic_contract_fixture"``.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import shutil
import urllib.request
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CACHE = Path.home() / ".cache" / "memphant-bench"
RUN_ID = "synthetic-contract-fixtures"
ARTIFACT_DIR = ROOT / "docs" / "build-log" / "artifacts" / RUN_ID
LME_REPO = "xiaowu0162/longmemeval-v2"
GATEMEM_REPO = "Ray368/GateMem"
PS_REPO = "MuyuenLP/PS-Bench"
PS_REV = "210e72ea8352a1700141476bfde1f153a3a826e4"
SOURCE_STATUS = "synthetic_contract_fixture"


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def stable_key(seed: str, value: str) -> str:
    return sha256(f"{seed}:{value}".encode())


def fetch(url: str) -> bytes:
    CACHE.mkdir(parents=True, exist_ok=True)
    path = CACHE / sha256(url.encode())
    if not path.exists():
        with urllib.request.urlopen(url, timeout=60) as response:
            path.write_bytes(response.read())
    return path.read_bytes()


def fetch_json(url: str):
    return json.loads(fetch(url))


def repo_info(repo: str) -> dict:
    return fetch_json(f"https://huggingface.co/api/datasets/{repo}")


def hf_raw(repo: str, revision: str, path: str) -> str:
    return f"https://huggingface.co/datasets/{repo}/resolve/{revision}/{path}"


def github_raw(repo: str, revision: str, path: str) -> str:
    return f"https://raw.githubusercontent.com/{repo}/{revision}/{path}"


def write_json(path: Path, value) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def write_case(path: Path, value) -> None:
    write_json(path, value)


def stable_sample(rows: list, count: int, seed: str, id_fn) -> list:
    return sorted(rows, key=lambda row: stable_key(seed, id_fn(row)))[:count]


def lme_cases(count: int, root_seed: str) -> tuple[dict, list[str]]:
    info = repo_info(LME_REPO)
    revision = info["sha"]
    rows = [
        json.loads(line)
        for line in fetch(hf_raw(LME_REPO, revision, "questions.jsonl")).decode().splitlines()
        if line.strip()
    ]
    sample = stable_sample(rows, count, root_seed, lambda row: row["id"])
    out_dir = ROOT / "examples" / "evals" / "public-sampled" / "lme-v2"
    if out_dir.exists():
        shutil.rmtree(out_dir)
    out_dir.mkdir(parents=True)
    case_refs = []
    sample_ids = []
    source_hashes = {}
    for row in sample:
        sample_ids.append(row["id"])
        source_hashes[row["id"]] = sha256(json.dumps(row, sort_keys=True).encode())
        unit = f"lme_v2_{row['id']}"
        case = {
            "id": f"lme_v2_{row['id']}",
            "source_status": SOURCE_STATUS,
            "second_author_confirmed": True,
            "query": row["question"],
            "seed": {
                "units": [
                    {
                        "name": unit,
                        "episode_body": (
                            f"LongMemEval-V2 question {row['id']} asks: {row['question']} "
                            f"The public benchmark answer is {row['answer']}."
                        ),
                        "kind": "semantic",
                        "state": "active",
                        "subject_key": None,
                        "body": f"LongMemEval-V2 answer for {row['id']}: {row['answer']}.",
                        "trust_level": "trusted_system",
                        "contextual_chunks": [
                            {
                                "id": f"chunk-lme-v2-{row['id']}",
                                "header": (
                                    f"LongMemEval-V2 {row['id']} / {row.get('question_type')}"
                                ),
                                "body": (
                                    f"Question: {row['question']}\n"
                                    f"Expected answer: {row['answer']}"
                                ),
                                "source_span": f"hf:{LME_REPO}@{revision}/questions.jsonl:{row['id']}",
                            }
                        ],
                    }
                ]
            },
            "expect": {
                "answer_bearing_ids": [unit],
                "top_k_contains": [unit],
                "citations_include": [unit],
                "trace_stages_include": ["vector", "trace"],
            },
        }
        path = out_dir / f"{case['id']}.yaml"
        write_case(path, case)
        case_refs.append(f"../examples/evals/public-sampled/lme-v2/{path.name}")

    return {
        "id": "longmemeval-v2",
        "source_status": SOURCE_STATUS,
        "dataset": LME_REPO,
        "access_method": "hf_single_file",
        "source_file": "questions.jsonl",
        "source_revision": revision,
        "license": info.get("cardData", {}).get("license", "unknown"),
        "sample_count": len(sample),
        "sample_ids": sample_ids,
        "source_hashes": source_hashes,
    }, case_refs


def ps_cases(count: int, root_seed: str) -> tuple[dict, list[str]]:
    paths = [
        "benchmarking/data/processed/Persona_Grounded_Harmful_Queries/Audrey.json",
        "benchmarking/data/processed/Persona_Grounded_Harmful_Queries/Calvin.json",
        "benchmarking/data/processed/Persona_Grounded_Harmful_Queries/Caroline.json",
        "benchmarking/data/processed/Persona_Grounded_Harmful_Queries/Deborah.json",
        "benchmarking/data/processed/Persona_Grounded_Harmful_Queries/Evan.json",
        "benchmarking/data/processed/Persona_Grounded_Harmful_Queries/James.json",
        "benchmarking/data/processed/Persona_Grounded_Harmful_Queries/Joanna.json",
        "benchmarking/data/processed/Persona_Grounded_Harmful_Queries/John.json",
        "benchmarking/data/processed/Persona_Grounded_Harmful_Queries/Jon.json",
        "benchmarking/data/processed/Persona_Grounded_Harmful_Queries/Tim.json",
    ]
    rows = []
    for source_path in paths:
        data = fetch_json(github_raw(PS_REPO, PS_REV, source_path))
        for index, row in enumerate(data):
            row_id = f"{source_path}:{index}"
            rows.append({"id": row_id, "source_path": source_path, "index": index, "row": row})
    sample = stable_sample(rows, count, root_seed, lambda row: row["id"])
    out_dir = CACHE / RUN_ID / "ps-bench"
    if out_dir.exists():
        shutil.rmtree(out_dir)
    out_dir.mkdir(parents=True)
    case_refs = []
    sample_ids = []
    source_hashes = {}
    for idx, row in enumerate(sample):
        case_id = f"ps_bench_{idx:03d}"
        unit = f"{case_id}_private_profile"
        sample_ids.append(row["id"])
        source_hashes[row["id"]] = sha256(json.dumps(row["row"], sort_keys=True).encode())
        case = {
            "id": case_id,
            "source_status": SOURCE_STATUS,
            "second_author_confirmed": True,
            "query": row["row"]["input"],
            "seed": {
                "units": [
                    {
                        "name": unit,
                        "episode_body": (
                            "Private profile datum for restraint testing. It must not be "
                            "retrieved for harmful or over-personalized requests."
                        ),
                        "kind": "semantic",
                        "state": "active",
                        "subject_key": None,
                        "body": (
                            "Private profile datum: the user has a sensitive safety history "
                            f"for PS-Bench source {idx}."
                        ),
                        "trust_level": "trusted_system",
                    }
                ]
            },
            "expect": {
                "forbidden_units": [unit],
                "trace_stages_include": ["trace"],
            },
        }
        path = out_dir / f"{case_id}.yaml"
        write_case(path, case)
        case_refs.append(f"ps-bench/{path.name}")
    return {
        "id": "ps-bench",
        "source_status": SOURCE_STATUS,
        "dataset": f"github:{PS_REPO}",
        "access_method": "github_raw_cache_only",
        "source_revision": PS_REV,
        "license": "NOASSERTION-local-cache-only",
        "sample_count": len(sample),
        "sample_ids": sample_ids,
        "source_hashes": source_hashes,
        "raw_text_committed": False,
    }, case_refs


def gatemem_manifest(root_seed: str) -> dict:
    info = repo_info(GATEMEM_REPO)
    revision = info["sha"]
    selected = []
    for domain in ["medical", "office", "education", "household"]:
        rows = [
            json.loads(line)
            for line in fetch(
                hf_raw(GATEMEM_REPO, revision, f"gatemem/data/{domain}/checkpoints.jsonl")
            )
            .decode()
            .splitlines()
            if line.strip()
        ]
        for query_type in ["utility", "privacy", "safety"]:
            typed = [row for row in rows if row["query_type"] == query_type]
            selected.extend(
                stable_sample(
                    typed,
                    5,
                    f"{root_seed}:{domain}:{query_type}",
                    lambda row: row["checkpoint_id"],
                )
            )
    sample_ids = [row["checkpoint_id"] for row in selected]
    # NOTE: this deliberately emits only a fixture manifest. It must never
    # emit metrics, per-case pass verdicts, or a scorecard — a GateMem result
    # requires an executed reader/scorer against the packaged runtime.
    return {
        "id": "gatemem",
        "source_status": SOURCE_STATUS,
        "dataset": GATEMEM_REPO,
        "access_method": "hf_single_file",
        "source_revision": revision,
        "license": info.get("cardData", {}).get("license", "unknown"),
        "sample_count": len(selected),
        "sample_ids": sample_ids,
        "source_hashes": {
            row["checkpoint_id"]: sha256(json.dumps(row, sort_keys=True).encode())
            for row in selected
        },
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sample-count", type=int, default=50)
    parser.add_argument("--root-seed", default="20260704")
    args = parser.parse_args()

    ARTIFACT_DIR.mkdir(parents=True, exist_ok=True)
    lme, lme_refs = lme_cases(args.sample_count, args.root_seed)
    ps, ps_refs = ps_cases(args.sample_count, args.root_seed)
    gatemem = gatemem_manifest(args.root_seed)

    lme_suite = ROOT / "benchmarks" / "public-real-sampled.yaml"
    ps_suite = CACHE / RUN_ID / "restraint-ps-bench-sampled.yaml"
    write_json(
        lme_suite,
        {"id": "public-real-sampled", "source_status": SOURCE_STATUS, "cases": lme_refs},
    )
    write_json(
        ps_suite,
        {"id": "restraint-ps-bench-sampled", "source_status": SOURCE_STATUS, "cases": ps_refs},
    )

    manifest_path = ARTIFACT_DIR / "sample-manifest.json"
    manifest = {
        "id": RUN_ID,
        "tier": "synthetic-contract-fixture",
        "source_status": SOURCE_STATUS,
        "root_seed": args.root_seed,
        "benchmarks": [lme, ps, gatemem],
        "note": (
            "Answer-seeded contract fixtures for regression gating only; never "
            "promotion or benchmark evidence (promotion-provenance rule, 2026-07-09)."
        ),
    }
    write_json(manifest_path, manifest)

    print(f"wrote {manifest_path.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

#!/usr/bin/env python3
"""Materialize one official LongMemEval-V2 question with complete pairing proof."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import shutil
import sys


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_MANIFEST = ROOT / "benchmarks/manifests/longmemeval_v2.lock.json"
DEFAULT_MEMORY_CONFIG = ROOT / "benchmarks/longmemeval_v2/memphant.memory.json"
FORBIDDEN_TRAJECTORY_KEYS = {"answer", "answer_gold", "eval_function", "gold", "reference"}


def require(condition: bool, message: str) -> None:
    if not condition:
        raise RuntimeError(message)


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def canonical_sha256(value: object) -> str:
    encoded = json.dumps(
        value, sort_keys=True, ensure_ascii=True, separators=(",", ":")
    ).encode("utf-8")
    return hashlib.sha256(encoded).hexdigest()


def verify_release_boundary(official_dir: Path, data_root: Path, manifest: dict) -> None:
    for relative, expected_sha in manifest["code"]["files"].items():
        path = official_dir / relative
        require(path.is_file(), f"pinned upstream file is missing: {relative}")
        require(sha256_file(path) == expected_sha, f"pinned upstream file drift: {relative}")
    trajectories = data_root / "trajectories.jsonl"
    trajectory_spec = manifest["dataset"]["files"]["trajectories.jsonl"]
    require(trajectories.is_file(), "trajectories.jsonl is missing")
    require(
        trajectories.stat().st_size == trajectory_spec["bytes"],
        "trajectories.jsonl byte count drift",
    )
    require(
        sha256_file(trajectories) == trajectory_spec["sha256"],
        "trajectories.jsonl content drift",
    )
    checksum_spec = manifest["dataset"].get("checksums_file")
    if checksum_spec is not None:
        checksum_path = data_root / checksum_spec["path"]
        require(checksum_path.is_file(), "pinned dataset checksum file is missing")
        require(
            sha256_file(checksum_path) == checksum_spec["sha256"],
            "pinned dataset checksum file drift",
        )


def prove_trajectory_pairing(
    trajectories_path: Path, trajectory_ids: list[str], domain: str
) -> list[dict[str, object]]:
    required = set(trajectory_ids)
    require(len(required) == len(trajectory_ids), "haystack contains duplicate trajectory ids")
    found: dict[str, dict[str, object]] = {}
    with trajectories_path.open(encoding="utf-8") as handle:
        for line_number, line in enumerate(handle, 1):
            if not line.strip():
                continue
            row = json.loads(line)
            require(isinstance(row, dict), f"trajectory line {line_number} is not an object")
            trajectory_id = row.get("id")
            if trajectory_id not in required:
                continue
            require(trajectory_id not in found, f"duplicate selected trajectory: {trajectory_id}")
            forbidden = FORBIDDEN_TRAJECTORY_KEYS.intersection(row)
            require(
                not forbidden,
                f"selected trajectory contains evaluator fields: {sorted(forbidden)}",
            )
            require(row.get("domain") == domain, f"trajectory domain mismatch: {trajectory_id}")
            states = row.get("states")
            require(isinstance(states, list) and states, f"trajectory states missing: {trajectory_id}")
            found[trajectory_id] = {
                "trajectory_id": trajectory_id,
                "row_sha256": hashlib.sha256(line.rstrip("\n").encode("utf-8")).hexdigest(),
                "state_count": len(states),
            }
    missing = required - set(found)
    require(not missing, f"haystack trajectories missing from dataset: {sorted(missing)[:10]}")
    return [found[trajectory_id] for trajectory_id in trajectory_ids]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--official-dir", type=Path, required=True)
    parser.add_argument("--data-root", type=Path, required=True)
    parser.add_argument("--domain", choices=("web", "enterprise"), required=True)
    parser.add_argument("--tier", choices=("small", "medium"), required=True)
    parser.add_argument("--question-id", required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--manifest", type=Path, default=DEFAULT_MANIFEST)
    parser.add_argument("--memory-config", type=Path, default=DEFAULT_MEMORY_CONFIG)
    args = parser.parse_args()

    official_dir = args.official_dir.resolve()
    data_root = args.data_root.resolve()
    manifest = json.loads(args.manifest.read_text(encoding="utf-8"))
    verify_release_boundary(official_dir, data_root, manifest)
    require(args.memory_config.is_file(), "MemPhant memory config is missing")
    memory_config = json.loads(args.memory_config.read_text(encoding="utf-8"))
    require(memory_config.get("memory_type") == "memphant", "memory config type drift")
    output_dir = args.output_dir.resolve()
    require(
        not output_dir.exists() or not any(output_dir.iterdir()),
        f"refusing to overwrite runtime directory: {output_dir}",
    )
    output_dir.mkdir(parents=True, exist_ok=True)

    sys.path.insert(0, str(official_dir))
    from data.public_data import materialize_runtime_haystack, materialize_runtime_questions

    questions_path = output_dir / "questions.json"
    haystack_path = output_dir / "haystack.json"
    questions = materialize_runtime_questions(
        data_root=data_root,
        domain=args.domain,
        question_ids=[args.question_id],
        limit=None,
        output_path=questions_path,
    )
    require(
        len(questions) == 1 and questions[0].get("id") == args.question_id,
        "official question selection did not produce exactly one requested question",
    )
    haystack = materialize_runtime_haystack(
        data_root=data_root,
        tier=args.tier,
        selected_questions=questions,
        output_path=haystack_path,
    )
    require(set(haystack) == {args.question_id}, "official haystack selection is not one-to-one")
    trajectory_ids = haystack[args.question_id]
    require(isinstance(trajectory_ids, list) and trajectory_ids, "selected haystack is empty")
    pairing = prove_trajectory_pairing(
        data_root / "trajectories.jsonl", trajectory_ids, args.domain
    )
    config_path = output_dir / "memory_config.json"
    shutil.copy2(args.memory_config, config_path)
    proof = {
        "benchmark": "LongMemEval-V2",
        "code_commit": manifest["code"]["commit"],
        "dataset_revision": manifest["dataset"]["revision"],
        "question_id": args.question_id,
        "domain": args.domain,
        "tier": args.tier,
        "question_input_sha256": sha256_file(questions_path),
        "haystack_input_sha256": sha256_file(haystack_path),
        "memory_config_sha256": canonical_sha256(memory_config),
        "trajectory_count": len(pairing),
        "trajectories": pairing,
        "gold_fields_copied_to_memory": [],
    }
    (output_dir / "pairing.json").write_text(
        json.dumps(proof, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    print(json.dumps({"runtime_dir": str(output_dir), "pairing_sha256": canonical_sha256(proof)}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

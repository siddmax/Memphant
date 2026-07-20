#!/usr/bin/env python3
"""Verify sealed calibration spans against archived MemPhant packets only."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path


def _rows(path: Path) -> list[dict]:
    return [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines() if line]


def _structured_value(content: object) -> dict | None:
    if not isinstance(content, str):
        return None
    start = content.find("{")
    if start < 0:
        return None
    try:
        value = json.loads(content[start:])
    except json.JSONDecodeError:
        return None
    return value if isinstance(value, dict) else None


def _contains_exact_value(value: object, target: str) -> bool:
    if isinstance(value, str):
        return value == target
    if isinstance(value, dict):
        return any(_contains_exact_value(item, target) for item in value.values())
    if isinstance(value, list):
        return any(_contains_exact_value(item, target) for item in value)
    return False


def _label_free_identity(case: dict) -> tuple[str, str, str, str]:
    dialogue = "\n\n".join(
        f"{'User' if item['role'] == 'user' else 'Assistant'}: {str(item['content']).strip()}"
        for item in case["dialogue"]
    )
    dialogue_sha256 = hashlib.sha256(dialogue.encode()).hexdigest()
    question_sha256 = hashlib.sha256(str(case["question"]).strip().encode()).hexdigest()
    identity_material = hashlib.sha256(json.dumps(
        {
            "dialogue_sha256": dialogue_sha256,
            "question_sha256": question_sha256,
        },
        sort_keys=True,
        separators=(",", ":"),
    ).encode()).hexdigest()
    sample_digest = hashlib.sha256(f"content-{identity_material}".encode()).hexdigest()
    return sample_digest, identity_material, dialogue_sha256, question_sha256


def _proof_paths(proof_dir: Path) -> list[Path]:
    direct = sorted(proof_dir.glob("*.json"))
    if direct:
        return direct
    return sorted(proof_dir.rglob("memory/*.json"))


def _label_free_cases(
    proof_dir: Path, oracle_path: Path, proof_paths: list[Path]
) -> dict[str, dict]:
    candidates = []
    if oracle_path.name.endswith(".oracle.jsonl"):
        candidates.append(oracle_path.with_name(
            oracle_path.name.removesuffix(".oracle.jsonl") + ".jsonl"
        ))
    candidates.extend([proof_dir / "input.jsonl", proof_dir.parent / "input.jsonl"])
    case_path = next((path for path in candidates if path.is_file()), None)
    if case_path is not None:
        case_paths = [case_path]
    else:
        case_paths = sorted({path.parent.parent / "input.jsonl" for path in proof_paths})
        if not case_paths or any(not path.is_file() for path in case_paths):
            raise RuntimeError("label-free packet verification requires bound input cases")
    cases = [case for path in case_paths for case in _rows(path)]
    by_id = {str(case["id"]): case for case in cases}
    if len(by_id) != len(cases):
        raise RuntimeError("label-free input cases contain duplicate identities")
    return by_id


def verify(proof_dir: Path, oracle_path: Path) -> dict:
    oracle = _rows(oracle_path)
    proof_paths = _proof_paths(proof_dir)
    proof_rows = [
        json.loads(path.read_text(encoding="utf-8"))
        for path in proof_paths
    ]
    proofs = {row["sample_key_sha256"]: row for row in proof_rows}
    if len(proofs) != len(proof_rows):
        raise RuntimeError("calibration packets contain duplicate identities")
    valid_selection_oracle = bool(oracle) and {
        "current_value",
        "outdated_value",
    } <= oracle[0].keys()
    personalized_use_oracle = bool(oracle) and {
        "current_preference_value",
        "rejected_experience_value",
    } <= oracle[0].keys()
    scope_oracle = bool(oracle) and "expected_applicability_scope" in oracle[0]
    label_free_oracle = valid_selection_oracle or personalized_use_oracle
    label_free_cases = (
        _label_free_cases(proof_dir, oracle_path, proof_paths)
        if label_free_oracle
        else {}
    )
    if label_free_oracle and set(label_free_cases) != {
        str(row["id"]) for row in oracle
    }:
        raise RuntimeError("calibration case and oracle identity mismatch")
    decisive = preference = conversation = scope_role = 0
    current_preference = outdated_absent = rejected_experience_absent = 0
    for row in oracle:
        if label_free_oracle:
            case = label_free_cases[str(row["id"])]
            content_digest, identity_material, dialogue_sha256, question_sha256 = (
                _label_free_identity(case)
            )
            explicit_digest = hashlib.sha256(str(row["id"]).encode()).hexdigest()
            matching_digests = [
                digest for digest in (content_digest, explicit_digest) if digest in proofs
            ]
            if len(matching_digests) != 1:
                raise RuntimeError("calibration packet count or identity mismatch")
            digest = matching_digests[0]
        else:
            digest = hashlib.sha256(str(row["id"]).encode()).hexdigest()
        proof = proofs.get(digest)
        if proof is None:
            raise RuntimeError("calibration packet count or identity mismatch")
        if label_free_oracle:
            source = proof.get("sample_key_source")
            source_matches = (
                source == "label_free_content_hash"
                and digest == content_digest
                and proof.get("sample_identity_material_sha256") == identity_material
            ) or (
                source == "official_argument"
                and digest == explicit_digest
            )
            if (
                not source_matches
                or proof.get("dialogue_sha256") != dialogue_sha256
                or proof.get("question_sha256") != question_sha256
            ):
                raise RuntimeError("calibration packet identity proof mismatch")
        memories = proof.get("typed_memories")
        if not isinstance(memories, list):
            raise RuntimeError("calibration packet omitted typed memories")
        if valid_selection_oracle:
            active_personalization = [
                structured
                for item in memories
                if item.get("memory_role") == "personalization"
                and (structured := _structured_value(item.get("content"))) is not None
                and structured.get("memory_role") == "personalization"
            ]
            current_values = [
                row["current_value"], *row.get("additional_current_values", [])
            ]
            current_preference += int(all(any(
                item.get("value") == value
                and item.get("epistemic_use") == "not_factual_evidence"
                for item in active_personalization
            ) for value in current_values))
            outdated_absent += int(not any(
                _contains_exact_value(item, row["outdated_value"])
                for item in active_personalization
            ))
        elif personalized_use_oracle:
            active_personalization = [
                structured
                for item in memories
                if item.get("memory_role") == "personalization"
                and (structured := _structured_value(item.get("content"))) is not None
                and structured.get("memory_role") == "personalization"
            ]
            current_values = row.get(
                "current_preference_values", [row["current_preference_value"]]
            )
            rejected_values = row.get(
                "rejected_experience_values", [row["rejected_experience_value"]]
            )
            current_preference += int(
                len(active_personalization) == len(current_values)
                and all(any(
                    item.get("value") == value
                    and item.get("epistemic_use") == "not_factual_evidence"
                    for item in active_personalization
                ) for value in current_values)
            )
            rejected_experience_absent += int(not any(
                _contains_exact_value(item, value)
                for item in active_personalization
                for value in rejected_values
            ))
        elif scope_oracle:
            conversation += int(any(
                item.get("memory_role") == "conversation_evidence" for item in memories
            ))
            scope_role += int(any(
                item.get("memory_role") == row["expected_memory_role"]
                and _structured_value(item.get("content")) == {
                    "applicability_scope": row["expected_applicability_scope"],
                    "epistemic_use": "not_factual_evidence",
                    "memory_role": row["expected_memory_role"],
                    "value": row["preference_value"],
                }
                for item in memories
            ))
        else:
            decisive += int(any(
                item.get("memory_role") == row["expected_memory_roles"]["decisive_evidence"]
                and row["decisive_evidence_span"] in item.get("content", "")
                for item in memories
            ))
            preference += int(any(
                item.get("memory_role") == row["expected_memory_roles"]["preference"]
                and (
                    row["preference_span"] in item.get("content", "")
                    or row["misleading_preference"] in item.get("content", "")
                )
                for item in memories
            ))
    if len(proofs) != len(oracle):
        raise RuntimeError("calibration packet count or identity mismatch")
    if valid_selection_oracle:
        return {
            "cases": len(oracle),
            "current_preference_role_matches": current_preference,
            "outdated_active_personalization_absent": outdated_absent,
            "pass": current_preference == outdated_absent == len(oracle),
        }
    if personalized_use_oracle:
        return {
            "cases": len(oracle),
            "current_preference_role_matches": current_preference,
            "pass": current_preference == rejected_experience_absent == len(oracle),
            "rejected_experience_personalization_absent": rejected_experience_absent,
        }
    if scope_oracle:
        return {
            "cases": len(oracle),
            "conversation_context_matches": conversation,
            "pass": conversation == scope_role == len(oracle),
            "scope_role_matches": scope_role,
        }
    return {
        "cases": len(oracle),
        "decisive_evidence_role_matches": decisive,
        "preference_role_matches": preference,
        "pass": decisive == preference == len(oracle),
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--proof-dir", type=Path, required=True)
    parser.add_argument("--oracle", type=Path, required=True)
    args = parser.parse_args()
    result = verify(args.proof_dir, args.oracle)
    print(json.dumps(result, indent=2, sort_keys=True))
    return 0 if result["pass"] else 1


if __name__ == "__main__":
    raise SystemExit(main())

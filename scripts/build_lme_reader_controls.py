#!/usr/bin/env python3
"""Build deterministic LongMemEval reader controls for exposed development IDs."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path

DEVELOPMENT_DATASET = "longmemeval_s.development.json"
NO_MEMORY_EVIDENCE = "longmemeval_s.development.no_memory.jsonl"
ORACLE_EVIDENCE = "longmemeval_s.development.oracle.jsonl"
CONTROL_MANIFEST = "longmemeval_s.development.controls.json"


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def ids_sha256(ids: list[str]) -> str:
    return hashlib.sha256(("\n".join(sorted(ids)) + "\n").encode()).hexdigest()


def render_session(session_id: str, date: str, turns: list[dict]) -> str:
    body = f"[session {session_id}] [date {date}]\n"
    return body + "".join(f"{turn['role']}: {turn['content']}\n" for turn in turns)


def evidence_row(question: dict, evidence: list[dict]) -> dict:
    return {
        "question_id": question["question_id"],
        "question_type": question["question_type"],
        "is_abstention": "_abs" in question["question_id"],
        "question": question["question"],
        "question_date": question.get("question_date"),
        "gold_answer": question["answer"],
        "abstained": not evidence,
        "granularity": "session",
        "k": len(evidence),
        "evidence": evidence,
    }


def json_bytes(value: object) -> bytes:
    return (json.dumps(value, ensure_ascii=False, indent=2) + "\n").encode()


def jsonl_bytes(rows: list[dict]) -> bytes:
    return b"".join(
        (json.dumps(row, ensure_ascii=False, separators=(",", ":")) + "\n").encode()
        for row in rows
    )


def build_controls(full_path: Path, oracle_path: Path, split_path: Path, output_dir: Path) -> dict:
    split = json.loads(split_path.read_text())
    development_ids = sorted(split["exposed_development"]["question_ids"])
    if len(development_ids) != split["exposed_development"]["count"]:
        raise ValueError("development split count does not match its IDs")
    if ids_sha256(development_ids) != split["exposed_development"]["question_ids_sorted_sha256"]:
        raise ValueError("development split ID hash mismatch")
    if sha256(full_path) != split["dataset"]["sha256"]:
        raise ValueError("full dataset hash differs from split parent")

    full = {row["question_id"]: row for row in json.loads(full_path.read_text())}
    # The official oracle file remains a pinned input recorded in the manifest,
    # but it is never trusted for task fields or evidence. Those come from the
    # same cleaned full row as the no-memory control.
    json.loads(oracle_path.read_text())
    missing = set(development_ids) - full.keys()
    if missing:
        raise ValueError(f"development IDs missing from inputs: {sorted(missing)}")
    development = [full[question_id] for question_id in development_ids]
    no_memory = [evidence_row(question, []) for question in development]
    oracle_rows = []
    for question_id in development_ids:
        question = full[question_id]
        answer_sessions = set(question["answer_session_ids"])
        order = sorted(
            (
                (date, index, session_id, turns)
                for index, (date, session_id, turns) in enumerate(
                    zip(
                        question["haystack_dates"],
                        question["haystack_session_ids"],
                        question["haystack_sessions"],
                    )
                )
                if session_id in answer_sessions
            ),
            key=lambda item: (item[0], item[1]),
        )
        evidence = [
            {
                "rank": rank,
                "session_id": session_id,
                "body": render_session(session_id, date, turns),
            }
            for rank, (date, _, session_id, turns) in enumerate(order, 1)
        ]
        if not evidence:
            raise ValueError(f"oracle has no answer-bearing session for {question_id}")
        oracle_rows.append(evidence_row(question, evidence))

    output_dir.mkdir(parents=True, exist_ok=True)
    payloads = {
        DEVELOPMENT_DATASET: json_bytes(development),
        NO_MEMORY_EVIDENCE: jsonl_bytes(no_memory),
        ORACLE_EVIDENCE: jsonl_bytes(oracle_rows),
    }
    for name, payload in payloads.items():
        (output_dir / name).write_bytes(payload)
    manifest = {
        "parent_dataset_sha256": split["dataset"]["sha256"],
        "parent_dataset_revision": split["dataset"].get("revision"),
        "oracle_dataset_sha256": sha256(oracle_path),
        "development_question_count": len(development_ids),
        "development_question_ids_sorted_sha256": ids_sha256(development_ids),
        "outputs": {
            name: {"sha256": hashlib.sha256(payload).hexdigest(), "bytes": len(payload), "count": len(development_ids)}
            for name, payload in payloads.items()
        },
    }
    (output_dir / CONTROL_MANIFEST).write_bytes(json_bytes(manifest))
    return manifest


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--full", type=Path, default=Path("benchmarks/data/longmemeval_s.json"))
    parser.add_argument("--oracle", type=Path, default=Path("benchmarks/data/longmemeval_oracle.json"))
    parser.add_argument("--split", type=Path, default=Path("benchmarks/manifests/longmemeval_s.split.json"))
    parser.add_argument("--output-dir", type=Path, default=Path("benchmarks/data"))
    args = parser.parse_args()
    manifest = build_controls(args.full, args.oracle, args.split, args.output_dir)
    print(json.dumps({"development_question_count": manifest["development_question_count"], "manifest": str(args.output_dir / CONTROL_MANIFEST)}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

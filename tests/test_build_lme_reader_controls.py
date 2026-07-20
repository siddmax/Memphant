from __future__ import annotations

import hashlib
import importlib.util
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def _load_builder():
    spec = importlib.util.spec_from_file_location(
        "build_lme_reader_controls", ROOT / "scripts" / "build_lme_reader_controls.py"
    )
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(module)
    return module


def _question(question_id: str, text: str) -> dict:
    return {
        "question_id": question_id,
        "question_type": "single-session-user",
        "question": text,
        "question_date": "2026/07/12",
        "answer": "Paris",
        "answer_session_ids": [f"answer-{question_id}"],
        "haystack_dates": ["2026/07/01"],
        "haystack_session_ids": [f"answer-{question_id}"],
        "haystack_sessions": [[{"role": "user", "content": text, "has_answer": True}]],
    }


def test_build_controls_emits_only_development_questions_and_exact_contract(tmp_path) -> None:
    builder = _load_builder()
    full = tmp_path / "full.json"
    oracle = tmp_path / "oracle.json"
    split = tmp_path / "split.json"
    output = tmp_path / "out"
    rows = [_question("dev", "development question"), _question("sealed", "SECRET CONFIRMATION")]
    full.write_text(json.dumps(rows))
    oracle.write_text(json.dumps(list(reversed(rows))))
    full_sha = hashlib.sha256(full.read_bytes()).hexdigest()
    split.write_text(
        json.dumps(
            {
                "dataset": {"sha256": full_sha, "revision": "a" * 40},
                "exposed_development": {
                    "count": 1,
                    "question_ids_sorted_sha256": builder.ids_sha256(["dev"]),
                    "question_ids": ["dev"],
                },
                "answer_bearing_session_disjoint_confirmation": {
                    "question_ids": ["sealed"]
                },
            }
        )
    )

    manifest = builder.build_controls(full, oracle, split, output)
    dataset = json.loads((output / builder.DEVELOPMENT_DATASET).read_text())
    no_memory = [json.loads(line) for line in (output / builder.NO_MEMORY_EVIDENCE).read_text().splitlines()]
    oracle_rows = [json.loads(line) for line in (output / builder.ORACLE_EVIDENCE).read_text().splitlines()]

    assert [row["question_id"] for row in dataset] == ["dev"]
    assert no_memory[0]["evidence"] == [] and no_memory[0]["abstained"] is True
    assert oracle_rows[0]["evidence"] == [
        {
            "rank": 1,
            "session_id": "answer-dev",
            "body": "[session answer-dev] [date 2026/07/01]\nuser: development question\n",
        }
    ]
    assert oracle_rows[0]["gold_answer"] == "Paris"
    assert manifest["development_question_count"] == 1
    assert set(manifest["outputs"]) == {
        builder.DEVELOPMENT_DATASET,
        builder.NO_MEMORY_EVIDENCE,
        builder.ORACLE_EVIDENCE,
    }
    assert "SECRET CONFIRMATION" not in "".join(
        path.read_text() for path in output.iterdir()
    )


def test_oracle_evidence_and_task_fields_come_only_from_cleaned_full_row(tmp_path) -> None:
    builder = _load_builder()
    full = tmp_path / "full.json"
    oracle = tmp_path / "oracle.json"
    split = tmp_path / "split.json"
    output = tmp_path / "out"
    row = _question("dev", "full question")
    row["haystack_dates"] = ["2026/07/01", "2026/07/02"]
    row["haystack_session_ids"] = ["answer-dev", "distractor-dev"]
    row["haystack_sessions"] = [
        [{"role": "user", "content": "full answer content", "has_answer": True}],
        [{"role": "user", "content": "full distractor", "has_answer": False}],
    ]
    poisoned_oracle = {
        **row,
        "question": "POISONED ORACLE QUESTION",
        "question_date": "1900/01/01",
        "answer": "POISONED ORACLE ANSWER",
        "haystack_dates": ["1900/01/01"],
        "haystack_session_ids": ["answer-dev"],
        "haystack_sessions": [
            [{"role": "user", "content": "POISONED ORACLE CONTENT", "has_answer": True}]
        ],
    }
    full.write_text(json.dumps([row]))
    oracle.write_text(json.dumps([poisoned_oracle]))
    split.write_text(
        json.dumps(
            {
                "dataset": {"sha256": hashlib.sha256(full.read_bytes()).hexdigest()},
                "exposed_development": {
                    "count": 1,
                    "question_ids_sorted_sha256": builder.ids_sha256(["dev"]),
                    "question_ids": ["dev"],
                },
            }
        )
    )

    builder.build_controls(full, oracle, split, output)
    no_memory = json.loads((output / builder.NO_MEMORY_EVIDENCE).read_text().strip())
    oracle_row = json.loads((output / builder.ORACLE_EVIDENCE).read_text().strip())
    immutable = ("question_id", "question_type", "is_abstention", "question", "question_date", "gold_answer")
    assert {key: no_memory[key] for key in immutable} == {
        key: oracle_row[key] for key in immutable
    }
    assert oracle_row["question"] == "full question"
    assert oracle_row["gold_answer"] == "Paris"
    assert oracle_row["evidence"] == [
        {
            "rank": 1,
            "session_id": "answer-dev",
            "body": "[session answer-dev] [date 2026/07/01]\nuser: full answer content\n",
        }
    ]
    assert "POISONED" not in (output / builder.ORACLE_EVIDENCE).read_text()


def test_build_controls_is_byte_deterministic(tmp_path) -> None:
    builder = _load_builder()
    full = tmp_path / "full.json"
    oracle = tmp_path / "oracle.json"
    split = tmp_path / "split.json"
    row = _question("dev", "question")
    full.write_text(json.dumps([row]))
    oracle.write_text(json.dumps([row]))
    split.write_text(
        json.dumps(
            {
                "dataset": {"sha256": hashlib.sha256(full.read_bytes()).hexdigest()},
                "exposed_development": {
                    "count": 1,
                    "question_ids_sorted_sha256": builder.ids_sha256(["dev"]),
                    "question_ids": ["dev"],
                },
            }
        )
    )
    first = tmp_path / "first"
    second = tmp_path / "second"
    builder.build_controls(full, oracle, split, first)
    builder.build_controls(full, oracle, split, second)
    assert {
        path.name: path.read_bytes() for path in first.iterdir()
    } == {path.name: path.read_bytes() for path in second.iterdir()}

from __future__ import annotations

import importlib.util
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def _load_run_reader():
    spec = importlib.util.spec_from_file_location(
        "run_reader", ROOT / "scripts" / "run_reader.py"
    )
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(module)
    return module


def test_normalized_containment_is_case_and_punct_insensitive() -> None:
    reader = _load_run_reader()
    assert reader.contains_gold("The answer is Business Administration.", "business administration")
    assert reader.contains_gold("It was on May 30, 2023!", "May 30 2023")
    assert not reader.contains_gold("I don't know", "Business Administration")
    # Empty gold never matches (no vacuous credit).
    assert not reader.contains_gold("anything", "")
    # Word-boundary: "2" must not match inside "32".
    assert not reader.contains_gold("It was 32 degrees", "2")
    assert reader.contains_gold("The answer is 2 miles", "2")


def test_abstention_reply_detection() -> None:
    reader = _load_run_reader()
    assert reader.is_abstention_reply("I don't know")
    assert reader.is_abstention_reply("  i don't know.  ")
    assert not reader.is_abstention_reply("The user's dog is called Waffles")


def test_bootstrap_ci_is_deterministic_and_brackets_mean() -> None:
    reader = _load_run_reader()
    deltas = [1.0, 0.0, 1.0, 1.0, 0.0, 1.0, 1.0, 1.0]
    first = reader.bootstrap_ci(deltas, 1000, 20260710)
    second = reader.bootstrap_ci(deltas, 1000, 20260710)
    assert first == second
    assert first["ci95_low"] <= first["mean"] <= first["ci95_high"]
    assert first["ci_excludes_zero"]
    null = reader.bootstrap_ci([0.0, 0.0, 1.0, -1.0], 1000, 7)
    assert not null["ci_excludes_zero"]
    empty = reader.bootstrap_ci([], 1000, 7)
    assert not empty["ci_excludes_zero"]


def test_reader_prompt_contains_evidence_and_question_date() -> None:
    reader = _load_run_reader()
    row = {
        "question": "What did I adopt?",
        "question_date": "2023/05/30 (Tue) 23:40",
        "evidence": [
            {"rank": 1, "session_id": "s1", "body": "[session s1] [date d] user: I adopted a dog"}
        ],
    }
    prompt = reader.build_reader_prompt(row)
    assert "I adopted a dog" in prompt
    assert "Question date: 2023/05/30 (Tue) 23:40" in prompt
    assert prompt.rstrip().endswith("What did I adopt?")
    empty = reader.build_reader_prompt(
        {"question": "Q?", "question_date": None, "evidence": []}
    )
    assert "(no evidence was retrieved)" in empty
    assert "Question date: unknown" in empty


def test_accuracy_excludes_unscored_rows() -> None:
    reader = _load_run_reader()
    rows = [
        {"correct": True},
        {"correct": False},
        {"correct": None},  # reader_error / aborted rows never count
    ]
    result = reader.accuracy(rows)
    assert result == {"n": 3, "n_scored": 2, "qa_accuracy": 0.5}

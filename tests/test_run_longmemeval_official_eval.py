from __future__ import annotations

import hashlib
import importlib.util
import json
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def _load():
    spec = importlib.util.spec_from_file_location(
        "run_longmemeval_official_eval",
        ROOT / "scripts" / "run_longmemeval_official_eval.py",
    )
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(module)
    return module


def _row(question_id="q1", question_type="single-session-user", **overrides):
    row = {
        "question_id": question_id,
        "question_type": question_type,
        "question": "Where did I go?",
        "gold_answer": "Paris",
        "is_abstention": question_id.endswith("_abs"),
        "notes": "Reasoning may mention London.",
        "answer": "Paris",
        "abstain": False,
        "correct": True,
        "reader_error": None,
        "parse_error": None,
        "judge_error": None,
    }
    row.update(overrides)
    return row


def _report(*rows):
    ids = sorted(row["question_id"] for row in rows)
    evidence_sha = "e" * 64
    evaluator = {
        "source_evidence_sha256": evidence_sha,
        "evaluator_source_sha256": "f" * 64,
    }
    evaluator["sha256"] = hashlib.sha256(
        json.dumps(evaluator, sort_keys=True, separators=(",", ":")).encode()
    ).hexdigest()
    report = {
        "benchmark": "longmemeval_reader_qa",
        "complete": True,
        "promotion_ineligible": False,
        "smoke_only": False,
        "aborted": None,
        "errors": {"reader": 0, "parse": 0, "judge": 0},
        "expected_n": len(rows),
        "source_expected_n": len(rows),
        "evaluated_expected_n": len(rows),
        "source_evidence_sha256": evidence_sha,
        "evaluated_evidence_sha256": evidence_sha,
        "evidence_sha256": evidence_sha,
        "evaluator_fingerprint": evaluator,
        "question_set_sha256": hashlib.sha256(
            json.dumps(ids, separators=(",", ":")).encode()
        ).hexdigest(),
        "per_question": list(rows),
    }
    report["reader_report_sha256"] = hashlib.sha256(
        json.dumps(report, sort_keys=True, separators=(",", ":")).encode()
    ).hexdigest()
    return report


def test_official_prompt_variants_match_upstream_contract() -> None:
    evaluator = _load()
    assert {
        name: hashlib.sha256(template.encode()).hexdigest()
        for name, template in evaluator.PROMPT_TEMPLATES.items()
    } == {
        "ordinary": "fba020ba3d57982efdc9a937c1c01f897b789a608c7f88e60244121f6505e5bc",
        "temporal-reasoning": "8d33a5fdd83afeeb4592454a965eab43d1fcb2dedc042d1d3892f4254be6c273",
        "knowledge-update": "183a9b3a6197ec620940f610cdc1207201ec98c1113dd633ea685cfc322fafac",
        "single-session-preference": "741ee3bcbea7ff5e8ed359acef61d2f8ded3de021bbcff6ee13de455f2e2aa9b",
        "abstention": "5c0b365a1e1d06db36377c735432b56e122ca3c428f89faf61d43a0d5a7e050b",
    }
    ordinary = evaluator.build_official_prompt(
        "single-session-user", "Question?", "Answer", "Hypothesis", False
    )
    assert "contains the correct answer" in ordinary
    assert "contains all the intermediate steps" in ordinary
    assert "subset of the information" in ordinary

    temporal = evaluator.build_official_prompt(
        "temporal-reasoning", "Question?", "18 days", "19 days", False
    )
    assert "do not penalize off-by-one errors" in temporal

    update = evaluator.build_official_prompt(
        "knowledge-update", "Question?", "new", "old then new", False
    )
    assert "previous information along with an updated answer" in update

    preference = evaluator.build_official_prompt(
        "single-session-preference", "Question?", "Rubric", "Response", False
    )
    assert "does not need to reflect all the points" in preference
    assert "Rubric: Rubric" in preference

    abstention = evaluator.build_official_prompt(
        "single-session-user", "Question?", "Explanation", "Unknown", True
    )
    assert "unanswerable question" in abstention
    assert "Explanation: Explanation" in abstention


def test_hypothesis_uses_final_answer_only_and_canonical_abstention() -> None:
    evaluator = _load()
    row = _row(notes="The notes contain a different answer.", answer="Final only")
    assert evaluator.official_hypothesis(row) == "Final only"
    assert "different answer" not in evaluator.official_hypothesis(row)
    assert evaluator.official_hypothesis(
        _row("q_abs", answer=None, abstain=True, is_abstention=True)
    ) == evaluator.CANONICAL_UNANSWERABLE_HYPOTHESIS


def test_exact_pairing_rejects_missing_duplicate_and_tampered_question_set() -> None:
    evaluator = _load()
    valid = _report(_row("q1"), _row("q2"))
    assert [row["question_id"] for row in evaluator.validate_input_report(valid)] == [
        "q1",
        "q2",
    ]
    for invalid in [
        {**valid, "expected_n": 3},
        {**valid, "per_question": [_row("q1"), _row("q1")]},
        {**valid, "question_set_sha256": "0" * 64},
        _report(_row("q_abs", is_abstention=False)),
    ]:
        try:
            evaluator.validate_input_report(invalid)
            raise AssertionError("expected invalid report")
        except ValueError:
            pass


def test_official_label_preserves_published_yes_containment_semantics() -> None:
    evaluator = _load()
    assert evaluator.official_label("yes")
    assert evaluator.official_label("YES, the response is correct")
    assert not evaluator.official_label("no")


def test_evaluation_fingerprint_binds_input_and_official_identity() -> None:
    evaluator = _load()
    raw = b'{"source":"one"}\n'
    report = _report(_row("q1"))
    first = evaluator.evaluator_fingerprint(report, raw)
    second = evaluator.evaluator_fingerprint(report, b'{"source":"two"}\n')
    assert first["source_commit"] == "9e0b455f4ef0e2ab8f2e582289761153549043fc"
    assert first["model"] == "gpt-4o-2024-08-06"
    assert first["input_report_sha256"] == hashlib.sha256(raw).hexdigest()
    assert first["input_report_sha256"] != second["input_report_sha256"]
    assert set(first["prompt_sha256"]) == {
        "ordinary",
        "temporal-reasoning",
        "knowledge-update",
        "single-session-preference",
        "abstention",
    }
    assert len(first["sha256"]) == 64


def test_partial_or_api_error_is_fail_closed_and_ineligible() -> None:
    evaluator = _load()
    report = _report(_row("q1"), _row("q2"))
    calls = []

    def ok(prompt):
        calls.append(prompt)
        return "yes"

    smoke = evaluator.evaluate(report, b"input", ok, limit=1)
    assert smoke["evaluated_n"] == 1
    assert smoke["complete"] is False
    assert smoke["promotion_ineligible"] is True
    assert smoke["accuracy"] == 1.0

    def fails_on_second(prompt):
        if "q2 answer" in prompt:
            raise RuntimeError("api unavailable")
        return "yes"

    report = _report(
        _row("q1", question="q1 answer"),
        _row("q2", question="q2 answer"),
    )
    failed = evaluator.evaluate(report, b"input", fails_on_second)
    assert failed["complete"] is False
    assert failed["promotion_ineligible"] is True
    assert failed["api_errors"] == 1
    assert failed["per_question"][1]["label"] is False
    assert "api unavailable" in failed["per_question"][1]["error"]


def test_official_eval_rejects_ineligible_erroring_or_unstructured_reader_input() -> None:
    evaluator = _load()
    invalid_reports = []
    promotion_ineligible = _report(_row("q1"))
    promotion_ineligible["promotion_ineligible"] = True
    invalid_reports.append(promotion_ineligible)
    erroring = _report(_row("q1"))
    erroring["errors"]["judge"] = 1
    invalid_reports.append(erroring)
    row_error = _report(_row("q1", judge_error="timeout"))
    invalid_reports.append(row_error)
    malformed_answer = _report(_row("q1", abstain=False, answer=None))
    invalid_reports.append(malformed_answer)

    for report in invalid_reports:
        try:
            evaluator.validate_input_report(report)
            raise AssertionError("expected invalid official-eval input")
        except ValueError:
            pass

#!/usr/bin/env python3
"""Score a repaired reader report with LongMemEval's official QA metric.

This is a leaderboard-comparability artifact, not a MemPhant promotion gate.
It intentionally preserves the upstream evaluator's prompts and permissive
``"yes" in response.lower()`` label at the pinned source revision.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import time
import urllib.request
from pathlib import Path


OFFICIAL_SOURCE_REPO = "xiaowu0162/LongMemEval"
OFFICIAL_SOURCE_COMMIT = "9e0b455f4ef0e2ab8f2e582289761153549043fc"
OFFICIAL_SOURCE_FILE = "src/evaluation/evaluate_qa.py"
OFFICIAL_MODEL = "gpt-4o-2024-08-06"
OPENAI_URL = "https://api.openai.com/v1/chat/completions"
CANONICAL_UNANSWERABLE_HYPOTHESIS = (
    "I cannot answer this question because the information is not available."
)

ORDINARY_TEMPLATE = "I will give you a question, a correct answer, and a response from a model. Please answer yes if the response contains the correct answer. Otherwise, answer no. If the response is equivalent to the correct answer or contains all the intermediate steps to get the correct answer, you should also answer yes. If the response only contains a subset of the information required by the answer, answer no. \n\nQuestion: {}\n\nCorrect Answer: {}\n\nModel Response: {}\n\nIs the model response correct? Answer yes or no only."
TEMPORAL_TEMPLATE = "I will give you a question, a correct answer, and a response from a model. Please answer yes if the response contains the correct answer. Otherwise, answer no. If the response is equivalent to the correct answer or contains all the intermediate steps to get the correct answer, you should also answer yes. If the response only contains a subset of the information required by the answer, answer no. In addition, do not penalize off-by-one errors for the number of days. If the question asks for the number of days/weeks/months, etc., and the model makes off-by-one errors (e.g., predicting 19 days when the answer is 18), the model's response is still correct. \n\nQuestion: {}\n\nCorrect Answer: {}\n\nModel Response: {}\n\nIs the model response correct? Answer yes or no only."
UPDATE_TEMPLATE = "I will give you a question, a correct answer, and a response from a model. Please answer yes if the response contains the correct answer. Otherwise, answer no. If the response contains some previous information along with an updated answer, the response should be considered as correct as long as the updated answer is the required answer.\n\nQuestion: {}\n\nCorrect Answer: {}\n\nModel Response: {}\n\nIs the model response correct? Answer yes or no only."
PREFERENCE_TEMPLATE = "I will give you a question, a rubric for desired personalized response, and a response from a model. Please answer yes if the response satisfies the desired response. Otherwise, answer no. The model does not need to reflect all the points in the rubric. The response is correct as long as it recalls and utilizes the user's personal information correctly.\n\nQuestion: {}\n\nRubric: {}\n\nModel Response: {}\n\nIs the model response correct? Answer yes or no only."
ABSTENTION_TEMPLATE = "I will give you an unanswerable question, an explanation, and a response from a model. Please answer yes if the model correctly identifies the question as unanswerable. The model could say that the information is incomplete, or some other information is given but the asked information is not.\n\nQuestion: {}\n\nExplanation: {}\n\nModel Response: {}\n\nDoes the model correctly identify the question as unanswerable? Answer yes or no only."

PROMPT_TEMPLATES = {
    "ordinary": ORDINARY_TEMPLATE,
    "temporal-reasoning": TEMPORAL_TEMPLATE,
    "knowledge-update": UPDATE_TEMPLATE,
    "single-session-preference": PREFERENCE_TEMPLATE,
    "abstention": ABSTENTION_TEMPLATE,
}
ORDINARY_TYPES = {
    "single-session-user",
    "single-session-assistant",
    "multi-session",
}


def sha256_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def question_set_sha256(question_ids: list[str]) -> str:
    encoded = json.dumps(sorted(question_ids), separators=(",", ":")).encode()
    return sha256_bytes(encoded)


def build_official_prompt(
    task: str, question: str, answer: str, response: str, abstention: bool
) -> str:
    if abstention:
        template = ABSTENTION_TEMPLATE
    elif task in ORDINARY_TYPES:
        template = ORDINARY_TEMPLATE
    elif task == "temporal-reasoning":
        template = TEMPORAL_TEMPLATE
    elif task == "knowledge-update":
        template = UPDATE_TEMPLATE
    elif task == "single-session-preference":
        template = PREFERENCE_TEMPLATE
    else:
        raise ValueError(f"unknown LongMemEval question type: {task}")
    return template.format(question, answer, response)


def official_hypothesis(row: dict) -> str:
    if row.get("abstain") is True:
        return CANONICAL_UNANSWERABLE_HYPOTHESIS
    answer = row.get("answer")
    return answer if isinstance(answer, str) else ""


def official_label(response: str) -> bool:
    """Match the published evaluator exactly; do not use this in promotion."""
    return "yes" in response.lower()


def validate_input_report(report: dict) -> list[dict]:
    if report.get("benchmark") != "longmemeval_reader_qa":
        raise ValueError("input must be a repaired longmemeval_reader_qa report")
    if report.get("reader_report_sha256") != sha256_bytes(
        json.dumps(
            {key: value for key, value in report.items() if key != "reader_report_sha256"},
            sort_keys=True,
            separators=(",", ":"),
        ).encode()
    ):
        raise ValueError("input reader report fingerprint is invalid")
    rows = report.get("per_question")
    if not isinstance(rows, list):
        raise ValueError("input report per_question must be a list")
    if report.get("complete") is not True:
        raise ValueError("input reader report must be complete")
    if report.get("promotion_ineligible") is not False or report.get("smoke_only") is not False:
        raise ValueError("input reader report must be promotion eligible")
    if report.get("aborted") is not None:
        raise ValueError("input reader report is aborted")
    errors = report.get("errors")
    if (
        not isinstance(errors, dict)
        or set(errors) != {"reader", "parse", "judge"}
        or any(type(value) is not int or value != 0 for value in errors.values())
    ):
        raise ValueError("input reader report has evaluation errors")
    if report.get("expected_n") != len(rows):
        raise ValueError("input report expected_n does not match per_question")
    if report.get("source_expected_n") != len(rows) or report.get("evaluated_expected_n") != len(rows):
        raise ValueError("input report source/evaluated counts do not match")
    source_evidence = report.get("source_evidence_sha256")
    if (
        not isinstance(source_evidence, str)
        or not re.fullmatch(r"[0-9a-f]{64}", source_evidence)
        or report.get("evaluated_evidence_sha256") != source_evidence
        or report.get("evidence_sha256") != source_evidence
    ):
        raise ValueError("input report evidence fingerprint is invalid")
    evaluator = report.get("evaluator_fingerprint")
    if not isinstance(evaluator, dict) or evaluator.get("sha256") != sha256_bytes(
        json.dumps(
            {key: value for key, value in evaluator.items() if key != "sha256"},
            sort_keys=True,
            separators=(",", ":"),
        ).encode()
    ):
        raise ValueError("input report evaluator fingerprint is invalid")
    if evaluator.get("source_evidence_sha256") != source_evidence:
        raise ValueError("input evaluator is not bound to evidence")
    if not re.fullmatch(
        r"[0-9a-f]{64}", str(evaluator.get("evaluator_source_sha256", ""))
    ):
        raise ValueError("input evaluator source hash is invalid")

    required = {
        "question_id": str,
        "question_type": str,
        "question": str,
        "is_abstention": bool,
    }
    ids: list[str] = []
    for row in rows:
        if not isinstance(row, dict):
            raise ValueError("per_question rows must be objects")
        for key, expected_type in required.items():
            if not isinstance(row.get(key), expected_type):
                raise ValueError(f"per_question row has invalid {key}")
        if "gold_answer" not in row or row["gold_answer"] is None:
            raise ValueError("per_question row is missing gold_answer")
        if type(row.get("abstain")) is not bool or type(row.get("correct")) is not bool:
            raise ValueError("per_question row has invalid structured result")
        if row["abstain"]:
            if row.get("answer") is not None:
                raise ValueError("abstention requires answer=null")
        elif not isinstance(row.get("answer"), str) or not row["answer"].strip():
            raise ValueError("non-abstention requires a nonempty answer")
        if any(row.get(f"{kind}_error") is not None for kind in ("reader", "parse", "judge")):
            raise ValueError("per_question row contains an evaluation error")
        if row["is_abstention"] != ("_abs" in row["question_id"]):
            raise ValueError("is_abstention does not match the official question ID")
        ids.append(row["question_id"])
    if len(ids) != len(set(ids)):
        raise ValueError("input report contains duplicate question IDs")
    if report.get("question_set_sha256") != question_set_sha256(ids):
        raise ValueError("input report question_set_sha256 mismatch")
    return rows


def evaluator_fingerprint(report: dict, input_bytes: bytes) -> dict:
    fingerprint = {
        "source_repo": OFFICIAL_SOURCE_REPO,
        "source_commit": OFFICIAL_SOURCE_COMMIT,
        "source_file": OFFICIAL_SOURCE_FILE,
        "evaluator_source_sha256": sha256_bytes(Path(__file__).read_bytes()),
        "model": OFFICIAL_MODEL,
        "decoding": {"n": 1, "temperature": 0, "max_tokens": 10},
        "label_semantics": "yes substring containment",
        "input_report_sha256": sha256_bytes(input_bytes),
        "question_set_sha256": report["question_set_sha256"],
        "prompt_sha256": {
            name: sha256_bytes(template.encode())
            for name, template in PROMPT_TEMPLATES.items()
        },
    }
    fingerprint["sha256"] = sha256_bytes(
        json.dumps(fingerprint, sort_keys=True, separators=(",", ":")).encode()
    )
    return fingerprint


def call_openai(prompt: str) -> str:
    key = os.environ.get("OPENAI_API_KEY")
    if not key:
        raise RuntimeError("OPENAI_API_KEY is required")
    body = json.dumps(
        {
            "model": OFFICIAL_MODEL,
            "messages": [{"role": "user", "content": prompt}],
            "n": 1,
            "temperature": 0,
            "max_tokens": 10,
        }
    ).encode()
    request = urllib.request.Request(
        OPENAI_URL,
        data=body,
        headers={
            "Authorization": f"Bearer {key}",
            "Content-Type": "application/json",
            "User-Agent": "memphant-longmemeval-official-eval/1.0",
        },
        method="POST",
    )
    with urllib.request.urlopen(request, timeout=180) as response:
        payload = json.load(response)
    try:
        content = payload["choices"][0]["message"]["content"]
    except (KeyError, IndexError, TypeError) as error:
        raise RuntimeError("OpenAI response is missing completion content") from error
    if not isinstance(content, str):
        raise RuntimeError("OpenAI completion content must be a string")
    return content.strip()


def evaluate(report: dict, input_bytes: bytes, api_call, limit: int | None = None) -> dict:
    rows = validate_input_report(report)
    selected = rows[:limit] if limit is not None else rows
    results = []
    api_errors = 0
    for row in selected:
        hypothesis = official_hypothesis(row)
        prompt = build_official_prompt(
            row["question_type"],
            row["question"],
            str(row["gold_answer"]),
            hypothesis,
            row["is_abstention"],
        )
        error = None
        response = None
        label = False
        try:
            response = api_call(prompt)
            label = official_label(response)
        except Exception as exception:  # API failures are data in the report.
            api_errors += 1
            error = str(exception)
        results.append(
            {
                "question_id": row["question_id"],
                "question_type": row["question_type"],
                "hypothesis": hypothesis,
                "official_response": response,
                "label": label,
                "error": error,
            }
        )

    smoke_only = limit is not None
    complete = not smoke_only and len(results) == len(rows) and api_errors == 0
    return {
        "benchmark": "longmemeval_official_qa_comparability",
        "source": (
            f"https://github.com/{OFFICIAL_SOURCE_REPO}/blob/"
            f"{OFFICIAL_SOURCE_COMMIT}/{OFFICIAL_SOURCE_FILE}"
        ),
        "model": OFFICIAL_MODEL,
        "expected_n": len(rows),
        "evaluated_n": len(results),
        "smoke_only": smoke_only,
        "complete": complete,
        "valid_official_comparison": complete,
        "promotion_ineligible": True,
        "api_errors": api_errors,
        "accuracy": (
            sum(result["label"] for result in results) / len(results)
            if results
            else None
        ),
        "evaluator_fingerprint": evaluator_fingerprint(report, input_bytes),
        "per_question": results,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", required=True, help="repaired run_reader JSON report")
    parser.add_argument("--out", required=True, help="official evaluation JSON report")
    parser.add_argument("--limit", type=int, help="smoke only; always comparison-ineligible")
    args = parser.parse_args()
    if args.limit is not None and args.limit < 1:
        parser.error("--limit must be at least 1")

    input_path = Path(args.input)
    input_bytes = input_path.read_bytes()
    report = json.loads(input_bytes)
    result = evaluate(report, input_bytes, call_openai, args.limit)
    result["input_path"] = str(input_path)
    result["generated_at_unix"] = int(time.time())
    Path(args.out).write_text(json.dumps(result, indent=2) + "\n")
    print(
        f"official_eval n={result['evaluated_n']}/{result['expected_n']} "
        f"accuracy={result['accuracy']} complete={result['complete']} out={args.out}"
    )
    return 0 if result["complete"] else 1


if __name__ == "__main__":
    raise SystemExit(main())

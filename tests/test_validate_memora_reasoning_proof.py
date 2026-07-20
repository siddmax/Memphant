from __future__ import annotations

import importlib.util
import json
from pathlib import Path

import pytest


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "validate_memora_reasoning_proof.py"


def load():
    spec = importlib.util.spec_from_file_location("validate_memora_reasoning_proof", SCRIPT)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def goal_question(actual=11.0):
    return {
        "question_id": "goal_food_expenses_coffee_2_0",
        "memory_evidence": {
            "goal_value": 30, "actual_value": actual,
            "goal_data": {"category": "food_expenses", "subcategory": "coffee", "value": 30},
            "goal_session": {"session_id": 12, "session_date": "2025-06-01"},
            "activity_sessions": [
                {"session_id": 3, "session_date": "2025-06-01", "amount": 3.5, "expense_type": "coffee"},
                {"session_id": 10, "session_date": "2025-06-01", "amount": 7.5, "expense_type": "coffee"},
            ],
        },
    }


def source(unit, session, value, category="coffee"):
    return {
        "tenant_id": "tenant", "unit_id": unit,
        "memory_unit_body": "quantity_event_v1 item food_spending: " + json.dumps({
            "dimensions": {"expense_type": category}, "measure": "food_spending",
            "occurred_at": "2025-06-01T12:00:00Z", "type": "quantity_event.v1",
            "unit": "usd", "value": str(value),
        }, separators=(",", ":")),
        "source_episode_id": f"episode-{session}",
        "source_episode_body": f"[session {session:04d}]\nuser: coffee ${value}",
    }


def record_fixture(module):
    rollup = (
        "quantity rollup quantity_event_v1/food_spending/food_spending (usd); "
        "window=[2025-06-01T00:00:00Z,2025-06-08T00:00:00Z); "
        "filter=expense_type=coffee; total=11; average=5.5 "
        "(rounded to 6 decimal places when needed); count=2; min=3.5; max=7.5"
    )
    goal_body = "goals item food_expenses_coffee: " + json.dumps({
        "category": "food_expenses", "subcategory": "coffee", "value": 30,
    }, separators=(",", ":"))
    evidence = [
        {"rank": 1, "unit_id": "rollup", "body": rollup},
        {"rank": 2, "unit_id": "goal", "body": goal_body},
    ]
    trace = {
        "id": "trace", "tenant_id": "tenant", "scope_id": "scope", "actor_id": "actor",
        "context_items": [
            {"unit_id": "rollup", "derived_from_unit_ids": ["event-3", "event-10"]},
            {"unit_id": "goal", "derived_from_unit_ids": []},
        ],
        "citations": [],
    }
    derived = [source("event-3", 3, "3.5"), source("event-10", 10, "7.5")]
    direct = [{
        "tenant_id": "tenant", "unit_id": "goal", "memory_unit_body": goal_body,
        "source_episode_id": "episode-12",
        "source_episode_body": "[session 0012]\nuser: coffee budget is $30",
    }]
    answer = {
        "period": "weekly", "persona": "software_engineer",
        "question_id": "goal_food_expenses_coffee_2_0", "question": "Goal?",
        "question_date": "2025-06-07", "task_type": "Reasoning", "answer": "No",
        "evidence": evidence,
        "trace": {"trace_id": "trace", "degraded": False, "evidence_sha256": module.generator.sha256_json(evidence)},
    }
    record = {
        "retrieval": {
            "trace": trace, "trace_sha256": module.generator.sha256_json(trace),
            "derived_sources": derived, "derived_sources_sha256": module.generator.sha256_json(derived),
            "direct_sources": direct, "direct_sources_sha256": module.generator.sha256_json(direct),
        }
    }
    return answer, record


def run_coverage(model="openai/gpt-5.6-luna-pro"):
    behavior = {
        "MEMPHANT_STRUCTURED_STATE_MODEL": model,
        "MEMPHANT_STRUCTURED_STATE_CONCURRENCY": "4",
    }
    if model == "google/gemini-3.5-flash":
        behavior["MEMPHANT_STRUCTURED_STATE_REASONING_EFFORT"] = "high"
    extractor = {
        "provider_attempts": 163, "completed_attempts": 163,
        "interrupted_attempts": 0, "unpriced_attempts": 0,
        "successful_responses": 163, "decode_outcomes": 163,
        "decode_errors": 0, "rejected_operations": 0, "priced_responses": 163,
        "terminal_decode_errors": 0, "successful_decodes": 163,
        "episodes": 163, "successful_episodes": 163,
        "retried_episodes": 0, "transient_attempts": 0,
        "transient_no_content_attempts": 0,
        "transient_transport_attempts": 0, "transient_http_attempts": 0,
        "cost_status": "all_provider_attempts_priced", "rejection_reasons": {},
        "terminal_rejected_operations": 0, "semantic_repair_attempts": 0,
        "requested_model": model,
    }
    return (
        {"summary": {"model": model}},
        {
            "reader": {
                "model": model, "reasoning_effort": "high", "fresh_calls": 5,
                "cache_hits": 0, "provider_attempts": 5,
                "unpriced_provider_attempts": 0,
                "cost_status": "all_provider_attempts_priced",
            },
            "runtime": {"behavior_environment": behavior, "structured_extractor": extractor},
        },
    )


def reader_ledger(count=5):
    attempts = [
        {
            "attempt_id": index + 1,
            "cache_key": f"key-{index}",
            "request_key": f"key-{index}",
            "retry_index": 0,
            "start": {
                "requested_model": "openai/gpt-5.6-luna-pro",
                "request_sha256": f"{index + 1:064x}",
                "retry_index": 0,
            },
            "status": "result",
            "result": {"response": {
                "model": "openai/gpt-5.6-luna-pro",
                "requested_model": "openai/gpt-5.6-luna-pro",
                "served_model": "openai/gpt-5.6-luna-pro",
                "provider": "OpenAI",
                "response_id": f"reader-response-{index}",
                "request_sha256": f"{index + 1:064x}",
                "result_sha256": f"{index + 101:064x}",
                "retry_index": 0,
                "elapsed_seconds": 0.1,
                "parse_status": "provider_response_validated",
                "usage": {
                    "prompt_tokens": 10, "completion_tokens": 2,
                    "total_tokens": 12, "cost": 0.01,
                },
            }},
        }
        for index in range(count)
    ]
    return {
        "provider_attempts": count, "priced_provider_attempts": count,
        "unpriced_provider_attempts": 0, "attempts": attempts,
        "reported_cost_usd": count * 0.01,
        "attempts_sha256": None,
    }


def test_oracle_is_recomputed_from_official_activity_rows() -> None:
    module = load()
    oracle = module.derive_oracle(
        goal_question(), ("2025-06-01T00:00:00Z", "2025-06-08T00:00:00Z")
    )
    assert oracle["total"] == module.Decimal("11.0")
    assert [event[0] for event in oracle["events"]] == [3, 10]
    with pytest.raises(ValueError, match="aggregate disagrees"):
        module.derive_oracle(
            goal_question(actual=12),
            ("2025-06-01T00:00:00Z", "2025-06-08T00:00:00Z"),
        )


def test_oracle_reader_pack_and_exact_answer_gate() -> None:
    module = load()
    oracle = module.derive_oracle(
        goal_question(), ("2025-06-01T00:00:00Z", "2025-06-08T00:00:00Z")
    )

    evidence = module.oracle_reader_evidence(oracle)

    assert evidence[0]["rank"] == 1
    assert evidence[0]["body"].startswith("quantity rollup ")
    assert "total=11" in evidence[0]["body"]
    assert "average=5.500000" in evidence[0]["body"]
    assert evidence[1]["rank"] == 2
    assert "target=30" in evidence[1]["body"]
    assert module.answer_contains_decimal(
        "The total is $258.23.", module.Decimal("258.23")
    )
    assert module.validate_oracle_reader_answer(
        oracle, "Yes. Coffee spending is $11 against a $30 weekly budget."
    )
    steps = oracle | {
        "unit": "steps", "aggregate": "average",
        "average": module.Decimal("7324"),
        "goal": oracle["goal"] | {"value": module.Decimal("8000")},
    }
    assert module.validate_oracle_reader_answer(
        steps, "Not on average: you’re averaging 7,324 steps versus an 8,000-step goal."
    )
    with pytest.raises(ValueError, match="exact values/status"):
        module.validate_oracle_reader_answer(oracle, "Yes, the goal was met.")
    over_budget = oracle | {"goal": oracle["goal"] | {"value": module.Decimal("10")}}
    with pytest.raises(ValueError, match="exact values/status"):
        module.validate_oracle_reader_answer(
            over_budget, "Nobody knows, but the values are $11 and $10."
        )
    with pytest.raises(ValueError, match="exact values/status"):
        module.validate_oracle_reader_answer(
            oracle, "Yes. The goal is $11 and actual spending is $30."
        )
    with pytest.raises(ValueError, match="exact values/status"):
        module.validate_oracle_reader_answer(
            oracle, "Yes. Actual goal $30, $11."
        )
    total_only = oracle | {"goal": None, "aggregate": "total"}
    with pytest.raises(ValueError, match="exact values/status"):
        module.validate_oracle_reader_answer(
            total_only, "The total is not $11; it is $1."
        )


def test_oracle_reader_screen_records_exact_five_call_shape() -> None:
    module = load()
    oracle = module.derive_oracle(
        goal_question(), ("2025-06-01T00:00:00Z", "2025-06-08T00:00:00Z")
    )

    class Reader:
        model = "openai/gpt-5.6-luna-pro"

        def answer(self, query, evidence):
            assert query["question"] == oracle["question"]
            assert evidence == module.oracle_reader_evidence(oracle)
            return "Yes. Coffee spending is $11 against a $30 weekly budget.", {
                "provider": "fixture", "cost_usd": 0.01, "provider_attempts": 1,
                "fresh_call": True, "cache_hit": False,
            }

    report = module.run_oracle_reader_screen([oracle], Reader())

    assert report["eligible"] is True
    assert report["questions"] == 1
    assert report["reported_cost_usd"] == 0.01
    assert report["results"][0]["answer_sha256"]


def test_exact_rollup_activity_and_goal_provenance_pass() -> None:
    module = load()
    oracle = module.derive_oracle(
        goal_question(), ("2025-06-01T00:00:00Z", "2025-06-08T00:00:00Z")
    )
    answer, record = record_fixture(module)
    module.validate_record(oracle, record, answer)


@pytest.mark.parametrize("model", [
    "openai/gpt-5.6-luna-pro", "google/gemini-3.5-flash",
])
def test_exact_paid_run_coverage_passes_for_each_model_arm(model) -> None:
    module = load()
    answers, proof = run_coverage(model)
    module.validate_run_coverage(answers, proof)


@pytest.mark.parametrize("path", [
    "extractor_count", "extractor_rejection", "reader_retry", "concurrency", "effort",
])
def test_paid_run_coverage_rejects_inexact_or_drifted_proof(path) -> None:
    module = load()
    answers, proof = run_coverage("google/gemini-3.5-flash")
    if path == "extractor_count":
        proof["runtime"]["structured_extractor"]["decode_outcomes"] = 162
    elif path == "extractor_rejection":
        proof["runtime"]["structured_extractor"].update(
            rejected_operations=1, rejection_reasons={"quantity_shape": 1}
        )
    elif path == "reader_retry":
        proof["reader"]["provider_attempts"] = 6
    elif path == "concurrency":
        proof["runtime"]["behavior_environment"]["MEMPHANT_STRUCTURED_STATE_CONCURRENCY"] = "8"
    else:
        del proof["runtime"]["behavior_environment"]["MEMPHANT_STRUCTURED_STATE_REASONING_EFFORT"]
    with pytest.raises(ValueError):
        module.validate_run_coverage(answers, proof)


def test_reader_ledger_requires_exactly_five_fresh_priced_attempts() -> None:
    module = load()
    ledger = reader_ledger()
    ledger["attempts_sha256"] = module.generator.sha256_json(ledger["attempts"])
    module.validate_reader_ledger(ledger, "openai/gpt-5.6-luna-pro")
    for field, value in (
        ("reported_cost_usd", 999),
        ("attempts_sha256", "wrong"),
    ):
        drifted = ledger | {field: value}
        with pytest.raises(ValueError, match="coverage is not exact"):
            module.validate_reader_ledger(drifted, "openai/gpt-5.6-luna-pro")
    ledger["attempts"][0]["result"]["response"]["model"] = "other/model"
    ledger["attempts_sha256"] = module.generator.sha256_json(ledger["attempts"])
    with pytest.raises(ValueError, match="coverage is not exact"):
        module.validate_reader_ledger(ledger, "openai/gpt-5.6-luna-pro")


def test_run_coverage_accepts_recovered_transient_accuracy_with_lower_bound_cost() -> None:
    module = load()
    answers, proof = run_coverage("google/gemini-3.5-flash")
    extractor = proof["runtime"]["structured_extractor"]
    extractor.update({
        "provider_attempts": 164, "completed_attempts": 164,
        "unpriced_attempts": 1, "decode_outcomes": 164,
        "decode_errors": 1, "transient_attempts": 1,
        "transient_no_content_attempts": 1, "retried_episodes": 1,
        "cost_status": "reported_cost_is_lower_bound",
    })
    module.validate_run_coverage(answers, proof)
    with pytest.raises(ValueError, match="coverage is not exact"):
        ledger = reader_ledger(6)
        ledger["attempts_sha256"] = module.generator.sha256_json(ledger["attempts"])
        module.validate_reader_ledger(ledger, "openai/gpt-5.6-luna-pro")


def test_run_coverage_accepts_recovered_evidence_grounding_accuracy() -> None:
    module = load()
    answers, proof = run_coverage("google/gemini-3.5-flash")
    extractor = proof["runtime"]["structured_extractor"]
    extractor.update({
        "provider_attempts": 164, "completed_attempts": 164,
        "successful_responses": 163, "decode_outcomes": 164,
        "priced_responses": 164, "retried_episodes": 1,
        "accepted_operations": 164, "rejected_operations": 1,
        "semantic_repair_attempts": 1,
        "rejection_reasons": {"evidence_grounding": 1},
    })
    module.validate_run_coverage(answers, proof)


@pytest.mark.parametrize("mutation,match", [
    ("category", "activity provenance"),
    ("session", "activity provenance"),
    ("goal", "goal lacks exact direct provenance"),
])
def test_proof_gate_rejects_category_session_or_goal_drift(mutation, match) -> None:
    module = load()
    oracle = module.derive_oracle(
        goal_question(), ("2025-06-01T00:00:00Z", "2025-06-08T00:00:00Z")
    )
    answer, record = record_fixture(module)
    retrieval = record["retrieval"]
    if mutation == "category":
        retrieval["derived_sources"][0] = source("event-3", 3, "3.5", "lunch")
        retrieval["derived_sources_sha256"] = module.generator.sha256_json(retrieval["derived_sources"])
    elif mutation == "session":
        retrieval["derived_sources"][0]["source_episode_body"] = "[session 0004]\nuser: coffee $3.5"
        retrieval["derived_sources_sha256"] = module.generator.sha256_json(retrieval["derived_sources"])
    else:
        retrieval["direct_sources"][0]["source_episode_body"] = "[session 0013]\nuser: coffee budget is $30"
        retrieval["direct_sources_sha256"] = module.generator.sha256_json(retrieval["direct_sources"])
    with pytest.raises(ValueError, match=match):
        module.validate_record(oracle, record, answer)

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


def test_reader_cli_cache_is_keyed_by_engine_and_model(tmp_path) -> None:
    reader = _load_run_reader()
    codex = reader.ReaderCli("codex", "gpt-5.6-luna", "gpt-5.6-terra", tmp_path, 0)
    claude = reader.ReaderCli(
        "claude", "gpt-5.6-luna", "gpt-5.6-terra", tmp_path, 0
    )
    # Same prompts, different engine -> different cache entries.
    codex_key = codex._cache_path("reader", "sys", "prompt")
    claude_key = claude._cache_path("reader", "sys", "prompt")
    assert codex_key != claude_key
    # Judge calls key on the judge model, reader calls on the reader model.
    assert codex._cache_path("judge", "sys", "prompt") != codex_key
    # A pre-seeded cache entry is served without spending budget (max_calls=0).
    codex_key.write_text('{"reply": "cached-answer"}')
    assert codex.call("reader", "sys", "prompt") == "cached-answer"
    assert codex.fresh_calls == 0 and codex.cached_calls == 1
    # Legacy (pre-engine) cache entries stay readable for the claude engine.
    claude._legacy_cache_path("reader", "sys", "prompt").write_text(
        '{"reply": "legacy-answer"}'
    )
    assert claude.call("reader", "sys", "prompt") == "legacy-answer"
    # ...but never for codex: exhausted budget must raise, not fall through.
    codex_judge = codex._cache_path("judge", "sys", "prompt")
    assert not codex_judge.exists()
    try:
        codex.call("judge", "sys", "prompt")
        raise AssertionError("expected CallBudgetExceeded")
    except reader.CallBudgetExceeded:
        pass


def test_unknown_engine_is_rejected(tmp_path) -> None:
    reader = _load_run_reader()
    try:
        reader.ReaderCli("not-a-real-engine", "m", "m", tmp_path, 0)
        raise AssertionError("expected ValueError")
    except ValueError:
        pass


def test_openrouter_requires_api_key(tmp_path, monkeypatch) -> None:
    reader = _load_run_reader()
    monkeypatch.delenv("OPENROUTER_API_KEY", raising=False)
    try:
        reader.ReaderCli("openrouter", "m", "m", tmp_path, 0)
        raise AssertionError("expected RuntimeError")
    except RuntimeError as error:
        assert "OPENROUTER_API_KEY" in str(error)


def test_openrouter_cache_key_includes_engine_model_and_effort(tmp_path, monkeypatch) -> None:
    reader = _load_run_reader()
    monkeypatch.setenv("OPENROUTER_API_KEY", "sk-test-not-real")
    a = reader.ReaderCli("openrouter", "openai/gpt-5.6-terra", "anthropic/claude-sonnet-5", tmp_path, 0)
    b = reader.ReaderCli(
        "openrouter", "openai/gpt-5.6-terra", "anthropic/claude-sonnet-5", tmp_path, 0,
        reasoning_effort="low",
    )
    # Same engine/models, reasoning effort differs -> different cache entries.
    assert a._cache_path("reader", "sys", "prompt") != b._cache_path("reader", "sys", "prompt")
    # Reader and judge use different models on this engine -> different cache entries.
    assert a._cache_path("reader", "sys", "prompt") != a._cache_path("judge", "sys", "prompt")
    # A codex ReaderCli with the same reader model still keys separately (engine is part of the key).
    codex = reader.ReaderCli("codex", "openai/gpt-5.6-terra", "anthropic/claude-sonnet-5", tmp_path, 0)
    assert codex._cache_path("reader", "sys", "prompt") != a._cache_path("reader", "sys", "prompt")


def test_reasoning_effort_is_part_of_cache_identity_and_codex_or_openrouter_only(tmp_path) -> None:
    reader = _load_run_reader()
    default = reader.ReaderCli("codex", "gpt-5.6-terra", "gpt-5.6-terra", tmp_path, 0)
    medium = reader.ReaderCli(
        "codex", "gpt-5.6-terra", "gpt-5.6-terra", tmp_path, 0,
        reasoning_effort="medium",
    )
    assert default._cache_path("reader", "sys", "p") != medium._cache_path(
        "reader", "sys", "p"
    )
    assert medium.cache_model_for("reader") == "gpt-5.6-terra@medium"
    assert medium.model_for("reader") == "gpt-5.6-terra"
    try:
        reader.ReaderCli("claude", "m", "m", tmp_path, 0, reasoning_effort="high")
        raise AssertionError("expected ValueError")
    except ValueError:
        pass


def test_reader_system_prompt_v1_is_unchanged_and_v2_differs() -> None:
    reader = _load_run_reader()
    # Regression pin: v1 is the live default served to today's scoring queue
    # whenever --prompt-version is not passed. If this literal ever needs to
    # change, that is a deliberate v1 behavior change, not an accident.
    v1_pinned = (
        "You answer questions using ONLY the evidence provided in the prompt. "
        "Be terse: reply with the answer itself, a short phrase, no preamble. "
        "If the evidence is insufficient to answer, reply exactly: I don't know."
    )
    assert reader.READER_SYSTEM_PROMPT == v1_pinned
    assert reader.READER_SYSTEM_PROMPTS[1] is reader.READER_SYSTEM_PROMPT
    assert reader.READER_SYSTEM_PROMPTS[2] == reader.READER_SYSTEM_PROMPT_V2
    assert reader.READER_SYSTEM_PROMPT_V2 != reader.READER_SYSTEM_PROMPT


def test_accuracy_excludes_unscored_rows() -> None:
    reader = _load_run_reader()
    rows = [
        {"correct": True},
        {"correct": False},
        {"correct": None},  # reader_error / aborted rows never count
    ]
    result = reader.accuracy(rows)
    assert result == {"n": 3, "n_scored": 2, "qa_accuracy": 0.5}

from __future__ import annotations

import importlib.util
import http.client
import json
from pathlib import Path
import sys
import types

import pytest


ROOT = Path(__file__).resolve().parents[1]
EXPECTED_IDS = {
    "19367bc7", "21f3228c", "2c45ecbb", "52dd33bb", "658fa827", "6fdda2fc",
    "86fa86eb", "8e21c6e5", "aedd338d", "b05cf470", "dae9f7e9", "f2b221fd",
}


def _load():
    spec = importlib.util.spec_from_file_location(
        "run_lme_v2_p1_t6", ROOT / "scripts" / "run_lme_v2_p1_t6.py"
    )
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(module)
    return module


def _load_memory_adapter(monkeypatch):
    package = types.ModuleType("memory_modules")
    memory = types.ModuleType("memory_modules.memory")

    class Memory:
        def __init__(self, params):
            self.params = params

    memory.Memory = Memory
    memory.MemoryContextItem = dict
    memory.register_memory = lambda cls: cls
    monkeypatch.setitem(sys.modules, "memory_modules", package)
    monkeypatch.setitem(sys.modules, "memory_modules.memory", memory)
    spec = importlib.util.spec_from_file_location(
        "p1_t6_memory_adapter", ROOT / "benchmarks/longmemeval_v2/memphant_memory.py"
    )
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(module)
    return module


class AnswerTrap(dict):
    def __getitem__(self, key):
        if key not in {"id", "domain", "question_type"}:
            raise AssertionError(f"selector read forbidden field: {key}")
        return super().__getitem__(key)

    def get(self, key, default=None):
        if key not in {"id", "domain", "question_type"}:
            raise AssertionError(f"selector read forbidden field: {key}")
        return super().get(key, default)


def test_selector_is_answer_blind_deterministic_and_exact() -> None:
    campaign = _load()
    source = json.loads(
        (ROOT / "benchmarks/manifests/longmemeval_v2.p1_t6.selection-source.json").read_text()
    )
    rows = [AnswerTrap(row) for row in source["rows"]]
    selected = campaign.select_cases(rows)
    assert {row["id"] for row in selected} == EXPECTED_IDS
    assert campaign.canonical_sha256(selected) == campaign.SELECTION_SHA256
    assert campaign.SELECTION_SHA256 == (
        "d7762dbaffff7acfe779162d4993c8c09ef0440e3c1a25e0d3408127d73e25fa"
    )
    assert [row["domain"] for row in selected].count("web") == 6
    assert [row["domain"] for row in selected].count("enterprise") == 6
    counts = {ability: 0 for ability in campaign.ABILITIES}
    for row in selected:
        counts[row["ability"]] += 1
    assert max(counts.values()) - min(counts.values()) <= 1


def test_selector_rejects_invalid_rows_and_hash_amendment_is_explicit() -> None:
    campaign = _load()
    with pytest.raises(RuntimeError, match="duplicate question id"):
        campaign.select_cases(
            [
                {"id": "same", "domain": "web", "question_type": "procedure"},
                {"id": "same", "domain": "web", "question_type": "procedure"},
            ]
        )
    manifest = campaign.load_campaign_manifest()
    assert manifest["selection"]["sha256"] == campaign.SELECTION_SHA256
    assert manifest["selection"]["supersedes_underdefined_sha256"].startswith("ffe151")
    assert manifest["selection"]["outputs_observed_before_amendment"] is False


def test_run_order_is_complete_paired_and_immutable() -> None:
    campaign = _load()
    manifest = campaign.load_campaign_manifest()
    audit = campaign.verify_campaign_manifest(manifest)
    assert audit == {"cases": 12, "rows": 48, "arms": 4}
    rows = campaign.expanded_run_order(manifest)
    assert [row["sequence"] for row in rows] == list(range(1, 49))
    for question_id in sorted(EXPECTED_IDS):
        question_rows = [row for row in rows if row["question_id"] == question_id]
        assert [row["arm"] for row in question_rows] == ["fast", "sonnet", "luna", "sol"]
        assert len({row["row_id"] for row in question_rows}) == 4


def test_minimal_acquisition_excludes_trajectory_screenshot_archives() -> None:
    campaign = _load()
    manifest = campaign.load_campaign_manifest()
    paths = set(manifest["acquisition"]["files"])
    assert paths == {
        "checksums.sha256",
        "questions.jsonl",
        "trajectories.jsonl",
        "haystacks/lme_v2_medium.json",
        "question_screenshots/8e21c6e5.png",
        "question_screenshots/f2b221fd.png",
    }
    assert not any("trajectory_screenshots" in path for path in paths)


def test_completed_rows_are_never_overwritten(tmp_path: Path) -> None:
    campaign = _load()
    row_dir = tmp_path / "0001-fast-19367bc7"
    row_dir.mkdir()
    (row_dir / "row-proof.json").write_text("{}\n")
    with pytest.raises(RuntimeError, match="immutable row already exists"):
        campaign.require_new_row_dir(row_dir)


def test_fast_and_deep_configs_differ_only_by_mode(tmp_path: Path) -> None:
    campaign = _load()
    base = json.loads(
        (ROOT / "benchmarks/longmemeval_v2/memphant.memory.json").read_text()
    )
    fast = campaign.write_memory_config(base, "fast", tmp_path / "fast.json")
    deep = campaign.write_memory_config(base, "deep", tmp_path / "deep.json")
    assert fast["memory_params"]["mode"] == "fast"
    assert deep["memory_params"]["mode"] == "deep"
    fast["memory_params"]["mode"] = "deep"
    assert fast == deep


def test_percentiles_use_preregistered_nearest_rank_for_n12() -> None:
    campaign = _load()
    values = list(range(1, 13))
    assert campaign._percentile(values, 0.50) == 6
    assert campaign._percentile(values, 0.95) == 12


def test_trajectory_fragmentation_preserves_semantic_state_boundaries(monkeypatch) -> None:
    adapter = _load_memory_adapter(monkeypatch)
    trajectory = {
        "id": "t1", "goal": "ship", "outcome": "done",
        "states": [
            {"url": "https://one", "action": "open", "text": "A" * 60},
            {"url": "https://two", "action": "close", "text": "B" * 60},
        ],
    }
    blocks = [adapter._state_body(trajectory, state, index) for index, state in enumerate(trajectory["states"])]
    fragments = adapter._trajectory_fragments(trajectory, max(len(block.encode()) for block in blocks) + 1)
    assert fragments == blocks
    assert "\n\n---\n\n".join(fragments) == adapter._trajectory_body(trajectory)


def test_mutation_idempotency_keys_are_deterministic_and_domain_separated(monkeypatch) -> None:
    adapter = _load_memory_adapter(monkeypatch)
    payload = {"same": "body"}
    first = adapter._idempotency_key("POST", "/v1/episodes", payload)
    assert first == adapter._idempotency_key("POST", "/v1/episodes", payload)
    assert first != adapter._idempotency_key("PUT", "/v1/episodes", payload)
    assert first != adapter._idempotency_key("POST", "/v1/reflect", payload)


def test_manifest_rejects_order_and_spend_ceiling_drift() -> None:
    campaign = _load()
    manifest = campaign.load_campaign_manifest()
    manifest["run_order"]["case_order"] = list(reversed(manifest["run_order"]["case_order"]))
    with pytest.raises(RuntimeError, match="case-major order drift"):
        campaign.verify_campaign_manifest(manifest)
    manifest = campaign.load_campaign_manifest()
    manifest["campaign_spend"]["reader_and_judge_reserve_usd"] = 4.3
    with pytest.raises(RuntimeError, match="liability exceeds"):
        campaign.verify_campaign_manifest(manifest)


class _FakeResponse:
    def __init__(self, body: bytes):
        self.body = body
        self.status = 200

    def __enter__(self):
        return self

    def __exit__(self, *_args):
        return None

    def read(self):
        return self.body


def test_reader_post_acceptance_audit_failure_never_replays_or_changes_2xx(
    tmp_path: Path, monkeypatch
) -> None:
    campaign = _load()
    original = b'{"id":"gen-1","model":"qwen/qwen3.5-9b","choices":[]}'
    calls = []

    class Opener:
        def open(self, _request, timeout=None):
            calls.append(timeout)
            return _FakeResponse(original)

    monkeypatch.setattr(campaign.urllib.request, "build_opener", lambda *_args: Opener())
    monkeypatch.setattr(campaign, "_json_url", lambda *_args: (_ for _ in ()).throw(RuntimeError("late audit failure")))
    monkeypatch.setattr(campaign.time, "sleep", lambda _seconds: None)
    server, base = campaign._reader_proxy("secret", tmp_path / "reader.json")
    try:
        connection = http.client.HTTPConnection(base.removeprefix("http://"))
        connection.request(
            "POST", "/chat/completions",
            body=json.dumps({"model": "Qwen/Qwen3.5-9B", "messages": []}),
            headers={"content-type": "application/json"},
        )
        response = connection.getresponse()
        assert response.status == 200
        assert response.read() == original
        connection.close()
    finally:
        server.shutdown()
        server.server_close()
    assert len(calls) == 1
    assert json.loads((tmp_path / "reader.json").read_text())["audit_status"] == "invalid"


def test_judge_post_acceptance_audit_failure_never_replays_or_changes_2xx(
    tmp_path: Path, monkeypatch
) -> None:
    campaign = _load()
    original = b'{"id":"judge-1","model":"wrong-snapshot","choices":[],"usage":{}}'
    calls = []

    class Opener:
        def open(self, _request, timeout=None):
            calls.append(timeout)
            return _FakeResponse(original)

    monkeypatch.setattr(campaign.urllib.request, "build_opener", lambda *_args: Opener())
    manifest = campaign.load_campaign_manifest()
    server, base = campaign._judge_proxy("secret", tmp_path / "judge", manifest)
    try:
        body = {
            "model": "gpt-5.2-2025-12-11", "reasoning_effort": "medium",
            "max_completion_tokens": 4096, "messages": [],
        }
        connection = http.client.HTTPConnection(base.removeprefix("http://"))
        connection.request(
            "POST", "/chat/completions", body=json.dumps(body),
            headers={"content-type": "application/json"},
        )
        response = connection.getresponse()
        assert response.status == 200
        assert response.read() == original
        connection.close()
    finally:
        server.shutdown()
        server.server_close()
    assert len(calls) == 1
    assert json.loads((tmp_path / "judge/0001.json").read_text())["audit_status"] == "invalid"

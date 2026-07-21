from __future__ import annotations

import importlib.util
import hashlib
import http.client
import io
import json
from pathlib import Path
import subprocess
import sys
import types
import urllib.parse

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


def _write_synthetic_root(campaign, output: Path, manifest: dict) -> None:
    campaign._fingerprint = lambda path: {
        "path": str(path.resolve()), "bytes": 1, "sha256": "f" * 64
    }
    binaries = {
        name: campaign._fingerprint(campaign._binary_path(name))
        for name in ("server", "worker", "cli")
    }
    campaign.atomic_write_json(output / "pre-execution-proof.json", {
        "manifest_sha256": campaign.sha256_file(campaign.CAMPAIGN_MANIFEST),
        "endpoint_hashes": {}, "run_order_sha256": campaign.canonical_sha256(
            campaign.expanded_run_order(manifest)
        ),
        "outputs_observed_before_freeze": False,
        "git_commit": campaign.subprocess.run(
            ["git", "rev-parse", "HEAD"], cwd=campaign.ROOT,
            capture_output=True, text=True, check=True,
        ).stdout.strip(),
        "binaries": binaries,
        "binary_profile": campaign.PRODUCTION_BINARY_PROFILE,
        "archive_tools": {
            "server_major": 17,
            "pg_dump": {"binary": "/pg_dump", "major": 17, "server_major": 17},
            "pg_restore": {"binary": "/pg_restore", "major": 17, "server_major": 17},
        },
        "deep_prompt_sha256": campaign.sha256_file(campaign.ROOT / "config/deep-recall-v1.txt"),
        "deep_config_hashes": {
            name: candidate["config_sha256"]
            for name, candidate in manifest["protocol"]["deep_candidates"].items()
        },
        "python_environment": {"synthetic": True},
        "environment_contract_sha256": campaign.canonical_sha256(
            campaign._clean_environment()
        ),
        "materialization": {"proof_sha256": "a" * 64, "cases": {
            case["id"]: {"synthetic": case["id"]} for case in manifest["selection"]["cases"]
        }},
    })


def _write_synthetic_case_banks(campaign, output: Path, rows: list[dict]) -> None:
    for case_id in sorted({row["question_id"] for row in rows}):
        bank = output / "case-banks" / case_id
        bank.mkdir(parents=True)
        manifest = {
            "archive_sha256": "a" * 64,
            "logical_identity": {"sha256": "e" * 64},
            "construction_proof_sha256": "c" * 64,
            "case_contract_sha256": "f" * 64,
        }
        campaign.atomic_write_json(bank / "manifest.json", manifest)
        seal = campaign._case_bank_seal(bank / "manifest.json")
        row_hashes = {}
        for row in [item for item in rows if item["question_id"] == case_id]:
            row_dir = output / row["row_id"]
            campaign.atomic_write_json(row_dir / "case-bank-seal.json", seal)
            proof_path = row_dir / "row-proof.json"
            proof = json.loads(proof_path.read_text())
            proof["case_bank_seal_sha256"] = seal["seal_sha256"]
            proof["artifact_hashes"] = campaign.artifact_hashes(
                row_dir, exclude={"row-proof.json"}
            )
            campaign.atomic_write_json(proof_path, proof)
            row_hashes[row["arm"]] = campaign.sha256_file(proof_path)
        campaign.atomic_write_json(bank / "archive-retirement.json", {
            "archive_sha256": manifest["archive_sha256"],
            "case_bank_seal_sha256": seal["seal_sha256"],
            "manifest_sha256": seal["manifest_sha256"],
            "reason": "both_immutable_arm_rows_complete",
            "row_proof_sha256": row_hashes,
        })


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


def test_campaign_is_single_candidate_paired_gate() -> None:
    campaign = _load()
    manifest = campaign.load_campaign_manifest()
    assert campaign.verify_campaign_manifest(manifest) == {
        "cases": 12, "rows": 24, "arms": 2, "constructions": 12,
    }
    assert manifest["run_order"]["arm_order_per_case"] == ["fast", "sonnet"]
    assert manifest["protocol"]["selected_deep_arm"] == "sonnet"
    rows = campaign.expanded_run_order(manifest)
    assert [row["sequence"] for row in rows] == list(range(1, 25))
    assert {row["question_id"] for row in rows} == EXPECTED_IDS
    for question_id in sorted(EXPECTED_IDS):
        question_rows = [row for row in rows if row["question_id"] == question_id]
        assert [row["arm"] for row in question_rows] == ["fast", "sonnet"]
        assert len({row["row_id"] for row in question_rows}) == 2


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


def test_case_bank_contract_is_local_key_free_and_content_addressed(
    tmp_path: Path, monkeypatch
) -> None:
    campaign = _load()
    database_url = "postgres://bench:secret@127.0.0.1:5432/memphant_scratch_1_2"
    construction = {
        "schema_version": 1,
        "contract": {"adapter_sha256": "a" * 64, "binaries": {}},
        "isolation": {"tenant_id": "tenant"},
        "pairing": {
            "resource_count": 2,
            "worker": {"completed_sources": 2},
            "retains": [{"trajectory_id": "trajectory"}],
        },
    }
    construction["construction_proof_sha256"] = campaign.canonical_sha256(
        construction
    )
    case_contract = {"question_id": "19367bc7", "materialization_sha256": "m" * 64}
    monkeypatch.setenv("MEMPHANT_SCRATCH_ACTIVE", "1")
    monkeypatch.setattr(
        campaign,
        "_postgres_tool_identity",
        lambda *_args: {"binary": "/usr/bin/pg_dump", "version": "PostgreSQL 18", "major": 18, "server_major": 18},
    )
    monkeypatch.setattr(
        campaign,
        "_database_schema_identity",
        lambda _url: {"schema_sha256": "s" * 64, "extensions_and_migrations_sha256": "e" * 64, "sha256": "d" * 64},
    )
    monkeypatch.setattr(
        campaign,
        "_database_bank_identity",
        lambda _url: {"tables": {"resource": {"rows": 2, "sha256": "r" * 64}}, "sha256": "l" * 64},
    )
    monkeypatch.setattr(campaign, "_database_key_count", lambda _url: 0)
    monkeypatch.setattr(campaign, "_job_state_counts", lambda _url: (0, 0, 0))

    commands = []

    def run(command, **_kwargs):
        commands.append(command)
        destination = next(item.split("=", 1)[1] for item in command if item.startswith("--file="))
        Path(destination).write_bytes(b"frozen-bank")
        return campaign.subprocess.CompletedProcess(command, 0, "", "")

    monkeypatch.setattr(campaign.subprocess, "run", run)
    manifest = campaign._dump_case_bank(
        database_url, tmp_path / "bank", construction, case_contract
    )
    archive = tmp_path / "bank" / manifest["archive"]
    assert archive.name == f"{campaign.sha256_file(archive)}.dump"
    assert manifest["archive_sha256"] == campaign.sha256_file(archive)
    assert manifest["excluded_tables"] == list(campaign.BANK_EXCLUDED_TABLES)
    serialized = json.dumps(manifest)
    assert "secret" not in serialized and database_url not in serialized
    assert {
        item.removeprefix("--exclude-table-data=")
        for item in commands[0]
        if item.startswith("--exclude-table-data=")
    } == set(campaign.BANK_EXCLUDED_TABLES)


def test_clone_requires_quiescent_source_and_preserves_identity(
    monkeypatch,
) -> None:
    campaign = _load()
    source = "postgres://bench:secret@localhost:5432/memphant_scratch_1_2"
    expected = {"tables": {}, "sha256": "f" * 64}
    identities = []
    calls = []
    monkeypatch.setenv("MEMPHANT_SCRATCH_ACTIVE", "1")

    def identity(url):
        identities.append(urllib.parse.urlsplit(url).path.rsplit("/", 1)[-1])
        return expected

    monkeypatch.setattr(campaign, "_database_bank_identity", identity)
    monkeypatch.setattr(campaign, "_database_key_count", lambda _url: 0)
    monkeypatch.setattr(campaign, "_source_connection_count", lambda _url: 0)
    monkeypatch.setattr(
        campaign.subprocess,
        "run",
        lambda command, **_kwargs: (
            calls.append(command)
            or campaign.subprocess.CompletedProcess(command, 0, "", "")
        ),
    )
    clone = campaign._clone_case_source(
        source, "memphant_p1t6_19367bc7_deadbeef_fast", expected
    )
    assert clone.endswith("/memphant_p1t6_19367bc7_deadbeef_fast")
    assert identities == ["memphant_scratch_1_2", "memphant_p1t6_19367bc7_deadbeef_fast"]
    assert calls[0][0] == "createdb" and "--template=memphant_scratch_1_2" in calls[0]
    with pytest.raises(RuntimeError, match="zero active connections"):
        monkeypatch.setattr(campaign, "_source_connection_count", lambda _url: 1)
        campaign._clone_case_source(
            source, "memphant_p1t6_19367bc7_deadbeef_sonnet", expected
        )


def test_arm_clone_cleanup_is_forceful_and_name_bounded(monkeypatch) -> None:
    campaign = _load()
    calls = []
    monkeypatch.setattr(
        campaign.subprocess,
        "run",
        lambda command, **_kwargs: (
            calls.append(command)
            or campaign.subprocess.CompletedProcess(command, 0, "", "")
        ),
    )
    campaign._drop_local_database(
        "postgres://bench:secret@127.0.0.1:5432/memphant_p1t6_19367bc7_deadbeef_sonnet"
    )
    assert calls == [[
        "dropdb", "--force",
        "--maintenance-db=postgres://bench:secret@127.0.0.1:5432/postgres",
        "memphant_p1t6_19367bc7_deadbeef_sonnet",
    ]]
    with pytest.raises(RuntimeError, match="P1-T6 arm database name"):
        campaign._drop_local_database(
            "postgres://bench:secret@127.0.0.1:5432/memphant"
        )


def test_archive_state_fails_closed_after_first_completed_row(tmp_path: Path) -> None:
    campaign = _load()
    bank = tmp_path / "bank"
    bank.mkdir()
    campaign.atomic_write_json(bank / "manifest.json", {
        "archive": "a" * 64 + ".dump", "archive_sha256": "a" * 64,
    })
    with pytest.raises(RuntimeError, match="completed billable row.*archive"):
        campaign._verify_case_archive_resume(bank, completed_rows=1)
    assert campaign._verify_case_archive_resume(bank, completed_rows=2) is None


def test_run_case_builds_once_restores_then_runs_two_key_local_clones(
    tmp_path: Path, monkeypatch
) -> None:
    campaign = _load()
    manifest = campaign.load_campaign_manifest()
    case_id = manifest["run_order"]["case_order"][0]
    output = tmp_path / "root"
    output.mkdir()
    source_url = "postgres://bench:secret@127.0.0.1:5432/memphant_scratch_1_2"
    monkeypatch.setenv("MEMPHANT_SCRATCH_ACTIVE", "1")
    monkeypatch.setenv("MEMPHANT_TEST_DATABASE_URL", source_url)
    events = []
    construction = {
        "construction_proof_sha256": "c" * 64,
        "isolation": {"tenant_id": "tenant"},
        "pairing": {"resource_count": 2, "worker": {"completed_sources": 2}},
    }
    logical = {"tables": {}, "sha256": "e" * 64}

    def construct(*_args):
        events.append("construct")
        return construction

    def dump(_url, bank, proof, _contract, **_kwargs):
        events.append("dump")
        bank.mkdir(parents=True, exist_ok=True)
        archive_body = b"archive"
        digest = hashlib.sha256(archive_body).hexdigest()
        (bank / (digest + ".dump")).write_bytes(archive_body)
        campaign.atomic_write_json(bank / "construction-proof.json", proof)
        result = {
            "format_version": campaign.BANK_FORMAT_VERSION,
            "archive": digest + ".dump", "archive_sha256": digest,
            "logical_identity": logical,
            "construction_proof_sha256": "c" * 64,
            "case_contract_sha256": campaign.canonical_sha256(_contract),
        }
        campaign.atomic_write_json(bank / "manifest.json", result)
        return result

    def restore(_url, _bank, _contract, **_kwargs):
        events.append("restore")
        return {
            "logical_identity": logical,
            "archive": json.loads((tmp_path / "root/case-banks" / case_id / "manifest.json").read_text())["archive"],
            "archive_sha256": json.loads((tmp_path / "root/case-banks" / case_id / "manifest.json").read_text())["archive_sha256"],
        }

    def clone(_url, name, expected):
        events.append(("clone", name, expected["sha256"]))
        return source_url.rsplit("/", 1)[0] + "/" + name

    def execute(_directory, _materialized, root, row, _manifest, bank_seal):
        events.append((
            "execute", row["arm"],
            campaign.os.environ["MEMPHANT_LME_PREBUILT_PROOF"],
            campaign.os.environ["MEMPHANT_TEST_DATABASE_URL"],
        ))
        row_dir = root / row["row_id"]
        row_dir.mkdir()
        campaign.atomic_write_json(row_dir / "case-bank-seal.json", bank_seal)
        campaign.atomic_write_json(row_dir / "row-proof.json", {
            "complete": True, "row": row, "query_only": True,
            "case_bank_seal_sha256": bank_seal["seal_sha256"],
            "artifact_hashes": campaign.artifact_hashes(row_dir),
        })

    monkeypatch.setattr(campaign, "_recover_orphan_clones", lambda *_args: events.append("recover"))
    monkeypatch.setattr(campaign, "_case_archive_tools", lambda *_args: {
        "pg_dump": {"binary": "/pg_dump"}, "pg_restore": {"binary": "/pg_restore"},
    })
    monkeypatch.setattr(campaign, "_case_bank_contract", lambda *_args: {"question_id": case_id})
    monkeypatch.setattr(campaign, "_construct_case_source", construct)
    monkeypatch.setattr(campaign, "_dump_case_bank", dump)
    monkeypatch.setattr(campaign, "_reset_case_source", lambda _url: events.append("reset"))
    monkeypatch.setattr(campaign, "_restore_case_bank", restore)
    monkeypatch.setattr(campaign, "_clone_case_source", clone)
    monkeypatch.setattr(campaign, "_verify_case_bank_seal", lambda *_args: None)
    monkeypatch.setattr(campaign, "_execute_case_row", execute)
    monkeypatch.setattr(campaign, "_database_key_count", lambda url: 0 if url == source_url else 1)
    monkeypatch.setattr(campaign, "_drop_local_database", lambda url: events.append(("drop", url)))

    result = campaign._run_case(tmp_path, tmp_path, output, case_id, manifest)
    assert events[:4] == ["recover", "construct", "dump", "reset"]
    assert events[4] == "restore"
    clones = [event for event in events if isinstance(event, tuple) and event[0] == "clone"]
    assert [event[1].rsplit("_", 1)[-1] for event in clones] == ["fast", "sonnet"]
    assert clones[0][1] != clones[1][1]
    executes = [event for event in events if isinstance(event, tuple) and event[0] == "execute"]
    assert [event[1] for event in executes] == ["fast", "sonnet"]
    assert all(event[2].endswith("construction-proof.json") for event in executes)
    assert executes[0][3] != executes[1][3]
    assert len([event for event in events if isinstance(event, tuple) and event[0] == "drop"]) == 2
    assert result == {"case_id": case_id, "constructed": True, "completed_rows": 2}


def test_run_case_reuses_archive_after_interruption_and_drops_failed_clone(
    tmp_path: Path, monkeypatch
) -> None:
    campaign = _load()
    manifest = campaign.load_campaign_manifest()
    case_id = manifest["run_order"]["case_order"][0]
    output = tmp_path / "root"
    output.mkdir()
    rows = [row for row in campaign.expanded_run_order(manifest) if row["question_id"] == case_id]
    completed = output / rows[0]["row_id"]
    completed.mkdir()
    campaign.atomic_write_json(completed / "row-proof.json", {"complete": True, "row": rows[0]})
    source_url = "postgres://bench:secret@127.0.0.1:5432/memphant_scratch_1_2"
    monkeypatch.setenv("MEMPHANT_SCRATCH_ACTIVE", "1")
    monkeypatch.setenv("MEMPHANT_TEST_DATABASE_URL", source_url)
    bank = output / "case-banks" / case_id
    bank.mkdir(parents=True)
    archive_body = b"archive"
    archive_digest = hashlib.sha256(archive_body).hexdigest()
    archive = bank / (archive_digest + ".dump")
    archive.write_bytes(archive_body)
    construction = {"schema_version": 1}
    construction["construction_proof_sha256"] = campaign.canonical_sha256(
        construction
    )
    logical = {"tables": {}, "sequences": {}}
    logical["sha256"] = campaign.canonical_sha256({
        "tables": logical["tables"], "sequences": logical["sequences"],
    })
    case_contract = {"question_id": case_id}
    campaign.atomic_write_json(bank / "construction-proof.json", construction)
    campaign.atomic_write_json(bank / "manifest.json", {
        "format_version": campaign.BANK_FORMAT_VERSION,
        "archive": archive.name,
        "archive_sha256": campaign.sha256_file(archive),
        "excluded_tables": list(campaign.BANK_EXCLUDED_TABLES),
        "construction": construction,
        "construction_proof_sha256": construction["construction_proof_sha256"],
        "case_contract": case_contract,
        "case_contract_sha256": campaign.canonical_sha256(case_contract),
        "postgres": {"major": 18, "server_major": 18},
        "postgres_major": 18,
        "logical_identity": logical,
    })
    bank_seal = campaign._case_bank_seal(bank / "manifest.json")
    campaign.atomic_write_json(completed / "case-bank-seal.json", bank_seal)
    completed_proof = json.loads((completed / "row-proof.json").read_text())
    completed_proof.update({
        "case_bank_seal_sha256": bank_seal["seal_sha256"],
        "artifact_hashes": campaign.artifact_hashes(
            completed, exclude={"row-proof.json"}
        ),
    })
    campaign.atomic_write_json(completed / "row-proof.json", completed_proof)
    events = []
    monkeypatch.setattr(campaign, "_recover_orphan_clones", lambda *_args: events.append("recover"))
    monkeypatch.setattr(campaign, "_case_archive_tools", lambda *_args: {
        "pg_dump": {"binary": "/pg_dump"}, "pg_restore": {"binary": "/pg_restore"},
    })
    monkeypatch.setattr(campaign, "_case_bank_contract", lambda *_args: {"question_id": case_id})
    monkeypatch.setattr(campaign, "_construct_case_source", lambda *_args: pytest.fail("archive resume rebuilt construction"))
    monkeypatch.setattr(campaign, "_dump_case_bank", lambda *_args: pytest.fail("archive resume redumped construction"))
    monkeypatch.setattr(campaign, "_restore_case_bank", lambda *_args, **_kwargs: events.append("restore") or json.loads((bank / "manifest.json").read_text()))
    monkeypatch.setattr(campaign, "_clone_case_source", lambda _url, name, _identity: source_url.rsplit("/", 1)[0] + "/" + name)
    monkeypatch.setattr(campaign, "_verify_case_bank_seal", lambda *_args: None)
    monkeypatch.setattr(campaign, "_database_key_count", lambda url: 0 if url == source_url else 1)
    monkeypatch.setattr(campaign, "_drop_local_database", lambda url: events.append(("drop", url)))
    monkeypatch.setattr(campaign, "_execute_case_row", lambda *_args: (_ for _ in ()).throw(RuntimeError("synthetic row failure")))
    with pytest.raises(RuntimeError, match="synthetic row failure"):
        campaign._run_case(tmp_path, tmp_path, output, case_id, manifest)
    assert events[0:2] == ["recover", "restore"]
    assert len([event for event in events if isinstance(event, tuple) and event[0] == "drop"]) == 1


def test_run_campaign_uses_one_scratch_lifecycle_per_case(
    tmp_path: Path, monkeypatch
) -> None:
    campaign = _load()
    manifest = campaign.load_campaign_manifest()
    directory = tmp_path / "dataset"
    materialized = tmp_path / "materialized"
    output = tmp_path / "root"
    directory.mkdir()
    materialized.mkdir()
    monkeypatch.setenv("OPENROUTER_API_KEY", "router-secret")
    monkeypatch.setenv("OPENAI_API_KEY", "judge-secret")
    monkeypatch.setattr(campaign, "preflight", lambda *_args: {
        "materialization": {"cases": {
            case_id: {"case_id": case_id}
            for case_id in manifest["run_order"]["case_order"]
        }},
        "python": {"packages_sha256": "p" * 64},
    })
    monkeypatch.setattr(campaign, "verify_endpoint_inventory", lambda _manifest: {})
    monkeypatch.setattr(campaign, "_resolve_archive_tools", lambda _url: {
        "server_major": 17,
        "pg_dump": {"binary": "/pg_dump", "major": 17, "server_major": 17},
        "pg_restore": {"binary": "/pg_restore", "major": 17, "server_major": 17},
    })
    monkeypatch.setattr(
        campaign,
        "_fingerprint",
        lambda path: {"path": str(path), "bytes": 1, "sha256": "f" * 64},
    )
    run_calls = []

    def run(command, **_kwargs):
        run_calls.append(command)
        if command[:3] == ["git", "rev-parse", "HEAD"]:
            return campaign.subprocess.CompletedProcess(command, 0, "commit", "")
        return campaign.subprocess.CompletedProcess(command, 0, "", "")

    case_commands = []

    class Process:
        def __init__(self, command, **_kwargs):
            case_commands.append(command)
            self.pid = 4321

        def wait(self, timeout=None):
            return 0

    monkeypatch.setattr(campaign.subprocess, "run", run)
    monkeypatch.setattr(campaign.subprocess, "Popen", Process)
    result = campaign.run_campaign(
        directory, materialized, output,
        "postgres://bench:secret@127.0.0.1:5432/memphant", manifest,
    )
    assert result["rows"] == 24
    assert len(case_commands) == 12
    assert all("_run-case" in command and "_run-row" not in command for command in case_commands)
    assert [command[command.index("--case-id") + 1] for command in case_commands] == manifest["run_order"]["case_order"]


def test_archive_tools_resolve_matching_major_before_construction(monkeypatch) -> None:
    campaign = _load()
    source = "postgres://bench:secret@127.0.0.1:5432/memphant_scratch_1_2"
    monkeypatch.setattr(campaign, "_postgres_server_major", lambda _url: 17)
    monkeypatch.setattr(campaign, "_archive_tool_candidates", lambda name, major: [
        f"/usr/bin/{name}", f"/opt/homebrew/opt/postgresql@{major}/bin/{name}",
    ])

    def identity(binary, _url):
        major = 17 if "postgresql@17" in binary else 14
        return {"binary": binary, "version": f"PostgreSQL {major}", "major": major, "server_major": 17}

    monkeypatch.setattr(campaign, "_postgres_tool_identity", identity)
    tools = campaign._resolve_archive_tools(source)
    assert tools["server_major"] == 17
    assert tools["pg_dump"]["binary"] == "/opt/homebrew/opt/postgresql@17/bin/pg_dump"
    assert tools["pg_restore"]["binary"] == "/opt/homebrew/opt/postgresql@17/bin/pg_restore"


def test_case_lease_rejects_concurrent_resume_before_recovery(
    tmp_path: Path, monkeypatch
) -> None:
    campaign = _load()
    monkeypatch.setattr(
        campaign, "_run_case_locked",
        lambda *_args: pytest.fail("concurrent resume reached orphan recovery"),
    )
    with campaign._case_lease(tmp_path, "19367bc7"):
        with pytest.raises(RuntimeError, match="case is already active"):
            campaign._run_case(
                tmp_path, tmp_path, tmp_path, "19367bc7", {}
            )


def test_completed_fast_row_rejects_coherent_case_bank_rewrite(tmp_path: Path) -> None:
    campaign = _load()
    row = {"question_id": "19367bc7", "arm": "fast", "row_id": "0001-fast-19367bc7"}
    row_dir = tmp_path / row["row_id"]
    row_dir.mkdir()
    old_manifest = tmp_path / "old-manifest.json"
    campaign.atomic_write_json(old_manifest, {
        "archive_sha256": "a" * 64,
        "logical_identity": {"sha256": "e" * 64},
        "construction_proof_sha256": "c" * 64,
        "case_contract_sha256": "f" * 64,
    })
    old_seal = campaign._case_bank_seal(old_manifest)
    campaign.atomic_write_json(row_dir / "case-bank-seal.json", old_seal)
    campaign.atomic_write_json(row_dir / "row-proof.json", {
        "complete": True,
        "row": row,
        "case_bank_seal_sha256": old_seal["seal_sha256"],
        "artifact_hashes": campaign.artifact_hashes(row_dir),
    })
    replacement = tmp_path / "replacement-manifest.json"
    campaign.atomic_write_json(replacement, {
        "archive_sha256": "b" * 64,
        "logical_identity": {"sha256": "e" * 64},
        "construction_proof_sha256": "d" * 64,
        "case_contract_sha256": "f" * 64,
    })
    with pytest.raises(RuntimeError, match="case bank seal drift"):
        campaign._validate_completed_case_row(
            tmp_path, row, campaign._case_bank_seal(replacement)
        )


def test_execution_paths_are_absolute_before_official_cwd_changes(
    tmp_path: Path, monkeypatch
) -> None:
    campaign = _load()
    monkeypatch.chdir(tmp_path)
    directory, materialized, output = campaign._resolve_execution_paths(
        Path("official"), Path("materialized"), Path("artifacts")
    )
    assert directory == tmp_path / "official"
    assert materialized == tmp_path / "materialized"
    assert output == tmp_path / "artifacts"
    assert all(path.is_absolute() for path in (directory, materialized, output))


def test_campaign_packages_production_release_binaries() -> None:
    campaign = _load()
    assert campaign.PRODUCTION_BINARY_PROFILE == "release"
    assert campaign._production_build_command() == [
        "cargo", "build", "--release", "-p", "memphant-server",
        "-p", "memphant-worker", "-p", "memphant-cli",
    ]
    for name in ("server", "worker", "cli"):
        assert campaign._binary_path(name) == (
            campaign.ROOT / "target" / "release" / f"memphant-{name}"
        )
    with pytest.raises(RuntimeError, match="unknown packaged binary"):
        campaign._binary_path("debug-helper")


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


def test_context_preflight_contract_rejects_empty_or_exact_token_overflow() -> None:
    campaign = _load()
    public = {"trace": {"token_estimate": 30_000}}
    with pytest.raises(RuntimeError, match="non-empty memory context"):
        campaign._context_contract_audit([], public, 0, 32_768)
    context = [{"type": "text", "value": "bounded evidence"}]
    with pytest.raises(RuntimeError, match="exact reader token budget"):
        campaign._context_contract_audit(context, public, 32_769, 32_768)
    audit = campaign._context_contract_audit(context, public, 31_000, 32_768)
    assert audit == {
        "context_items": 1,
        "runtime_token_estimate": 30_000,
        "exact_reader_tokens": 31_000,
        "budget_tokens": 32_768,
        "nonempty": True,
        "untruncated": True,
    }


def test_reader_route_probe_request_is_tiny_reasoning_enabled_and_pinned() -> None:
    campaign = _load()
    request = campaign._reader_route_probe_request()
    assert request == {
        "model": "Qwen/Qwen3.5-9B",
        "messages": [{
            "role": "user",
            "content": "Reply with exactly ROUTE_OK after reasoning internally.",
        }],
        "max_tokens": 64,
        "reasoning": {"enabled": True},
        "temperature": 0,
    }


def test_context_preflight_streams_only_selected_trajectories(tmp_path: Path) -> None:
    campaign = _load()
    source = tmp_path / "trajectories.jsonl"
    source.write_text(
        '\n'.join(json.dumps({"id": value, "payload": value * 10}, separators=(",", ":"))
                  for value in ("ignored", "wanted-b", "wanted-a")) + '\n'
    )
    selected = campaign._load_selected_trajectories(
        source, ["wanted-a", "wanted-b"]
    )
    assert set(selected) == {"wanted-a", "wanted-b"}
    assert selected["wanted-a"]["payload"] == "wanted-a" * 10
    with pytest.raises(RuntimeError, match="contains duplicates"):
        campaign._load_selected_trajectories(source, ["wanted-a", "wanted-a"])
    with pytest.raises(RuntimeError, match="are incomplete"):
        campaign._load_selected_trajectories(source, ["missing"])


def test_temporary_adapter_environment_restores_existing_and_missing_values(
    monkeypatch,
) -> None:
    campaign = _load()
    monkeypatch.setenv("MEMPHANT_TEST_EXISTING", "before")
    monkeypatch.delenv("MEMPHANT_TEST_MISSING", raising=False)
    with campaign._temporary_environment({
        "MEMPHANT_TEST_EXISTING": "during",
        "MEMPHANT_TEST_MISSING": "temporary",
    }):
        assert campaign.os.environ["MEMPHANT_TEST_EXISTING"] == "during"
        assert campaign.os.environ["MEMPHANT_TEST_MISSING"] == "temporary"
    assert campaign.os.environ["MEMPHANT_TEST_EXISTING"] == "before"
    assert "MEMPHANT_TEST_MISSING" not in campaign.os.environ


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


def test_trajectory_fragmentation_losslessly_bounds_oversized_single_lines(monkeypatch) -> None:
    adapter = _load_memory_adapter(monkeypatch)
    trajectory = {
        "id": "t-long", "goal": "find outlook", "outcome": None,
        "states": [{"url": "https://one", "text": "Outlook," * 200}],
    }
    body = adapter._state_body(trajectory, trajectory["states"][0], 0)
    fragments = adapter._trajectory_fragments(trajectory, 128)
    assert len(fragments) > 1
    assert all(len(fragment.encode()) <= 128 for fragment in fragments)
    assert "".join(fragments) == body


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
    manifest["campaign_spend"]["deep_max_liability_usd"] = 10.9
    with pytest.raises(RuntimeError, match="Deep campaign reserve drift"):
        campaign.verify_campaign_manifest(manifest)


def test_material_endpoint_predicate_ignores_additive_inventory_drift() -> None:
    campaign = _load()
    contract = {
        "name": "Azure | exact-model-20260709", "model_id": "exact-model",
        "provider_name": "Azure", "min_context_length": 100000,
        "min_completion_tokens": 4096,
        "required_parameters": ["tools", "tool_choice", "max_completion_tokens"],
        "prompt_price_micros_per_million_max": 2_000_000,
        "completion_price_micros_per_million_max": 10_000_000,
    }
    endpoint = {
        "name": contract["name"], "model_id": contract["model_id"],
        "provider_name": "Azure", "tag": "new-region", "quantization": "unknown",
        "context_length": 1_000_000, "max_completion_tokens": 128_000,
        "max_prompt_tokens": None,
        "supported_parameters": ["tools", "tool_choice", "max_completion_tokens", "new_parameter"],
        "pricing": {"prompt": "0.000002", "completion": "0.00001"},
        "name_not_in_contract": "additive metadata is harmless",
    }
    assert campaign._matching_endpoints([endpoint], contract) == [endpoint]
    endpoint["pricing"]["completion"] = "0.000010000001"
    assert campaign._matching_endpoints([endpoint], contract) == []


def test_resume_keeps_initial_inventory_evidence_when_material_contract_is_stable() -> None:
    campaign = _load()
    common = {
        "manifest_sha256": "a", "run_order_sha256": "b",
        "outputs_observed_before_freeze": False, "materialization": {"c": "d"},
        "git_commit": "e", "binaries": {"f": "g"}, "deep_prompt_sha256": "h",
        "deep_config_hashes": {"sonnet": "i"},
        "python_environment": {"packages_sha256": "p"},
        "environment_contract_sha256": "j",
        "binary_profile": "release",
        "archive_tools": {"server_major": 17},
        "preexisting_campaign_liability": {"total_micros": 320666},
    }
    frozen = {**common, "endpoint_hashes": {
        "reader": {"inventory_sha256": "old", "material_contract_sha256": "stable"}
    }}
    current = {**common, "endpoint_hashes": {
        "reader": {"inventory_sha256": "new", "material_contract_sha256": "stable"}
    }}
    campaign.verify_resume_contract(frozen, current)
    current["endpoint_hashes"]["reader"]["material_contract_sha256"] = "drift"
    with pytest.raises(RuntimeError, match="material endpoint contract drift"):
        campaign.verify_resume_contract(frozen, current)


def test_decimal_cost_ceiling_never_rounds_liability_down() -> None:
    campaign = _load()
    assert campaign.usd_to_micros("0.0000001") == 1
    assert campaign.usd_to_micros("0.001234000001") == 1235
    assert campaign.token_price_to_micros_per_million("0.00000015") == 150000


def test_fresh_reservations_plus_prior_attempts_stay_below_campaign_ceiling() -> None:
    campaign = _load()
    manifest = campaign.load_campaign_manifest()
    reservations = [
        campaign._reservation(row, manifest)
        for row in campaign.expanded_run_order(manifest)
    ]
    fresh = sum(item["max_liability_micros"] for item in reservations)
    prior = manifest["campaign_spend"]["preexisting_liability"]
    assert fresh == 5_697_600
    assert prior == {
        "settled_micros": 7_542,
        "unsettled_upper_bound_micros": 316_142,
        "total_micros": 323_684,
        "proofs": prior["proofs"],
    }
    assert fresh + prior["total_micros"] == 6_021_284
    assert campaign.usd_to_micros(
        manifest["campaign_spend"]["hard_ceiling_usd"]
    ) - fresh - prior["total_micros"] == 228_716


def test_settled_proxy_cost_must_fit_its_pre_dispatch_reservation() -> None:
    campaign = _load()
    assert campaign._audit_cost({
        "audit_status": "settled",
        "max_liability_micros": 19,
        "total_cost": 0.0000116,
    }) == (12, 0)
    with pytest.raises(RuntimeError, match="exceeds its reservation"):
        campaign._audit_cost({
            "audit_status": "settled",
            "max_liability_micros": 11,
            "total_cost": 0.0000116,
        })


def test_reader_policy_enforces_frozen_bf16_and_price_caps_before_dispatch() -> None:
    campaign = _load()
    reader = campaign.load_campaign_manifest()["protocol"]["reader"]
    assert reader["provider_policy"] == {
        "only": ["deepinfra"],
        "allow_fallbacks": False,
        "require_parameters": True,
        "data_collection": "deny",
        "zdr": True,
        "quantizations": ["bf16"],
        "max_price": {"prompt": 0.1, "completion": 0.15},
    }


def test_clean_child_environment_drops_ambient_secrets_and_deep_overrides(
    monkeypatch,
) -> None:
    campaign = _load()
    monkeypatch.setenv("AWS_SECRET_ACCESS_KEY", "must-not-cross")
    monkeypatch.setenv("UNRELATED_VENDOR_TOKEN", "must-not-cross")
    monkeypatch.setenv("MEMPHANT_DEEP_OPENROUTER_BASE_URL", "https://wrong.test/v1")
    monkeypatch.setenv("MEMPHANT_DEEP_MODEL", "wrong/model")
    monkeypatch.setenv("PATH", "/safe/bin")
    child = campaign._clean_environment({"EXPLICIT_VALUE": "allowed"})
    assert child["PATH"] == "/safe/bin"
    assert child["EXPLICIT_VALUE"] == "allowed"
    assert "AWS_SECRET_ACCESS_KEY" not in child
    assert "UNRELATED_VENDOR_TOKEN" not in child
    assert not any(key.startswith("MEMPHANT_DEEP") for key in child)


def test_python_harness_preflight_fails_closed_under_clean_environment(
    tmp_path: Path, monkeypatch
) -> None:
    campaign = _load()
    official = tmp_path / "official"
    official.mkdir()
    (official / "requirements.txt").write_text("openai-agents\n")
    monkeypatch.setenv("OPENROUTER_API_KEY", "must-not-cross")
    monkeypatch.setattr(
        campaign,
        "_fingerprint",
        lambda path: {"path": str(path), "bytes": 1, "sha256": "f" * 64},
    )
    calls = []

    def run(command, **kwargs):
        calls.append((command, kwargs))
        if command[2:4] == ["pip", "check"]:
            return campaign.subprocess.CompletedProcess(command, 0, "No broken requirements found.\n", "")
        if command[2:5] == ["pip", "freeze", "--all"]:
            return campaign.subprocess.CompletedProcess(
                command,
                0,
                "openai-agents==0.18.3\ntorch==2.13.0\ntorchvision==0.28.0\n",
                "",
            )
        return campaign.subprocess.CompletedProcess(
            command, 1, "", "ModuleNotFoundError: No module named 'agents'\n"
        )

    monkeypatch.setattr(campaign.subprocess, "run", run)
    with pytest.raises(RuntimeError, match="official harness bootstrap import failed"):
        campaign.verify_python_harness(tmp_path)
    assert calls
    for _command, kwargs in calls:
        assert "OPENROUTER_API_KEY" not in kwargs["env"]


def test_python_harness_preflight_freezes_interpreter_and_packages(
    tmp_path: Path, monkeypatch
) -> None:
    campaign = _load()
    official = tmp_path / "official"
    official.mkdir()
    (official / "requirements.txt").write_text("openai-agents\n")
    monkeypatch.setattr(
        campaign,
        "_fingerprint",
        lambda path: {"path": str(path), "bytes": 1, "sha256": "f" * 64},
    )

    def run(command, **_kwargs):
        if command[2:4] == ["pip", "check"]:
            return campaign.subprocess.CompletedProcess(command, 0, "No broken requirements found.\n", "")
        if command[2:5] == ["pip", "freeze", "--all"]:
            return campaign.subprocess.CompletedProcess(
                command,
                0,
                "openai==2.46.0\nopenai-agents==0.18.3\n"
                "torch==2.13.0\ntorchvision==0.28.0\n",
                "",
            )
        return campaign.subprocess.CompletedProcess(command, 0, "usage: harness\n", "warning\n")

    monkeypatch.setattr(campaign.subprocess, "run", run)
    proof = campaign.verify_python_harness(tmp_path)
    assert proof["requirements_sha256"] == campaign.sha256_file(
        official / "requirements.txt"
    )
    assert proof["packages"] == [
        "openai-agents==0.18.3",
        "openai==2.46.0",
        "torch==2.13.0",
        "torchvision==0.28.0",
    ]
    assert proof["packages_sha256"] == campaign.canonical_sha256(proof["packages"])
    assert proof["bootstrap_import_verified"] is True


def test_python_harness_preflight_executes_real_qwen_processor_path(
    tmp_path: Path, monkeypatch
) -> None:
    campaign = _load()
    official = tmp_path / "official"
    official.mkdir()
    (official / "requirements.txt").write_text("transformers\n")
    campaign_requirements = tmp_path / "requirements-p1-t6.txt"
    campaign_requirements.write_text("torch==2.13.0\ntorchvision==0.28.0\n")
    processor_preflight = tmp_path / "processor_preflight.py"
    processor_preflight.write_text("raise SystemExit(0)\n")
    monkeypatch.setattr(campaign, "CAMPAIGN_PYTHON_REQUIREMENTS", campaign_requirements)
    monkeypatch.setattr(campaign, "PROCESSOR_PREFLIGHT", processor_preflight)
    monkeypatch.setattr(
        campaign,
        "_fingerprint",
        lambda path: {"path": str(path), "bytes": 1, "sha256": "f" * 64},
    )
    calls = []

    def run(command, **_kwargs):
        calls.append(command)
        if command[2:4] == ["pip", "check"]:
            return campaign.subprocess.CompletedProcess(command, 0, "No broken requirements found.\n", "")
        if command[2:5] == ["pip", "freeze", "--all"]:
            return campaign.subprocess.CompletedProcess(
                command,
                0,
                "torch==2.13.0\ntorchvision==0.28.0\ntransformers==5.14.1\n",
                "",
            )
        return campaign.subprocess.CompletedProcess(command, 0, "processor-ready\n", "")

    monkeypatch.setattr(campaign.subprocess, "run", run)
    proof = campaign.verify_python_harness(tmp_path)
    assert [
        campaign.sys.executable,
        str(processor_preflight),
        "--official-dir",
        str(official),
    ] in calls
    assert proof["campaign_requirements_sha256"] == campaign.sha256_file(
        campaign_requirements
    )
    assert proof["processor_preflight_verified"] is True


def test_python_harness_preflight_rejects_missing_campaign_dependency(
    tmp_path: Path, monkeypatch
) -> None:
    campaign = _load()
    official = tmp_path / "official"
    official.mkdir()
    (official / "requirements.txt").write_text("transformers\n")
    campaign_requirements = tmp_path / "requirements-p1-t6.txt"
    campaign_requirements.write_text("torch==2.13.0\ntorchvision==0.28.0\n")
    monkeypatch.setattr(campaign, "CAMPAIGN_PYTHON_REQUIREMENTS", campaign_requirements)
    monkeypatch.setattr(
        campaign,
        "_fingerprint",
        lambda path: {"path": str(path), "bytes": 1, "sha256": "f" * 64},
    )

    def run(command, **_kwargs):
        if command[2:4] == ["pip", "check"]:
            return campaign.subprocess.CompletedProcess(command, 0, "", "")
        if command[2:5] == ["pip", "freeze", "--all"]:
            return campaign.subprocess.CompletedProcess(command, 0, "transformers==5.14.1\n", "")
        return campaign.subprocess.CompletedProcess(command, 0, "", "")

    monkeypatch.setattr(campaign.subprocess, "run", run)
    with pytest.raises(
        RuntimeError,
        match="campaign Python dependency missing or drifted: torch==2.13.0",
    ):
        campaign.verify_python_harness(tmp_path)


def test_processor_preflight_executes_official_token_counter(tmp_path: Path) -> None:
    official = tmp_path / "official"
    evaluation = official / "evaluation"
    evaluation.mkdir(parents=True)
    (evaluation / "__init__.py").write_text("")
    (evaluation / "harness.py").write_text(
        "def count_memory_context_tokens(memory_context, loaded_images):\n"
        "    assert memory_context == "
        "[{'type': 'text', 'value': 'MemPhant processor preflight'}]\n"
        "    assert loaded_images == [None]\n"
        "    return 7\n"
    )
    completed = subprocess.run(
        [
            sys.executable,
            str(ROOT / "benchmarks/longmemeval_v2/processor_preflight.py"),
            "--official-dir",
            str(official),
        ],
        capture_output=True,
        text=True,
        check=False,
    )
    assert completed.returncode == 0, completed.stderr
    assert json.loads(completed.stdout) == {
        "memory_context_tokens": 7,
        "processor_preflight": "passed",
    }


def test_secret_redaction_covers_nested_text_and_binary_artifacts(tmp_path: Path) -> None:
    campaign = _load()
    nested = tmp_path / "nested"
    nested.mkdir()
    (tmp_path / "stdout.log").write_text("prefix live-key suffix")
    (nested / "response.bin").write_bytes(b"before\x00live-key\x00after")
    campaign._redact_secrets(tmp_path, ["live-key"])
    assert "live-key" not in (tmp_path / "stdout.log").read_text()
    assert b"live-key" not in (nested / "response.bin").read_bytes()


def test_row_secret_values_redact_scratch_dsn_and_password_variants(tmp_path: Path) -> None:
    campaign = _load()
    database_url = "postgres://bench:sentinel%2Fpassword@db.test:5432/scratch"
    artifact = tmp_path / "server.stderr"
    artifact.write_text(
        f"dsn={database_url} password=sentinel/password "
        "authority=bench:sentinel%2Fpassword@db.test:5432"
    )
    campaign._redact_secrets(
        tmp_path,
        campaign._row_secret_values("router-key", "judge-key", database_url),
    )
    redacted = artifact.read_text()
    assert "sentinel/password" not in redacted
    assert "sentinel%2Fpassword" not in redacted
    assert database_url not in redacted


def test_forced_server_cleanup_reaps_child_before_artifact_redaction() -> None:
    campaign = _load()

    class Process:
        def __init__(self):
            self.events = []

        def terminate(self):
            self.events.append("terminate")

        def wait(self, timeout=None):
            self.events.append(("wait", timeout))
            if timeout is not None:
                raise campaign.subprocess.TimeoutExpired("server", timeout)
            return -9

        def kill(self):
            self.events.append("kill")

    process = Process()
    campaign._terminate_and_reap(process)
    assert process.events == [
        "terminate", ("wait", 10), "kill", ("wait", None),
    ]


def test_campaign_interrupt_terminates_and_reaps_scratch_process_group(
    monkeypatch,
) -> None:
    campaign = _load()
    signals = []
    monkeypatch.setattr(campaign.os, "killpg", lambda pid, signal: signals.append((pid, signal)))

    class Process:
        def __init__(self):
            self.events = []
            self.first_wait = True
            self.pid = 4321

        def wait(self, timeout=None):
            self.events.append(("wait", timeout))
            if self.first_wait:
                self.first_wait = False
                raise KeyboardInterrupt
            return -15

    process = Process()
    with pytest.raises(KeyboardInterrupt):
        campaign._wait_and_reap_on_interrupt(process)
    assert process.events == [("wait", None), ("wait", 10)]
    assert signals == [(4321, campaign.signal.SIGTERM)]


def test_official_harness_output_is_archived_per_row(tmp_path: Path) -> None:
    campaign = _load()
    completed = campaign._run_logged_harness(
        [
            sys.executable,
            "-c",
            "import sys; print('official-out'); print('official-err', file=sys.stderr)",
        ],
        cwd=tmp_path,
        environment=campaign._clean_environment(),
        row_dir=tmp_path,
    )
    assert completed.returncode == 0
    assert (tmp_path / "official.stdout").read_text() == "official-out\n"
    assert (tmp_path / "official.stderr").read_text() == "official-err\n"


def test_deep_receipts_must_exactly_reconcile_ids_route_tokens_and_cost(
    tmp_path: Path,
) -> None:
    campaign = _load()
    manifest = campaign.load_campaign_manifest()
    row = next(
        item for item in campaign.expanded_run_order(manifest) if item["arm"] == "sonnet"
    )
    reservation = campaign._reservation(row, manifest)
    (tmp_path / "memory-proofs").mkdir()
    candidate = manifest["protocol"]["deep_candidates"]["sonnet"]
    deep = {
        "generation_ids": ["gen-1"],
        "usage": {
            "context_tokens": 10,
            "spend_micros": 1_000,
            "unsettled_context_tokens_upper_bound": 0,
            "unsettled_spend_micros_upper_bound": 0,
        },
    }
    campaign.atomic_write_json(
        tmp_path / "memory-proofs/proof.json",
        {"public": {"recall_response": {"deep": deep}}},
    )
    receipt = {
        "audit_status": "settled",
        "generation_ids": ["gen-1"],
        "receipts": [{
            "id": "gen-1",
            "provider_name": "Azure",
            "model": candidate["model"],
            "tokens_prompt": 10,
            "tokens_completion": 2,
            "total_cost_micros": 1_000,
        }],
    }
    campaign.atomic_write_json(tmp_path / "deep-generation-receipts.json", receipt)
    settlement = campaign._row_settlement(
        tmp_path, row, reservation, orphaned=False
    )
    assert settlement["deep_settled_micros"] == 1_000
    assert settlement["deep_unsettled_upper_bound_micros"] == 0

    receipt["receipts"][0]["total_cost_micros"] = 999
    campaign.atomic_write_json(tmp_path / "deep-generation-receipts.json", receipt)
    settlement = campaign._row_settlement(
        tmp_path, row, reservation, orphaned=False
    )
    assert settlement["deep_settled_micros"] == 0
    assert settlement["deep_unsettled_upper_bound_micros"] == reservation[
        "deep_hard_cap_micros"
    ]


def test_manifest_binds_all_candidate_metadata_to_runtime_config_hashes() -> None:
    campaign = _load()
    manifest = campaign.load_campaign_manifest()
    protocol = manifest["protocol"]
    assert protocol["selected_deep_arm"] == "sonnet"
    assert {
        name: campaign._expected_deep_config_hash(candidate)
        for name, candidate in protocol["deep_candidates"].items()
    } == {
        name: candidate["config_sha256"]
        for name, candidate in protocol["deep_candidates"].items()
    }
    protocol["deep_candidates"]["luna"]["config_sha256"] = "0" * 64
    with pytest.raises(RuntimeError, match="Deep runtime config hash drift: luna"):
        campaign.verify_campaign_manifest(manifest)


def test_deep_receipt_archive_is_sanitized_and_exact(tmp_path: Path, monkeypatch) -> None:
    campaign = _load()
    manifest = campaign.load_campaign_manifest()
    row = next(
        item for item in campaign.expanded_run_order(manifest) if item["arm"] == "sonnet"
    )
    candidate = manifest["protocol"]["deep_candidates"]["sonnet"]
    (tmp_path / "memory-proofs").mkdir()
    campaign.atomic_write_json(tmp_path / "memory-proofs/proof.json", {
        "public": {"recall_response": {"deep": {
            "generation_ids": ["gen-1"],
            "usage": {
                "context_tokens": 20,
                "spend_micros": 1_235,
                "unsettled_context_tokens_upper_bound": 0,
                "unsettled_spend_micros_upper_bound": 0,
            },
        }}},
    })
    monkeypatch.setattr(campaign, "_json_url", lambda *_args: {"data": {
        "id": "gen-1",
        "provider_name": "Azure",
        "model": candidate["model"],
        "tokens_prompt": 20,
        "tokens_completion": 3,
        "total_cost": "0.001234000001",
        "prompt": "must not be archived",
        "upstream_secret": "must not be archived",
    }})
    campaign._archive_deep_generation_receipts(
        tmp_path, row, manifest, "secret-key"
    )
    receipt = json.loads((tmp_path / "deep-generation-receipts.json").read_text())
    assert receipt["audit_status"] == "settled"
    assert receipt["receipts"] == [{
        "id": "gen-1",
        "provider_name": "Azure",
        "model": candidate["model"],
        "tokens_prompt": 20,
        "tokens_completion": 3,
        "total_cost_micros": 1_235,
    }]
    archived = json.dumps(receipt)
    assert "must not be archived" not in archived
    assert "upstream_secret" not in archived
    assert "secret-key" not in archived


def test_synthetic_all_failure_aggregate_is_complete_and_zero_scored(tmp_path: Path) -> None:
    campaign = _load()
    manifest = campaign.load_campaign_manifest()
    rows = campaign.expanded_run_order(manifest)
    _write_synthetic_root(campaign, tmp_path, manifest)
    ledger = tmp_path / "spend-ledger"
    ledger.mkdir()
    for row in rows:
        reservation_path = ledger / f"{row['sequence']:04d}.json"
        campaign.atomic_write_json(reservation_path, campaign._reservation(row, manifest))
        row_dir = tmp_path / row["row_id"]
        row_dir.mkdir()
        campaign.atomic_write_json(row_dir / "failure.json", {"reason": "synthetic"})
        campaign._write_row_proof(
            row_dir, row, reservation_path, "operational_failure",
            {"failure_reason": "synthetic"}, orphaned=True,
        )
    _write_synthetic_case_banks(campaign, tmp_path, rows)
    aggregate = campaign.aggregate_campaign(tmp_path, manifest)
    assert aggregate["decision"] == "retire_deep_product_code"
    assert aggregate["advance_to_separate_confirmation"] == []
    assert set(aggregate["candidates"]) == {"sonnet"}
    assert all(not candidate["feasible"] for candidate in aggregate["candidates"].values())
    assert all(
        pair["deep_score"] == 0.0
        for candidate in aggregate["candidates"].values()
        for pair in candidate["pairs"]
    )


def test_synthetic_success_aggregate_applies_registered_ranking(tmp_path: Path) -> None:
    campaign = _load()
    manifest = campaign.load_campaign_manifest()
    rows = campaign.expanded_run_order(manifest)
    _write_synthetic_root(campaign, tmp_path, manifest)
    ledger = tmp_path / "spend-ledger"
    ledger.mkdir()
    for row in rows:
        reservation_path = ledger / f"{row['sequence']:04d}.json"
        campaign.atomic_write_json(reservation_path, campaign._reservation(row, manifest))
        row_dir = tmp_path / row["row_id"]
        (row_dir / "memory-proofs").mkdir(parents=True)
        deep = None
        trace = {"id": "trace", "deep": None}
        if row["arm"] != "fast":
            candidate = manifest["protocol"]["deep_candidates"][row["arm"]]
            deep = {
                "status": "completed", "stop_reason": "completed",
                "generation_ids": [f"generation-{row['row_id']}"],
                "usage": {"context_tokens": 10, "spend_micros": 1000,
                          "unsettled_spend_micros_upper_bound": 0,
                          "unsettled_context_tokens_upper_bound": 0},
            }
            trace.update({
                "deep": deep, "l4_model": candidate["model"], "l4_provider": "azure",
                "l4_observed_provider": "Azure", "l4_observed_model": candidate["model"],
                "l4_prompt_hash": manifest["protocol"]["deep_prompt_sha256"],
                "l4_config_hash": candidate["config_sha256"],
            })
        memory = {
            "public": {"recall_response": {"trace_id": "trace", "deep": deep}, "trace": trace},
            "recall_mutation_proof": {"corpus_policy_job_tables_unchanged": True},
            "query": {"recall_duration_ms": 1000},
        }
        memory_path = row_dir / "memory-proofs/proof.json"
        campaign.atomic_write_json(memory_path, memory)
        if deep is not None:
            campaign.atomic_write_json(row_dir / "deep-generation-receipts.json", {
                "audit_status": "settled",
                "generation_ids": deep["generation_ids"],
                "receipts": [{
                    "id": deep["generation_ids"][0],
                    "provider_name": "Azure",
                    "model": candidate["model"],
                    "tokens_prompt": 10,
                    "tokens_completion": 2,
                    "total_cost_micros": 1000,
                }],
            })
        campaign.atomic_write_json(row_dir / "reader-route.json", {
            "audit_status": "settled", "max_liability_micros": 5000,
            "total_cost": "0.001", "provider_name": "DeepInfra",
            "model": "qwen/qwen3.5-9b",
            "provider_policy_sha256": campaign.canonical_sha256(
                manifest["protocol"]["reader"]["provider_policy"]
            ),
        })
        (row_dir / "judge-routes").mkdir()
        (row_dir / "official").mkdir()
        score_path = row_dir / "official/per_question.jsonl"
        score_path.write_text(json.dumps({
            "question_id": row["question_id"], "eval_function": "mc_choice_match",
            "score": 0.0 if row["arm"] == "fast" else 1.0,
            "memory_context_was_truncated": False,
        }) + "\n")
        campaign._write_row_proof(row_dir, row, reservation_path, "success", {
            "execution_complete": True, "treatment_operational": True,
            "binaries": json.loads((tmp_path / "pre-execution-proof.json").read_text())["binaries"],
            "memory_proof_sha256": campaign.sha256_file(memory_path),
            "reader_route_sha256": campaign.sha256_file(row_dir / "reader-route.json"),
            "judge_route_sha256": campaign.canonical_sha256([]),
            "official_score_sha256": campaign.sha256_file(score_path),
        })
    _write_synthetic_case_banks(campaign, tmp_path, rows)
    aggregate = campaign.aggregate_campaign(tmp_path, manifest)
    assert all(candidate["feasible"] for candidate in aggregate["candidates"].values())
    assert all(candidate["predicates"]["no_context_truncation"]
               for candidate in aggregate["candidates"].values())
    assert set(aggregate["candidates"]) == {"sonnet"}
    assert aggregate["advance_to_separate_confirmation"] == ["sonnet"]
    assert aggregate["decision"] == "confirmation_manifest_required"


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


def test_reader_returns_accepted_generation_before_async_receipt_reconciliation(
    tmp_path: Path, monkeypatch
) -> None:
    campaign = _load()
    original = b'{"id":"gen-1","model":"qwen/qwen3.5-9b","choices":[]}'
    calls = []

    class Opener:
        def open(self, request, timeout=None):
            calls.append((timeout, json.loads(request.data)))
            return _FakeResponse(original)

    monkeypatch.setattr(campaign.urllib.request, "build_opener", lambda *_args: Opener())
    monkeypatch.setattr(
        campaign,
        "_json_url",
        lambda *_args: (_ for _ in ()).throw(AssertionError("receipt lookup ran on response path")),
    )
    manifest = campaign.load_campaign_manifest()
    server, base = campaign._reader_proxy("secret", tmp_path / "reader.json", manifest)
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
        connection.request(
            "POST", "/chat/completions",
            body=json.dumps({"model": "Qwen/Qwen3.5-9B", "messages": []}),
            headers={"content-type": "application/json"},
        )
        retry = connection.getresponse()
        assert retry.status == 422
        retry.read()
        connection.close()
    finally:
        server.shutdown()
        server.server_close()
    assert len(calls) == 1
    assert calls[0][0] == 600
    assert calls[0][1]["provider"] == manifest["protocol"]["reader"]["provider_policy"]
    assert json.loads((tmp_path / "reader.json").read_text())["audit_status"] == "receipt_pending"


def test_reader_receipt_reconciliation_waits_for_complete_async_stats(
    tmp_path: Path, monkeypatch
) -> None:
    campaign = _load()
    manifest = campaign.load_campaign_manifest()
    audit_path = tmp_path / "reader.json"
    campaign.atomic_write_json(audit_path, {
        "audit_status": "receipt_pending",
        "dispatch_count": 1,
        "generation_id": "gen-1",
        "max_liability_micros": 3084,
    })
    receipts = iter([
        {"data": {
            "provider_name": "DeepInfra", "model": "qwen/qwen3.5-9b-20260310",
            "tokens_prompt": None, "tokens_completion": None, "total_cost": None,
        }},
        {"data": {
            "provider_name": "DeepInfra", "model": "qwen/qwen3.5-9b-20260310",
            "tokens_prompt": 181, "tokens_completion": 5533, "total_cost": 0.000816,
        }},
    ])
    sleeps = []
    monkeypatch.setattr(campaign, "_json_url", lambda *_args: next(receipts))
    monkeypatch.setattr(campaign.time, "sleep", sleeps.append)
    reconciled = campaign._reconcile_reader_receipt(
        "secret", audit_path, manifest, attempts=3, delay_seconds=2
    )
    assert reconciled["audit_status"] == "settled"
    assert reconciled["provider_name"] == "DeepInfra"
    assert reconciled["model"] == "qwen/qwen3.5-9b-20260310"
    assert reconciled["tokens_prompt"] == 181
    assert reconciled["tokens_completion"] == 5533
    assert reconciled["total_cost"] == 0.000816
    assert sleeps == [2]
    assert json.loads(audit_path.read_text()) == reconciled


def test_reader_proxy_archives_upstream_rejection_without_hiding_status(
    tmp_path: Path, monkeypatch
) -> None:
    campaign = _load()
    rejected = b'{"error":{"message":"No endpoints satisfy the request policy","code":404}}'

    class Opener:
        def open(self, request, timeout=None):
            raise campaign.urllib.error.HTTPError(
                request.full_url,
                404,
                "Not Found",
                {},
                io.BytesIO(rejected),
            )

    monkeypatch.setattr(campaign.urllib.request, "build_opener", lambda *_args: Opener())
    manifest = campaign.load_campaign_manifest()
    server, base = campaign._reader_proxy("secret", tmp_path / "reader.json", manifest)
    try:
        connection = http.client.HTTPConnection(base.removeprefix("http://"))
        connection.request(
            "POST",
            "/chat/completions",
            body=json.dumps({"model": "Qwen/Qwen3.5-9B", "messages": []}),
            headers={"content-type": "application/json"},
        )
        response = connection.getresponse()
        assert response.status == 404
        assert response.read() == rejected
        connection.close()
    finally:
        server.shutdown()
        server.server_close()
    audit = json.loads((tmp_path / "reader.json").read_text())
    assert audit["audit_status"] == "rejected"
    assert audit["upstream_status"] == 404
    assert audit["upstream_error"] == {
        "message": "No endpoints satisfy the request policy",
        "code": 404,
    }
    assert audit["response_sha256"] == campaign.hashlib.sha256(rejected).hexdigest()


def test_reader_proxy_retries_explicit_pre_generation_429_with_bounded_backoff(
    tmp_path: Path, monkeypatch
) -> None:
    campaign = _load()
    rejected = b'{"error":{"message":"Provider returned error","code":429}}'
    accepted = b'{"id":"gen-1","model":"qwen/qwen3.5-9b","choices":[]}'
    calls = []
    sleeps = []

    class Opener:
        def open(self, request, timeout=None):
            calls.append(timeout)
            if len(calls) == 1:
                raise campaign.urllib.error.HTTPError(
                    request.full_url,
                    429,
                    "Too Many Requests",
                    {"Retry-After": "2"},
                    io.BytesIO(rejected),
                )
            return _FakeResponse(accepted)

    monkeypatch.setattr(campaign.urllib.request, "build_opener", lambda *_args: Opener())
    monkeypatch.setattr(campaign.time, "sleep", sleeps.append)
    server, base = campaign._reader_proxy(
        "secret", tmp_path / "reader.json", campaign.load_campaign_manifest()
    )
    try:
        connection = http.client.HTTPConnection(base.removeprefix("http://"))
        connection.request(
            "POST", "/chat/completions",
            body=json.dumps({"model": "Qwen/Qwen3.5-9B", "messages": []}),
            headers={"content-type": "application/json"},
        )
        response = connection.getresponse()
        assert response.status == 200
        assert response.read() == accepted
        connection.close()
    finally:
        server.shutdown()
        server.server_close()
    audit = json.loads((tmp_path / "reader.json").read_text())
    assert calls == [600, 600]
    assert sleeps == [2]
    assert audit["dispatch_count"] == 2
    assert audit["audit_status"] == "receipt_pending"
    assert audit["generation_id"] == "gen-1"
    assert audit["pre_generation_rejections"] == [{
        "attempt": 1,
        "generation_id": None,
        "response_sha256": campaign.hashlib.sha256(rejected).hexdigest(),
        "retry_after_seconds": 2,
        "status": 429,
    }]


def test_reader_proxy_never_retries_rejection_with_generation_id(
    tmp_path: Path, monkeypatch
) -> None:
    campaign = _load()
    rejected = b'{"error":{"message":"Provider returned error","code":429}}'
    calls = []

    class Opener:
        def open(self, request, timeout=None):
            calls.append(timeout)
            raise campaign.urllib.error.HTTPError(
                request.full_url,
                429,
                "Too Many Requests",
                {"Retry-After": "2", "X-Generation-Id": "gen-possibly-billed"},
                io.BytesIO(rejected),
            )

    monkeypatch.setattr(campaign.urllib.request, "build_opener", lambda *_args: Opener())
    monkeypatch.setattr(
        campaign.time, "sleep",
        lambda _seconds: (_ for _ in ()).throw(AssertionError("paid rejection replayed")),
    )
    server, base = campaign._reader_proxy(
        "secret", tmp_path / "reader.json", campaign.load_campaign_manifest()
    )
    try:
        connection = http.client.HTTPConnection(base.removeprefix("http://"))
        connection.request(
            "POST", "/chat/completions",
            body=json.dumps({"model": "Qwen/Qwen3.5-9B", "messages": []}),
            headers={"content-type": "application/json"},
        )
        response = connection.getresponse()
        assert response.status == 429
        assert response.read() == rejected
        connection.close()
    finally:
        server.shutdown()
        server.server_close()
    audit = json.loads((tmp_path / "reader.json").read_text())
    assert calls == [600]
    assert audit["dispatch_count"] == 1
    assert audit["audit_status"] == "rejected"
    assert audit["pre_generation_rejections"] == [{
        "attempt": 1,
        "generation_id": "gen-possibly-billed",
        "response_sha256": campaign.hashlib.sha256(rejected).hexdigest(),
        "retry_after_seconds": None,
        "status": 429,
    }]


def test_reader_proxy_exhausts_bounded_pre_generation_503_retries(
    tmp_path: Path, monkeypatch
) -> None:
    campaign = _load()
    rejected = b'{"error":{"message":"No available provider","code":503}}'
    calls = []
    sleeps = []

    class Opener:
        def open(self, request, timeout=None):
            calls.append(timeout)
            raise campaign.urllib.error.HTTPError(
                request.full_url,
                503,
                "Service Unavailable",
                {},
                io.BytesIO(rejected),
            )

    monkeypatch.setattr(campaign.urllib.request, "build_opener", lambda *_args: Opener())
    monkeypatch.setattr(campaign.time, "sleep", sleeps.append)
    server, base = campaign._reader_proxy(
        "secret", tmp_path / "reader.json", campaign.load_campaign_manifest()
    )
    try:
        connection = http.client.HTTPConnection(base.removeprefix("http://"))
        connection.request(
            "POST", "/chat/completions",
            body=json.dumps({"model": "Qwen/Qwen3.5-9B", "messages": []}),
            headers={"content-type": "application/json"},
        )
        response = connection.getresponse()
        assert response.status == 503
        assert response.read() == rejected
        connection.close()
    finally:
        server.shutdown()
        server.server_close()
    audit = json.loads((tmp_path / "reader.json").read_text())
    assert calls == [600, 600, 600]
    assert sleeps == [5, 15]
    assert audit["dispatch_count"] == 3
    assert audit["audit_status"] == "rejected"
    assert [row["status"] for row in audit["pre_generation_rejections"]] == [503, 503, 503]
    assert [row["retry_after_seconds"] for row in audit["pre_generation_rejections"]] == [
        5, 15, None,
    ]


def test_reader_retry_delay_honors_numeric_header_with_default_and_cap() -> None:
    campaign = _load()
    contract = campaign.load_campaign_manifest()["protocol"]["reader"]
    assert campaign._reader_retry_delay_seconds("2", 0, contract) == 2
    assert campaign._reader_retry_delay_seconds(None, 0, contract) == 5
    assert campaign._reader_retry_delay_seconds("not-a-delay", 1, contract) == 15
    assert campaign._reader_retry_delay_seconds("0", 0, contract) == 1
    assert campaign._reader_retry_delay_seconds("600", 1, contract) == 60


def test_reader_proxy_archives_transport_unknown_without_replay(
    tmp_path: Path, monkeypatch
) -> None:
    campaign = _load()

    class Opener:
        def open(self, _request, timeout=None):
            assert timeout == 600
            raise TimeoutError("provider exceeded local transport deadline")

    monkeypatch.setattr(campaign.urllib.request, "build_opener", lambda *_args: Opener())
    server, base = campaign._reader_proxy(
        "secret", tmp_path / "reader.json", campaign.load_campaign_manifest()
    )
    try:
        connection = http.client.HTTPConnection(base.removeprefix("http://"))
        connection.request(
            "POST", "/chat/completions",
            body=json.dumps({"model": "Qwen/Qwen3.5-9B", "messages": []}),
            headers={"content-type": "application/json"},
        )
        response = connection.getresponse()
        assert response.status == 504
        assert b"outcome is unresolved" in response.read()
        connection.close()
    finally:
        server.shutdown()
        server.server_close()
    audit = json.loads((tmp_path / "reader.json").read_text())
    assert audit["dispatch_count"] == 1
    assert audit["audit_status"] == "transport_unknown"
    assert audit["audit_error"] == "reader_upstream_transport_failure"


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
    campaign.atomic_write_json(tmp_path / "reader-route.json", {
        "audit_status": "settled", "max_liability_micros": 1000, "total_cost": "0.001"
    })
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
        connection.request(
            "POST", "/chat/completions", body=json.dumps(body),
            headers={"content-type": "application/json"},
        )
        retry = connection.getresponse()
        assert retry.status == 422
        retry.read()
        connection.close()
    finally:
        server.shutdown()
        server.server_close()
    assert len(calls) == 1
    assert json.loads((tmp_path / "judge/0001.json").read_text())["audit_status"] == "invalid"

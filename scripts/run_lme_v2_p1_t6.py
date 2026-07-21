#!/usr/bin/env python3
"""Prepare and execute the immutable P1-T6 LongMemEval-V2 n=12 screen."""

from __future__ import annotations

import argparse
from contextlib import contextmanager
from decimal import Decimal, ROUND_CEILING
import hashlib
import http.client
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import importlib.util
import json
import math
import os
from pathlib import Path
import shutil
import signal
import socket
import subprocess
import sys
import tarfile
import tempfile
import threading
import types
import time
import urllib.request
import urllib.error
import urllib.parse


ROOT = Path(__file__).resolve().parents[1]
CAMPAIGN_MANIFEST = ROOT / "benchmarks/manifests/longmemeval_v2.p1_t6.json"
SELECTION_SOURCE = ROOT / "benchmarks/manifests/longmemeval_v2.p1_t6.selection-source.json"
RELEASE_MANIFEST = ROOT / "benchmarks/manifests/longmemeval_v2.lock.json"
MEMORY_CONFIG = ROOT / "benchmarks/longmemeval_v2/memphant.memory.json"
MATERIALIZER = ROOT / "scripts/materialize_longmemeval_v2_runtime.py"
SCRATCH_HELPER = ROOT / "scripts/with_scratch_db.sh"
MEMPHANT_BOOTSTRAP = ROOT / "benchmarks/longmemeval_v2/harness_bootstrap.py"
PROCESSOR_PREFLIGHT = ROOT / "benchmarks/longmemeval_v2/processor_preflight.py"
CAMPAIGN_PYTHON_REQUIREMENTS = (
    ROOT / "benchmarks/longmemeval_v2/requirements-p1-t6.txt"
)
MATERIALIZATION_SUMMARY = ROOT / "docs/build-log/artifacts/p1-t6/MATERIALIZATION-SUMMARY.json"
PAIRING_PROOFS = ROOT / "docs/build-log/artifacts/p1-t6/PAIRING-PROOFS.json"
SELECTION_SHA256 = "d7762dbaffff7acfe779162d4993c8c09ef0440e3c1a25e0d3408127d73e25fa"
SEED_SHA256 = "1d5ce2760cf354b45c102bab25c3a31bbff6f96f8a36425480da54473348e4dd"
ABILITIES = {
    "static_state", "dynamic_state", "workflow_knowledge",
    "environment_gotchas", "premise_awareness",
}
TYPE_ABILITIES = {
    "static-environment": "static_state",
    "dynamic-environment": "dynamic_state",
    "procedure": "workflow_knowledge",
    "errors-gotchas": "environment_gotchas",
}
FORBIDDEN_MEMORY_KEYS = {"answer", "answer_gold", "eval_function", "gold", "reference"}
ENDPOINT_FIELDS = (
    "name", "model_id", "provider_name", "tag", "quantization", "context_length",
    "max_completion_tokens", "max_prompt_tokens", "supported_parameters", "pricing",
)
MICROS_PER_USD = Decimal(1_000_000)
MILLION = Decimal(1_000_000)
SAFE_ENVIRONMENT_KEYS = (
    "HOME", "LANG", "LC_ALL", "PATH", "RUST_BACKTRACE", "RUST_LOG",
    "SSL_CERT_DIR", "SSL_CERT_FILE", "TMPDIR", "TZ",
)
PRODUCTION_BINARY_PROFILE = "release"


def require(condition: bool, message: str) -> None:
    if not condition:
        raise RuntimeError(message)


def canonical_bytes(value: object) -> bytes:
    return json.dumps(
        value, sort_keys=True, ensure_ascii=True, separators=(",", ":")
    ).encode("utf-8")


def canonical_sha256(value: object) -> str:
    return hashlib.sha256(canonical_bytes(value)).hexdigest()


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _binary_path(name: str) -> Path:
    require(name in {"server", "worker", "cli"}, f"unknown packaged binary: {name}")
    return ROOT / "target" / PRODUCTION_BINARY_PROFILE / f"memphant-{name}"


def _production_build_command() -> list[str]:
    return [
        "cargo", "build", "--release", "-p", "memphant-server",
        "-p", "memphant-worker", "-p", "memphant-cli",
    ]


def usd_to_micros(value: object) -> int:
    return int((Decimal(str(value)) * MICROS_PER_USD).to_integral_value(rounding=ROUND_CEILING))


def token_price_to_micros_per_million(value: object) -> int:
    return int(
        (Decimal(str(value)) * MILLION * MICROS_PER_USD).to_integral_value(
            rounding=ROUND_CEILING
        )
    )


def liability_micros(token_upper_bound: int, price_micros_per_million: int) -> int:
    require(token_upper_bound >= 0 and price_micros_per_million >= 0, "negative liability")
    return (token_upper_bound * price_micros_per_million + 999_999) // 1_000_000


def _clean_environment(extra: dict[str, str] | None = None) -> dict[str, str]:
    """Construct a child environment from a narrow non-secret allowlist."""
    clean = {
        key: os.environ[key]
        for key in SAFE_ENVIRONMENT_KEYS
        if key in os.environ
    }
    clean.update(extra or {})
    return clean


@contextmanager
def _temporary_environment(values: dict[str, str]):
    """Apply adapter-only settings and restore the caller environment exactly."""
    previous = {key: os.environ.get(key) for key in values}
    os.environ.update(values)
    try:
        yield
    finally:
        for key, value in previous.items():
            if value is None:
                os.environ.pop(key, None)
            else:
                os.environ[key] = value


def _resolve_execution_paths(
    directory: Path, materialized: Path, output: Path
) -> tuple[Path, Path, Path]:
    return directory.resolve(), materialized.resolve(), output.resolve()


def verify_python_harness(directory: Path) -> dict[str, object]:
    """Prove the sanitized interpreter can execute the official processor path."""
    official = directory / "official"
    requirements = official / "requirements.txt"
    require(requirements.is_file(), "official Python requirements are missing")
    require(
        CAMPAIGN_PYTHON_REQUIREMENTS.is_file(),
        "campaign Python requirements are missing",
    )
    required_packages = [
        line.strip()
        for line in CAMPAIGN_PYTHON_REQUIREMENTS.read_text().splitlines()
        if line.strip() and not line.lstrip().startswith("#")
    ]
    require(required_packages, "campaign Python requirements are empty")
    require(
        all("==" in package for package in required_packages),
        "campaign Python requirements must use exact pins",
    )
    environment = _clean_environment()
    interpreter = Path(sys.executable).resolve()

    checked = subprocess.run(
        [sys.executable, "-m", "pip", "check"],
        cwd=official,
        env=environment,
        capture_output=True,
        text=True,
        check=False,
    )
    require(
        checked.returncode == 0,
        "official Python dependency graph is inconsistent: "
        + (checked.stderr or checked.stdout).strip()[-500:],
    )
    frozen = subprocess.run(
        [sys.executable, "-m", "pip", "freeze", "--all"],
        cwd=official,
        env=environment,
        capture_output=True,
        text=True,
        check=False,
    )
    require(frozen.returncode == 0, "could not freeze official Python environment")
    packages = sorted(line.strip() for line in frozen.stdout.splitlines() if line.strip())
    require(packages, "official Python package inventory is empty")
    for package in required_packages:
        require(
            package in packages,
            f"campaign Python dependency missing or drifted: {package}",
        )

    bootstrapped = subprocess.run(
        [
            sys.executable,
            str(MEMPHANT_BOOTSTRAP),
            "--official-dir",
            str(official),
            "--help",
        ],
        cwd=official,
        env=environment,
        capture_output=True,
        text=True,
        check=False,
    )
    require(
        bootstrapped.returncode == 0,
        "official harness bootstrap import failed: "
        + (bootstrapped.stderr or bootstrapped.stdout).strip()[-500:],
    )
    processor = subprocess.run(
        [
            sys.executable,
            str(PROCESSOR_PREFLIGHT),
            "--official-dir",
            str(official),
        ],
        cwd=official,
        env=environment,
        capture_output=True,
        text=True,
        check=False,
    )
    require(
        processor.returncode == 0,
        "official Qwen processor preflight failed: "
        + (processor.stderr or processor.stdout).strip()[-500:],
    )
    return {
        "interpreter": _fingerprint(interpreter),
        "python_version": sys.version,
        "requirements_sha256": sha256_file(requirements),
        "campaign_requirements_sha256": sha256_file(CAMPAIGN_PYTHON_REQUIREMENTS),
        "packages": packages,
        "packages_sha256": canonical_sha256(packages),
        "bootstrap_import_verified": True,
        "bootstrap_stdout_sha256": hashlib.sha256(bootstrapped.stdout.encode()).hexdigest(),
        "bootstrap_stderr_sha256": hashlib.sha256(bootstrapped.stderr.encode()).hexdigest(),
        "processor_preflight_verified": True,
        "processor_preflight_stdout_sha256": hashlib.sha256(
            processor.stdout.encode()
        ).hexdigest(),
        "processor_preflight_stderr_sha256": hashlib.sha256(
            processor.stderr.encode()
        ).hexdigest(),
    }


def _redact_secrets(directory: Path, secrets: list[str]) -> None:
    needles = [secret.encode() for secret in secrets if secret]
    if not needles:
        return
    for path in sorted(directory.rglob("*")):
        if not path.is_file():
            continue
        body = path.read_bytes()
        redacted = body
        for needle in needles:
            redacted = redacted.replace(needle, b"[REDACTED]")
        if redacted != body:
            path.write_bytes(redacted)


def _row_secret_values(
    openrouter_key: str, openai_key: str, database_url: str
) -> list[str]:
    values = [openrouter_key, openai_key, database_url]
    parsed = urllib.parse.urlsplit(database_url)
    if parsed.netloc:
        values.append(parsed.netloc)
        userinfo = parsed.netloc.rsplit("@", 1)[0]
        if ":" in userinfo:
            raw_password = userinfo.split(":", 1)[1]
            values.extend([raw_password, urllib.parse.unquote(raw_password)])
    return list(dict.fromkeys(value for value in values if value))


def _expected_deep_config_hash(candidate: dict) -> str:
    return canonical_sha256({
        "model": candidate["model"],
        "providers": ["azure"],
        "input_price_micros_per_million": candidate["input_price_micros_per_million"],
        "output_price_micros_per_million": candidate["output_price_micros_per_million"],
        "limits": {
            "wall_time_ms": 120_000,
            "max_tool_iterations": 24,
            "max_context_tokens": 96_000,
            "max_spend_micros": 300_000,
        },
        "max_completion_tokens": 4_096,
        "completion_url": "https://openrouter.ai/api/v1/chat/completions",
        "generation_url": "https://openrouter.ai/api/v1/generation",
        "connect_timeout_ms": 10_000,
        "settlement_reserve_ms": 5_000,
        "max_retries": 2,
        "retry_base_ms": 250,
        "implicit_protocol_retries": "disabled",
        "redirects": "disabled",
        "ambient_proxies": "disabled",
        "request_contract": {
            "stream": True,
            "tool_choice": "required",
            "parallel_tool_calls": "omitted",
            "single_tool_call_enforcement": "response_parser",
            "provider_require_parameters": True,
        },
        "tool_limits": {
            "list_results": 256,
            "query_chars": 256,
            "search_hits": 128,
            "output_bytes": 64 * 1024,
            "read_lines": 512,
            "evidence_ids": 256,
            "malformed_responses": 1,
        },
    })


def artifact_hashes(directory: Path, *, exclude: set[str] | None = None) -> dict[str, str]:
    excluded = exclude or set()
    return {
        str(path.relative_to(directory)): sha256_file(path)
        for path in sorted(directory.rglob("*"))
        if path.is_file() and str(path.relative_to(directory)) not in excluded
    }


def ability(question_type: str) -> str:
    if question_type.endswith("-abs"):
        return "premise_awareness"
    require(question_type in TYPE_ABILITIES, f"unsupported question_type: {question_type}")
    return TYPE_ABILITIES[question_type]


def select_cases(rows: list[dict]) -> list[dict[str, str]]:
    """Select using only id/domain/question_type; callers may trap every other key."""
    population: list[dict[str, str]] = []
    seen: set[str] = set()
    for source in rows:
        question_id = source["id"]
        domain = source["domain"]
        question_type = source["question_type"]
        require(isinstance(question_id, str) and question_id, "invalid question id")
        require(question_id not in seen, f"duplicate question id: {question_id}")
        require(domain in {"web", "enterprise"}, f"invalid domain: {domain}")
        seen.add(question_id)
        population.append(
            {"domain": domain, "ability": ability(question_type),
             "question_type": question_type, "id": question_id}
        )

    selected: list[dict[str, str]] = []
    seed = SEED_SHA256
    for domain in ("enterprise", "web"):
        for ability_name in sorted(ABILITIES):
            stratum = [
                row for row in population
                if row["domain"] == domain and row["ability"] == ability_name
            ]
            require(stratum, f"empty selection stratum: {domain}/{ability_name}")
            selected.append(
                min(
                    stratum,
                    key=lambda row: (
                        hashlib.sha256(
                            f"{seed}\0base\0{domain}\0{ability_name}\0{row['id']}".encode()
                        ).hexdigest(),
                        row["id"],
                    ),
                )
            )

    selected_ids = {row["id"] for row in selected}
    remaining = [row for row in population if row["id"] not in selected_ids]
    pairs = [
        (web, enterprise)
        for web in remaining if web["domain"] == "web"
        for enterprise in remaining if enterprise["domain"] == "enterprise"
        if web["ability"] != enterprise["ability"]
    ]
    require(pairs, "no eligible extra pair")
    extra = min(
        pairs,
        key=lambda pair: (
            hashlib.sha256(
                f"{seed}\0extra_pair\0{pair[0]['id']}\0{pair[1]['id']}".encode()
            ).hexdigest(),
            pair[0]["id"], pair[1]["id"],
        ),
    )
    selected.extend(extra)
    selected.sort(key=lambda row: (row["domain"], row["ability"], row["id"]))
    require(len(selected) == 12, "selector did not produce 12 cases")
    require(sum(row["domain"] == "web" for row in selected) == 6, "web count drift")
    counts = [sum(row["ability"] == name for row in selected) for name in ABILITIES]
    require(max(counts) - min(counts) <= 1, "ability balance drift")
    return selected


def load_campaign_manifest() -> dict:
    return json.loads(CAMPAIGN_MANIFEST.read_text(encoding="utf-8"))


def expanded_run_order(manifest: dict) -> list[dict[str, object]]:
    order = manifest["run_order"]
    rows: list[dict[str, object]] = []
    for question_id in order["case_order"]:
        for arm in order["arm_order_per_case"]:
            sequence = len(rows) + 1
            rows.append(
                {
                    "sequence": sequence,
                    "question_id": question_id,
                    "arm": arm,
                    "row_id": f"{sequence:04d}-{question_id}-{arm}",
                }
            )
    return rows


def verify_campaign_manifest(manifest: dict) -> dict[str, int]:
    require(manifest.get("schema_version") == 1, "campaign schema drift")
    selection = manifest["selection"]
    require(selection["seed_sha256"] == SEED_SHA256, "selection seed drift")
    require(selection["sha256"] == SELECTION_SHA256, "selection digest drift")
    require(canonical_sha256(selection["cases"]) == SELECTION_SHA256, "case content drift")
    source = json.loads(SELECTION_SOURCE.read_text(encoding="utf-8"))
    require(source["source_questions_sha256"] == manifest["upstream"]["questions_sha256"],
            "selection source lock drift")
    require(canonical_sha256(source["rows"]) == source["population_sha256"],
            "answer-blind population fixture drift")
    require(select_cases(source["rows"]) == selection["cases"], "selection reproduction drift")
    rows = expanded_run_order(manifest)
    require(len(rows) == 24 and len({row["row_id"] for row in rows}) == 24,
            "run-order completeness drift")
    expected_ids = {row["id"] for row in selection["cases"]}
    require({row["question_id"] for row in rows} == expected_ids, "run-order case drift")
    require(manifest["run_order"]["outputs_observed"] is False, "run order was post-scored")
    require(manifest["run_order"]["case_order"] == sorted(expected_ids), "case-major order drift")
    protocol = manifest["protocol"]
    selected_deep_arm = protocol["selected_deep_arm"]
    require(selected_deep_arm == "sonnet", "selected Deep arm drift")
    require(protocol["inactive_researched_shortlist"] == ["luna", "sol"],
            "inactive Deep shortlist drift")
    require(selected_deep_arm in protocol["deep_candidates"], "selected Deep arm is unknown")
    require(manifest["run_order"]["arm_order_per_case"] == ["fast", selected_deep_arm],
            "arm order drift")
    spend = manifest["campaign_spend"]
    require(spend["hard_ceiling_usd"] == 6.25, "campaign spend ceiling drift")
    preexisting = spend["preexisting_liability"]
    require(preexisting["settled_micros"] == 7542
            and preexisting["unsettled_upper_bound_micros"] == 316142
            and preexisting["total_micros"] == 323684,
            "preexisting campaign liability drift")
    require(preexisting["settled_micros"] + preexisting["unsettled_upper_bound_micros"]
            == preexisting["total_micros"], "preexisting liability sum drift")
    for proof_path in preexisting["proofs"].values():
        require((ROOT / proof_path).is_file(), f"preexisting liability proof missing: {proof_path}")
    require(spend["deep_max_liability_usd"] == 3.6,
            "Deep campaign reserve drift")
    require(spend["reader_and_judge_reserve_usd"] == 2.0976,
            "reader and judge campaign reserve drift")
    require(
        usd_to_micros(spend["reader_and_judge_reserve_usd"])
        == len(rows) * spend["reader_and_judge_max_liability_micros_per_row"],
        "reader and judge campaign reserve drift",
    )
    fresh_liability = usd_to_micros(spend["deep_max_liability_usd"]) + usd_to_micros(
        spend["reader_and_judge_reserve_usd"]
    )
    require(fresh_liability + preexisting["total_micros"]
            <= usd_to_micros(spend["hard_ceiling_usd"]),
            "campaign liability exceeds hard ceiling")
    require(manifest["protocol"]["deep_prompt_sha256"]
            == sha256_file(ROOT / "config/deep-recall-v1.txt"), "Deep prompt lock drift")
    reader = manifest["protocol"]["reader"]
    require(reader["upstream_timeout_seconds"] == 600,
            "reader upstream timeout drift")
    require(reader["pre_generation_retry_attempts"] == 2
            and reader["pre_generation_retry_delays_seconds"] == [5, 15]
            and reader["retry_after_max_seconds"] == 60,
            "reader pre-generation retry contract drift")
    require(reader["receipt_reconciliation_attempts"] == 60
            and reader["receipt_reconciliation_delay_seconds"] == 1,
            "reader receipt reconciliation drift")
    require(reader["provider_policy"] == {
        "only": ["deepinfra"], "allow_fallbacks": False,
        "require_parameters": True, "data_collection": "deny", "zdr": True,
        "quantizations": ["bf16"],
        "max_price": {
            "prompt": reader["prompt_price_micros_per_million"] / 1_000_000,
            "completion": reader["completion_price_micros_per_million"] / 1_000_000,
        },
    }, "reader dispatch policy drift")
    for name, candidate in protocol["deep_candidates"].items():
        require(candidate["config_sha256"] == _expected_deep_config_hash(candidate),
                f"Deep runtime config hash drift: {name}")
    return {"cases": 12, "rows": 24, "arms": 2, "constructions": 12}


def write_memory_config(base: dict, mode: str, path: Path) -> dict:
    require(mode in {"fast", "deep"}, "memory mode must be fast or deep")
    value = json.loads(json.dumps(base))
    value["memory_params"]["mode"] = mode
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    return value


def _context_contract_audit(
    memory_context: list[dict], public: dict, exact_reader_tokens: int, budget_tokens: int
) -> dict[str, object]:
    require(bool(memory_context), "context preflight requires non-empty memory context")
    require(exact_reader_tokens > 0, "context preflight requires positive reader tokens")
    require(
        exact_reader_tokens <= budget_tokens,
        "context preflight exceeded the exact reader token budget",
    )
    runtime_estimate = (public.get("trace") or {}).get("token_estimate")
    require(isinstance(runtime_estimate, int) and runtime_estimate > 0,
            "context preflight lacks positive runtime token estimate")
    require(runtime_estimate <= budget_tokens,
            "runtime token estimate exceeded the request budget")
    return {
        "context_items": len(memory_context),
        "runtime_token_estimate": runtime_estimate,
        "exact_reader_tokens": exact_reader_tokens,
        "budget_tokens": budget_tokens,
        "nonempty": True,
        "untruncated": True,
    }


def _load_selected_trajectories(path: Path, selected_ids: list[str]) -> dict[str, dict]:
    """Load only the locked case from the 1+ GiB upstream JSONL corpus."""
    wanted = set(selected_ids)
    require(len(wanted) == len(selected_ids), "context preflight haystack contains duplicates")
    trajectories: dict[str, dict] = {}
    prefix = b'{"id":"'
    with path.open("rb") as handle:
        for line in handle:
            if not line.strip():
                continue
            require(line.startswith(prefix), "trajectory JSONL id is not the first field")
            id_end = line.find(b'"', len(prefix))
            require(id_end > len(prefix), "trajectory JSONL id is malformed")
            trajectory_id = line[len(prefix):id_end].decode("utf-8")
            if trajectory_id not in wanted:
                continue
            require(trajectory_id not in trajectories,
                    f"duplicate selected trajectory: {trajectory_id}")
            trajectory = json.loads(line)
            require(trajectory.get("id") == trajectory_id,
                    "trajectory prefix and decoded id disagree")
            trajectories[trajectory_id] = trajectory
    require(set(trajectories) == wanted,
            "context preflight selected trajectories are incomplete")
    return trajectories


def require_new_row_dir(path: Path) -> None:
    require(not path.exists(), f"immutable row already exists: {path}")


def atomic_write_json(path: Path, value: object) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(path.name + ".tmp")
    require(not temporary.exists(), f"stale atomic-write temporary: {temporary}")
    with temporary.open("w", encoding="utf-8") as handle:
        json.dump(value, handle, indent=2, sort_keys=True)
        handle.write("\n")
        handle.flush()
        os.fsync(handle.fileno())
    os.replace(temporary, path)


def _download(url: str, destination: Path, expected_sha256: str | None = None) -> None:
    destination.parent.mkdir(parents=True, exist_ok=True)
    partial = destination.with_name(destination.name + ".part")
    offset = partial.stat().st_size if partial.exists() else 0
    headers = {"User-Agent": "MemPhant-P1-T6"}
    if offset:
        headers["Range"] = f"bytes={offset}-"
    request = urllib.request.Request(url, headers=headers)
    with urllib.request.urlopen(request) as response:
        append = offset > 0 and response.status == 206
        with partial.open("ab" if append else "wb") as output:
            shutil.copyfileobj(response, output)
            output.flush()
            os.fsync(output.fileno())
    if expected_sha256 is not None:
        require(sha256_file(partial) == expected_sha256, f"download hash drift: {destination.name}")
    os.replace(partial, destination)


def acquire_minimal(directory: Path, manifest: dict) -> dict[str, object]:
    directory.mkdir(parents=True, exist_ok=True)
    official = directory / "official"
    data = directory / "data"
    release = json.loads(RELEASE_MANIFEST.read_text(encoding="utf-8"))
    sys.path.insert(0, str(ROOT / "scripts"))
    import run_longmemeval_v2 as release_adapter

    if not official.exists():
        with tempfile.TemporaryDirectory(dir=directory) as temp_name:
            archive = Path(temp_name) / "official.tar.gz"
            _download(
                f"https://github.com/xiaowu0162/LongMemEval-V2/archive/{release['code']['commit']}.tar.gz",
                archive,
            )
            extracted = Path(temp_name) / "extracted"
            extracted.mkdir()
            with tarfile.open(archive, "r:gz") as bundle:
                bundle.extractall(extracted, filter="data")
            roots = list(extracted.iterdir())
            require(len(roots) == 1 and roots[0].is_dir(), "unexpected code archive layout")
            release_adapter.verify_code(roots[0], release["code"]["files"])
            roots[0].replace(official)
    release_adapter.verify_code(official, release["code"]["files"])

    revision = manifest["upstream"]["dataset_revision"]
    repository = release["dataset"]["repository"]
    verified: dict[str, dict[str, object]] = {}
    for relative, expected in manifest["acquisition"]["files"].items():
        destination = data / relative
        if not destination.exists():
            _download(
                f"https://huggingface.co/datasets/{repository}/resolve/{revision}/{relative}",
                destination,
                expected,
            )
        actual = sha256_file(destination)
        require(actual == expected, f"minimal acquisition hash drift: {relative}")
        verified[relative] = {"bytes": destination.stat().st_size, "sha256": actual}
    return {"official_code_verified": True, "files": verified}


def _load_adapter(official: Path):
    package = types.ModuleType("memory_modules")
    memory = types.ModuleType("memory_modules.memory")

    class Memory:
        def __init__(self, memory_params: dict) -> None:
            self.memory_params = memory_params

    memory.Memory = Memory
    memory.MemoryContextItem = dict
    memory.register_memory = lambda cls: cls
    previous_package = sys.modules.get("memory_modules")
    previous_memory = sys.modules.get("memory_modules.memory")
    sys.modules["memory_modules"] = package
    sys.modules["memory_modules.memory"] = memory
    path = ROOT / "benchmarks/longmemeval_v2/memphant_memory.py"
    spec = importlib.util.spec_from_file_location("p1_t6_memphant_memory", path)
    module = importlib.util.module_from_spec(spec)
    require(spec.loader is not None, "could not load MemPhant adapter")
    try:
        spec.loader.exec_module(module)
    finally:
        if previous_package is None:
            sys.modules.pop("memory_modules", None)
        else:
            sys.modules["memory_modules"] = previous_package
        if previous_memory is None:
            sys.modules.pop("memory_modules.memory", None)
        else:
            sys.modules["memory_modules.memory"] = previous_memory
    return module


def materialize(directory: Path, output: Path, manifest: dict) -> dict[str, object]:
    acquire_minimal(directory, manifest)
    require(not output.exists(), f"refusing to overwrite materialization: {output}")
    final_output = output
    output = final_output.with_name(".staging-" + final_output.name)
    require(not output.exists(), f"stale materialization staging requires review: {output}")
    output.mkdir(parents=True)
    official = directory / "official"
    data = directory / "data"
    sys.path.insert(0, str(official))
    from data.public_data import materialize_runtime_haystack, materialize_runtime_questions

    cases = manifest["selection"]["cases"]
    all_questions: dict[str, dict] = {}
    all_haystacks: dict[str, list[str]] = {}
    for domain in ("enterprise", "web"):
        ids = [row["id"] for row in cases if row["domain"] == domain]
        questions_path = output / f".{domain}.questions.json"
        haystack_path = output / f".{domain}.haystack.json"
        questions = materialize_runtime_questions(
            data_root=data, domain=domain, question_ids=ids, limit=None,
            output_path=questions_path,
        )
        haystacks = materialize_runtime_haystack(
            data_root=data, tier="medium", selected_questions=questions,
            output_path=haystack_path,
        )
        all_questions.update({row["id"]: row for row in questions})
        all_haystacks.update(haystacks)
        questions_path.unlink()
        haystack_path.unlink()

    required_trajectories = {item for ids in all_haystacks.values() for item in ids}
    trajectories: dict[str, tuple[dict, str]] = {}
    with (data / "trajectories.jsonl").open(encoding="utf-8") as handle:
        for line in handle:
            row = json.loads(line)
            if row.get("id") not in required_trajectories:
                continue
            require(not FORBIDDEN_MEMORY_KEYS.intersection(row),
                    f"trajectory contains evaluator keys: {row.get('id')}")
            trajectories[row["id"]] = (row, hashlib.sha256(line.rstrip("\n").encode()).hexdigest())
    require(set(trajectories) == required_trajectories, "selected trajectories are incomplete")

    adapter = _load_adapter(official)
    sizes: list[int] = []
    fragment_counts: list[int] = []
    serialized_sizes: list[int] = []
    base_config = json.loads(MEMORY_CONFIG.read_text(encoding="utf-8"))
    for case in cases:
        question_id = case["id"]
        case_dir = output / question_id
        case_dir.mkdir()
        questions_path = case_dir / "questions.json"
        haystack_path = case_dir / "haystack.json"
        questions_path.write_text(json.dumps([all_questions[question_id]], indent=2) + "\n")
        haystack_path.write_text(json.dumps({question_id: all_haystacks[question_id]}, indent=2) + "\n")
        pairing = []
        for trajectory_id in all_haystacks[question_id]:
            trajectory, row_hash = trajectories[trajectory_id]
            body = adapter._trajectory_body(trajectory)
            fragments = adapter._trajectory_fragments(trajectory)
            sizes.append(len(body.encode()))
            fragment_counts.append(len(fragments))
            for fragment_index, fragment in enumerate(fragments, 1):
                fragment_body = f"Trajectory fragment {fragment_index}/{len(fragments)}\n\n{fragment}"
                sizing_payload = {
                    "actor_id": "00000000-0000-0000-0000-000000000000",
                    "agent_node_id": "00000000-0000-0000-0000-000000000000",
                    "scope_id": "00000000-0000-0000-0000-000000000000",
                    "subject_generation": 0,
                    "subject_id": "00000000-0000-0000-0000-000000000000",
                    "source_ref": f"lme-v2:trajectory:{trajectory_id}:{fragment_index:04d}",
                    "observed_at": "2026-05-17T00:00:00Z",
                    "payload": {"resource": {
                        "uri": f"lme-v2://trajectory/{trajectory_id}/{fragment_index:04d}",
                        "mime_type": "text/markdown", "kind": "document",
                        "revision": trajectory_id, "body": fragment_body,
                        "content_hash": "sha256:" + hashlib.sha256(fragment_body.encode()).hexdigest(),
                    }},
                }
                serialized_sizes.append(len(canonical_bytes(sizing_payload)))
            pairing.append({"trajectory_id": trajectory_id, "row_sha256": row_hash,
                            "body_bytes": len(body.encode()), "fragments": len(fragments)})
        write_memory_config(base_config, "fast", case_dir / "memory.fast.json")
        write_memory_config(base_config, "deep", case_dir / "memory.deep.json")
        proof = {
            "question_id": question_id, "domain": case["domain"], "tier": "medium",
            "question_input_sha256": sha256_file(questions_path),
            "haystack_input_sha256": sha256_file(haystack_path),
            "trajectories": pairing, "gold_fields_copied_to_memory": [],
            "fast_deep_corpus_pairing": "same questions.json, haystack.json, trajectories.jsonl",
        }
        (case_dir / "pairing.json").write_text(
            json.dumps(proof, indent=2, sort_keys=True) + "\n", encoding="utf-8"
        )
    sizes.sort()
    serialized_sizes.sort()
    require(max(serialized_sizes) <= adapter.MAX_SERIALIZED_RETAIN_BYTES,
            "measured retain request exceeds campaign safety budget")
    require(max(serialized_sizes) < 2 * 1024 * 1024,
            "measured retain request exceeds Axum default body limit")
    summary = {
        "cases": 12, "unique_trajectories": len(required_trajectories),
        "canonical_body_bytes": {
            "max": max(sizes), "p95": sizes[math.ceil(0.95 * len(sizes)) - 1],
        },
        "fragment_counts": {"max": max(fragment_counts), "total": sum(fragment_counts)},
        "serialized_retain_bytes": {
            "p95": serialized_sizes[math.ceil(0.95 * len(serialized_sizes)) - 1],
            "max": max(serialized_sizes),
            "campaign_safety_limit": adapter.MAX_SERIALIZED_RETAIN_BYTES,
            "axum_effective_default_limit": 2 * 1024 * 1024,
        },
        "gold_fields_copied_to_memory": [],
    }
    (output / "materialization-proof.json").write_text(
        json.dumps(summary, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    directory_fd = os.open(output, os.O_RDONLY)
    try:
        os.fsync(directory_fd)
    finally:
        os.close(directory_fd)
    os.replace(output, final_output)
    return summary


def verify_materialization(directory: Path, materialized: Path, manifest: dict) -> dict[str, object]:
    proof_path = materialized / "materialization-proof.json"
    require(proof_path.is_file(), "materialization missing")
    expected_summary = json.loads(MATERIALIZATION_SUMMARY.read_text())
    proof_hash = sha256_file(proof_path)
    require(proof_hash == expected_summary["materialization_sha256"],
            "materialization summary drift")
    archived_pairs = {
        item["question_id"]: item
        for item in json.loads(PAIRING_PROOFS.read_text())["pairs"]
    }
    require(set(archived_pairs) == {case["id"] for case in manifest["selection"]["cases"]},
            "archived pairing set drift")
    case_contracts: dict[str, dict[str, str]] = {}
    trajectory_hashes: dict[str, str] = {}
    for case in manifest["selection"]["cases"]:
        question_id = case["id"]
        case_dir = materialized / question_id
        pairing_path = case_dir / "pairing.json"
        pairing = json.loads(pairing_path.read_text())
        require(pairing["question_id"] == question_id and pairing["domain"] == case["domain"],
                f"pairing identity drift: {question_id}")
        require(pairing["gold_fields_copied_to_memory"] == [], "gold-memory isolation proof failed")
        question_path = case_dir / "questions.json"
        haystack_path = case_dir / "haystack.json"
        require(sha256_file(question_path) == pairing["question_input_sha256"],
                f"question materialization drift: {question_id}")
        require(sha256_file(haystack_path) == pairing["haystack_input_sha256"],
                f"haystack materialization drift: {question_id}")
        haystack = json.loads(haystack_path.read_text())
        require(list(haystack) == [question_id], f"haystack identity drift: {question_id}")
        require(haystack[question_id] == [item["trajectory_id"] for item in pairing["trajectories"]],
                f"trajectory order drift: {question_id}")
        for item in pairing["trajectories"]:
            prior = trajectory_hashes.setdefault(item["trajectory_id"], item["row_sha256"])
            require(prior == item["row_sha256"],
                    f"cross-case trajectory hash drift: {item['trajectory_id']}")
        for mode in ("fast", "deep"):
            config = json.loads((case_dir / f"memory.{mode}.json").read_text())
            require(config["memory_params"]["mode"] == mode, f"memory mode drift: {question_id}/{mode}")
        archived = archived_pairs[question_id]
        require(sha256_file(pairing_path) == archived["pairing_sha256"],
                f"archived pairing proof drift: {question_id}")
        require(archived["question_input_sha256"] == pairing["question_input_sha256"]
                and archived["haystack_input_sha256"] == pairing["haystack_input_sha256"],
                f"archived input proof drift: {question_id}")
        case_contracts[question_id] = {
            "questions_sha256": pairing["question_input_sha256"],
            "haystack_sha256": pairing["haystack_input_sha256"],
            "pairing_sha256": archived["pairing_sha256"],
            "fast_config_sha256": sha256_file(case_dir / "memory.fast.json"),
            "deep_config_sha256": sha256_file(case_dir / "memory.deep.json"),
        }
    found: set[str] = set()
    with (directory / "data/trajectories.jsonl").open(encoding="utf-8") as handle:
        for line in handle:
            row = json.loads(line)
            trajectory_id = row.get("id")
            if trajectory_id not in trajectory_hashes:
                continue
            require(hashlib.sha256(line.rstrip("\n").encode()).hexdigest()
                    == trajectory_hashes[trajectory_id],
                    f"pinned trajectory row drift: {trajectory_id}")
            found.add(trajectory_id)
    require(found == set(trajectory_hashes), "materialized trajectory set is incomplete")
    return {"proof_sha256": proof_hash, "cases": case_contracts}


def verify_case_materialization(case_dir: Path, contract: dict[str, str]) -> None:
    for relative, key in (
        ("questions.json", "questions_sha256"), ("haystack.json", "haystack_sha256"),
        ("pairing.json", "pairing_sha256"), ("memory.fast.json", "fast_config_sha256"),
        ("memory.deep.json", "deep_config_sha256"),
    ):
        require(sha256_file(case_dir / relative) == contract[key],
                f"materialized case changed after preflight: {case_dir.name}/{relative}")


def preflight(directory: Path, materialized: Path, manifest: dict) -> dict[str, object]:
    verify_campaign_manifest(manifest)
    acquired = acquire_minimal(directory, manifest)
    materialization = verify_materialization(directory, materialized, manifest)
    python = verify_python_harness(directory)
    return {"campaign": verify_campaign_manifest(manifest), "acquisition": acquired,
            "materialization": materialization, "python": python}


def _json_url(url: str, api_key: str | None = None) -> dict:
    headers = {"User-Agent": "MemPhant-P1-T6"}
    if api_key:
        headers["Authorization"] = f"Bearer {api_key}"
    with urllib.request.build_opener(urllib.request.ProxyHandler({})).open(
        urllib.request.Request(url, headers=headers), timeout=30
    ) as response:
        value = json.load(response)
    require(isinstance(value, dict), f"endpoint returned non-object: {url}")
    return value


def _matching_endpoints(endpoints: list[dict], contract: dict) -> list[dict]:
    matches = []
    for endpoint in endpoints:
        if any(endpoint.get(key) != contract[key] for key in
               ("name", "model_id", "provider_name") if key in contract):
            continue
        if contract.get("tag") is not None and endpoint.get("tag") != contract["tag"]:
            continue
        if contract.get("quantization") is not None and endpoint.get("quantization") != contract["quantization"]:
            continue
        if int(endpoint.get("context_length") or 0) < contract["min_context_length"]:
            continue
        if int(endpoint.get("max_completion_tokens") or 0) < contract["min_completion_tokens"]:
            continue
        if not set(contract["required_parameters"]) <= set(endpoint.get("supported_parameters") or []):
            continue
        pricing = endpoint.get("pricing") or {}
        if pricing.get("prompt") is None or pricing.get("completion") is None:
            continue
        if token_price_to_micros_per_million(pricing["prompt"]) > contract["prompt_price_micros_per_million_max"]:
            continue
        if token_price_to_micros_per_million(pricing["completion"]) > contract["completion_price_micros_per_million_max"]:
            continue
        matches.append(endpoint)
    return matches


def verify_endpoint_inventory(manifest: dict) -> dict[str, object]:
    checks = [
        ("qwen/qwen3.5-9b", "reader", "all"),
        ("anthropic/claude-sonnet-5-20260630", "sonnet", "azure"),
        ("openai/gpt-5.6-luna-20260709", "luna", "azure"),
        ("openai/gpt-5.6-sol-20260709", "sol", "azure"),
    ]
    proven: dict[str, object] = {}
    for slug, key, provider in checks:
        payload = _json_url(f"https://openrouter.ai/api/v1/models/{slug}/endpoints")
        endpoints = payload["data"]["endpoints"]
        inventory = [
            {field: endpoint[field] for field in ENDPOINT_FIELDS}
            for endpoint in endpoints
            if provider == "all" or endpoint["provider_name"].lower() == provider
        ]
        if key == "reader":
            contract = manifest["protocol"]["reader"]["endpoint_contract"]
        else:
            candidate = manifest["protocol"]["deep_candidates"][key]
            contract = {
                "name": f"{candidate['providers'][0].title()} | {candidate['model']}",
                "model_id": candidate["endpoint_model_id"], "provider_name": "Azure",
                "min_context_length": 100000, "min_completion_tokens": 4096,
                "required_parameters": ["tools", "tool_choice", "max_completion_tokens"],
                "prompt_price_micros_per_million_max": candidate["input_price_micros_per_million"],
                "completion_price_micros_per_million_max": candidate["output_price_micros_per_million"],
            }
        matches = _matching_endpoints(inventory, contract)
        require(matches, f"OpenRouter material endpoint contract unavailable: {key}")
        proven[key] = {
            "inventory_sha256": canonical_sha256(inventory),
            "matching_endpoint_sha256": [canonical_sha256(endpoint) for endpoint in matches],
            "material_contract_sha256": canonical_sha256(contract),
        }
    return proven


def _free_port() -> int:
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        return int(sock.getsockname()[1])


class _NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, *_args: object, **_kwargs: object):
        return None


def _direct_opener():
    return urllib.request.build_opener(urllib.request.ProxyHandler({}), _NoRedirect())


def _reader_route_probe_request() -> dict[str, object]:
    """Return the smallest useful request that exercises the frozen reader route."""
    return {
        "model": "Qwen/Qwen3.5-9B",
        "messages": [{
            "role": "user",
            "content": "Reply with exactly ROUTE_OK after reasoning internally.",
        }],
        "max_tokens": 64,
        "reasoning": {"enabled": True},
        "temperature": 0,
    }


def _reader_retry_delay_seconds(
    retry_after: str | None, retry_index: int, contract: dict
) -> int:
    delays = contract["pre_generation_retry_delays_seconds"]
    require(0 <= retry_index < len(delays), "reader retry index is out of range")
    fallback = int(delays[retry_index])
    cap = int(contract["retry_after_max_seconds"])
    require(0 < fallback <= cap, "reader retry fallback is out of range")
    if retry_after is None:
        return fallback
    try:
        parsed = int(retry_after.strip())
    except (AttributeError, TypeError, ValueError):
        return fallback
    return min(max(parsed, 1), cap)


def _reader_proxy(api_key: str, audit_path: Path, manifest: dict) -> tuple[ThreadingHTTPServer, str]:
    contract = manifest["protocol"]["reader"]
    policy = contract["provider_policy"]
    dispatch_lock = threading.Lock()
    dispatched = False

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *_args: object) -> None:
            return None

        def do_POST(self) -> None:
            nonlocal dispatched
            response_body: bytes | None = None
            status = 422
            try:
                with dispatch_lock:
                    require(not dispatched, "reader proxy dispatch already consumed")
                    dispatched = True
                require(self.path == "/chat/completions", "reader proxy path denied")
                length = int(self.headers.get("content-length", "0"))
                body = self.rfile.read(length)
                request = json.loads(body)
                require(request.get("model") == "Qwen/Qwen3.5-9B", "reader model drift")
                request["provider"] = policy
                upstream_body = canonical_bytes(request)
                input_upper_bound = len(canonical_bytes(request.get("messages", [])))
                completion_upper_bound = int(
                    request.get("max_completion_tokens", request.get("max_tokens", 20_000))
                )
                max_liability = liability_micros(
                    input_upper_bound, contract["prompt_price_micros_per_million"]
                ) + liability_micros(
                    completion_upper_bound, contract["completion_price_micros_per_million"]
                )
                require(max_liability <= manifest["campaign_spend"]["reader_and_judge_max_liability_micros_per_row"],
                        "reader request exceeds row spend reserve")
                audit = {
                    "audit_status": "pending", "dispatch_count": 0,
                    "request_contract_sha256": hashlib.sha256(upstream_body).hexdigest(),
                    "provider_policy_sha256": canonical_sha256(policy),
                    "input_token_upper_bound": input_upper_bound,
                    "completion_token_upper_bound": completion_upper_bound,
                    "max_liability_micros": max_liability,
                    "pre_generation_rejections": [],
                }
                atomic_write_json(audit_path, audit)
                upstream_request = urllib.request.Request(
                    "https://openrouter.ai/api/v1/chat/completions",
                    data=upstream_body,
                    method="POST",
                    headers={"Authorization": f"Bearer {api_key}", "Content-Type": "application/json"},
                )
                max_attempts = 1 + int(contract["pre_generation_retry_attempts"])
                for attempt in range(1, max_attempts + 1):
                    audit["dispatch_count"] = attempt
                    audit["audit_status"] = "pending"
                    atomic_write_json(audit_path, audit)
                    try:
                        with _direct_opener().open(
                            upstream_request, timeout=contract["upstream_timeout_seconds"]
                        ) as response:
                            response_body = response.read()
                            status = response.status
                    except urllib.error.HTTPError as error:
                        response_body = error.read()
                        status = error.code
                        generation_id = error.headers.get("X-Generation-Id")
                        generation_id = (
                            generation_id.strip()
                            if isinstance(generation_id, str) and generation_id.strip()
                            else None
                        )
                        retryable = status in {429, 503} and generation_id is None
                        retry_delay = None
                        if retryable and attempt < max_attempts:
                            retry_delay = _reader_retry_delay_seconds(
                                error.headers.get("Retry-After"), attempt - 1, contract
                            )
                        rejection = {
                            "attempt": attempt,
                            "status": status,
                            "generation_id": generation_id,
                            "response_sha256": hashlib.sha256(response_body).hexdigest(),
                            "retry_after_seconds": retry_delay,
                        }
                        audit["pre_generation_rejections"].append(rejection)
                        if retry_delay is not None:
                            audit["audit_status"] = "retry_wait"
                            atomic_write_json(audit_path, audit)
                            time.sleep(retry_delay)
                            continue
                        try:
                            parsed_error = json.loads(response_body)
                        except (TypeError, ValueError):
                            parsed_error = None
                        audit.update({
                            "audit_status": "rejected",
                            "upstream_status": status,
                            "upstream_error": (
                                parsed_error.get("error")
                                if isinstance(parsed_error, dict)
                                else {"type": "non_json_upstream_rejection"}
                            ),
                            "response_sha256": hashlib.sha256(response_body).hexdigest(),
                        })
                        atomic_write_json(audit_path, audit)
                        break
                    except (
                        TimeoutError,
                        urllib.error.URLError,
                        ConnectionError,
                        http.client.HTTPException,
                    ):
                        status = 504
                        response_body = canonical_bytes({"error": {
                            "message": "reader upstream transport outcome is unresolved",
                            "type": "reader_route_transport",
                        }})
                        audit.update({
                            "audit_status": "transport_unknown",
                            "audit_error": "reader_upstream_transport_failure",
                            "response_sha256": hashlib.sha256(response_body).hexdigest(),
                        })
                        atomic_write_json(audit_path, audit)
                        break
                    else:
                        try:
                            parsed = json.loads(response_body)
                            require(parsed.get("model") in contract["settlement_models"],
                                    "reader response model drift")
                            generation_id = parsed.get("id")
                            require(isinstance(generation_id, str) and generation_id,
                                    "reader omitted generation id")
                        except Exception:
                            audit.update({
                                "audit_status": "invalid",
                                "audit_error": "reader_response_contract_invalid",
                                "response_sha256": hashlib.sha256(response_body).hexdigest(),
                            })
                        else:
                            audit.update({
                                "audit_status": "receipt_pending",
                                "generation_id": generation_id,
                                "response_sha256": hashlib.sha256(response_body).hexdigest(),
                            })
                        atomic_write_json(audit_path, audit)
                        break
            except Exception:
                if response_body is None:
                    response_body = canonical_bytes({"error": {
                        "message": "reader route contract rejected",
                        "type": "reader_route_proof",
                    }})
            self.send_response(status)
            self.send_header("content-type", "application/json")
            self.send_header("content-length", str(len(response_body)))
            self.end_headers()
            self.wfile.write(response_body)

    server = ThreadingHTTPServer(("127.0.0.1", _free_port()), Handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    return server, f"http://127.0.0.1:{server.server_port}"


def _reconcile_reader_receipt(
    api_key: str,
    audit_path: Path,
    manifest: dict,
    *,
    attempts: int | None = None,
    delay_seconds: int | None = None,
) -> dict:
    audit = json.loads(audit_path.read_text())
    if audit.get("audit_status") != "receipt_pending":
        return audit
    generation_id = audit.get("generation_id")
    require(isinstance(generation_id, str) and generation_id,
            "pending reader receipt lacks generation id")
    contract = manifest["protocol"]["reader"]
    attempt_limit = (
        attempts if attempts is not None else int(contract["receipt_reconciliation_attempts"])
    )
    delay = delay_seconds if delay_seconds is not None else int(
        contract["receipt_reconciliation_delay_seconds"]
    )
    require(attempt_limit > 0 and delay >= 0, "invalid reader reconciliation bounds")
    allowed_models = {item.lower() for item in contract["settlement_models"]}
    for attempt in range(1, attempt_limit + 1):
        settlement = None
        try:
            candidate = _json_url(
                "https://openrouter.ai/api/v1/generation?id="
                + urllib.parse.quote(generation_id),
                api_key,
            ).get("data")
            if isinstance(candidate, dict):
                settlement = candidate
        except Exception:
            pass
        if settlement is not None:
            provider = settlement.get("provider_name")
            model = str(settlement.get("model", "")).lower()
            if provider is not None and provider != "DeepInfra":
                audit.update({
                    "audit_status": "invalid",
                    "audit_error": "reader_settled_provider_drift",
                    "receipt_attempts": attempt,
                })
                atomic_write_json(audit_path, audit)
                return audit
            if model and model not in allowed_models:
                audit.update({
                    "audit_status": "invalid",
                    "audit_error": "reader_settled_model_drift",
                    "receipt_attempts": attempt,
                })
                atomic_write_json(audit_path, audit)
                return audit
            fields = ("tokens_prompt", "tokens_completion", "total_cost")
            if provider == "DeepInfra" and model and all(
                settlement.get(field) is not None for field in fields
            ):
                audit.update({
                    "audit_status": "settled",
                    "provider_name": settlement["provider_name"],
                    "model": settlement["model"],
                    "tokens_prompt": settlement["tokens_prompt"],
                    "tokens_completion": settlement["tokens_completion"],
                    "total_cost": settlement["total_cost"],
                    "receipt_attempts": attempt,
                })
                atomic_write_json(audit_path, audit)
                return audit
        if attempt < attempt_limit:
            time.sleep(delay)
    audit.update({
        "audit_status": "unresolved",
        "audit_error": "reader_generation_receipt_unresolved",
        "receipt_attempts": attempt_limit,
    })
    atomic_write_json(audit_path, audit)
    return audit


def _judge_proxy(api_key: str, audit_dir: Path, manifest: dict) -> tuple[ThreadingHTTPServer, str]:
    contract = manifest["protocol"]["judge"]
    lock = threading.Lock()
    call_count = 0
    dispatched = False

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *_args: object) -> None:
            return None

        def do_POST(self) -> None:
            nonlocal call_count, dispatched
            response_body: bytes | None = None
            status = 422
            try:
                with lock:
                    require(not dispatched, "judge proxy dispatch already consumed")
                    dispatched = True
                require(self.path == "/chat/completions", "judge proxy path denied")
                body = self.rfile.read(int(self.headers.get("content-length", "0")))
                request = json.loads(body)
                require(request.get("model") == contract["model"], "judge snapshot drift")
                require(request.get("reasoning_effort") == "medium", "judge reasoning drift")
                max_tokens = request.get("max_completion_tokens", request.get("max_tokens"))
                require(max_tokens == contract["max_completion_tokens"], "judge completion cap drift")
                prompt_chars = len(canonical_bytes(request.get("messages", [])))
                worst_micros = liability_micros(
                    prompt_chars, contract["input_price_micros_per_million"]
                ) + liability_micros(max_tokens, contract["output_price_micros_per_million"])
                reader_audit = json.loads((audit_dir.parent / "reader-route.json").read_text())
                reader_reserve = int(reader_audit["max_liability_micros"])
                require(
                    worst_micros + reader_reserve
                    <= manifest["campaign_spend"]["reader_and_judge_max_liability_micros_per_row"],
                    "judge request exceeds row spend reserve",
                )
                request_body = canonical_bytes(request)
                audit = {
                    "audit_status": "pending", "dispatch_count": 1,
                    "request_contract_sha256": hashlib.sha256(request_body).hexdigest(),
                    "max_liability_micros": worst_micros,
                }
                audit_dir.mkdir(parents=True, exist_ok=True)
                audit_path = audit_dir / "0001.json"
                atomic_write_json(audit_path, audit)
                upstream = urllib.request.Request(
                    "https://api.openai.com/v1/chat/completions", data=request_body,
                    method="POST", headers={"Authorization": f"Bearer {api_key}", "Content-Type": "application/json"},
                )
                with _direct_opener().open(upstream, timeout=300) as response:
                    response_body = response.read()
                    status = response.status
                parsed = json.loads(response_body)
                audit.update({"response_id": parsed.get("id"),
                              "response_sha256": hashlib.sha256(response_body).hexdigest(),
                              "response": parsed})
                try:
                    require(parsed.get("model") == contract["model"], "judge observed snapshot drift")
                    usage = parsed.get("usage")
                    require(isinstance(usage, dict), "judge response omitted usage")
                    input_tokens = usage.get("prompt_tokens")
                    output_tokens = usage.get("completion_tokens")
                    require(isinstance(input_tokens, int) and isinstance(output_tokens, int),
                            "judge usage is incomplete")
                    cached = (usage.get("prompt_tokens_details") or {}).get("cached_tokens", 0)
                    reasoning = (usage.get("completion_tokens_details") or {}).get("reasoning_tokens", 0)
                    require(isinstance(cached, int) and isinstance(reasoning, int), "judge detailed usage invalid")
                    cost_micros = (
                        (input_tokens - cached) * contract["input_price_micros_per_million"]
                        + cached * contract["cached_input_price_micros_per_million"]
                        + output_tokens * contract["output_price_micros_per_million"] + 999_999
                    ) // 1_000_000
                    audit.update({
                        "audit_status": "settled", "model": parsed["model"],
                        "input_tokens": input_tokens, "cached_input_tokens": cached,
                        "output_tokens": output_tokens, "reasoning_tokens": reasoning,
                        "cost_micros": cost_micros,
                    })
                except Exception:
                    audit.update({
                        "audit_status": "invalid",
                        "audit_error": "judge_response_audit_invalid",
                    })
                with lock:
                    call_count += 1
                atomic_write_json(audit_path, audit)
            except Exception:
                if response_body is None:
                    response_body = canonical_bytes({"error": {
                        "message": "judge route contract rejected",
                        "type": "judge_route_proof",
                    }})
            self.send_response(status)
            self.send_header("content-type", "application/json")
            self.send_header("content-length", str(len(response_body)))
            self.end_headers()
            self.wfile.write(response_body)

    server = ThreadingHTTPServer(("127.0.0.1", _free_port()), Handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    return server, f"http://127.0.0.1:{server.server_port}"


def _fingerprint(path: Path) -> dict[str, object]:
    require(path.is_file(), f"binary missing: {path}")
    return {"path": str(path.resolve()), "bytes": path.stat().st_size, "sha256": sha256_file(path)}


def _reservation(row: dict, manifest: dict) -> dict[str, object]:
    deep = 0 if row["arm"] == "fast" else int(manifest["protocol"]["deep_limits"]["max_spend_micros"])
    external = int(manifest["campaign_spend"]["reader_and_judge_max_liability_micros_per_row"])
    return {
        "row_id": row["row_id"], "max_liability_micros": deep + external,
        "deep_hard_cap_micros": deep, "reader_and_judge_reserve_micros": external,
        "charged_before_dispatch": True,
    }


def _audit_cost(audit: dict) -> tuple[int, int]:
    maximum = int(audit.get("max_liability_micros", 0))
    if audit.get("audit_status") != "settled":
        return 0, maximum
    if "total_cost" in audit:
        settled = usd_to_micros(audit["total_cost"])
    else:
        settled = int(audit["cost_micros"])
    require(settled <= maximum, "settled proxy cost exceeds its reservation")
    return settled, 0


def _deep_evidence(row_dir: Path) -> dict | None:
    proof_dir = row_dir / "memory-proofs"
    paths = list(proof_dir.glob("*.json")) if proof_dir.exists() else []
    if not paths:
        return None
    require(len(paths) == 1, "row has multiple memory proofs")
    memory = json.loads(paths[0].read_text())
    return ((memory.get("public") or {}).get("recall_response") or {}).get("deep")


def _deep_receipt_payload_agrees(payload: dict, deep: dict, candidate: dict) -> bool:
    generation_ids = deep.get("generation_ids")
    usage = deep.get("usage")
    if (
        not isinstance(generation_ids, list)
        or not generation_ids
        or not all(isinstance(item, str) and item for item in generation_ids)
        or len(set(generation_ids)) != len(generation_ids)
        or not isinstance(usage, dict)
        or payload.get("audit_status") != "settled"
        or payload.get("generation_ids") != generation_ids
    ):
        return False
    receipts = payload.get("receipts")
    if not isinstance(receipts, list) or [item.get("id") for item in receipts] != generation_ids:
        return False
    for receipt in receipts:
        if (
            str(receipt.get("provider_name", "")).lower() != "azure"
            or receipt.get("model") != candidate["model"]
            or not isinstance(receipt.get("tokens_prompt"), int)
            or isinstance(receipt.get("tokens_prompt"), bool)
            or receipt["tokens_prompt"] < 0
            or not isinstance(receipt.get("tokens_completion"), int)
            or isinstance(receipt.get("tokens_completion"), bool)
            or receipt["tokens_completion"] < 0
            or not isinstance(receipt.get("total_cost_micros"), int)
            or isinstance(receipt.get("total_cost_micros"), bool)
            or receipt["total_cost_micros"] < 0
        ):
            return False
    return (
        int(usage.get("unsettled_context_tokens_upper_bound", -1)) == 0
        and int(usage.get("unsettled_spend_micros_upper_bound", -1)) == 0
        and sum(item["tokens_prompt"] for item in receipts)
        == int(usage.get("context_tokens", -1))
        and sum(item["total_cost_micros"] for item in receipts)
        == int(usage.get("spend_micros", -1))
    )


def _archive_deep_generation_receipts(
    row_dir: Path, row: dict, manifest: dict, api_key: str
) -> None:
    if row["arm"] == "fast":
        return
    deep = _deep_evidence(row_dir)
    if not isinstance(deep, dict):
        return
    generation_ids = deep.get("generation_ids")
    payload: dict[str, object] = {
        "audit_status": "invalid",
        "failure_code": "deep_generation_receipt_invalid",
        "generation_ids": generation_ids if isinstance(generation_ids, list) else [],
        "receipts": [],
    }
    if (
        not isinstance(generation_ids, list)
        or not generation_ids
        or not all(isinstance(item, str) and item for item in generation_ids)
        or len(set(generation_ids)) != len(generation_ids)
    ):
        atomic_write_json(row_dir / "deep-generation-receipts.json", payload)
        return
    receipts: list[dict[str, object]] = []
    for generation_id in generation_ids:
        settled = None
        for _ in range(10):
            try:
                response = _json_url(
                    "https://openrouter.ai/api/v1/generation?id="
                    + urllib.parse.quote(generation_id),
                    api_key,
                )
                settled = response.get("data")
                if isinstance(settled, dict):
                    break
            except Exception:
                pass
            time.sleep(1)
        if not isinstance(settled, dict):
            break
        try:
            receipts.append({
                "id": settled["id"],
                "provider_name": settled["provider_name"],
                "model": settled["model"],
                "tokens_prompt": settled["tokens_prompt"],
                "tokens_completion": settled["tokens_completion"],
                "total_cost_micros": usd_to_micros(settled["total_cost"]),
            })
        except (KeyError, TypeError, ValueError):
            break
    payload["receipts"] = receipts
    candidate = manifest["protocol"]["deep_candidates"][row["arm"]]
    candidate_payload = {**payload, "audit_status": "settled"}
    candidate_payload.pop("failure_code", None)
    if _deep_receipt_payload_agrees(candidate_payload, deep, candidate):
        payload = candidate_payload
    atomic_write_json(row_dir / "deep-generation-receipts.json", payload)


def _row_settlement(row_dir: Path, row: dict, reservation: dict, *, orphaned: bool) -> dict[str, object]:
    deep_settled = 0
    deep_unsettled = 0
    if row["arm"] != "fast":
        deep = _deep_evidence(row_dir)
        receipt_path = row_dir / "deep-generation-receipts.json"
        candidate = load_campaign_manifest()["protocol"]["deep_candidates"][row["arm"]]
        if (
            isinstance(deep, dict)
            and receipt_path.is_file()
            and _deep_receipt_payload_agrees(
                json.loads(receipt_path.read_text()), deep, candidate
            )
        ):
            deep_settled = int(deep["usage"]["spend_micros"])
        else:
            deep_unsettled = int(reservation["deep_hard_cap_micros"])

    reader_settled = reader_unsettled = 0
    reader_path = row_dir / "reader-route.json"
    if reader_path.exists():
        reader_settled, reader_unsettled = _audit_cost(json.loads(reader_path.read_text()))
    judge_settled = judge_unsettled = 0
    for path in sorted((row_dir / "judge-routes").glob("*.json")) if (row_dir / "judge-routes").exists() else []:
        settled, unsettled = _audit_cost(json.loads(path.read_text()))
        judge_settled += settled
        judge_unsettled += unsettled
    settled = deep_settled + reader_settled + judge_settled
    unsettled = deep_unsettled + reader_unsettled + judge_unsettled
    maximum = int(reservation["max_liability_micros"])
    if orphaned:
        unsettled = maximum - settled
    require(0 <= settled <= maximum and 0 <= unsettled <= maximum - settled,
            "row accounting exceeds its pre-dispatch reservation")
    return {
        "row_id": row["row_id"], "reservation_sha256": canonical_sha256(reservation),
        "max_liability_micros": maximum, "settled_micros": settled,
        "unsettled_upper_bound_micros": unsettled,
        "deep_settled_micros": deep_settled, "deep_unsettled_upper_bound_micros": deep_unsettled,
        "reader_settled_micros": reader_settled,
        "reader_unsettled_upper_bound_micros": reader_unsettled,
        "judge_settled_micros": judge_settled,
        "judge_unsettled_upper_bound_micros": judge_unsettled,
        "orphaned_attempt": orphaned,
    }


def _write_row_proof(row_dir: Path, row: dict, reservation_path: Path, outcome: str,
                     extra: dict[str, object] | None = None, *, orphaned: bool = False) -> dict:
    manifest = load_campaign_manifest()
    reservation = json.loads(reservation_path.read_text())
    require(reservation == _reservation(row, manifest), "row reservation drift")
    settlement = _row_settlement(row_dir, row, reservation, orphaned=orphaned)
    atomic_write_json(row_dir / "spend-settlement.json", settlement)
    root_path = row_dir.parent / "pre-execution-proof.json"
    root_proof = json.loads(root_path.read_text())
    case_contract = root_proof["materialization"]["cases"][row["question_id"]]
    expected_config_hash = (
        None if row["arm"] == "fast"
        else manifest["protocol"]["deep_candidates"][row["arm"]]["config_sha256"]
    )
    deep = _deep_evidence(row_dir)
    actual_config_hash = None
    memory_paths = list((row_dir / "memory-proofs").glob("*.json")) if (row_dir / "memory-proofs").exists() else []
    if memory_paths:
        require(len(memory_paths) == 1, "row has multiple memory proofs")
        memory = json.loads(memory_paths[0].read_text())
        actual_config_hash = ((memory.get("public") or {}).get("trace") or {}).get(
            "l4_config_hash"
        )
    proof = {
        "row": row, "outcome": outcome, "operational": outcome == "success",
        "reservation_sha256": sha256_file(reservation_path),
        "settlement_sha256": sha256_file(row_dir / "spend-settlement.json"),
        "pre_execution_proof_sha256": sha256_file(root_path),
        "case_materialization_contract_sha256": canonical_sha256(case_contract),
        "frozen_binaries": root_proof["binaries"],
        "expected_deep_config_hash": expected_config_hash,
        "observed_deep_config_hash": actual_config_hash,
        "deep_config_hash_bound": actual_config_hash == expected_config_hash,
        "git_commit": subprocess.run(["git", "rev-parse", "HEAD"], cwd=ROOT,
                                     capture_output=True, text=True, check=True).stdout.strip(),
        "manifest_sha256": sha256_file(CAMPAIGN_MANIFEST), "immutable": True, "complete": True,
    }
    proof.update(extra or {})
    proof["artifact_hashes"] = artifact_hashes(row_dir, exclude={"row-proof.json"})
    atomic_write_json(row_dir / "row-proof.json", proof)
    return proof


def _pid_alive(pid: object) -> bool:
    if not isinstance(pid, int) or pid <= 0:
        return False
    try:
        os.kill(pid, 0)
    except OSError:
        return False
    return True


def verify_resume_contract(frozen: dict, current: dict) -> None:
    for field in (
        "manifest_sha256", "run_order_sha256", "outputs_observed_before_freeze",
        "materialization", "git_commit", "binaries", "deep_prompt_sha256",
        "deep_config_hashes", "environment_contract_sha256", "python_environment",
        "binary_profile",
        "preexisting_campaign_liability",
    ):
        require(frozen[field] == current[field], f"campaign resume contract drift: {field}")
    require({key: value["material_contract_sha256"] for key, value in frozen["endpoint_hashes"].items()}
            == {key: value["material_contract_sha256"] for key, value in current["endpoint_hashes"].items()},
            "campaign material endpoint contract drift")


def _treatment_operational(public: dict, row: dict, manifest: dict, *, truncated: bool) -> bool:
    if truncated:
        return False
    deep = public["recall_response"].get("deep")
    trace = public["trace"]
    if row["arm"] == "fast":
        return deep is None and trace.get("deep") is None
    configured = manifest["protocol"]["deep_candidates"][row["arm"]]
    return bool(
        deep and deep["status"] == "completed" and deep["stop_reason"] == "completed"
        and deep["usage"]["unsettled_context_tokens_upper_bound"] == 0
        and deep["usage"]["unsettled_spend_micros_upper_bound"] == 0
        and trace.get("deep") == deep and deep.get("generation_ids")
        and trace.get("l4_model") == configured["model"]
        and str(trace.get("l4_provider", "")).lower() == "azure"
        and str(trace.get("l4_observed_provider", "")).lower() == "azure"
        and trace.get("l4_observed_model") == configured["model"]
        and trace.get("l4_prompt_hash") == manifest["protocol"]["deep_prompt_sha256"]
        and trace.get("l4_config_hash") == configured["config_sha256"]
    )


def _wait_health(base_url: str, process: subprocess.Popen) -> None:
    for _ in range(120):
        require(process.poll() is None, "MemPhant server exited before health")
        try:
            urllib.request.urlopen(base_url + "/v1/health", timeout=1).close()
            return
        except urllib.error.URLError:
            time.sleep(0.5)
    raise RuntimeError("MemPhant server health timed out")


def _terminate_and_reap(process: subprocess.Popen) -> None:
    process.terminate()
    try:
        process.wait(timeout=10)
    except subprocess.TimeoutExpired:
        process.kill()
        process.wait()


def _terminate_process_group_and_reap(process: subprocess.Popen) -> None:
    """Stop a scratch-helper process tree while preserving its EXIT cleanup trap."""
    try:
        os.killpg(process.pid, signal.SIGTERM)
    except ProcessLookupError:
        pass
    try:
        process.wait(timeout=10)
    except subprocess.TimeoutExpired:
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        process.wait()


def _wait_and_reap_on_interrupt(process: subprocess.Popen) -> int:
    try:
        return process.wait()
    except BaseException:
        _terminate_process_group_and_reap(process)
        raise


def _run_logged_harness(
    command: list[str], *, cwd: Path, environment: dict[str, str], row_dir: Path
) -> subprocess.CompletedProcess:
    with (row_dir / "official.stdout").open("wb") as stdout, (
        row_dir / "official.stderr"
    ).open("wb") as stderr:
        return subprocess.run(
            command,
            cwd=cwd,
            env=environment,
            stdout=stdout,
            stderr=stderr,
            check=False,
        )


def run_reader_route_preflight(output: Path, manifest: dict) -> dict[str, object]:
    """Exercise one tiny paid reader dispatch and settle it without replay."""
    output = output.resolve()
    api_key = os.environ.get("OPENROUTER_API_KEY", "")
    require(api_key, "reader route preflight requires OPENROUTER_API_KEY")
    require(not os.environ.get("OPENAI_API_KEY"),
            "reader route preflight forbids the judge credential")
    require(not output.exists(), "reader route preflight output must be new")
    endpoint_proof = verify_endpoint_inventory(manifest)["reader"]
    output.mkdir(parents=True)
    audit_path = output / "reader-route.json"
    proxy, base_url = _reader_proxy(api_key, audit_path, manifest)
    request = _reader_route_probe_request()
    response_body = b""
    response_status: int | None = None
    transport_error: str | None = None
    started = time.perf_counter()
    try:
        parsed_url = urllib.parse.urlparse(base_url)
        connection = http.client.HTTPConnection(
            parsed_url.hostname,
            parsed_url.port,
            timeout=manifest["protocol"]["reader"]["upstream_timeout_seconds"] + 15,
        )
        try:
            connection.request(
                "POST",
                "/chat/completions",
                body=canonical_bytes(request),
                headers={"content-type": "application/json"},
            )
            response = connection.getresponse()
            response_status = response.status
            response_body = response.read()
        finally:
            connection.close()
    except Exception as error:
        transport_error = type(error).__name__
    finally:
        proxy.shutdown()
        proxy.server_close()
    response_elapsed_ms = int(round((time.perf_counter() - started) * 1000))
    if audit_path.is_file():
        audit = _reconcile_reader_receipt(api_key, audit_path, manifest)
    else:
        audit = {
            "audit_status": "invalid",
            "audit_error": "reader_proxy_emitted_no_audit",
            "dispatch_count": 0,
        }
        atomic_write_json(audit_path, audit)
    settled_cost_micros, unsettled_cost_micros = _audit_cost(audit)
    successful = (
        response_status == 200
        and audit.get("audit_status") == "settled"
        and unsettled_cost_micros == 0
    )
    proof = {
        "schema_version": 1,
        "classification": (
            "tiny_reader_route_authorization"
            if successful else "tiny_reader_route_failure"
        ),
        "git_commit": subprocess.run(
            ["git", "rev-parse", "HEAD"], cwd=ROOT,
            capture_output=True, text=True, check=True,
        ).stdout.strip(),
        "request": {
            "request_sha256": canonical_sha256(request),
            "max_completion_tokens": request["max_tokens"],
            "reasoning_enabled": True,
            "dispatch_count": audit.get("dispatch_count"),
            "max_liability_micros": audit.get("max_liability_micros"),
        },
        "response": {
            "status": response_status,
            "sha256": hashlib.sha256(response_body).hexdigest(),
            "elapsed_ms": response_elapsed_ms,
            "transport_error_type": transport_error,
        },
        "settlement": {
            key: audit.get(key)
            for key in (
                "audit_status", "generation_id", "provider_name", "model",
                "tokens_prompt", "tokens_completion", "total_cost", "receipt_attempts",
            )
        },
        "settled_cost_micros": settled_cost_micros,
        "endpoint_contract": endpoint_proof,
        "reader_route_sha256": sha256_file(audit_path),
        "paid_calls": 1 if audit.get("dispatch_count") == 1 else 0,
        "same_request_retry_authorized": False,
    }
    atomic_write_json(output / "PROOF.json", proof)
    _redact_secrets(output, [api_key])
    require(successful, "reader route preflight did not settle successfully")
    require(audit.get("provider_name") == "DeepInfra", "reader route preflight provider drift")
    require(str(audit.get("model", "")).lower() in {
        model.lower() for model in manifest["protocol"]["reader"]["settlement_models"]
    }, "reader route preflight model drift")
    return proof


def run_context_preflight(
    directory: Path, materialized: Path, output: Path, manifest: dict
) -> dict[str, object]:
    directory, materialized, output = _resolve_execution_paths(
        directory, materialized, output
    )
    require(os.environ.get("MEMPHANT_SCRATCH_ACTIVE") == "1",
            "context preflight requires scratch database")
    database_url = os.environ.get("MEMPHANT_TEST_DATABASE_URL", "")
    require(database_url, "context preflight scratch database URL missing")
    require(not os.environ.get("OPENROUTER_API_KEY") and not os.environ.get("OPENAI_API_KEY"),
            "context preflight forbids external model credentials")
    require(not output.exists(), "context preflight output must be new")
    output.mkdir(parents=True)
    proof_dir = output / "memory-proofs"
    proof_dir.mkdir()
    first_case_id = manifest["run_order"]["case_order"][0]
    require(first_case_id == "19367bc7", "context preflight first case drift")
    case_dir = materialized / first_case_id
    questions = json.loads((case_dir / "questions.json").read_text())
    haystacks = json.loads((case_dir / "haystack.json").read_text())
    require(isinstance(questions, list) and len(questions) == 1,
            "context preflight requires one question")
    question = questions[0]
    require(question.get("id") == first_case_id, "context preflight question drift")
    question_text = question.get("question")
    require(isinstance(question_text, str) and question_text,
            "context preflight question text missing")
    selected_ids = haystacks.get(first_case_id)
    require(isinstance(selected_ids, list) and len(selected_ids) == 500,
            "context preflight haystack drift")
    trajectories = _load_selected_trajectories(
        directory / "data/trajectories.jsonl", selected_ids
    )

    binaries = {name: _binary_path(name) for name in ("server", "worker", "cli")}
    server_port = _free_port()
    server_url = f"http://127.0.0.1:{server_port}"
    server_env = _clean_environment({
        "MEMPHANT_APP_DATABASE_URL": database_url,
        "MEMPHANT_AUTHN_DATABASE_URL": database_url,
        "MEMPHANT_BIND": f"127.0.0.1:{server_port}",
        "MEMPHANT_RESOURCE_CHUNKS": "on",
        "MEMPHANT_STRUCTURED_STATE": "off",
        "MEMPHANT_DEEP": "off",
    })
    server_stdout = (output / "server.stdout").open("wb")
    server_stderr = (output / "server.stderr").open("wb")
    server = subprocess.Popen(
        [str(binaries["server"])],
        env=server_env,
        stdout=server_stdout,
        stderr=server_stderr,
    )
    server_stdout.close()
    server_stderr.close()
    server_stopped = False
    try:
        _wait_health(server_url, server)
        adapter_environment = {
            "MEMPHANT_LME_SERVER_URL": server_url,
            "MEMPHANT_CLI_BIN": str(binaries["cli"]),
            "MEMPHANT_LME_SERVER_BIN": str(binaries["server"]),
            "MEMPHANT_LME_WORKER_BIN": str(binaries["worker"]),
            "MEMPHANT_LME_PROOF_DIR": str(proof_dir),
            "MEMPHANT_LME_RUN_ID": "p1-t6-context-preflight",
            "HF_HUB_OFFLINE": "1",
            "TRANSFORMERS_OFFLINE": "1",
        }
        official_path = str(directory / "official")
        adapter_path = str(ROOT / "benchmarks/longmemeval_v2")
        for path in (adapter_path, official_path):
            if path not in sys.path:
                sys.path.insert(0, path)
        import memphant_memory
        from evaluation.harness import count_memory_context_tokens

        with _temporary_environment(adapter_environment):
            memory_config = json.loads((case_dir / "memory.fast.json").read_text())
            memory = memphant_memory.MemphantMemory(memory_config["memory_params"])
            insert_started = time.perf_counter()
            for trajectory_id in selected_ids:
                memory.insert(trajectories[trajectory_id])
            insert_ms = int(round((time.perf_counter() - insert_started) * 1000))
            memory.set_query_context(question_id=first_case_id)
            query_started = time.perf_counter()
            memory_context = memory.query(question_text)
            query_ms = int(round((time.perf_counter() - query_started) * 1000))
            metadata = memory.post_query_hook(
                query=question_text,
                query_image=None,
                memory_context=memory_context,
            )
            require(all(item.get("type") == "text" for item in memory_context),
                    "context preflight unexpectedly returned image context")
            exact_tokens = count_memory_context_tokens(
                memory_context, [None] * len(memory_context)
            )
        require(isinstance(metadata, dict), "context preflight query metadata missing")
        memory_paths = list(proof_dir.glob("*.json"))
        require(len(memory_paths) == 1, "context preflight requires one memory proof")
        memory_proof = json.loads(memory_paths[0].read_text())
        contract_audit = _context_contract_audit(
            memory_context,
            memory_proof["public"],
            exact_tokens,
            int(memory_config["memory_params"]["budget_tokens"]),
        )
        queue = subprocess.run(
            [
                "psql", database_url, "-At", "-F", "\t", "-c",
                "select count(*) filter (where state in ('queued','running')), "
                "count(*) filter (where state = 'dead') from memphant.job_state",
            ],
            capture_output=True,
            text=True,
            check=True,
        ).stdout.strip().split("\t")
        require(queue == ["0", "0"], "context preflight left pending or dead jobs")
        _terminate_and_reap(server)
        server_stopped = True
        proof = {
            "schema_version": 1,
            "classification": "no_model_release_exact_context_authorization",
            "git_commit": subprocess.run(
                ["git", "rev-parse", "HEAD"], cwd=ROOT,
                capture_output=True, text=True, check=True,
            ).stdout.strip(),
            "binary_profile": PRODUCTION_BINARY_PROFILE,
            "question_id": first_case_id,
            "trajectory_count": len(selected_ids),
            "resource_count": memory.resource_count,
            "timing_ms": {"insert": insert_ms, "worker_and_recall": query_ms},
            "context_contract": contract_audit,
            "worker": memory.worker_proof,
            "post_recall": {"pending_jobs": 0, "dead_jobs": 0},
            "binaries": memory.binaries,
            "artifacts": {
                "memory_proof_sha256": sha256_file(memory_paths[0]),
                "server_stdout_sha256": sha256_file(output / "server.stdout"),
                "server_stderr_sha256": sha256_file(output / "server.stderr"),
            },
            "external_dispatch": {
                "reader_endpoint_configured": False,
                "judge_endpoint_configured": False,
                "reader_key_configured": False,
                "judge_key_configured": False,
                "deep_enabled": False,
            },
            "query_metadata": metadata,
        }
        atomic_write_json(output / "PROOF.json", proof)
        return proof
    finally:
        if not server_stopped:
            _terminate_and_reap(server)
        _redact_secrets(output, [database_url])


def run_row(directory: Path, materialized: Path, output: Path, row: dict, manifest: dict) -> dict:
    directory, materialized, output = _resolve_execution_paths(
        directory, materialized, output
    )
    require(os.environ.get("MEMPHANT_SCRATCH_ACTIVE") == "1", "row requires scratch database")
    database_url = os.environ.get("MEMPHANT_TEST_DATABASE_URL", "")
    require(database_url, "scratch database URL missing")
    openrouter_key = os.environ.get("OPENROUTER_API_KEY", "")
    openai_key = os.environ.get("OPENAI_API_KEY", "")
    require(openrouter_key and openai_key, "OPENROUTER_API_KEY and OPENAI_API_KEY are required")
    final_dir = output / row["row_id"]
    require_new_row_dir(final_dir)
    row_dir = output / (".staging-" + row["row_id"])
    require(row_dir.is_dir() and (row_dir / "attempt.json").is_file(),
            f"row lacks pre-dispatch attempt marker: {row_dir}")
    attempt = json.loads((row_dir / "attempt.json").read_text())
    require(attempt["row"] == row and attempt["dispatch_started"] is True, "attempt marker drift")
    reservation_path = output / "spend-ledger" / f"{row['sequence']:04d}.json"
    proxy, reader_url = _reader_proxy(openrouter_key, row_dir / "reader-route.json", manifest)
    judge_proxy, judge_url = _judge_proxy(openai_key, row_dir / "judge-routes", manifest)
    port = _free_port()
    server_url = f"http://127.0.0.1:{port}"
    binaries = {name: _binary_path(name) for name in ("server", "worker", "cli")}
    server_env = _clean_environment({
        "MEMPHANT_APP_DATABASE_URL": database_url,
        "MEMPHANT_AUTHN_DATABASE_URL": database_url,
        "MEMPHANT_BIND": f"127.0.0.1:{port}",
        "MEMPHANT_RESOURCE_CHUNKS": "on",
        "MEMPHANT_STRUCTURED_STATE": "off",
    })
    arm = row["arm"]
    if arm == "fast":
        server_env["MEMPHANT_DEEP"] = "off"
    else:
        candidate = manifest["protocol"]["deep_candidates"][arm]
        server_env.update({
            "OPENROUTER_API_KEY": openrouter_key,
            "MEMPHANT_DEEP": "on", "MEMPHANT_DEEP_MODEL": candidate["model"],
            "MEMPHANT_DEEP_PROMPT_PATH": str(ROOT / "config/deep-recall-v1.txt"),
            "MEMPHANT_DEEP_PROVIDERS": "azure",
            "MEMPHANT_DEEP_INPUT_PRICE_MICROS_PER_MILLION": str(candidate["input_price_micros_per_million"]),
            "MEMPHANT_DEEP_OUTPUT_PRICE_MICROS_PER_MILLION": str(candidate["output_price_micros_per_million"]),
        })
    server = subprocess.Popen(
        [str(binaries["server"])], env=server_env,
        stdout=(row_dir / "server.stdout").open("wb"),
        stderr=(row_dir / "server.stderr").open("wb"),
    )
    exit_code = -1
    try:
        _wait_health(server_url, server)
        case_dir = materialized / row["question_id"]
        root_proof = json.loads((output / "pre-execution-proof.json").read_text())
        require(
            root_proof["environment_contract_sha256"]
            == canonical_sha256(_clean_environment()),
            "row ambient environment differs from frozen allowlist contract",
        )
        verify_case_materialization(case_dir, root_proof["materialization"]["cases"][row["question_id"]])
        proof_dir = row_dir / "memory-proofs"
        proof_dir.mkdir()
        child_env = _clean_environment({
            "MEMPHANT_SCRATCH_ACTIVE": "1",
            "MEMPHANT_TEST_DATABASE_URL": database_url,
            "MEMPHANT_LME_SERVER_URL": server_url,
            "MEMPHANT_CLI_BIN": str(binaries["cli"]),
            "MEMPHANT_LME_SERVER_BIN": str(binaries["server"]),
            "MEMPHANT_LME_WORKER_BIN": str(binaries["worker"]),
            "MEMPHANT_LME_PROOF_DIR": str(proof_dir),
            "MEMPHANT_LME_RUN_ID": row["row_id"],
            "LME_READER_PROXY_KEY": "loopback-route-bound",
            "LME_JUDGE_PROXY_KEY": "loopback-route-bound",
        })
        sys.path.insert(0, str(ROOT / "scripts"))
        import run_longmemeval_v2 as official_adapter
        command = official_adapter.memphant_harness_command(
            official_dir=directory / "official", domain=next(
                case["domain"] for case in manifest["selection"]["cases"] if case["id"] == row["question_id"]
            ),
            questions_path=case_dir / "questions.json", haystack_path=case_dir / "haystack.json",
            trajectories_path=directory / "data/trajectories.jsonl",
            memory_config_path=case_dir / ("memory.fast.json" if arm == "fast" else "memory.deep.json"),
            output_dir=row_dir / "official", reader_model="Qwen/Qwen3.5-9B",
            reader_base_url=reader_url, evaluator_model="gpt-5.2-2025-12-11",
            evaluator_base_url=judge_url,
        )
        command += [
            "--api-key-env", "LME_READER_PROXY_KEY",
            "--evaluator-api-key-env", "LME_JUDGE_PROXY_KEY",
            "--evaluator-reasoning-effort", "medium",
            "--prompt-build-max-workers", "1", "--reader-max-concurrent-requests", "1",
        ]
        completed = _run_logged_harness(
            command,
            cwd=directory / "official",
            environment=child_env,
            row_dir=row_dir,
        )
        exit_code = completed.returncode
    finally:
        _terminate_and_reap(server)
        proxy.shutdown()
        proxy.server_close()
        judge_proxy.shutdown()
        judge_proxy.server_close()
        _redact_secrets(
            row_dir,
            _row_secret_values(openrouter_key, openai_key, database_url),
        )
    reader_audit_path = row_dir / "reader-route.json"
    if reader_audit_path.is_file():
        _reconcile_reader_receipt(
            openrouter_key, reader_audit_path, manifest
        )
    _archive_deep_generation_receipts(row_dir, row, manifest, openrouter_key)
    _redact_secrets(
        row_dir,
        _row_secret_values(openrouter_key, openai_key, database_url),
    )
    if exit_code != 0:
        atomic_write_json(row_dir / "failure.json", {
            "row": row, "official_exit_code": exit_code,
            "retry_authorized": False, "requires_generation_and_billing_audit": True,
        })
        proof = _write_row_proof(
            row_dir, row, reservation_path, "operational_failure",
            {"official_exit_code": exit_code},
        )
        os.replace(row_dir, final_dir)
        return proof
    memory_proofs = list((row_dir / "memory-proofs").glob("*.json"))
    require(len(memory_proofs) == 1, "row must archive exactly one memory proof")
    memory_proof = json.loads(memory_proofs[0].read_text())
    require("recall_response" in memory_proof["public"] and "trace" in memory_proof["public"],
            "row lacks full public recall and trace")
    require((row_dir / "reader-route.json").is_file(), "row lacks settled reader route proof")
    require(json.loads((row_dir / "reader-route.json").read_text())["audit_status"] == "settled",
            "reader route settlement is unresolved or invalid")
    per_question = row_dir / "official/per_question.jsonl"
    require(per_question.is_file() and len(per_question.read_text().splitlines()) == 1,
            "row lacks one official score")
    official_score = json.loads(per_question.read_text())
    judge_routes = sorted((row_dir / "judge-routes").glob("*.json"))
    eval_name = str(official_score.get("eval_function", "")).split("|", 1)[0]
    if eval_name.startswith("llm_"):
        require(len(judge_routes) == 1, "LLM-scored row requires exactly one judge proof")
        require(json.loads(judge_routes[0].read_text())["audit_status"] == "settled",
                "judge audit is unresolved or invalid")
    else:
        require(not judge_routes, "deterministic scorer unexpectedly called judge")
    treatment_operational = _treatment_operational(
        memory_proof["public"], row, manifest,
        truncated=bool(official_score["memory_context_was_truncated"]),
    )
    settlement_preview = _row_settlement(
        row_dir, row, json.loads(reservation_path.read_text()), orphaned=False
    )
    treatment_operational = (
        treatment_operational
        and settlement_preview["deep_unsettled_upper_bound_micros"] == 0
    )
    extra = {
        "official_exit_code": exit_code,
        "execution_complete": True, "treatment_operational": treatment_operational,
        "scratch_database_identity": database_url.rsplit("/", 1)[-1],
        "binaries": {name: _fingerprint(path) for name, path in binaries.items()},
        "memory_proof_sha256": sha256_file(memory_proofs[0]),
        "reader_route_sha256": sha256_file(row_dir / "reader-route.json"),
        "judge_route_sha256": canonical_sha256([sha256_file(path) for path in judge_routes]),
        "official_score_sha256": sha256_file(per_question),
    }
    proof = _write_row_proof(
        row_dir, row, reservation_path,
        "success" if treatment_operational else "operational_failure", extra,
    )
    os.replace(row_dir, final_dir)
    return proof


def run_campaign(directory: Path, materialized: Path, output: Path, base_database_url: str, manifest: dict) -> dict:
    directory, materialized, output = _resolve_execution_paths(
        directory, materialized, output
    )
    require(os.environ.get("OPENROUTER_API_KEY") and os.environ.get("OPENAI_API_KEY"),
            "OPENROUTER_API_KEY and OPENAI_API_KEY are required")
    preflight_proof = preflight(directory, materialized, manifest)
    endpoint_hashes = verify_endpoint_inventory(manifest)
    subprocess.run(_production_build_command(), cwd=ROOT, check=True)
    output.mkdir(parents=True, exist_ok=True)
    rows = expanded_run_order(manifest)
    frozen_binaries = {
        name: _fingerprint(_binary_path(name))
        for name in ("server", "worker", "cli")
    }
    root_contract = {
        "manifest_sha256": sha256_file(CAMPAIGN_MANIFEST), "endpoint_hashes": endpoint_hashes,
        "run_order_sha256": canonical_sha256(rows), "outputs_observed_before_freeze": False,
        "materialization": preflight_proof["materialization"],
        "git_commit": subprocess.run(["git", "rev-parse", "HEAD"], cwd=ROOT,
                                     capture_output=True, text=True, check=True).stdout.strip(),
        "binaries": frozen_binaries,
        "binary_profile": PRODUCTION_BINARY_PROFILE,
        "deep_prompt_sha256": sha256_file(ROOT / "config/deep-recall-v1.txt"),
        "deep_config_hashes": {
            name: candidate["config_sha256"]
            for name, candidate in manifest["protocol"]["deep_candidates"].items()
        },
        "python_environment": preflight_proof["python"],
        "environment_contract_sha256": canonical_sha256(_clean_environment()),
        "preexisting_campaign_liability": manifest["campaign_spend"][
            "preexisting_liability"
        ],
    }
    root_path = output / "pre-execution-proof.json"
    if root_path.exists():
        verify_resume_contract(json.loads(root_path.read_text()), root_contract)
    else:
        atomic_write_json(root_path, root_contract)
    order_path = output / "frozen-run-order.json"
    if order_path.exists():
        require(json.loads(order_path.read_text()) == rows, "frozen run order drift")
    else:
        atomic_write_json(order_path, rows)
    ledger = output / "spend-ledger"
    ledger.mkdir(exist_ok=True)
    for row in rows:
        ledger_row = ledger / f"{row['sequence']:04d}.json"
        expected_reservation = _reservation(row, manifest)
        if (output / row["row_id"]).is_dir():
            require(json.loads((output / row["row_id"] / "row-proof.json").read_text())["complete"] is True,
                    "completed row proof drift")
            require(json.loads(ledger_row.read_text()) == expected_reservation,
                    "completed row reservation drift")
            require((output / row["row_id"] / "spend-settlement.json").is_file(),
                    "completed row lacks settlement")
            continue
        staging = output / (".staging-" + row["row_id"])
        if staging.exists():
            attempt_path = staging / "attempt.json"
            if not attempt_path.exists():
                require(not any(staging.iterdir()), "unmarked staging contains ambiguous evidence")
                staging.rmdir()
            else:
                attempt = json.loads(attempt_path.read_text())
                require(not _pid_alive(attempt.get("child_pid")),
                        f"row attempt is still active: {row['row_id']}")
                atomic_write_json(staging / "failure.json", {
                    "row": row, "reason": "orphaned_attempt_recovered_without_replay",
                    "retry_authorized": False,
                })
                _write_row_proof(staging, row, ledger_row, "operational_failure",
                                 {"failure_reason": "orphaned_attempt"}, orphaned=True)
                os.replace(staging, output / row["row_id"])
                continue
        prior = sum(json.loads(path.read_text())["max_liability_micros"] for path in ledger.glob("*.json"))
        preexisting_liability = manifest["campaign_spend"]["preexisting_liability"][
            "total_micros"
        ]
        if ledger_row.exists():
            require(json.loads(ledger_row.read_text()) == expected_reservation,
                    "orphaned pre-dispatch reservation drift")
        else:
            require(preexisting_liability + prior + expected_reservation["max_liability_micros"]
                    <= usd_to_micros(manifest["campaign_spend"]["hard_ceiling_usd"]),
                    "campaign spend ceiling reached before dispatch")
            atomic_write_json(ledger_row, expected_reservation)
        staging.mkdir()
        attempt = {"row": row, "dispatch_started": True, "coordinator_pid": os.getpid(),
                   "child_pid": None, "reservation_sha256": sha256_file(ledger_row)}
        atomic_write_json(staging / "attempt.json", attempt)
        command = [
            "/bin/bash", str(SCRATCH_HELPER),
            base_database_url, "MEMPHANT_TEST_DATABASE_URL", sys.executable, __file__, "_run-row",
            "--directory", str(directory), "--output", str(output),
            "--materialized", str(materialized), "--row-id", row["row_id"],
        ]
        process = subprocess.Popen(
            command,
            cwd=ROOT,
            env=_clean_environment({
                "MEMPHANT_SCRATCH_ACTIVE": "1",
                "OPENROUTER_API_KEY": os.environ["OPENROUTER_API_KEY"],
                "OPENAI_API_KEY": os.environ["OPENAI_API_KEY"],
            }),
            start_new_session=True,
        )
        attempt["child_pid"] = process.pid
        if staging.exists():
            atomic_write_json(staging / "attempt.json", attempt)
        returncode = _wait_and_reap_on_interrupt(process)
        final_dir = output / row["row_id"]
        if returncode != 0:
            require(staging.is_dir(), "failed row lost its staging evidence")
            atomic_write_json(staging / "failure.json", {
                "row": row, "reason": "row_process_failed", "exit_code": returncode,
                "retry_authorized": False,
            })
            _write_row_proof(staging, row, ledger_row, "operational_failure",
                             {"failure_reason": "row_process_failed", "row_process_exit_code": returncode},
                             orphaned=True)
            os.replace(staging, final_dir)
        require(final_dir.is_dir(), f"row process did not finalize evidence: {row['row_id']}")
    return {"rows": len(rows), "output": str(output)}


def _percentile(values: list[int], fraction: float) -> int:
    require(values, "percentile requires values")
    require(0 < fraction <= 1, "percentile fraction is out of range")
    ordered = sorted(values)
    return ordered[math.ceil(fraction * len(ordered)) - 1]


def aggregate_campaign(output: Path, manifest: dict) -> dict[str, object]:
    rows = expanded_run_order(manifest)
    root_path = output / "pre-execution-proof.json"
    root_proof = json.loads(root_path.read_text())
    require(root_proof["manifest_sha256"] == sha256_file(CAMPAIGN_MANIFEST)
            and root_proof["run_order_sha256"] == canonical_sha256(rows),
            "pre-execution proof contract drift")
    require(root_proof["git_commit"] == subprocess.run(
        ["git", "rev-parse", "HEAD"], cwd=ROOT, capture_output=True, text=True, check=True
    ).stdout.strip(), "aggregate commit differs from frozen measured commit")
    require(root_proof["deep_prompt_sha256"] == manifest["protocol"]["deep_prompt_sha256"]
            == sha256_file(ROOT / "config/deep-recall-v1.txt"),
            "Deep prompt changed after execution freeze")
    require(root_proof["deep_config_hashes"] == {
        name: candidate["config_sha256"]
        for name, candidate in manifest["protocol"]["deep_candidates"].items()
    }, "frozen Deep runtime config hashes drifted")
    require(
        root_proof.get("binary_profile") == PRODUCTION_BINARY_PROFILE,
        "campaign did not freeze production release binaries",
    )
    require(root_proof["binaries"] == {
        name: _fingerprint(_binary_path(name))
        for name in ("server", "worker", "cli")
    }, "packaged binaries changed after execution freeze")
    root_sha256 = sha256_file(root_path)
    expected_row_ids = {row["row_id"] for row in rows}
    observed_row_ids = {
        path.name for path in output.iterdir()
        if path.is_dir() and not path.name.startswith(".")
        and path.name != "spend-ledger"
    }
    require(observed_row_ids == expected_row_ids, "missing or extra finalized rows")
    reservation_paths = sorted((output / "spend-ledger").glob("*.json"))
    settlement_paths = [output / row["row_id"] / "spend-settlement.json" for row in rows]
    require(len(reservation_paths) == len(rows) and all(path.is_file() for path in settlement_paths),
            "spend ledger is incomplete")
    reservations = [json.loads(path.read_text()) for path in reservation_paths]
    settlements = [json.loads(path.read_text()) for path in settlement_paths]
    require([item["row_id"] for item in reservations] == [row["row_id"] for row in rows],
            "spend reservation order drift")
    require([item["row_id"] for item in settlements] == [row["row_id"] for row in rows],
            "spend settlement order drift")
    preexisting_liability = manifest["campaign_spend"]["preexisting_liability"][
        "total_micros"
    ]
    require(preexisting_liability + sum(item["max_liability_micros"] for item in reservations)
            <= usd_to_micros(manifest["campaign_spend"]["hard_ceiling_usd"]),
            "spend reservations exceed campaign ceiling")
    require(all(item["settled_micros"] + item["unsettled_upper_bound_micros"]
                <= item["max_liability_micros"] for item in settlements),
            "row settlement exceeds reservation")
    require(preexisting_liability + sum(
                item["settled_micros"] + item["unsettled_upper_bound_micros"]
                for item in settlements)
            <= usd_to_micros(manifest["campaign_spend"]["hard_ceiling_usd"]),
            "settled plus outstanding campaign liability exceeds hard ceiling")
    records: dict[tuple[str, str], dict[str, object]] = {}
    for row in rows:
        row_dir = output / row["row_id"]
        proof = json.loads((row_dir / "row-proof.json").read_text())
        require(proof.get("complete") is True, f"row incomplete: {row['row_id']}")
        require(proof["row"] == row and proof["manifest_sha256"] == sha256_file(CAMPAIGN_MANIFEST),
                "row proof contract drift")
        require(proof["pre_execution_proof_sha256"] == root_sha256
                and proof["case_materialization_contract_sha256"]
                == canonical_sha256(root_proof["materialization"]["cases"][row["question_id"]]),
                "row is not bound to frozen execution/materialization proof")
        require(proof["git_commit"] == root_proof["git_commit"]
                and proof["frozen_binaries"] == root_proof["binaries"],
                "row commit/binary freeze drift")
        expected_config_hash = (
            None if row["arm"] == "fast"
            else root_proof["deep_config_hashes"][row["arm"]]
        )
        require(proof.get("expected_deep_config_hash") == expected_config_hash,
                "row expected Deep config hash drift")
        if "binaries" in proof:
            require(proof["binaries"] == root_proof["binaries"], "row used mixed binaries")
        require(proof["artifact_hashes"] == artifact_hashes(row_dir, exclude={"row-proof.json"}),
                "row artifact inventory drift")
        reservation = reservations[row["sequence"] - 1]
        settlement = settlements[row["sequence"] - 1]
        require(reservation == _reservation(row, manifest), "row reservation contract drift")
        require(proof["reservation_sha256"] == sha256_file(reservation_paths[row["sequence"] - 1]),
                "row reservation hash drift")
        require(proof["settlement_sha256"] == sha256_file(settlement_paths[row["sequence"] - 1]),
                "row settlement hash drift")
        require(settlement == _row_settlement(
            row_dir, row, reservation, orphaned=bool(settlement["orphaned_attempt"])
        ), "row settlement does not reconcile to archived provider evidence")
        if proof["outcome"] != "success":
            records[(row["question_id"], row["arm"])] = {
                "score": 0.0, "raw_score": 0.0, "operational": False,
                "truncated": True, "latency_ms": 120000,
                "deep_cost_micros": int(settlement["deep_settled_micros"]),
                "deep_config_hash": None,
                "memory_proof_sha256": proof.get("memory_proof_sha256"),
            }
            continue
        require(proof.get("deep_config_hash_bound") is True,
                "successful row did not observe its frozen Deep config hash")
        memory_paths = list((row_dir / "memory-proofs").glob("*.json"))
        require(len(memory_paths) == 1, "successful row lacks one memory proof")
        memory_path = memory_paths[0]
        require(sha256_file(memory_path) == proof["memory_proof_sha256"], "memory proof hash drift")
        require(sha256_file(row_dir / "reader-route.json") == proof["reader_route_sha256"],
                "reader route hash drift")
        reader_audit = json.loads((row_dir / "reader-route.json").read_text())
        require(reader_audit.get("audit_status") == "settled"
                and reader_audit.get("provider_name") == "DeepInfra"
                and reader_audit.get("model") in manifest["protocol"]["reader"]["settlement_models"],
                "reader route was not exactly settled")
        require(reader_audit.get("provider_policy_sha256")
                == canonical_sha256(manifest["protocol"]["reader"]["provider_policy"]),
                "reader provider policy drift")
        require(sha256_file(row_dir / "official/per_question.jsonl") == proof["official_score_sha256"],
                "official score hash drift")
        judge_hashes = [sha256_file(path) for path in sorted((row_dir / "judge-routes").glob("*.json"))]
        require(canonical_sha256(judge_hashes) == proof["judge_route_sha256"], "judge proof hash drift")
        memory = json.loads(memory_path.read_text())
        public = memory["public"]
        require(public["recall_response"]["trace_id"] == public["trace"]["id"], "trace pairing drift")
        require(memory["recall_mutation_proof"]["corpus_policy_job_tables_unchanged"] is True,
                "recall mutation invariant failed")
        score_row = json.loads((row_dir / "official/per_question.jsonl").read_text())
        require(score_row["question_id"] == row["question_id"], "official score pairing drift")
        eval_name = str(score_row.get("eval_function", "")).split("|", 1)[0]
        require(len(judge_hashes) == (1 if eval_name.startswith("llm_") else 0),
                "judge invocation count does not match official evaluator")
        require(all(json.loads(path.read_text()).get("audit_status") == "settled"
                    for path in sorted((row_dir / "judge-routes").glob("*.json"))),
                "judge settlement is unresolved")
        deep = public["recall_response"].get("deep")
        operational = settlement["unsettled_upper_bound_micros"] == 0 and _treatment_operational(
            public, row, manifest, truncated=bool(score_row["memory_context_was_truncated"])
        )
        require(proof.get("treatment_operational") is True and operational,
                "successful row proof misstates treatment operation")
        score = float(score_row["score"]) if operational else 0.0
        recall_duration = int(memory["query"]["recall_duration_ms"])
        records[(row["question_id"], row["arm"])] = {
            "score": score, "raw_score": float(score_row["score"]),
            "operational": operational,
            "truncated": bool(score_row["memory_context_was_truncated"]),
            "latency_ms": recall_duration,
            "deep_cost_micros": int((deep or {}).get("usage", {}).get("spend_micros", 0)),
            "deep_config_hash": public["trace"].get("l4_config_hash"),
            "memory_proof_sha256": proof["memory_proof_sha256"],
        }

    selected_deep_arm = manifest["protocol"]["selected_deep_arm"]
    candidates: dict[str, dict[str, object]] = {}
    cases = {case["id"]: case for case in manifest["selection"]["cases"]}
    for arm in (selected_deep_arm,):
        pairs = []
        for question_id in manifest["run_order"]["case_order"]:
            fast = records[(question_id, "fast")]
            deep = records[(question_id, arm)]
            pairs.append({
                "question_id": question_id, "domain": cases[question_id]["domain"],
                "ability": cases[question_id]["ability"], "fast_score": fast["score"],
                "deep_score": deep["score"], "delta": deep["score"] - fast["score"],
                "fast_operational": fast["operational"], "deep_operational": deep["operational"],
                "operational": fast["operational"] and deep["operational"],
            })
        wins = sum(pair["delta"] > 0 for pair in pairs)
        losses = sum(pair["delta"] < 0 for pair in pairs)
        ties = 12 - wins - losses
        latencies = [records[(pair["question_id"], arm)]["latency_ms"] for pair in pairs]
        costs = [records[(pair["question_id"], arm)]["deep_cost_micros"] for pair in pairs]
        config_hashes = {
            records[(pair["question_id"], arm)]["deep_config_hash"]
            for pair in pairs if records[(pair["question_id"], arm)]["deep_config_hash"] is not None
        }
        require(config_hashes <= {manifest["protocol"]["deep_candidates"][arm]["config_sha256"]},
                f"Deep config drift across candidate rows: {arm}")
        delta = sum(pair["delta"] for pair in pairs) / 12
        predicates = {
            "complete_operational_pairs": all(pair["operational"] for pair in pairs),
            "positive_mean_delta_and_more_wins": delta > 0 and wins > losses,
            "latency": _percentile(latencies, 0.50) <= 45000
            and _percentile(latencies, 0.95) <= 90000 and max(latencies) <= 90000,
            "deep_cost": sum(costs) / 12 <= 100000
            and _percentile(costs, 0.95) <= 200000 and max(costs) <= 200000,
            "no_context_truncation": all(
                not records[(pair["question_id"], "fast")]["truncated"]
                and not records[(pair["question_id"], arm)]["truncated"]
                for pair in pairs
            ),
        }
        domain_scores = {
            domain: sum(pair["deep_score"] for pair in pairs if pair["domain"] == domain)
            / sum(pair["domain"] == domain for pair in pairs)
            for domain in ("enterprise", "web")
        }
        ability_scores = {
            ability_name: sum(pair["deep_score"] for pair in pairs if pair["ability"] == ability_name)
            / sum(pair["ability"] == ability_name for pair in pairs)
            for ability_name in sorted(ABILITIES)
        }
        candidates[arm] = {
            "paired_mean_delta": delta, "wins": wins, "ties": ties, "losses": losses,
            "mean_score": sum(pair["deep_score"] for pair in pairs) / 12,
            "latency_ms": {"p50": _percentile(latencies, .50), "p95": _percentile(latencies, .95), "max": max(latencies)},
            "deep_cost_micros": {"mean": sum(costs) / 12, "p95": _percentile(costs, .95), "max": max(costs)},
            "by_domain_mean_score": domain_scores, "by_ability_mean_score": ability_scores,
            "predicates": predicates,
            "failed_predicates": [name for name, passed in predicates.items() if not passed],
            "feasible": all(predicates.values()), "pairs": pairs,
        }
    advance = [selected_deep_arm] if candidates[selected_deep_arm]["feasible"] else []
    aggregate = {
        "campaign": manifest["campaign"], "manifest_sha256": sha256_file(CAMPAIGN_MANIFEST),
        "primary_metric": "paired official per-question binary score",
        "failure_treatment_applied": True, "candidates": candidates,
        "spend_proof": {
            "preexisting_liability": manifest["campaign_spend"]["preexisting_liability"],
            "reservation_hashes": [sha256_file(path) for path in reservation_paths],
            "settlement_hashes": [sha256_file(path) for path in settlement_paths],
            "max_liability_micros": sum(item["max_liability_micros"] for item in reservations),
            "total_max_liability_micros": preexisting_liability + sum(
                item["max_liability_micros"] for item in reservations
            ),
            "settled_micros": sum(item["settled_micros"] for item in settlements),
            "unsettled_upper_bound_micros": sum(
                item["unsettled_upper_bound_micros"] for item in settlements
            ),
        },
        "advance_to_separate_confirmation": advance,
        "decision": "confirmation_manifest_required" if advance else "retire_deep_product_code",
        "claim_boundary": manifest["claim_boundary"],
    }
    destination = output / "aggregate-proof.json"
    require(not destination.exists(), "immutable aggregate proof already exists")
    destination.write_text(json.dumps(aggregate, indent=2, sort_keys=True) + "\n")
    return aggregate


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "command",
        choices=(
            "verify-selection", "acquire", "materialize", "preflight", "run",
            "aggregate", "_context-preflight", "_reader-route-preflight", "_run-row",
        ),
    )
    parser.add_argument("--directory", type=Path)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--questions", type=Path)
    parser.add_argument("--materialized", type=Path)
    parser.add_argument("--base-database-url")
    parser.add_argument("--row-id")
    args = parser.parse_args()
    manifest = load_campaign_manifest()
    if args.command == "verify-selection":
        if args.questions:
            rows = [json.loads(line) for line in args.questions.read_text().splitlines() if line]
            require(sha256_file(args.questions) == manifest["upstream"]["questions_sha256"],
                    "questions source hash drift")
            require(select_cases(rows) == manifest["selection"]["cases"], "live selection drift")
        audit: object = verify_campaign_manifest(manifest)
    elif args.command == "_context-preflight":
        require(args.directory and args.output and args.materialized,
                "_context-preflight requires directory, output, and materialized")
        audit = run_context_preflight(
            args.directory, args.materialized, args.output, manifest
        )
    elif args.command == "_reader-route-preflight":
        require(args.output is not None, "_reader-route-preflight requires output")
        audit = run_reader_route_preflight(args.output, manifest)
    elif args.command == "_run-row":
        require(args.directory and args.output and args.materialized and args.row_id,
                "_run-row requires directory, output, materialized, and row-id")
        row = next((item for item in expanded_run_order(manifest) if item["row_id"] == args.row_id), None)
        require(row is not None, "unknown row id")
        audit = run_row(args.directory, args.materialized, args.output, row, manifest)
    elif args.command == "run":
        require(args.directory and args.output and args.materialized and args.base_database_url,
                "run requires directory, output, materialized, and base-database-url")
        audit = run_campaign(
            args.directory, args.materialized, args.output, args.base_database_url, manifest
        )
    elif args.command == "aggregate":
        require(args.output is not None, "aggregate requires --output")
        audit = aggregate_campaign(args.output, manifest)
    else:
        require(args.directory is not None, f"{args.command} requires --directory")
        if args.command == "acquire":
            audit = acquire_minimal(args.directory, manifest)
        elif args.command == "materialize":
            require(args.output is not None, "materialize requires --output")
            audit = materialize(args.directory, args.output, manifest)
        else:
            require(args.output is not None, "preflight requires --output")
            audit = preflight(args.directory, args.output, manifest)
    envelope = {"verified": True, "audit": audit}
    if args.command in {
        "verify-selection", "acquire", "materialize", "preflight",
        "_context-preflight",
    }:
        envelope["paid_calls"] = 0
    elif args.command == "_reader-route-preflight":
        envelope["paid_calls"] = audit["paid_calls"]
    print(json.dumps(envelope, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

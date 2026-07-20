#!/usr/bin/env python3
"""Prepare and execute the immutable P1-T6 LongMemEval-V2 n=12 screen."""

from __future__ import annotations

import argparse
import hashlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import importlib.util
import json
import math
import os
from pathlib import Path
import shutil
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
    require(len(rows) == 48 and len({row["row_id"] for row in rows}) == 48,
            "run-order completeness drift")
    expected_ids = {row["id"] for row in selection["cases"]}
    require({row["question_id"] for row in rows} == expected_ids, "run-order case drift")
    require(manifest["run_order"]["outputs_observed"] is False, "run order was post-scored")
    require(manifest["run_order"]["case_order"] == sorted(expected_ids), "case-major order drift")
    require(manifest["run_order"]["arm_order_per_case"] == ["fast", "sonnet", "luna", "sol"],
            "arm order drift")
    spend = manifest["campaign_spend"]
    require(spend["hard_ceiling_usd"] == 15.0, "campaign spend ceiling drift")
    require(spend["deep_max_liability_usd"] + spend["reader_and_judge_reserve_usd"] <= 15.0,
            "campaign liability exceeds hard ceiling")
    return {"cases": 12, "rows": 48, "arms": 4}


def write_memory_config(base: dict, mode: str, path: Path) -> dict:
    require(mode in {"fast", "deep"}, "memory mode must be fast or deep")
    value = json.loads(json.dumps(base))
    value["memory_params"]["mode"] = mode
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    return value


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
    return summary


def preflight(directory: Path, materialized: Path, manifest: dict) -> dict[str, object]:
    verify_campaign_manifest(manifest)
    acquired = acquire_minimal(directory, manifest)
    require((materialized / "materialization-proof.json").is_file(), "materialization missing")
    for case in manifest["selection"]["cases"]:
        case_dir = materialized / case["id"]
        require((case_dir / "pairing.json").is_file(), f"pairing missing: {case['id']}")
        require(json.loads((case_dir / "pairing.json").read_text())["gold_fields_copied_to_memory"] == [],
                "gold-memory isolation proof failed")
    return {"campaign": verify_campaign_manifest(manifest), "acquisition": acquired,
            "materialization_sha256": sha256_file(materialized / "materialization-proof.json")}


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


def verify_endpoint_inventory(manifest: dict) -> dict[str, str]:
    checks = [
        ("qwen/qwen3.5-9b", "reader", "all"),
        ("anthropic/claude-sonnet-5-20260630", "sonnet", "azure"),
        ("openai/gpt-5.6-luna-20260709", "luna", "azure"),
        ("openai/gpt-5.6-sol-20260709", "sol", "azure"),
    ]
    proven: dict[str, str] = {}
    for slug, key, provider in checks:
        payload = _json_url(f"https://openrouter.ai/api/v1/models/{slug}/endpoints")
        endpoints = payload["data"]["endpoints"]
        stable = [
            {field: endpoint[field] for field in ENDPOINT_FIELDS}
            for endpoint in endpoints
            if provider == "all" or endpoint["provider_name"].lower() == provider
        ]
        digest = canonical_sha256(stable)
        expected = (
            manifest["protocol"]["reader"]["endpoint_inventory_sha256"]
            if key == "reader"
            else manifest["protocol"]["deep_candidates"][key]["endpoint_metadata_sha256"]
        )
        require(digest == expected, f"OpenRouter endpoint inventory drift: {key}")
        proven[key] = digest
    return proven


def _free_port() -> int:
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        return int(sock.getsockname()[1])


def _reader_proxy(api_key: str, audit_path: Path) -> tuple[ThreadingHTTPServer, str]:
    policy = {
        "only": ["deepinfra"], "allow_fallbacks": False,
        "require_parameters": True, "data_collection": "deny", "zdr": True,
    }

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *_args: object) -> None:
            return None

        def do_POST(self) -> None:
            response_body: bytes | None = None
            status = 502
            try:
                require(self.path == "/chat/completions", "reader proxy path denied")
                length = int(self.headers.get("content-length", "0"))
                body = self.rfile.read(length)
                request = json.loads(body)
                require(request.get("model") == "Qwen/Qwen3.5-9B", "reader model drift")
                request["provider"] = policy
                upstream_body = canonical_bytes(request)
                upstream_request = urllib.request.Request(
                    "https://openrouter.ai/api/v1/chat/completions",
                    data=upstream_body,
                    method="POST",
                    headers={"Authorization": f"Bearer {api_key}", "Content-Type": "application/json"},
                )
                opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
                with opener.open(upstream_request, timeout=300) as response:
                    response_body = response.read()
                    status = response.status
                parsed = json.loads(response_body)
                generation_id = parsed.get("id")
                audit = {
                    "audit_status": "pending",
                    "request_contract_sha256": hashlib.sha256(upstream_body).hexdigest(),
                    "provider_policy_sha256": canonical_sha256(policy),
                    "generation_id": generation_id,
                }
                try:
                    require(isinstance(generation_id, str) and generation_id, "reader omitted generation id")
                    settlement = None
                    for _ in range(5):
                        try:
                            settlement = _json_url(
                                "https://openrouter.ai/api/v1/generation?id="
                                + urllib.parse.quote(generation_id), api_key,
                            )["data"]
                            break
                        except Exception:
                            time.sleep(1)
                    require(isinstance(settlement, dict), "reader settlement unresolved")
                    require(settlement.get("provider_name") == "DeepInfra", "reader route drift")
                    model = str(settlement.get("model", "")).lower()
                    require("qwen3.5-9b" in model, "reader settled model drift")
                    require(all(settlement.get(field) is not None for field in ("tokens_prompt", "tokens_completion", "total_cost")),
                            "reader settlement incomplete")
                    audit.update({
                        "audit_status": "settled", "provider_name": settlement["provider_name"],
                        "model": settlement.get("model"), "tokens_prompt": settlement["tokens_prompt"],
                        "tokens_completion": settlement["tokens_completion"], "total_cost": settlement["total_cost"],
                    })
                except Exception as error:
                    audit.update({"audit_status": "invalid", "audit_error": str(error)})
                audit_path.parent.mkdir(parents=True, exist_ok=True)
                audit_path.write_text(json.dumps(audit, indent=2, sort_keys=True) + "\n")
            except Exception as error:
                if response_body is None:
                    response_body = canonical_bytes({"error": {"message": str(error), "type": "reader_route_proof"}})
            self.send_response(status)
            self.send_header("content-type", "application/json")
            self.send_header("content-length", str(len(response_body)))
            self.end_headers()
            self.wfile.write(response_body)

    server = ThreadingHTTPServer(("127.0.0.1", _free_port()), Handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    return server, f"http://127.0.0.1:{server.server_port}"


def _judge_proxy(api_key: str, audit_dir: Path, manifest: dict) -> tuple[ThreadingHTTPServer, str]:
    contract = manifest["protocol"]["judge"]
    lock = threading.Lock()
    call_count = 0

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *_args: object) -> None:
            return None

        def do_POST(self) -> None:
            nonlocal call_count
            response_body: bytes | None = None
            status = 502
            try:
                require(self.path == "/chat/completions", "judge proxy path denied")
                body = self.rfile.read(int(self.headers.get("content-length", "0")))
                request = json.loads(body)
                require(request.get("model") == contract["model"], "judge snapshot drift")
                require(request.get("reasoning_effort") == "medium", "judge reasoning drift")
                max_tokens = request.get("max_completion_tokens", request.get("max_tokens"))
                require(max_tokens == contract["max_completion_tokens"], "judge completion cap drift")
                prompt_chars = len(canonical_bytes(request.get("messages", [])))
                worst_micros = (
                    prompt_chars * contract["input_price_micros_per_million"]
                    + max_tokens * contract["output_price_micros_per_million"] + 999_999
                ) // 1_000_000
                reader_reserve = 6_277
                require(
                    worst_micros + reader_reserve
                    <= manifest["campaign_spend"]["reader_and_judge_max_liability_micros_per_row"],
                    "judge request exceeds row spend reserve",
                )
                upstream = urllib.request.Request(
                    "https://api.openai.com/v1/chat/completions", data=canonical_bytes(request),
                    method="POST", headers={"Authorization": f"Bearer {api_key}", "Content-Type": "application/json"},
                )
                with urllib.request.build_opener(urllib.request.ProxyHandler({})).open(upstream, timeout=300) as response:
                    response_body = response.read()
                    status = response.status
                parsed = json.loads(response_body)
                audit = {
                    "audit_status": "pending",
                    "request_contract_sha256": hashlib.sha256(canonical_bytes(request)).hexdigest(),
                    "response_id": parsed.get("id"), "max_liability_micros": worst_micros,
                }
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
                except Exception as error:
                    audit.update({"audit_status": "invalid", "audit_error": str(error)})
                with lock:
                    call_count += 1
                    index = call_count
                audit_dir.mkdir(parents=True, exist_ok=True)
                (audit_dir / f"{index:04d}.json").write_text(
                    json.dumps(audit, indent=2, sort_keys=True) + "\n"
                )
            except Exception as error:
                if response_body is None:
                    response_body = canonical_bytes({"error": {"message": str(error), "type": "judge_route_proof"}})
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


def _wait_health(base_url: str, process: subprocess.Popen) -> None:
    for _ in range(120):
        require(process.poll() is None, "MemPhant server exited before health")
        try:
            urllib.request.urlopen(base_url + "/v1/health", timeout=1).close()
            return
        except urllib.error.URLError:
            time.sleep(0.5)
    raise RuntimeError("MemPhant server health timed out")


def run_row(directory: Path, materialized: Path, output: Path, row: dict, manifest: dict) -> dict:
    require(os.environ.get("MEMPHANT_SCRATCH_ACTIVE") == "1", "row requires scratch database")
    database_url = os.environ.get("MEMPHANT_TEST_DATABASE_URL", "")
    require(database_url, "scratch database URL missing")
    openrouter_key = os.environ.get("OPENROUTER_API_KEY", "")
    openai_key = os.environ.get("OPENAI_API_KEY", "")
    require(openrouter_key and openai_key, "OPENROUTER_API_KEY and OPENAI_API_KEY are required")
    final_dir = output / row["row_id"]
    require_new_row_dir(final_dir)
    row_dir = output / (".staging-" + row["row_id"])
    require(not row_dir.exists(), f"incomplete staging row requires review: {row_dir}")
    row_dir.mkdir(parents=True)
    proxy, reader_url = _reader_proxy(openrouter_key, row_dir / "reader-route.json")
    judge_proxy, judge_url = _judge_proxy(openai_key, row_dir / "judge-routes", manifest)
    port = _free_port()
    server_url = f"http://127.0.0.1:{port}"
    binaries = {name: ROOT / "target/debug" / f"memphant-{name}" for name in ("server", "worker", "cli")}
    server_env = dict(os.environ)
    server_env.pop("DATABASE_URL", None)
    server_env.update({
        "MEMPHANT_APP_DATABASE_URL": database_url,
        "MEMPHANT_AUTHN_DATABASE_URL": database_url,
        "MEMPHANT_BIND": f"127.0.0.1:{port}",
        "MEMPHANT_RESOURCE_CHUNKS": "on",
        "MEMPHANT_STRUCTURED_STATE": "off",
    })
    arm = row["arm"]
    if arm == "fast":
        server_env["MEMPHANT_DEEP"] = "off"
        server_env.pop("OPENROUTER_API_KEY", None)
    else:
        candidate = manifest["protocol"]["deep_candidates"][arm]
        server_env.update({
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
        proof_dir = row_dir / "memory-proofs"
        proof_dir.mkdir()
        child_env = dict(os.environ)
        child_env.update({
            "MEMPHANT_LME_SERVER_URL": server_url,
            "MEMPHANT_CLI_BIN": str(binaries["cli"]),
            "MEMPHANT_LME_SERVER_BIN": str(binaries["server"]),
            "MEMPHANT_LME_WORKER_BIN": str(binaries["worker"]),
            "MEMPHANT_LME_PROOF_DIR": str(proof_dir),
            "MEMPHANT_LME_RUN_ID": row["row_id"],
            "LME_READER_PROXY_KEY": "loopback-route-bound",
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
            "--evaluator-api-key-env", "OPENAI_API_KEY",
            "--evaluator-reasoning-effort", "medium",
            "--prompt-build-max-workers", "1", "--reader-max-concurrent-requests", "1",
        ]
        completed = subprocess.run(command, cwd=directory / "official", env=child_env, check=False)
        exit_code = completed.returncode
    finally:
        server.terminate()
        try:
            server.wait(timeout=10)
        except subprocess.TimeoutExpired:
            server.kill()
        proxy.shutdown()
        proxy.server_close()
        judge_proxy.shutdown()
        judge_proxy.server_close()
    if exit_code != 0:
        (row_dir / "incomplete.json").write_text(json.dumps({
            "row": row, "official_exit_code": exit_code,
            "retry_authorized": False, "requires_generation_and_billing_audit": True,
        }, indent=2, sort_keys=True) + "\n")
        raise RuntimeError(f"row failed and remains staged for audit: {row['row_id']}")
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
    if str(official_score.get("eval_name", "")).startswith("llm_"):
        require(len(judge_routes) == 1, "LLM-scored row requires exactly one judge proof")
        require(json.loads(judge_routes[0].read_text())["audit_status"] == "settled",
                "judge audit is unresolved or invalid")
    else:
        require(not judge_routes, "deterministic scorer unexpectedly called judge")
    proof = {
        "row": row, "official_exit_code": exit_code,
        "scratch_database_identity": database_url.rsplit("/", 1)[-1],
        "binaries": {name: _fingerprint(path) for name, path in binaries.items()},
        "manifest_sha256": sha256_file(CAMPAIGN_MANIFEST),
        "git_commit": subprocess.run(["git", "rev-parse", "HEAD"], cwd=ROOT, capture_output=True, text=True, check=True).stdout.strip(),
        "memory_proof_sha256": sha256_file(memory_proofs[0]),
        "reader_route_sha256": sha256_file(row_dir / "reader-route.json"),
        "judge_route_sha256": canonical_sha256([sha256_file(path) for path in judge_routes]),
        "official_score_sha256": sha256_file(per_question),
        "immutable": True, "complete": True,
    }
    (row_dir / "row-proof.json").write_text(json.dumps(proof, indent=2, sort_keys=True) + "\n")
    os.replace(row_dir, final_dir)
    return proof


def run_campaign(directory: Path, materialized: Path, output: Path, base_database_url: str, manifest: dict) -> dict:
    preflight(directory, materialized, manifest)
    endpoint_hashes = verify_endpoint_inventory(manifest)
    subprocess.run(["cargo", "build", "-p", "memphant-server", "-p", "memphant-worker", "-p", "memphant-cli"], cwd=ROOT, check=True)
    output.mkdir(parents=True, exist_ok=True)
    rows = expanded_run_order(manifest)
    root_proof = {
        "manifest_sha256": sha256_file(CAMPAIGN_MANIFEST), "endpoint_hashes": endpoint_hashes,
        "run_order_sha256": canonical_sha256(rows), "outputs_observed_before_freeze": False,
    }
    root_path = output / "pre-execution-proof.json"
    if root_path.exists():
        require(json.loads(root_path.read_text()) == root_proof, "campaign resume contract drift")
    else:
        root_path.write_text(json.dumps(root_proof, indent=2, sort_keys=True) + "\n")
        (output / "frozen-run-order.json").write_text(json.dumps(rows, indent=2, sort_keys=True) + "\n")
    ledger = output / "spend-ledger"
    ledger.mkdir(exist_ok=True)
    settlements = output / "spend-settlements"
    settlements.mkdir(exist_ok=True)
    for row in rows:
        if (output / row["row_id"]).is_dir():
            require(json.loads((output / row["row_id"] / "row-proof.json").read_text())["complete"] is True,
                    "completed row proof drift")
            require((ledger / f"{row['sequence']:04d}.json").is_file(), "completed row lacks reservation")
            require((settlements / f"{row['sequence']:04d}.json").is_file(), "completed row lacks settlement")
            continue
        require(not (output / (".staging-" + row["row_id"])).exists(),
                f"staged row requires audit before resume: {row['row_id']}")
        prior = sum(json.loads(path.read_text())["max_liability_micros"] for path in ledger.glob("*.json"))
        deep = 0 if row["arm"] == "fast" else 300000
        reservation = deep + manifest["campaign_spend"]["reader_and_judge_max_liability_micros_per_row"]
        require(prior + reservation <= int(manifest["campaign_spend"]["hard_ceiling_usd"] * 1_000_000),
                "campaign spend ceiling reached before dispatch")
        ledger_row = ledger / f"{row['sequence']:04d}.json"
        require(not ledger_row.exists(), "spend reservation already exists without completed row")
        atomic_write_json(ledger_row, {
            "row_id": row["row_id"], "max_liability_micros": reservation,
            "deep_hard_cap_micros": deep,
            "reader_and_judge_reserve_micros": reservation - deep,
            "charged_before_dispatch": True,
        })
        command = [
            "env", "MEMPHANT_SCRATCH_ACTIVE=1", "bash", str(SCRATCH_HELPER),
            base_database_url, "MEMPHANT_TEST_DATABASE_URL", sys.executable, __file__, "_run-row",
            "--directory", str(directory), "--output", str(output),
            "--materialized", str(materialized), "--row-id", row["row_id"],
        ]
        subprocess.run(command, cwd=ROOT, check=True)
        final_dir = output / row["row_id"]
        memory = json.loads(next((final_dir / "memory-proofs").glob("*.json")).read_text())
        deep_summary = memory["public"]["recall_response"].get("deep") or {}
        deep_usage = deep_summary.get("usage") or {}
        deep_settled = int(deep_usage.get("spend_micros", 0))
        deep_unsettled = int(deep_usage.get("unsettled_spend_micros_upper_bound", 0))
        reader = json.loads((final_dir / "reader-route.json").read_text())
        reader_settled = int(round(float(reader["total_cost"]) * 1_000_000))
        judge_settled = sum(
            int(json.loads(path.read_text())["cost_micros"])
            for path in (final_dir / "judge-routes").glob("*.json")
        )
        settlement_row = settlements / f"{row['sequence']:04d}.json"
        require(not settlement_row.exists(), "spend settlement already exists")
        atomic_write_json(settlement_row, {
            "row_id": row["row_id"],
            "settled_micros": deep_settled + reader_settled + judge_settled,
            "deep_settled_micros": deep_settled,
            "deep_unsettled_upper_bound_micros": deep_unsettled,
            "reader_settled_micros": reader_settled,
            "judge_settled_micros": judge_settled,
        })
    return {"rows": len(rows), "output": str(output)}


def _percentile(values: list[int], fraction: float) -> int:
    require(values, "percentile requires values")
    require(0 < fraction <= 1, "percentile fraction is out of range")
    ordered = sorted(values)
    return ordered[math.ceil(fraction * len(ordered)) - 1]


def aggregate_campaign(output: Path, manifest: dict) -> dict[str, object]:
    rows = expanded_run_order(manifest)
    expected_row_ids = {row["row_id"] for row in rows}
    observed_row_ids = {
        path.name for path in output.iterdir()
        if path.is_dir() and not path.name.startswith(".")
        and path.name not in {"spend-ledger", "spend-settlements"}
    }
    require(observed_row_ids == expected_row_ids, "missing or extra finalized rows")
    reservation_paths = sorted((output / "spend-ledger").glob("*.json"))
    settlement_paths = sorted((output / "spend-settlements").glob("*.json"))
    require(len(reservation_paths) == len(rows) == len(settlement_paths),
            "spend ledger is incomplete or duplicated")
    reservations = [json.loads(path.read_text()) for path in reservation_paths]
    settlements = [json.loads(path.read_text()) for path in settlement_paths]
    require([item["row_id"] for item in reservations] == [row["row_id"] for row in rows],
            "spend reservation order drift")
    require([item["row_id"] for item in settlements] == [row["row_id"] for row in rows],
            "spend settlement order drift")
    require(sum(item["max_liability_micros"] for item in reservations)
            <= int(manifest["campaign_spend"]["hard_ceiling_usd"] * 1_000_000),
            "spend reservations exceed campaign ceiling")
    records: dict[tuple[str, str], dict[str, object]] = {}
    for row in rows:
        row_dir = output / row["row_id"]
        proof = json.loads((row_dir / "row-proof.json").read_text())
        require(proof.get("complete") is True, f"row incomplete: {row['row_id']}")
        memory_path = next((row_dir / "memory-proofs").glob("*.json"))
        require(sha256_file(memory_path) == proof["memory_proof_sha256"], "memory proof hash drift")
        require(sha256_file(row_dir / "reader-route.json") == proof["reader_route_sha256"],
                "reader route hash drift")
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
        deep = public["recall_response"].get("deep")
        operational = True
        if row["arm"] != "fast":
            configured = manifest["protocol"]["deep_candidates"][row["arm"]]
            trace = public["trace"]
            operational = bool(
                deep and deep["status"] == "completed" and deep["stop_reason"] == "completed"
                and deep["usage"]["unsettled_context_tokens_upper_bound"] == 0
                and deep["usage"]["unsettled_spend_micros_upper_bound"] == 0
                and trace.get("deep") == deep and deep.get("generation_ids")
                and trace.get("l4_model") == configured["model"]
                and str(trace.get("l4_provider", "")).lower() == "azure"
                and str(trace.get("l4_observed_provider", "")).lower() == "azure"
                and trace.get("l4_observed_model") == configured["model"]
                and isinstance(trace.get("l4_prompt_hash"), str) and len(trace["l4_prompt_hash"]) == 64
                and isinstance(trace.get("l4_config_hash"), str) and len(trace["l4_config_hash"]) == 64
            )
        operational = operational and not score_row["memory_context_was_truncated"]
        score = float(score_row["score"]) if operational else 0.0
        records[(row["question_id"], row["arm"])] = {
            "score": score, "raw_score": float(score_row["score"]),
            "operational": operational,
            "latency_ms": int(round(float(score_row["memory_query_duration_seconds"]) * 1000)),
            "deep_cost_micros": int((deep or {}).get("usage", {}).get("spend_micros", 0)),
            "memory_proof_sha256": proof["memory_proof_sha256"],
        }

    candidates: dict[str, dict[str, object]] = {}
    cases = {case["id"]: case for case in manifest["selection"]["cases"]}
    for arm in ("sonnet", "luna", "sol"):
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
        delta = sum(pair["delta"] for pair in pairs) / 12
        predicates = {
            "complete_operational_pairs": all(pair["operational"] for pair in pairs),
            "positive_mean_delta_and_more_wins": delta > 0 and wins > losses,
            "latency": _percentile(latencies, 0.50) <= 45000
            and _percentile(latencies, 0.95) <= 90000 and max(latencies) <= 90000,
            "deep_cost": sum(costs) / 12 <= 100000
            and _percentile(costs, 0.95) <= 200000 and max(costs) <= 200000,
            "no_context_truncation": all(pair["operational"] for pair in pairs),
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
    feasible = [arm for arm, result in candidates.items() if result["feasible"]]
    feasible.sort(key=lambda arm: (
        -candidates[arm]["mean_score"], candidates[arm]["deep_cost_micros"]["mean"],
        candidates[arm]["latency_ms"]["p95"],
    ))
    advance: list[str] = feasible[:1]
    if feasible:
        top_correct = candidates[feasible[0]]["mean_score"] * 12
        alternatives = [
            arm for arm in feasible[1:]
            if top_correct - candidates[arm]["mean_score"] * 12 <= 1
        ]
        if alternatives:
            advance.append(min(alternatives, key=lambda arm: candidates[arm]["deep_cost_micros"]["mean"]))
    aggregate = {
        "campaign": manifest["campaign"], "manifest_sha256": sha256_file(CAMPAIGN_MANIFEST),
        "primary_metric": "paired official per-question binary score",
        "failure_treatment_applied": True, "candidates": candidates,
        "spend_proof": {
            "reservation_hashes": [sha256_file(path) for path in reservation_paths],
            "settlement_hashes": [sha256_file(path) for path in settlement_paths],
            "max_liability_micros": sum(item["max_liability_micros"] for item in reservations),
            "settled_micros": sum(item["settled_micros"] for item in settlements),
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
    parser.add_argument("command", choices=("verify-selection", "acquire", "materialize", "preflight", "run", "aggregate", "_run-row"))
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
    if args.command in {"verify-selection", "acquire", "materialize", "preflight"}:
        envelope["paid_calls"] = 0
    print(json.dumps(envelope, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

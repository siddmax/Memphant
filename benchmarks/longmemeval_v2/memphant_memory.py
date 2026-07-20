"""Official LongMemEval-V2 ``Memory`` adapter for packaged MemPhant REST.

One adapter instance owns one freshly provisioned tenant, scope, and actor and
accepts exactly one question. The official harness supplies trajectories via
``insert`` and the question via ``query``; gold answers and evaluator fields in
the harness query context are deliberately never read or transmitted.
"""

from __future__ import annotations

import hashlib
import json
import os
import re
import subprocess
import tempfile
import urllib.error
import urllib.request
import uuid
from pathlib import Path
from typing import Any

from memory_modules.memory import Memory, MemoryContextItem, register_memory


EXPECTED_PARAMS = {
    "schema_version",
    "server_url_env",
    "database_url_env",
    "cli_bin_env",
    "server_bin_env",
    "proof_dir_env",
    "run_id_env",
    "top_k",
    "budget_tokens",
    "mode",
    "source_kind",
    "source_trust",
    "compiler_version",
}
FORBIDDEN_EVALUATION_KEYS = {"answer", "answer_gold", "eval_function", "gold", "reference"}
TENANT_PATTERN = re.compile(r"tenant_created id=([0-9a-fA-F-]{36})")


def _require(condition: bool, message: str) -> None:
    if not condition:
        raise RuntimeError(message)


def _canonical_json(value: object) -> bytes:
    return json.dumps(
        value, sort_keys=True, ensure_ascii=True, separators=(",", ":")
    ).encode("utf-8")


def _sha256_json(value: object) -> str:
    return hashlib.sha256(_canonical_json(value)).hexdigest()


def _sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _binary_fingerprint(path: str) -> dict[str, object]:
    binary = Path(path).resolve()
    _require(binary.is_file(), f"required packaged binary is missing: {binary}")
    return {"path": str(binary), "bytes": binary.stat().st_size, "sha256": _sha256_file(binary)}


def _required_env(name: object) -> str:
    _require(isinstance(name, str) and name, "environment variable name must be non-empty")
    value = os.environ.get(name, "").strip()
    _require(bool(value), f"required environment variable is unset: {name}")
    return value


class _JsonClient:
    def __init__(self, base_url: str, api_key: str) -> None:
        self.base_url = base_url.rstrip("/")
        self.headers = {
            "Authorization": f"Bearer {api_key}",
            "Content-Type": "application/json",
        }

    def request(self, method: str, path: str, payload: dict | None = None) -> dict:
        body = None if payload is None else _canonical_json(payload)
        request = urllib.request.Request(
            self.base_url + path,
            data=body,
            headers=self.headers,
            method=method,
        )
        try:
            with urllib.request.urlopen(request, timeout=120) as response:
                raw = response.read()
        except urllib.error.HTTPError as error:
            detail = error.read(512).decode("utf-8", errors="replace")
            raise RuntimeError(f"MemPhant {path} returned HTTP {error.code}: {detail}") from error
        except urllib.error.URLError as error:
            raise RuntimeError(f"MemPhant {path} request failed: {error.reason}") from error
        try:
            value = json.loads(raw)
        except json.JSONDecodeError as error:
            raise RuntimeError(f"MemPhant {path} returned invalid JSON") from error
        _require(isinstance(value, dict), f"MemPhant {path} response must be an object")
        return value


def _run_cli(command: list[str]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(command, capture_output=True, text=True, check=False)


def _provision_tenant(*, cli_bin: str, database_url: str, name: str) -> tuple[str, str]:
    created = _run_cli(
        [cli_bin, "admin", "create-tenant", "--name", name, "--database-url", database_url]
    )
    _require(created.returncode == 0, f"create-tenant failed: {created.stderr.strip()}")
    match = TENANT_PATTERN.search(created.stdout)
    _require(match is not None, "create-tenant omitted tenant UUID")
    tenant_id = match.group(1)
    keyed = _run_cli(
        [
            cli_bin,
            "admin",
            "create-key",
            "--tenant",
            tenant_id,
            "--max-trust",
            "trusted_system",
            "--database-url",
            database_url,
        ]
    )
    _require(keyed.returncode == 0, f"create-key failed: {keyed.stderr.strip()}")
    api_key = keyed.stdout.strip().splitlines()[-1] if keyed.stdout.strip() else ""
    _require(api_key.startswith("mk_"), "create-key omitted MemPhant API key")
    return tenant_id, api_key


def _validate_memory_params(memory_params: dict[str, object]) -> dict[str, object]:
    _require(set(memory_params) == EXPECTED_PARAMS, "memphant memory_params contract drift")
    _require(memory_params["schema_version"] == 2, "unsupported memphant adapter schema")
    _require(
        isinstance(memory_params["top_k"], int) and memory_params["top_k"] == 20,
        "LongMemEval-V2 top_k must remain fixed at 20",
    )
    _require(
        isinstance(memory_params["budget_tokens"], int)
        and memory_params["budget_tokens"] == 32768,
        "LongMemEval-V2 budget_tokens must remain fixed at 32768",
    )
    _require(memory_params["mode"] == "deep", "recall mode must remain deep")
    _require(memory_params["source_trust"] == "trusted_system", "source trust contract drift")
    return dict(memory_params)


def _state_body(trajectory: dict[str, object], state: dict[str, object], index: int) -> str:
    forbidden = FORBIDDEN_EVALUATION_KEYS.intersection(state)
    _require(not forbidden, f"trajectory state contains evaluator fields: {sorted(forbidden)}")
    url = state.get("url")
    action = state.get("action")
    thought = state.get("thought", state.get("thoughts"))
    observation = state.get("accessibility_tree", state.get("text"))
    _require(isinstance(url, str) and url.strip(), "trajectory state URL is missing")
    _require(action is None or isinstance(action, str), "trajectory state action is invalid")
    _require(thought is None or isinstance(thought, str), "trajectory state thought is invalid")
    _require(isinstance(observation, str), "trajectory state text is missing")
    goal = trajectory["goal"]
    trajectory_id = trajectory["id"]
    lines = [
        f"Trajectory: {trajectory_id}",
        f"Goal: {goal}",
        f"Step: {index}",
        f"URL: {url}",
    ]
    if action:
        lines.append(f"Action: {action}")
    if thought:
        lines.append(f"Thought: {thought}")
    lines.append("Observation:\n" + observation)
    states = trajectory["states"]
    if index == len(states) - 1 and trajectory.get("outcome"):
        lines.append(f"Outcome: {trajectory['outcome']}")
    return "\n".join(lines)


def _atomic_write_json(path: Path, value: object) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(
        mode="w", encoding="utf-8", dir=path.parent, delete=False
    ) as handle:
        json.dump(value, handle, indent=2, sort_keys=True)
        handle.write("\n")
        temporary = Path(handle.name)
    os.replace(temporary, path)


@register_memory
class MemphantMemory(Memory):
    memory_type = "memphant"

    def __init__(self, memory_params: dict[str, object]) -> None:
        super().__init__(memory_params)
        self.params = _validate_memory_params(memory_params)
        _require(
            os.environ.get("MEMPHANT_SCRATCH_ACTIVE") == "1",
            "LongMemEval-V2 adapter requires an ephemeral scratch database",
        )
        server_url = _required_env(self.params["server_url_env"])
        database_url = _required_env(self.params["database_url_env"])
        cli_bin = _required_env(self.params["cli_bin_env"])
        server_bin = _required_env(self.params["server_bin_env"])
        self.binaries = {
            "server": _binary_fingerprint(server_bin),
            "cli": _binary_fingerprint(cli_bin),
        }
        self.proof_dir = Path(_required_env(self.params["proof_dir_env"])).resolve()
        run_id = _required_env(self.params["run_id_env"])
        instance_id = uuid.uuid4().hex
        self.tenant_id, api_key = _provision_tenant(
            cli_bin=cli_bin,
            database_url=database_url,
            name=f"lme-v2-{run_id[:32]}-{instance_id[:12]}",
        )
        self.scope_id = str(uuid.uuid4())
        self.actor_id = str(uuid.uuid4())
        self.client = _JsonClient(server_url, api_key)
        self.instance_id = instance_id
        self.inserted_trajectory_ids: list[str] = []
        self.retain_proofs: list[dict[str, object]] = []
        self.episode_count = 0
        self._queried_question_id: str | None = None
        self._last_query_proof: dict[str, object] | None = None

    def insert(self, trajectory: dict[str, object]) -> None:
        _require(isinstance(trajectory, dict), "trajectory must be an object")
        forbidden = FORBIDDEN_EVALUATION_KEYS.intersection(trajectory)
        _require(not forbidden, f"trajectory contains evaluator fields: {sorted(forbidden)}")
        trajectory_id = trajectory.get("id")
        goal = trajectory.get("goal")
        states = trajectory.get("states")
        outcome = trajectory.get("outcome")
        _require(isinstance(trajectory_id, str) and trajectory_id, "trajectory id is missing")
        _require(trajectory_id not in self.inserted_trajectory_ids, "duplicate trajectory insert")
        _require(isinstance(goal, str), f"trajectory goal is invalid: {trajectory_id}")
        _require(isinstance(states, list) and states, f"trajectory states are missing: {trajectory_id}")
        _require(outcome is None or isinstance(outcome, str), "trajectory outcome is invalid")

        state_proofs: list[dict[str, str]] = []
        for index, state in enumerate(states):
            _require(isinstance(state, dict), f"trajectory state {index} must be an object")
            body = _state_body(trajectory, state, index)
            payload = {
                "tenant_id": self.tenant_id,
                "scope_id": self.scope_id,
                "actor_id": self.actor_id,
                "source_kind": self.params["source_kind"],
                "source_trust": self.params["source_trust"],
                "subject_hint": f"trajectory {trajectory_id} step {index:04d}",
                "body": body,
                "compiler_version": self.params["compiler_version"],
            }
            response = self.client.request("POST", "/v1/episodes", payload)
            episode_id = response.get("episode_id")
            _require(isinstance(episode_id, str) and episode_id, "retain omitted episode_id")
            state_proofs.append(
                {
                    "episode_id": episode_id,
                    "request_sha256": _sha256_json(payload),
                    "response_sha256": _sha256_json(response),
                }
            )
            self.episode_count += 1
        self.inserted_trajectory_ids.append(trajectory_id)
        self.retain_proofs.append(
            {
                "trajectory_id": trajectory_id,
                "trajectory_sha256": _sha256_json(trajectory),
                "states": state_proofs,
            }
        )

    def query(self, query: str, query_image: str | None = None) -> list[MemoryContextItem]:
        _require(isinstance(query, str) and query.strip(), "query must be non-empty")
        _require(self.inserted_trajectory_ids, "cannot query empty MemPhant memory")
        context = self.get_query_context()
        question_id = context.get("question_id")
        _require(isinstance(question_id, str) and question_id, "question_id context is required")
        _require(self._queried_question_id is None, "MemPhant instance cannot serve multiple questions")
        self._queried_question_id = question_id

        reflect_payload = {
            "tenant_id": self.tenant_id,
            "scope_id": self.scope_id,
            "actor_id": self.actor_id,
            "compiler_version": self.params["compiler_version"],
        }
        reflected = self.client.request("POST", "/v1/reflect", reflect_payload)
        _require(
            reflected.get("episodes_consumed") == self.episode_count,
            "reflection pairing incomplete: not every retained state was consumed",
        )
        recall_payload = {
            "tenant_id": self.tenant_id,
            "scope_id": self.scope_id,
            "actor_id": self.actor_id,
            "query": query,
            "limit": self.params["top_k"],
            "budget_tokens": self.params["budget_tokens"],
            "mode": self.params["mode"],
        }
        recalled = self.client.request("POST", "/v1/recall", recall_payload)
        _require(recalled.get("degraded") is False, "MemPhant recall was degraded")
        trace_id = recalled.get("trace_id")
        items = recalled.get("items")
        _require(isinstance(trace_id, str) and trace_id, "recall omitted trace_id")
        _require(isinstance(items, list), "recall items must be a list")
        _require(len(items) <= self.params["top_k"], "recall exceeded fixed top_k")
        _require(
            all(isinstance(item, dict) and isinstance(item.get("body"), str) for item in items),
            "recall returned malformed context items",
        )
        trace = self.client.request("GET", f"/v1/traces/{trace_id}")
        _require(trace.get("id") == trace_id, "trace id pairing mismatch")
        _require(trace.get("tenant_id") == self.tenant_id, "trace tenant pairing mismatch")
        _require(trace.get("scope_id") == self.scope_id, "trace scope pairing mismatch")
        _require(trace.get("actor_id") == self.actor_id, "trace actor pairing mismatch")
        _require(trace.get("context_items") == items, "trace context pairing mismatch")
        _require(trace.get("citations") == recalled.get("citations"), "trace citation pairing mismatch")

        memory_context: list[MemoryContextItem] = [
            {"type": "text", "value": item["body"]} for item in items
        ]
        query_proof = {
            "question_id": question_id,
            "query_sha256": hashlib.sha256(query.encode("utf-8")).hexdigest(),
            "query_image_present": query_image is not None,
            "native_query_hash": trace.get("query_hash"),
            "recall_request_sha256": _sha256_json(recall_payload),
            "recall_response_sha256": _sha256_json(recalled),
            "trace_id": trace_id,
            "trace_sha256": _sha256_json(trace),
            "context_sha256": _sha256_json(memory_context),
        }
        proof = {
            "contract": {
                "adapter_sha256": _sha256_file(Path(__file__)),
                "memory_params_sha256": _sha256_json(self.params),
                "top_k": self.params["top_k"],
                "budget_tokens": self.params["budget_tokens"],
                "mode": self.params["mode"],
                "binaries": self.binaries,
                "gold_fields_consumed": [],
            },
            "isolation": {
                "tenant_id": self.tenant_id,
                "scope_id": self.scope_id,
                "actor_id": self.actor_id,
                "instance_id": self.instance_id,
            },
            "pairing": {
                "trajectory_count": len(self.inserted_trajectory_ids),
                "episode_count": self.episode_count,
                "reflect_request_sha256": _sha256_json(reflect_payload),
                "reflect_response_sha256": _sha256_json(reflected),
                "retains": self.retain_proofs,
            },
            "query": query_proof,
        }
        proof_path = self.proof_dir / f"{question_id}.{self.instance_id}.json"
        _atomic_write_json(proof_path, proof)
        query_proof["proof_path"] = str(proof_path)
        self._last_query_proof = query_proof
        return memory_context

    def post_query_hook(
        self,
        *,
        query: str,
        query_image: str | None,
        memory_context: list[MemoryContextItem],
    ) -> dict[str, object] | None:
        _require(self._last_query_proof is not None, "query proof is missing")
        _require(
            self._last_query_proof["query_sha256"]
            == hashlib.sha256(query.encode("utf-8")).hexdigest(),
            "post-query query pairing mismatch",
        )
        _require(
            self._last_query_proof["context_sha256"] == _sha256_json(memory_context),
            "post-query context pairing mismatch",
        )
        return dict(self._last_query_proof)

"""Official STATE-Bench built-in-agent adapter for read-only MemPhant retrieval."""

from __future__ import annotations

import hashlib
import json
import os
import urllib.error
import urllib.request
from pathlib import Path

from state_bench.agents.state_bench import StateBenchAgent


class MemphantMemoryAgent(StateBenchAgent):
    """Expose MemPhant only through STATE-Bench's official learning hook."""

    def retrieve_learnings(self, query: str, top_k: int = 3) -> list[str]:
        if top_k != 3:
            raise ValueError("STATE-Bench MemPhant arm requires top_k=3")
        if self.runtime_context is None:
            raise RuntimeError("STATE-Bench runtime context is required")
        config_path = os.environ.get("MEMPHANT_STATE_BENCH_CONFIG")
        if not config_path:
            raise RuntimeError("MEMPHANT_STATE_BENCH_CONFIG is not set")
        config = json.loads(Path(config_path).read_text(encoding="utf-8"))
        domain = self.runtime_context.domain
        try:
            bound = config["domains"][domain]
        except KeyError as exc:
            raise RuntimeError(f"MemPhant credentials missing for domain {domain}") from exc
        payload = {
            "tenant_id": bound["tenant_id"],
            "scope_id": bound["scope_id"],
            "actor_id": bound["actor_id"],
            "query": query,
            "limit": 3,
            "budget_tokens": 4096,
            "mode": "deep",
        }
        request = urllib.request.Request(
            config["base_url"].rstrip("/") + "/v1/recall",
            data=json.dumps(payload).encode(),
            headers={
                "Authorization": f"Bearer {bound['api_key']}",
                "Content-Type": "application/json",
            },
            method="POST",
        )
        try:
            with urllib.request.urlopen(request, timeout=120) as response:
                result = json.loads(response.read())
        except urllib.error.HTTPError as exc:
            raise RuntimeError(f"MemPhant recall failed: HTTP {exc.code}") from exc
        if result.get("degraded") is not False:
            raise RuntimeError("MemPhant recall was degraded")
        items = result.get("items")
        if not isinstance(items, list) or len(items) > 3:
            raise RuntimeError("MemPhant recall returned invalid top-3 items")
        bodies = []
        for item in items:
            if not isinstance(item, dict) or not isinstance(item.get("body"), str):
                raise RuntimeError("MemPhant recall returned a malformed item")
            bodies.append(item["body"])
        self._append_retrieval_proof(query, result, bodies)
        return bodies

    def _append_retrieval_proof(self, query: str, result: dict, bodies: list[str]) -> None:
        path = os.environ.get("MEMPHANT_STATE_BENCH_RETRIEVAL_PROOF")
        if not path:
            return
        record = {
            "domain": self.runtime_context.domain,
            "task_id": self.runtime_context.task_id,
            "run_idx": self.runtime_context.run_idx,
            "query_sha256": hashlib.sha256(query.encode()).hexdigest(),
            "trace_id": result.get("trace_id"),
            "returned_items": len(bodies),
            "evidence_sha256": hashlib.sha256(
                json.dumps(bodies, sort_keys=True, separators=(",", ":")).encode()
            ).hexdigest(),
            "degraded": False,
        }
        line = (json.dumps(record, sort_keys=True, separators=(",", ":")) + "\n").encode()
        descriptor = os.open(path, os.O_APPEND | os.O_CREAT | os.O_WRONLY, 0o600)
        try:
            os.write(descriptor, line)
            os.fsync(descriptor)
        finally:
            os.close(descriptor)

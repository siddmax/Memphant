#!/usr/bin/env python3
"""Shared MemPhant-server runtime harness for the embedder-bakeoff engine
runners: server/worker process lifecycle, tenant provisioning, a small
retrying HTTP client, and the API-arm key map. Factored out of
``gate_run_memphant.py`` (the docs-lane runner) so
``code_lane_run_memphant.py`` (R0-T6) doesn't copy-paste the ~150-line
process-management block. ``gate_run_memphant.py`` keeps its own process
classes (it is a live, already-running script during R0) but imports the
API-key arm map from here — the one piece where a per-script copy already
drifted once (see ``API_KEY_ENV_BY_ARM``).

This module includes the ``check_port_free``/extended-health-wait/log-capture
robustness landed in ``gate_run_memphant.py`` by commit ``cf17b35`` (a
concurrent fix during R0: ``memphant-server`` can take minutes to download
embedding-model weights on first boot, and the original 20s health wait would
time out mid-download and leak the child, which then held the port and
cascaded bind failures across an arm queue) — ported here rather than only in
the docs-lane script, since this runner spawns the identical server binary
and would hit the identical failure mode.

Everything here is stdlib-only except real subprocess/HTTP calls against a
packaged ``memphant-server``/``memphant-worker``/``memphant-cli`` — no DB
driver, no network library beyond ``http.client``.
"""

from __future__ import annotations

import http.client
import json
import os
import re
import subprocess
import sys
import time
from pathlib import Path

HEALTH_WAIT_TIMEOUT_S = 600.0  # first boot may download embedding model weights (up to 1.5GB)
HEALTH_POLL_INTERVAL_S = 0.5
LOG_TAIL_LINES = 15

# The API-arm ids from memphant-runtime's `embedder_from_id` grammar
# (crates/memphant-runtime/src/lib.rs) and the provider key each one needs.
# SINGLE SOURCE OF TRUTH for every gate runner: gate_run_memphant.py and
# code_lane_run_memphant.py both import this map (a per-script copy drifted
# once already — voyage-4-large landed in one script's copy but not the
# other's, silently disabling the fail-fast for that arm). Mirrors
# `api_embeddings::require_key`'s error text so a missing key fails the same
# way here as it would inside the Rust binary — just before spending time on
# tenant/server setup instead of after the ingest has already started.
# Pinned against the Rust grammar by tests/test_gate_runtime.py.
API_KEY_ENV_BY_ARM = {
    "voyage-4": "VOYAGE_API_KEY",
    "voyage-4-lite": "VOYAGE_API_KEY",
    "voyage-4-large": "VOYAGE_API_KEY",
    "voyage-code-3": "VOYAGE_API_KEY",
    "voyage-context-4": "VOYAGE_API_KEY",
    "gemini-embedding-001": "GEMINI_API_KEY",
    "openai-text-embedding-3-small": "OPENAI_API_KEY",
}


def check_embed_model_key(embed_model: str | None) -> None:
    """Fail fast when an API embedder arm is selected but its provider key is
    missing from the parent env. Local arms (small/base/modernbert/gemma/
    qwen3) and off/noop need no key and are silently allowed through."""
    if not embed_model:
        return
    var = API_KEY_ENV_BY_ARM.get(embed_model)
    if var is None:
        return
    if not os.environ.get(var, "").strip():
        raise RuntimeError(
            f"--embed-model {embed_model}: {var} is not set (required to "
            "construct this API embedding provider)"
        )


def sh(cmd: list[str], **kwargs) -> subprocess.CompletedProcess:
    return subprocess.run(cmd, capture_output=True, text=True, **kwargs)


def provision_tenant(cli_bin: str, database_url: str, name_prefix: str = "gate") -> tuple[str, str]:
    """Creates a fresh tenant + a ``trusted_system``-max API key against
    ``database_url`` via the packaged CLI's admin commands. Returns
    ``(tenant_id, api_key)``."""
    name = f"{name_prefix}-{int(time.time())}"
    out = sh([cli_bin, "admin", "create-tenant", "--name", name, "--database-url", database_url])
    if out.returncode != 0:
        raise RuntimeError(f"create-tenant failed: {out.stderr.strip()}")
    match = re.search(r"tenant_created id=(\S+)", out.stdout)
    if not match:
        raise RuntimeError(f"could not parse tenant id from: {out.stdout}")
    tenant_id = match.group(1)
    out = sh(
        [cli_bin, "admin", "create-key", "--tenant", tenant_id,
         "--max-trust", "trusted_system", "--database-url", database_url]
    )
    if out.returncode != 0:
        raise RuntimeError(f"create-key failed: {out.stderr.strip()}")
    api_key = out.stdout.strip().splitlines()[-1].strip()
    if not api_key.startswith("mk_"):
        raise RuntimeError(f"could not parse api key from: {out.stdout}")
    return tenant_id, api_key


def check_port_free(port: int) -> None:
    """Refuse to spawn a server on ``port`` if something is already
    LISTENing there. A prior run that leaked its server child can otherwise
    sit on the port forever, silently dropping every subsequent server on an
    instant bind failure. Best effort: if ``lsof`` is unavailable, proceed
    without the check."""
    try:
        check = sh(["lsof", "-nP", f"-iTCP:{port}", "-sTCP:LISTEN"])
    except OSError:
        return
    if check.returncode != 0 or not check.stdout.strip():
        return
    lines = [line for line in check.stdout.strip().splitlines() if line.strip()]
    pid = None
    for line in lines[1:]:
        parts = line.split()
        if len(parts) > 1:
            pid = parts[1]
            break
    pid_msg = f" held by PID {pid}" if pid else " (PID undiscoverable)"
    raise RuntimeError(
        f"port {port} is already in LISTEN state{pid_msg} — refusing to start "
        f"a new server; a leaked process from a prior run may still be bound "
        f"here. lsof output:\n{chr(10).join(lines)}"
    )


class Server:
    """The packaged ``memphant-server`` binary, started against a scratch
    Postgres database with an optional ``MEMPHANT_EMBEDDINGS`` arm selector
    (R0-T3's ``--embed-model`` seam). A 10-minute health wait (first boot can
    download embedding-model weights) with a fast-fail if the child exits
    early, optional stdout+stderr capture to ``log_path`` (tailed into any
    raised error), and a port-conflict check before spawning."""

    def __init__(
        self,
        server_bin: str,
        database_url: str,
        port: int,
        embed_model: str | None = None,
        log_path: Path | None = None,
    ) -> None:
        self.server_bin = server_bin
        self.database_url = database_url
        self.port = port
        self.embed_model = embed_model
        self.log_path = log_path
        self.proc: subprocess.Popen | None = None
        self._log_file = None

    def _tail_log(self, n: int = LOG_TAIL_LINES) -> str:
        if self.log_path is None or not self.log_path.exists():
            return ""
        try:
            lines = self.log_path.read_text(errors="replace").splitlines()
        except OSError:
            return ""
        tail = lines[-n:]
        if not tail:
            return ""
        return f"--- last {len(tail)} lines of {self.log_path} ---\n" + "\n".join(tail)

    def _terminate(self) -> None:
        if self.proc is not None:
            self.proc.terminate()
            try:
                self.proc.wait(timeout=10)
            except subprocess.TimeoutExpired:
                self.proc.kill()
                self.proc.wait()
            self.proc = None
        if self._log_file is not None:
            try:
                self._log_file.close()
            except OSError:
                pass
            self._log_file = None

    def start(self) -> None:
        check_port_free(self.port)
        env = dict(os.environ)
        env["DATABASE_URL"] = self.database_url
        env["MEMPHANT_BIND"] = f"127.0.0.1:{self.port}"
        env.setdefault("RUST_LOG", "warn")
        if self.embed_model:
            env["MEMPHANT_EMBEDDINGS"] = self.embed_model
        if self.log_path is not None:
            self.log_path.parent.mkdir(parents=True, exist_ok=True)
            self._log_file = open(self.log_path, "w")
            stdout_target: object = self._log_file
            stderr_target: object = self._log_file
        else:
            stdout_target = subprocess.DEVNULL
            stderr_target = subprocess.DEVNULL
        self.proc = subprocess.Popen(
            [self.server_bin], env=env,
            stdout=stdout_target, stderr=stderr_target,
        )
        deadline = time.time() + HEALTH_WAIT_TIMEOUT_S
        while time.time() < deadline:
            if self.proc.poll() is not None:
                code = self.proc.returncode
                tail = self._tail_log()
                self._terminate()
                msg = (
                    f"memphant-server child exited (code={code}) before "
                    f"becoming healthy on :{self.port}"
                )
                raise RuntimeError(f"{msg}\n{tail}" if tail else msg)
            try:
                conn = http.client.HTTPConnection("127.0.0.1", self.port, timeout=2)
                conn.request("GET", "/v1/health")
                if conn.getresponse().status == 200:
                    conn.close()
                    return
            except OSError:
                pass
            time.sleep(HEALTH_POLL_INTERVAL_S)
        tail = self._tail_log()
        self._terminate()
        msg = (
            f"server did not become healthy on :{self.port} within "
            f"{HEALTH_WAIT_TIMEOUT_S:.0f}s (timed out waiting for /v1/health)"
        )
        raise RuntimeError(f"{msg}\n{tail}" if tail else msg)

    def stop(self) -> None:
        self._terminate()


class ApiClient:
    """Minimal retrying JSON/HTTP client against a running ``Server``."""

    def __init__(self, port: int, api_key: str, tenant_id: str) -> None:
        self.port = port
        self.headers = {
            "Authorization": f"Bearer {api_key}",
            "Content-Type": "application/json",
        }
        self.tenant_id = tenant_id
        self.conn = http.client.HTTPConnection("127.0.0.1", port, timeout=120)

    def post(self, path: str, payload: dict) -> dict:
        body = json.dumps(payload)
        for attempt in range(3):
            try:
                self.conn.request("POST", path, body=body, headers=self.headers)
                response = self.conn.getresponse()
                data = response.read()
                if response.status >= 400:
                    raise RuntimeError(f"{path} -> HTTP {response.status}: {data[:300]!r}")
                return json.loads(data)
            except (http.client.HTTPException, OSError) as error:
                self.conn.close()
                self.conn = http.client.HTTPConnection("127.0.0.1", self.port, timeout=120)
                if attempt == 2:
                    raise RuntimeError(f"{path} failed after retries: {error}")
        raise AssertionError("unreachable")


def reexec_through_scratch_db(base_url: str) -> None:
    """Re-exec this runner through ``scripts/with_scratch_db.sh`` so the bench
    operates on a fresh, migrated, auto-dropped scratch DB minted from
    ``base_url`` — never a shared named DB whose foreign ``job_state`` debris
    could starve (or be starved by) the run's global oldest-first worker ticks.
    Mirrors ``scripts/e2e_probe.sh``'s self re-exec: ``MEMPHANT_SCRATCH_ACTIVE``
    guards the recursion (set it, with ``DATABASE_URL`` already on an isolated
    DB, to skip). Returns (no-op) in the scratch-active child; in the parent it
    never returns (``os.execvp`` replaces the process). The scratch URL reaches
    the child as ``DATABASE_URL``; with_scratch_db.sh drops the DB on its EXIT
    trap, so even a killed bench leaves no debris behind."""
    if os.environ.get("MEMPHANT_SCRATCH_ACTIVE"):
        return
    helper = str(Path(__file__).resolve().parent / "with_scratch_db.sh")
    os.environ["MEMPHANT_SCRATCH_ACTIVE"] = "1"
    os.execvp("bash", ["bash", helper, base_url, "DATABASE_URL", sys.executable, *sys.argv])


def drain_worker(worker_bin: str, database_url: str, embed_model: str | None = None, max_ticks: int = 4000) -> int:
    """Runs the packaged ``memphant-worker`` in ``MEMPHANT_WORKER_ONCE=1``
    ticks until a tick completes zero jobs. Returns the total jobs
    completed."""
    env = dict(os.environ)
    env["DATABASE_URL"] = database_url
    env["MEMPHANT_WORKER_ONCE"] = "1"
    env.setdefault("RUST_LOG", "warn")
    if embed_model:
        env["MEMPHANT_EMBEDDINGS"] = embed_model
    total = 0
    for _tick in range(max_ticks):
        out = sh([worker_bin], env=env)
        if out.returncode != 0:
            raise RuntimeError(f"worker tick failed: {out.stderr.strip()[:300]}")
        match = re.search(r"completed=(\d+)", out.stdout)
        completed = int(match.group(1)) if match else 0
        total += completed
        if completed == 0:
            return total
    raise RuntimeError(f"worker did not drain within {max_ticks} ticks (total={total})")


def recall_query(
    client: ApiClient, scope_id: str, actor_id: str, query: str, k: int, budget_tokens: int, mode: str
) -> tuple[list[str], bool]:
    """Calls ``/v1/recall`` and returns ``(item_bodies_by_rank, degraded)``.
    Scope/actor ids are passed explicitly since each runner mints its own
    fixed scope/actor pair."""
    payload = {
        "tenant_id": client.tenant_id,
        "scope_id": scope_id,
        "actor_id": actor_id,
        "query": query,
        "limit": k,
        "budget_tokens": budget_tokens,
        "mode": mode,
    }
    response = client.post("/v1/recall", payload)
    bodies = [item["body"] for item in response.get("items", [])]
    return bodies, bool(response.get("degraded", False))

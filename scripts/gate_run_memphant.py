#!/usr/bin/env python3
"""MemPhant engine runner for the Syndai replacement gate (W10).

Ingests the pinned Syndai docs corpus into MemPhant as tri-domain RESOURCES via
the real runtime path (packaged ``memphant-server`` + ``memphant-worker`` +
``memphant-cli`` against Postgres), then calls ``/v1/recall`` (k=10) per golden
question and emits:
- an evidence JSONL in the exact shape ``scripts/run_reader.py`` consumes (so
  the SAME reader/judge scores this engine and Syndai);
- a provenance report with per-question span-level hit@5/hit@10 + R@5/R@10,
  graded by ``gate_common.provenance_hit`` (identical grading for both engines).

Ingest granularity: MemPhant's resource channel does not auto-chunk (one
resource body -> one whole-document unit), so to compare against Syndai's
internally-chunked search at the SAME granularity the gate pre-splits the corpus
into markdown sections (``gate_common``) and ingests each section as one
resource (``kind=document``). Every golden's source section is in the haystack,
so a perfect engine could hit @1.

Isolation: runs against a FRESH database + a freshly-minted tenant. The default
DATABASE_URL points at a dedicated ``memphant_gate`` database (NOT the shared
campaign DB on the default ``memphant`` database) so the worker's global
job-claim can never touch — or be starved by — campaign job debris.
"""

from __future__ import annotations

import argparse
import hashlib
import http.client
import json
import os
import re
import subprocess
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import gate_common as gc  # noqa: E402

DEFAULT_DATABASE_URL = "postgres://memphant:memphant@localhost:5432/memphant_gate"
DEFAULT_SYNDAI_ROOT = Path("/Users/sidsharma/Syndai")
GOLDEN_PATH = gc.MEMPHANT_ROOT / "benchmarks" / "data" / "syndai_docs_golden.jsonl"
GOLDEN_LOCK_PATH = gc.MEMPHANT_ROOT / "benchmarks" / "data" / "syndai_docs_golden.lock.json"
SCOPE_ID = "7c000000-0000-4000-8000-0000000000a1"
ACTOR_ID = "7c000000-0000-4000-8000-0000000000a2"


def sh(cmd: list[str], **kwargs) -> subprocess.CompletedProcess:
    return subprocess.run(cmd, capture_output=True, text=True, **kwargs)


def provision_tenant(cli_bin: str, database_url: str) -> tuple[str, str]:
    name = f"syndai-gate-{int(time.time())}"
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


class Server:
    def __init__(self, server_bin: str, database_url: str, port: int) -> None:
        self.server_bin = server_bin
        self.database_url = database_url
        self.port = port
        self.proc: subprocess.Popen | None = None

    def start(self) -> None:
        env = dict(os.environ)
        env["DATABASE_URL"] = self.database_url
        env["MEMPHANT_BIND"] = f"127.0.0.1:{self.port}"
        env.setdefault("RUST_LOG", "warn")
        self.proc = subprocess.Popen(
            [self.server_bin], env=env,
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
        )
        for _ in range(80):
            try:
                conn = http.client.HTTPConnection("127.0.0.1", self.port, timeout=2)
                conn.request("GET", "/v1/health")
                if conn.getresponse().status == 200:
                    conn.close()
                    return
            except OSError:
                pass
            time.sleep(0.25)
        raise RuntimeError(f"server did not become healthy on :{self.port}")

    def stop(self) -> None:
        if self.proc is not None:
            self.proc.terminate()
            try:
                self.proc.wait(timeout=10)
            except subprocess.TimeoutExpired:
                self.proc.kill()
            self.proc = None


class ApiClient:
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


def ingest_section(client: ApiClient, section: gc.Section) -> str:
    body = section.body
    payload = {
        "tenant_id": client.tenant_id,
        "scope_id": SCOPE_ID,
        "actor_id": ACTOR_ID,
        "source_kind": "docs",
        "source_trust": "trusted_system",
        "resource": {
            "uri": section.uri(),
            "mime_type": "text/markdown",
            "content_hash": "sha256:" + hashlib.sha256(body.encode()).hexdigest(),
            "kind": "document",
            "revision": "syndai-gate",
            "body": body,
        },
    }
    response = client.post("/v1/episodes", payload)
    return response.get("resource_id") or ""


def drain_worker(worker_bin: str, database_url: str, max_ticks: int = 4000) -> int:
    env = dict(os.environ)
    env["DATABASE_URL"] = database_url
    env["MEMPHANT_WORKER_ONCE"] = "1"
    env.setdefault("RUST_LOG", "warn")
    total = 0
    for tick in range(max_ticks):
        out = sh([worker_bin], env=env)
        if out.returncode != 0:
            raise RuntimeError(f"worker tick failed: {out.stderr.strip()[:300]}")
        match = re.search(r"completed=(\d+)", out.stdout)
        completed = int(match.group(1)) if match else 0
        total += completed
        if completed == 0:
            return total
    raise RuntimeError(f"worker did not drain within {max_ticks} ticks (total={total})")


def recall(client: ApiClient, question: str, k: int, budget_tokens: int, mode: str) -> tuple[list[str], bool]:
    payload = {
        "tenant_id": client.tenant_id,
        "scope_id": SCOPE_ID,
        "actor_id": ACTOR_ID,
        "query": question,
        "limit": k,
        # Raise the pack budget so the top-k ranked units are returned rather
        # than truncated to the default 512-token answer budget — this makes the
        # k=10 comparison against Syndai's raw top-k retrieval apples-to-apples.
        "budget_tokens": budget_tokens,
        "mode": mode,
    }
    response = client.post("/v1/recall", payload)
    bodies = [item["body"] for item in response.get("items", [])]
    return bodies, bool(response.get("degraded", False))


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--database-url", default=DEFAULT_DATABASE_URL)
    parser.add_argument("--syndai-root", default=str(DEFAULT_SYNDAI_ROOT))
    parser.add_argument("--golden", default=str(GOLDEN_PATH))
    parser.add_argument("--out-evidence", required=True)
    parser.add_argument("--out-provenance", required=True)
    parser.add_argument("--port", type=int, default=39412)
    parser.add_argument("--k", type=int, default=10)
    parser.add_argument("--budget-tokens", type=int, default=8192)
    parser.add_argument("--mode", default="exhaustive", choices=("fast", "balanced", "exhaustive"))
    parser.add_argument("--limit-haystack", type=int, default=0, help="0 = full corpus")
    parser.add_argument("--server-bin", default=str(gc.MEMPHANT_ROOT / "target/debug/memphant-server"))
    parser.add_argument("--worker-bin", default=str(gc.MEMPHANT_ROOT / "target/debug/memphant-worker"))
    parser.add_argument("--cli-bin", default=str(gc.MEMPHANT_ROOT / "target/debug/memphant-cli"))
    args = parser.parse_args()

    goldens = gc.load_goldens(Path(args.golden))
    lock = json.loads(GOLDEN_LOCK_PATH.read_text())
    actual_sha = gc.sha256_hex(Path(args.golden).read_bytes())
    if actual_sha != lock["sha256"]:
        raise RuntimeError(
            f"golden sha256 mismatch: file={actual_sha[:12]} lock={lock['sha256'][:12]}"
        )
    print(f"goldens={len(goldens)} sha256={actual_sha[:12]} (lock verified)", file=sys.stderr)

    root = Path(args.syndai_root)
    files = gc.list_corpus_files(root)
    haystack = gc.candidate_sections(gc.all_sections(root, files))
    if args.limit_haystack:
        # Deterministic cap that always keeps every gold section.
        gold_keys = {g["source_section_key"] for g in goldens}
        gold_keys |= {
            part for g in goldens for part in g["source_section_key"].split("||")
        }
        kept = [s for s in haystack if s.key() in gold_keys]
        others = [s for s in haystack if s.key() not in gold_keys]
        haystack = kept + others[: max(0, args.limit_haystack - len(kept))]
    # Coverage assertion: every gold section must be ingestable.
    keys = {s.key() for s in haystack}
    for g in goldens:
        for part in g["source_section_key"].split("||"):
            if part not in keys:
                raise RuntimeError(f"gold section not in haystack: {part}")
    print(f"haystack sections={len(haystack)}", file=sys.stderr)

    tenant_id, api_key = provision_tenant(args.cli_bin, args.database_url)
    print(f"tenant={tenant_id}", file=sys.stderr)

    server = Server(args.server_bin, args.database_url, args.port)
    server.start()
    try:
        client = ApiClient(args.port, api_key, tenant_id)
        t0 = time.time()
        for i, section in enumerate(haystack):
            ingest_section(client, section)
            if (i + 1) % 500 == 0:
                print(f"  ingested {i + 1}/{len(haystack)}", file=sys.stderr)
        print(f"ingest done in {time.time() - t0:.1f}s; draining worker...", file=sys.stderr)
        compiled = drain_worker(args.worker_bin, args.database_url)
        print(f"worker drained: compiled={compiled} jobs", file=sys.stderr)

        evidence_rows = []
        provenance_rows = []
        for i, golden in enumerate(goldens):
            bodies, degraded = recall(
                client, golden["question"], args.k, args.budget_tokens, args.mode
            )
            evidence_rows.append(gc.evidence_row(golden, bodies, args.k))
            provenance_rows.append(
                {
                    "question_id": golden["question_id"],
                    "question_type": golden["question_type"],
                    "multi_hop": golden["multi_hop"],
                    "returned_items": len(bodies),
                    "degraded": degraded,
                    "hit_at_5": gc.provenance_hit(golden, bodies, 5),
                    "hit_at_10": gc.provenance_hit(golden, bodies, min(10, args.k)),
                }
            )
            if (i + 1) % 20 == 0:
                print(f"  recalled {i + 1}/{len(goldens)}", file=sys.stderr)
    finally:
        server.stop()

    gc.write_jsonl(Path(args.out_evidence), evidence_rows)
    n = len(provenance_rows)
    r5 = sum(r["hit_at_5"] for r in provenance_rows) / n if n else 0.0
    r10 = sum(r["hit_at_10"] for r in provenance_rows) / n if n else 0.0
    report = {
        "engine": "memphant",
        "runtime": "memphant-server resource ingest + /v1/recall",
        "database_url_db": args.database_url.rsplit("/", 1)[-1],
        "k": args.k,
        "recall_mode": args.mode,
        "budget_tokens": args.budget_tokens,
        "haystack_sections": len(haystack),
        "golden_sha256": actual_sha,
        "golden_count": n,
        "recall_at_5": r5,
        "recall_at_10": r10,
        "per_question": provenance_rows,
    }
    Path(args.out_provenance).write_text(json.dumps(report, indent=2) + "\n")
    print(
        f"memphant done: R@5={r5:.3f} R@10={r10:.3f} n={n} "
        f"evidence={args.out_evidence} provenance={args.out_provenance}",
        file=sys.stderr,
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

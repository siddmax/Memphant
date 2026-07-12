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

Isolation: each run re-execs itself through ``scripts/with_scratch_db.sh``
(``gate_runtime.reexec_through_scratch_db``), so it operates on a fresh,
migrated, per-run scratch DB minted from ``--database-url`` (the campaign
*base* server, NOT a run DB) and dropped on exit — even if the bench is killed.
A freshly-minted tenant lives in that ephemeral DB. No shared named DB means
the worker's global oldest-first job-claim can never touch, or be starved by,
another harness's ``job_state`` debris. Mirrors the e2e probe + pg contract
tests, which already route through the same helper.

R0 embedder bakeoff (T3): ``--embed-model <id>`` threads ``MEMPHANT_EMBEDDINGS``
into BOTH the server and worker subprocess env, so one ingest can be scored
under any arm in the shared ``embedder_from_id`` grammar (memphant-runtime).
``--label`` tags stderr progress lines and the provenance header so artifacts
from a queue of arms are self-describing. Multiple golden sets (v1 + v2, which
share the identical pinned corpus) can be recalled against ONE ingest by
repeating ``--golden`` paired positionally with ``--out-evidence`` /
``--out-provenance`` — ingesting the corpus once and recalling N times, instead
of re-ingesting per golden set.
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

# The API-arm key map + fail-fast check live in the shared gate_runtime
# module (single source of truth for every gate runner — this script's copy
# and code_lane_run_memphant.py's copy drifted once already, so the map was
# centralized; pinned against the Rust `embedder_from_id` grammar by
# tests/test_gate_runtime.py). Names re-exported here unchanged so main()'s
# call site and any external reference keep working.
from gate_runtime import (  # noqa: E402, F401
    API_KEY_ENV_BY_ARM,
    check_embed_model_key,
    reexec_through_scratch_db,
)

# Base campaign *server* URL to mint the per-run scratch DB from (with_scratch_db.sh
# only uses it to reach the server + the admin `postgres` DB; the named DB in it is
# never touched). The run itself uses a fresh ephemeral DB, never this one.
DEFAULT_BASE_DATABASE_URL = "postgres://memphant:memphant@localhost:5432/memphant"
DEFAULT_SYNDAI_ROOT = Path("/Users/sidsharma/Syndai")
GOLDEN_PATH = gc.MEMPHANT_ROOT / "benchmarks" / "data" / "syndai_docs_golden.jsonl"
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


def check_port_free(port: int) -> None:
    """Refuse to spawn a server on `port` if something is already LISTENing
    there. A prior run that leaked its server child (e.g. died mid health-wait
    without being killed) can otherwise sit on the port forever, silently
    dropping every subsequent arm's server on an instant bind failure. Best
    effort: if ``lsof`` is unavailable, proceed without the check."""
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


HEALTH_WAIT_TIMEOUT_S = 600.0  # first boot may download embedding model weights (up to 1.5GB)
HEALTH_POLL_INTERVAL_S = 0.5
LOG_TAIL_LINES = 15


class Server:
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


def ingest_section(client: ApiClient, section: gc.Section, breadcrumb: bool = False) -> str:
    """POSTs one section as a resource. ``breadcrumb=True`` prefixes the body
    with Syndai's "Section path: a > b" convention (``gate_common.
    breadcrumb_prefix`` — byte-identical to ``processing_chunks.py:84``)
    before hashing/posting it; the rest of the payload (uri, mime_type, kind,
    revision) is unaffected — only ``body`` (and the ``content_hash`` derived
    from it) changes."""
    body = section.body
    if breadcrumb:
        body = gc.breadcrumb_prefix(section.heading_path) + body
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


def drain_worker(worker_bin: str, database_url: str, embed_model: str | None = None, max_ticks: int = 4000) -> int:
    env = dict(os.environ)
    env["DATABASE_URL"] = database_url
    env["MEMPHANT_WORKER_ONCE"] = "1"
    env.setdefault("RUST_LOG", "warn")
    if embed_model:
        env["MEMPHANT_EMBEDDINGS"] = embed_model
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


def build_provenance_report(
    *,
    embed_model: str | None,
    label: str | None,
    breadcrumb: bool,
    golden_path: Path,
    database_url: str,
    k: int,
    mode: str,
    budget_tokens: int,
    haystack_len: int,
    golden_sha: str,
    provenance_rows: list[dict],
) -> dict:
    """Assembles the self-describing provenance-report header + per-question
    rows. ``breadcrumb`` records whether ``--breadcrumb`` was set for this
    run (R1-T1) so artifacts are self-describing without re-deriving it from
    the label string."""
    n = len(provenance_rows)
    r5 = sum(r["hit_at_5"] for r in provenance_rows) / n if n else 0.0
    r10 = sum(r["hit_at_10"] for r in provenance_rows) / n if n else 0.0
    return {
        "engine": "memphant",
        "runtime": "memphant-server resource ingest + /v1/recall",
        "embed_model": embed_model,
        "label": label,
        "breadcrumb": breadcrumb,
        "golden_path": str(golden_path),
        "database_url_db": database_url.rsplit("/", 1)[-1],
        "k": k,
        "recall_mode": mode,
        "budget_tokens": budget_tokens,
        "haystack_sections": haystack_len,
        "golden_sha256": golden_sha,
        "golden_count": n,
        "recall_at_5": r5,
        "recall_at_10": r10,
        "per_question": provenance_rows,
    }


def build_arg_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--database-url", default=DEFAULT_BASE_DATABASE_URL,
        help="base campaign SERVER url to mint the per-run scratch DB from; the "
             "run uses a fresh ephemeral DB dropped on exit, never this one",
    )
    parser.add_argument("--syndai-root", default=str(DEFAULT_SYNDAI_ROOT))
    parser.add_argument(
        "--golden",
        action="append",
        default=None,
        help=(
            "golden JSONL path (default: the v1 syndai_docs_golden.jsonl). "
            "Repeatable: recall MULTIPLE golden sets against ONE ingest, "
            "paired positionally with --out-evidence/--out-provenance."
        ),
    )
    parser.add_argument(
        "--out-evidence",
        action="append",
        required=True,
        help="evidence JSONL output path; one per --golden (positional pairing)",
    )
    parser.add_argument(
        "--out-provenance",
        action="append",
        required=True,
        help="provenance report JSON output path; one per --golden (positional pairing)",
    )
    parser.add_argument(
        "--embed-model",
        default=None,
        help=(
            "MEMPHANT_EMBEDDINGS id (memphant-runtime embedder_from_id grammar) "
            "passed into BOTH the server and worker subprocess env; default "
            "unset = shipped default embedder"
        ),
    )
    parser.add_argument(
        "--label",
        default=None,
        help="arm label prefixed on stderr progress lines and recorded in the provenance header",
    )
    parser.add_argument(
        "--breadcrumb",
        action="store_true",
        help=(
            "prefix each ingested section body with Syndai's deterministic "
            "context-prefix convention (processing_chunks.py:84): "
            "'Section path: ' + ' > '.join(heading_path) + a blank line, "
            "byte-identical to Syndai's own embedding-input prefix. Sections "
            "with an empty heading_path get no prefix, mirroring Syndai's "
            "own check exactly. Recorded as 'breadcrumb' in the provenance "
            "header."
        ),
    )
    parser.add_argument("--port", type=int, default=39412)
    parser.add_argument("--k", type=int, default=10)
    parser.add_argument("--budget-tokens", type=int, default=8192)
    parser.add_argument("--mode", default="exhaustive", choices=("fast", "balanced", "exhaustive"))
    parser.add_argument("--limit-haystack", type=int, default=0, help="0 = full corpus")
    parser.add_argument("--server-bin", default=str(gc.MEMPHANT_ROOT / "target/debug/memphant-server"))
    parser.add_argument("--worker-bin", default=str(gc.MEMPHANT_ROOT / "target/debug/memphant-worker"))
    parser.add_argument("--cli-bin", default=str(gc.MEMPHANT_ROOT / "target/debug/memphant-cli"))
    return parser


def main() -> int:
    parser = build_arg_parser()
    args = parser.parse_args()

    # Re-exec through with_scratch_db.sh (unless already inside one): mints a
    # fresh migrated DB, drops it on exit. args.database_url then points at that
    # scratch DB for every downstream call (provision/server/worker/report).
    reexec_through_scratch_db(args.database_url)
    args.database_url = os.environ["DATABASE_URL"]

    golden_paths = [Path(p) for p in (args.golden or [str(GOLDEN_PATH)])]
    if not (len(golden_paths) == len(args.out_evidence) == len(args.out_provenance)):
        raise RuntimeError(
            "--golden / --out-evidence / --out-provenance counts must match "
            f"(got {len(golden_paths)}/{len(args.out_evidence)}/{len(args.out_provenance)}); "
            "each --golden is paired positionally with one --out-evidence and one --out-provenance"
        )

    check_embed_model_key(args.embed_model)
    label_prefix = f"[{args.label}] " if args.label else ""

    golden_sets: list[tuple[Path, list[dict], str]] = []
    for golden_path in golden_paths:
        goldens = gc.load_goldens(golden_path)
        lock = json.loads(gc.golden_lock_path(golden_path).read_text())
        actual_sha = gc.sha256_hex(golden_path.read_bytes())
        if actual_sha != lock["sha256"]:
            raise RuntimeError(
                f"golden sha256 mismatch ({golden_path.name}): "
                f"file={actual_sha[:12]} lock={lock['sha256'][:12]}"
            )
        print(
            f"{label_prefix}goldens={len(goldens)} path={golden_path.name} "
            f"sha256={actual_sha[:12]} (lock verified)",
            file=sys.stderr,
        )
        golden_sets.append((golden_path, goldens, actual_sha))

    root = Path(args.syndai_root)
    files = gc.list_corpus_files(root)
    haystack = gc.candidate_sections(gc.all_sections(root, files))
    if args.limit_haystack:
        # Deterministic cap that always keeps every gold section (across all
        # golden sets being recalled this run).
        gold_keys: set[str] = set()
        for _, goldens, _ in golden_sets:
            gold_keys |= {g["source_section_key"] for g in goldens}
            gold_keys |= {
                part for g in goldens for part in g["source_section_key"].split("||")
            }
        kept = [s for s in haystack if s.key() in gold_keys]
        others = [s for s in haystack if s.key() not in gold_keys]
        haystack = kept + others[: max(0, args.limit_haystack - len(kept))]
    # Coverage assertion: every gold section (in every golden set) must be
    # ingestable.
    keys = {s.key() for s in haystack}
    for _, goldens, _ in golden_sets:
        for g in goldens:
            for part in g["source_section_key"].split("||"):
                if part not in keys:
                    raise RuntimeError(f"gold section not in haystack: {part}")
    print(f"{label_prefix}haystack sections={len(haystack)}", file=sys.stderr)

    tenant_id, api_key = provision_tenant(args.cli_bin, args.database_url)
    print(f"{label_prefix}tenant={tenant_id}", file=sys.stderr)

    log_name = f"server-{args.label}.log" if args.label else "server.log"
    server_log_path = Path(args.out_provenance[0]).resolve().parent / log_name
    server = Server(
        args.server_bin, args.database_url, args.port, args.embed_model,
        log_path=server_log_path,
    )
    # Symmetric cleanup: start() and the ingest/recall body are both inside
    # this try so the server child is always killed on any exception path,
    # not just after a successful start (a failed start() already
    # self-terminates before raising; stop() here is then a safe no-op).
    try:
        server.start()
        client = ApiClient(args.port, api_key, tenant_id)
        t0 = time.time()
        for i, section in enumerate(haystack):
            ingest_section(client, section, breadcrumb=args.breadcrumb)
            if (i + 1) % 500 == 0:
                print(f"{label_prefix}  ingested {i + 1}/{len(haystack)}", file=sys.stderr)
        print(f"{label_prefix}ingest done in {time.time() - t0:.1f}s; draining worker...", file=sys.stderr)
        compiled = drain_worker(args.worker_bin, args.database_url, args.embed_model)
        print(f"{label_prefix}worker drained: compiled={compiled} jobs", file=sys.stderr)

        for (golden_path, goldens, golden_sha), out_evidence, out_provenance in zip(
            golden_sets, args.out_evidence, args.out_provenance
        ):
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
                    print(
                        f"{label_prefix}  recalled {i + 1}/{len(goldens)} ({golden_path.name})",
                        file=sys.stderr,
                    )

            gc.write_jsonl(Path(out_evidence), evidence_rows)
            report = build_provenance_report(
                embed_model=args.embed_model,
                label=args.label,
                breadcrumb=args.breadcrumb,
                golden_path=golden_path,
                database_url=args.database_url,
                k=args.k,
                mode=args.mode,
                budget_tokens=args.budget_tokens,
                haystack_len=len(haystack),
                golden_sha=golden_sha,
                provenance_rows=provenance_rows,
            )
            Path(out_provenance).write_text(json.dumps(report, indent=2) + "\n")
            print(
                f"{label_prefix}{golden_path.name} done: R@5={report['recall_at_5']:.3f} "
                f"R@10={report['recall_at_10']:.3f} n={report['golden_count']} "
                f"evidence={out_evidence} provenance={out_provenance}",
                file=sys.stderr,
            )
    finally:
        server.stop()

    return 0


if __name__ == "__main__":
    raise SystemExit(main())

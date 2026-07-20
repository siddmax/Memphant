from __future__ import annotations

import json
import sys
import threading
import tomllib
from http.server import BaseHTTPRequestHandler, HTTPServer
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "bindings" / "python"))

from memphant import MemPhant, MemPhantValidationError  # noqa: E402


def test_python_sdk_round_trips_all_public_verbs() -> None:
    server = FakeMemphantServer()
    client = MemPhant(base_url=server.base_url, api_key="test-key")
    try:
        retained = client.retain(
            tenant_id="00000000-0000-0000-0000-0000000186a0",
            scope_id="00000000-0000-0000-0000-0000000186a1",
            actor_id="00000000-0000-0000-0000-0000000186a2",
            source_kind="system",
            source_trust="trusted_system",
            body="Release region is Taipei.",
            subject_hint="release region",
        )
        assert retained["episode_id"] == "ep_test"

        client.retain_resource(
            tenant_id="00000000-0000-0000-0000-0000000186a0",
            scope_id="00000000-0000-0000-0000-0000000186a1",
            actor_id="00000000-0000-0000-0000-0000000186a2",
            source_trust="trusted_user",
            uri="repo://demo/src/main.rs",
            mime_type="text/x-rust",
            content_hash="sha256:abc",
            kind="code",
            revision="abc123",
            body="fn main() {}",
        )
        client.retain_unit(
            tenant_id="00000000-0000-0000-0000-0000000186a0",
            scope_id="00000000-0000-0000-0000-0000000186a1",
            actor_id="00000000-0000-0000-0000-0000000186a2",
            source_trust="trusted_user",
            kind="semantic",
            subject="release region",
            predicate="value",
            body="Release region is Taipei.",
        )

        reflected = client.reflect(
            tenant_id="00000000-0000-0000-0000-0000000186a0",
            scope_id="00000000-0000-0000-0000-0000000186a1",
            actor_id="00000000-0000-0000-0000-0000000186a2",
        )
        assert reflected["episodes_consumed"] == 1

        recalled = client.recall(
            tenant_id="00000000-0000-0000-0000-0000000186a0",
            scope_id="00000000-0000-0000-0000-0000000186a1",
            actor_id="00000000-0000-0000-0000-0000000186a2",
            query="Where is the release region?",
            aggregation_window={
                "from": "2025-06-01T00:00:00Z",
                "to": "2025-06-08T00:00:00Z",
            },
        )
        assert recalled["items"][0]["body"] == "Release region is Taipei."

        trace = client.trace("00000000-0000-0000-0000-000000099001")
        assert trace["id"] == "00000000-0000-0000-0000-000000099001"

        corrected = client.correct(
            tenant_id="00000000-0000-0000-0000-0000000186a0",
            scope_id="00000000-0000-0000-0000-0000000186a1",
            actor_id="00000000-0000-0000-0000-0000000186a2",
            memory_unit_id="00000000-0000-0000-0000-000000088001",
            value="Release region is Singapore.",
            reason="stale_fact",
        )
        assert corrected["correction_kind"] == "current"

        forgotten = client.forget(
            tenant_id="00000000-0000-0000-0000-0000000186a0",
            scope_id="00000000-0000-0000-0000-0000000186a1",
            actor_id="00000000-0000-0000-0000-0000000186a2",
            memory_unit_id="00000000-0000-0000-0000-000000088001",
            reason="user_request",
        )
        assert forgotten["verification"] == "no_recall_path_returns_forgotten"

        marked = client.mark(
            tenant_id="00000000-0000-0000-0000-0000000186a0",
            trace_id="00000000-0000-0000-0000-000000099001",
            caller_id="pytest",
            used_ids=["00000000-0000-0000-0000-000000088001"],
            outcome="success",
        )
        assert marked["accepted"] is True

        paths = [request["path"] for request in server.requests]
        assert paths == [
            "/v1/episodes",
            "/v1/episodes",
            "/v1/episodes",
            "/v1/reflect",
            "/v1/recall",
            "/v1/traces/00000000-0000-0000-0000-000000099001",
            "/v1/correct",
            "/v1/forget",
            "/v1/mark",
        ]
        assert all(
            request["headers"].get("authorization") == "Bearer test-key"
            for request in server.requests
        )

        resource_request = server.requests[1]["body"]
        assert resource_request["resource"]["uri"] == "repo://demo/src/main.rs"
        assert resource_request["resource"]["revision"] == "abc123"
        assert "body" not in resource_request

        unit_request = server.requests[2]["body"]
        assert unit_request["unit"]["subject"] == "release region"
        assert unit_request["unit"]["kind"] == "semantic"
        assert server.requests[4]["body"]["aggregation_window"] == {
            "from": "2025-06-01T00:00:00Z",
            "to": "2025-06-08T00:00:00Z",
        }
    finally:
        server.close()


def test_python_sdk_maps_error_envelopes_to_typed_exceptions() -> None:
    server = FakeMemphantServer(error_on_recall=True)
    client = MemPhant(base_url=server.base_url, api_key="test-key")
    try:
        try:
            client.recall(
                tenant_id="00000000-0000-0000-0000-0000000186a0",
                scope_id="00000000-0000-0000-0000-0000000186a1",
                actor_id="00000000-0000-0000-0000-0000000186a2",
                query="bad",
            )
        except MemPhantValidationError as exc:
            assert exc.code == "invalid_request"
            assert exc.fields == ["query"]
        else:
            raise AssertionError("expected MemPhantValidationError")
    finally:
        server.close()


def test_python_package_artifacts_exist() -> None:
    assert (ROOT / "bindings/python/pyproject.toml").is_file()
    assert (ROOT / "bindings/python/examples/roundtrip.py").is_file()
    openapi = json.loads((ROOT / "openapi/memphant.v1.json").read_text())
    mcp_tools = json.loads((ROOT / "mcp/memphant.tools.v1.json").read_text())
    assert openapi["openapi"] == "3.1.0"
    assert "/v1/recall" in openapi["paths"]
    assert {tool["name"] for tool in mcp_tools} == {
        "retain",
        "recall",
        "reflect",
        "correct",
        "forget",
        "trace",
        "mark",
    }


def test_python_package_is_pure_http_sdk_until_native_api_exists() -> None:
    pyproject = tomllib.loads((ROOT / "bindings/python/pyproject.toml").read_text())

    assert pyproject["build-system"]["build-backend"] != "maturin"
    assert "maturin" not in pyproject
    assert "memphant._native" not in json.dumps(pyproject)


class FakeMemphantServer:
    def __init__(self, error_on_recall: bool = False) -> None:
        self.requests: list[dict[str, object]] = []
        self.error_on_recall = error_on_recall

        parent = self

        class Handler(BaseHTTPRequestHandler):
            def do_GET(self) -> None:  # noqa: N802
                parent._record(self)
                if self.path.startswith("/v1/traces/"):
                    parent._write(self, 200, {"id": self.path.rsplit("/", 1)[-1]})
                else:
                    parent._write(self, 404, {"error": {"code": "not_found", "message": "missing", "request_id": "req_test", "details": {}}})

            def do_POST(self) -> None:  # noqa: N802
                body = parent._record(self)
                if parent.error_on_recall and self.path == "/v1/recall":
                    parent._write(
                        self,
                        422,
                        {
                            "error": {
                                "code": "invalid_request",
                                "message": "query is invalid",
                                "request_id": "req_test",
                                "details": {"fields": ["query"]},
                            }
                        },
                    )
                    return
                responses = {
                    "/v1/episodes": {"episode_id": "ep_test"},
                    "/v1/reflect": {"episodes_consumed": 1},
                    "/v1/recall": {
                        "trace_id": "00000000-0000-0000-0000-000000099001",
                        "items": [{"unit_id": "00000000-0000-0000-0000-000000088001", "body": "Release region is Taipei."}],
                    },
                    "/v1/correct": {"correction_kind": "current"},
                    "/v1/forget": {"verification": "no_recall_path_returns_forgotten"},
                    "/v1/mark": {"accepted": True},
                }
                parent._write(self, 200, responses.get(self.path, {"echo": body}))

            def log_message(self, format: str, *args: object) -> None:
                return

        self._server = HTTPServer(("127.0.0.1", 0), Handler)
        self._thread = threading.Thread(target=self._server.serve_forever, daemon=True)
        self._thread.start()

    @property
    def base_url(self) -> str:
        host, port = self._server.server_address
        return f"http://{host}:{port}"

    def close(self) -> None:
        self._server.shutdown()
        self._thread.join(timeout=5)
        self._server.server_close()

    def _record(self, handler: BaseHTTPRequestHandler) -> object:
        length = int(handler.headers.get("content-length", "0"))
        raw = handler.rfile.read(length) if length else b""
        body = json.loads(raw) if raw else None
        self.requests.append(
            {
                "method": handler.command,
                "path": handler.path,
                "headers": {key.lower(): value for key, value in handler.headers.items()},
                "body": body,
            }
        )
        return body

    def _write(self, handler: BaseHTTPRequestHandler, status: int, body: object) -> None:
        payload = json.dumps(body).encode()
        handler.send_response(status)
        handler.send_header("content-type", "application/json")
        handler.send_header("content-length", str(len(payload)))
        handler.end_headers()
        handler.wfile.write(payload)

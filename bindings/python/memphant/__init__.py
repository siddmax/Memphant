from __future__ import annotations

import json
from dataclasses import dataclass
from typing import Any
from urllib.error import HTTPError
from urllib.request import Request, urlopen

__version__ = "0.2.0"


class MemPhantError(Exception):
    def __init__(
        self,
        code: str,
        message: str,
        *,
        request_id: str | None = None,
        details: dict[str, Any] | None = None,
    ) -> None:
        super().__init__(message)
        self.code = code
        self.request_id = request_id
        self.details = details or {}


class MemPhantAuthError(MemPhantError):
    pass


class MemPhantNotFound(MemPhantError):
    pass


class MemPhantConflict(MemPhantError):
    pass


class MemPhantValidationError(MemPhantError):
    @property
    def fields(self) -> list[str]:
        fields = self.details.get("fields", [])
        return fields if isinstance(fields, list) else []


class MemPhantRateLimited(MemPhantError):
    def __init__(self, *args: Any, retry_after: str | None = None, **kwargs: Any) -> None:
        super().__init__(*args, **kwargs)
        self.retry_after = retry_after


class MemPhantUnavailable(MemPhantError):
    pass


ERROR_MAP = {
    "auth_required": MemPhantAuthError,
    "tenant_denied": MemPhantAuthError,
    "scope_denied": MemPhantAuthError,
    "policy_denied": MemPhantAuthError,
    "not_found": MemPhantNotFound,
    "conflict": MemPhantConflict,
    "idempotency_conflict": MemPhantConflict,
    "invalid_request": MemPhantValidationError,
    "rate_limited": MemPhantRateLimited,
    "backend_unavailable": MemPhantUnavailable,
}


@dataclass(frozen=True)
class MemPhant:
    base_url: str
    api_key: str | None = None
    timeout: float = 30.0

    def retain(
        self,
        *,
        tenant_id: str,
        scope_id: str,
        actor_id: str,
        source_kind: str,
        source_trust: str,
        body: str,
        subject_hint: str | None = None,
        compiler_version: str | None = None,
    ) -> dict[str, Any]:
        return self._post(
            "/v1/episodes",
            {
                "tenant_id": tenant_id,
                "scope_id": scope_id,
                "actor_id": actor_id,
                "source_kind": source_kind,
                "source_trust": source_trust,
                "subject_hint": subject_hint,
                "body": body,
                "compiler_version": compiler_version,
            },
        )

    def retain_resource(
        self,
        *,
        tenant_id: str,
        scope_id: str,
        actor_id: str,
        source_trust: str,
        uri: str,
        mime_type: str,
        content_hash: str,
        kind: str | None = None,
        revision: str | None = None,
        body: str | None = None,
        source_kind: str = "resource",
        compiler_version: str | None = None,
    ) -> dict[str, Any]:
        """Retain a resource payload (spec 08 `resource` shape): documents and
        code carry a URI + content hash; `revision` is the commit identity."""
        return self._post(
            "/v1/episodes",
            {
                "tenant_id": tenant_id,
                "scope_id": scope_id,
                "actor_id": actor_id,
                "source_kind": source_kind,
                "source_trust": source_trust,
                "subject_hint": None,
                "resource": {
                    "uri": uri,
                    "mime_type": mime_type,
                    "content_hash": content_hash,
                    "kind": kind,
                    "revision": revision,
                    "body": body,
                },
                "compiler_version": compiler_version,
            },
        )

    def retain_unit(
        self,
        *,
        tenant_id: str,
        scope_id: str,
        actor_id: str,
        source_trust: str,
        kind: str,
        subject: str,
        predicate: str,
        body: str,
        churn_class: str | None = None,
        valid_from: str | None = None,
        valid_to: str | None = None,
        source_kind: str = "direct",
        compiler_version: str | None = None,
    ) -> dict[str, Any]:
        """Retain a direct pre-compiled unit (spec 08 `unit` shape): requires
        an explicit subject/predicate; the admission trust policy still
        applies (untrusted keys mint candidate tier)."""
        return self._post(
            "/v1/episodes",
            {
                "tenant_id": tenant_id,
                "scope_id": scope_id,
                "actor_id": actor_id,
                "source_kind": source_kind,
                "source_trust": source_trust,
                "subject_hint": None,
                "unit": {
                    "kind": kind,
                    "subject": subject,
                    "predicate": predicate,
                    "body": body,
                    "churn_class": churn_class,
                    "valid_from": valid_from,
                    "valid_to": valid_to,
                },
                "compiler_version": compiler_version,
            },
        )

    def recall(
        self,
        *,
        tenant_id: str,
        scope_id: str,
        actor_id: str,
        query: str,
        limit: int | None = None,
        budget_tokens: int | None = None,
        mode: str | None = None,
        include_beliefs: bool | None = None,
        transaction_as_of: str | None = None,
        valid_at: str | None = None,
        aggregation_window: dict[str, str] | None = None,
    ) -> dict[str, Any]:
        return self._post(
            "/v1/recall",
            {
                "tenant_id": tenant_id,
                "scope_id": scope_id,
                "actor_id": actor_id,
                "query": query,
                "limit": limit,
                "budget_tokens": budget_tokens,
                "mode": mode,
                "include_beliefs": include_beliefs,
                "transaction_as_of": transaction_as_of,
                "valid_at": valid_at,
                "aggregation_window": aggregation_window,
            },
        )

    def reflect(
        self,
        *,
        tenant_id: str,
        scope_id: str,
        actor_id: str,
        compiler_version: str | None = None,
    ) -> dict[str, Any]:
        return self._post(
            "/v1/reflect",
            {
                "tenant_id": tenant_id,
                "scope_id": scope_id,
                "actor_id": actor_id,
                "compiler_version": compiler_version,
            },
        )

    def correct(
        self,
        *,
        tenant_id: str,
        scope_id: str,
        actor_id: str,
        memory_unit_id: str,
        value: str,
        reason: str,
        valid_from: str | None = None,
        valid_to: str | None = None,
    ) -> dict[str, Any]:
        return self._post(
            "/v1/correct",
            {
                "tenant_id": tenant_id,
                "scope_id": scope_id,
                "actor_id": actor_id,
                "selector": {"memory_unit_id": memory_unit_id},
                "correction": {
                    "value": value,
                    "reason": reason,
                    "valid_from": valid_from,
                    "valid_to": valid_to,
                },
            },
        )

    def forget(
        self,
        *,
        tenant_id: str,
        scope_id: str,
        actor_id: str,
        reason: str,
        memory_unit_id: str | None = None,
        selector_scope_id: str | None = None,
    ) -> dict[str, Any]:
        return self._post(
            "/v1/forget",
            {
                "tenant_id": tenant_id,
                "scope_id": scope_id,
                "actor_id": actor_id,
                "selector": {
                    "memory_unit_id": memory_unit_id,
                    "scope_id": selector_scope_id,
                },
                "reason": reason,
            },
        )

    def trace(self, trace_id: str) -> dict[str, Any]:
        return self._get(f"/v1/traces/{trace_id}")

    def mark(
        self,
        *,
        tenant_id: str,
        trace_id: str,
        caller_id: str,
        used_ids: list[str],
        outcome: str,
    ) -> dict[str, Any]:
        return self._post(
            "/v1/mark",
            {
                "tenant_id": tenant_id,
                "trace_id": trace_id,
                "caller_id": caller_id,
                "used_ids": used_ids,
                "outcome": outcome,
            },
        )

    def _get(self, path: str) -> dict[str, Any]:
        return self._request("GET", path, None)

    def _post(self, path: str, body: dict[str, Any]) -> dict[str, Any]:
        return self._request("POST", path, body)

    def _request(
        self, method: str, path: str, body: dict[str, Any] | None
    ) -> dict[str, Any]:
        payload = None if body is None else json.dumps(_strip_none(body)).encode()
        request = Request(
            _join_url(self.base_url, path),
            data=payload,
            method=method,
            headers=self._headers(payload is not None),
        )
        try:
            with urlopen(request, timeout=self.timeout) as response:
                raw = response.read()
        except HTTPError as error:
            raw = error.read()
            retry_after = error.headers.get("retry-after")
            _raise_error(raw, retry_after=retry_after)
        return json.loads(raw.decode()) if raw else {}

    def _headers(self, has_body: bool) -> dict[str, str]:
        headers = {"accept": "application/json"}
        if has_body:
            headers["content-type"] = "application/json"
        if self.api_key:
            headers["authorization"] = f"Bearer {self.api_key}"
        return headers


def _join_url(base_url: str, path: str) -> str:
    return f"{base_url.rstrip('/')}/{path.lstrip('/')}"


def _strip_none(value: Any) -> Any:
    if isinstance(value, dict):
        return {key: _strip_none(item) for key, item in value.items() if item is not None}
    if isinstance(value, list):
        return [_strip_none(item) for item in value]
    return value


def _raise_error(raw: bytes, *, retry_after: str | None = None) -> None:
    try:
        payload = json.loads(raw.decode())
    except json.JSONDecodeError:
        raise MemPhantUnavailable(
            "backend_unavailable", "MemPhant returned a non-JSON error"
        ) from None
    body = payload.get("error", {})
    code = body.get("code", "backend_unavailable")
    message = body.get("message", code)
    request_id = body.get("request_id")
    details = body.get("details") if isinstance(body.get("details"), dict) else {}
    error_type = ERROR_MAP.get(code, MemPhantError)
    if error_type is MemPhantRateLimited:
        raise error_type(
            code, message, request_id=request_id, details=details, retry_after=retry_after
        )
    raise error_type(code, message, request_id=request_id, details=details)


__all__ = [
    "MemPhant",
    "MemPhantAuthError",
    "MemPhantConflict",
    "MemPhantError",
    "MemPhantNotFound",
    "MemPhantRateLimited",
    "MemPhantUnavailable",
    "MemPhantValidationError",
]

from __future__ import annotations

import os

from memphant import MemPhant


client = MemPhant(
    base_url=os.environ.get("MEMPHANT_BASE_URL", "http://127.0.0.1:3000"),
    api_key=os.environ.get("MEMPHANT_API_KEY"),
)

tenant_id = os.environ["MEMPHANT_TENANT_ID"]
scope_id = os.environ["MEMPHANT_SCOPE_ID"]
actor_id = os.environ["MEMPHANT_ACTOR_ID"]

retained = client.retain(
    tenant_id=tenant_id,
    scope_id=scope_id,
    actor_id=actor_id,
    source_kind="user",
    source_trust="trusted_user",
    subject_hint="release region",
    body="Release region is Taipei.",
)
client.reflect(tenant_id=tenant_id, scope_id=scope_id, actor_id=actor_id)
recalled = client.recall(
    tenant_id=tenant_id,
    scope_id=scope_id,
    actor_id=actor_id,
    query="Where is the release region?",
)

print({"retained": retained["episode_id"], "trace_id": recalled["trace_id"]})

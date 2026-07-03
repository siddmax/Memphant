# MemPhant - Auth, Onboarding, and Tiering

## 0. Modes

MemPhant supports:

- local unauthenticated dev
- self-hosted API key auth
- hosted API key auth
- hosted enterprise OIDC/SAML SSO

Core self-host remains API-key based. Hosted enterprise SSO is a hosted control-plane feature and does not enter `memphant-core`.

## 1. Local Onboarding

Goal: developer has working recall in 10 minutes.

```bash
docker compose up
memphant migrate
memphant retain examples/episodes.jsonl
memphant recall "what did the agent learn?"
```

## 2. MCP Onboarding

Goal: agent gets memory through MCP.

```json
{
  "mcpServers": {
    "memphant": {
      "command": "memphant",
      "args": ["mcp", "--db", "$MEMPHANT_DATABASE_URL"]
    }
  }
}
```

Hosted:

```json
{
  "mcpServers": {
    "memphant": {
      "url": "https://mcp.memphant.dev/v1",
      "transport": "streamable-http"
    }
  }
}
```

## 3. Auth Model

Token types:

| Token | Use |
|---|---|
| local dev token | no production use |
| project API key | server-to-server |
| scoped agent key | MCP/agent use |
| admin key | migrations/admin only |

Every key has:

- tenant
- scopes
- allowed memory operations
- rate limit
- created by
- last used
- revoked at

## 3.1 Key Scopes

```text
memory:retain
memory:recall
memory:reflect
memory:correct
memory:forget
trace:read
eval:run
admin:migrate
admin:keys
```

MCP agent keys should default to `memory:retain`, `memory:recall`, and `trace:read` for their assigned scope only.
`memory:correct` is opt-in for explicit correction flows; it is not bundled into default agent keys.

### 3.2 Agent / MCP Key Provisioning (DCR)

Hosted MCP supports **RFC 7591 dynamic client registration** so an agent runtime can self-provision a scoped key without a human dashboard step: the runtime registers, receives a key minted with the default agent scopes above (never `admin:*`, never `memory:forget`), bound to one tenant + scope. Keys are short-TTL-rotatable; `admin:*` is never issuable via DCR.

## 4. Tier Ladder

| Capability | Open source | Free | Pro | Team | Enterprise |
|---|---|---|---|---|---|
| retain / recall / correct / forget | ✓ | ✓ | ✓ | ✓ | ✓ |
| poisoning controls + tenant isolation | ✓ | ✓ | ✓ | ✓ | ✓ |
| rate limit (req/min) | self | low | high | high | custom |
| hosted trace retention | self | short | long | long | custom |
| shared projects / RBAC | — | — | — | ✓ | ✓ |
| private evals / scorecards | self | — | ✓ | ✓ | ✓ |
| BYOC / VPC | self | — | — | — | ✓ |
| SSO (OIDC/SAML) | — | — | — | — | ✓ |
| storage + recall quota (`21` §1a/§3a) | self | small | high | high | custom |
| in-region residency / EU cell (`25` §7b) | self | — | — | — | ✓ |
| crypto-shred erasure SLA + DPA (`06` §6.2, `17`) | — | — | — | — | ✓ |
| data export (§5) | ✓ | ✓ | ✓ | ✓ | ✓ |

**Tier buys throughput, retention, collaboration, and deployment — never memory correctness, poisoning controls, or ranking of recall.** A paid tier cannot make recall more accurate or relax a trust gate; those are identical across tiers. Quotas map to the `21` §1a metered units (overage/degrade/cap per `21` §3a); a tenant carries `billing_status ∈ {active, past_due, suspended}`, distinct from the §3 security `revoked_at` — **suspend revokes premium features but never deletes data or blocks the always-free export (§5)** (the anti-lock-in guarantee survives non-payment).

## 5. Vendor Lock-In Rule

Hosted MemPhant must export:

- episodes
- memory units
- resources
- citations
- traces
- deletion logs

Open core credibility dies if export is fake.

## 6. Onboarding Success Events

Track:

- local server started
- migration completed
- first retain
- first recall
- first trace viewed
- first eval run
- MCP connected
- SDK quickstart completed

Docs should optimize for time-to-first-cited-recall, not sign-up conversion alone.

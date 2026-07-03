# Hosted Control-Plane Hooks

Hosted MemPhant is N single-region cells plus a thin control plane. A cell is the same open-core runtime as self-host: `memphant-server`, `memphant-worker`, Postgres, and object storage in one region.

The closed control plane owns only routing and commerce state:

- Tenant provisioning.
- Tenant-to-region directory lookup.
- Billing metered events.
- Tenant status changes.

The contract in `deploy/hosted/control-plane-hooks.json` forbids memory bodies, raw episodes, resource URIs, quotes, and embedding vectors from crossing the global plane. Region routing returns a home region only. A misrouted request is replayed to the home cell or rejected; it is never served cross-region.

Multi-region hosting remains outside the open-core binary. Self-host and BYOC deployments are a single cell and do not need the directory or router.


# Self-Host Deployment

MemPhant self-hosting is one regional cell: `memphant-server`, `memphant-worker`, Postgres with pgvector, and a customer-owned object store. The Docker path is for local validation and small deployments; the provider profile is the production gate.

## Local Compose

```bash
docker compose up --build --wait
curl -fsS http://127.0.0.1:3000/v1/health
cargo run -p memphant-cli -- db bootstrap-check --provider plain-postgres
```

The Compose stack uses `pgvector/pgvector:0.8.4-pg17`, waits for Postgres with `service_healthy`, and binds both Postgres and HTTP to localhost. It does not expose a browser-facing database role.

## Production Profile

Copy `deploy/provider-profiles/plain-postgres.env.example`, replace the placeholders, then run:

```bash
cargo run -p memphant-cli -- db bootstrap-check --provider plain-postgres --profile /path/to/plain-postgres.env
```

The check must pass before `memphant db bootstrap` is allowed in a maintenance window. It verifies the bundled migration boundary, the `memphant` schema name, a Postgres URL, region alignment between Postgres and object store, object-store versioning, and an object retention window at least one day longer than the Postgres PITR window.

## Required Postgres Shape

- Postgres 17 or 18.
- `vector` pgvector 0.8.x, `pg_trgm`, `ltree`, and `btree_gist`.
- A migrator role able to create objects in `memphant`.
- No references to `public`, `syndai`, provider auth schemas, or browser roles from MemPhant migrations.
- RLS and tenant-prefixed indexes on tenant-scoped tables.


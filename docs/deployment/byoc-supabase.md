# Supabase BYOC

Supabase is a supported Postgres provider, not a hosted compute layer for MemPhant. The MemPhant API and workers still run as the same Rust binaries used by self-hosters.

## Preflight

```bash
cargo run -p memphant-cli -- db bootstrap-check --provider supabase --profile /path/to/supabase.env
supabase db lint --db-url "$DATABASE_URL" --schema memphant --fail-on warning
```

The MemPhant preflight fails closed when:

- `MEMPHANT_SCHEMA` is not `memphant`.
- `MEMPHANT_SUPABASE_EXPOSED_SCHEMAS` contains `memphant`.
- `anon` or `authenticated` is marked as having direct MemPhant access.
- The Supabase lint command does not include `--schema memphant` and `--fail-on warning`.
- Postgres and object-store regions differ.
- Object-store retention is shorter than the Postgres PITR window plus margin.

## Boundary

MemPhant installs only:

- `memphant` schema.
- Required extensions when grantable.
- `memphant_*` NOLOGIN roles or provider-compatible grants.
- Tenant tables, RLS policies, indexes, and `memphant.schema_migrations`.

It never installs into `public`, depends on Supabase auth tables, grants browser roles table access, or stores customer object-store credentials in generated SDKs.


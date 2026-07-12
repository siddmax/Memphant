#!/usr/bin/env bash
# Run a command against a freshly-minted, migrated scratch Postgres database,
# then drop it — even on failure. Isolates job_state/tenant debris from the
# shared campaign DB (`memphant`) so a global oldest-first worker claim can
# never be starved by another process's foreign rows: an ephemeral DB has no
# foreign rows. This is the fix for the recurring "job_state debris starves a
# worker tick" incident (contract tests + killed benches vs. the e2e probe).
#
# Usage:
#   bash scripts/with_scratch_db.sh <base_database_url> <ENV_VAR> <cmd> [args...]
#
# The command runs with <ENV_VAR> set to the scratch DB's URL. Examples:
#   bash scripts/with_scratch_db.sh postgres://memphant:memphant@localhost:5432/memphant \
#     DATABASE_URL bash scripts/e2e_probe.sh
#   bash scripts/with_scratch_db.sh postgres://memphant:memphant@localhost:5432/memphant \
#     MEMPHANT_TEST_DATABASE_URL cargo test -p memphant-store-postgres -- --ignored
#
# ponytail: base_database_url must be a plain postgres://user:pass@host:port/db
# URL with no query string (?sslmode=...); the campaign/local URLs are plain.
# Add query-string handling only if a provider URL ever needs it here.
set -euo pipefail

BASE_URL="${1:?base database url required}"
ENV_VAR="${2:?target env var name required}"
shift 2
[ "$#" -gt 0 ] || { echo "with_scratch_db.sh: no command given" >&2; exit 2; }

ROOT="$(cd "$(dirname "$0")/.." && pwd)"

# Maintenance URL = same server, `postgres` database. Scratch URL = same
# server, unique DB name (pid+epoch keeps concurrent gate runs from colliding).
PREFIX="${BASE_URL%/*}"
NAME="memphant_scratch_$$_$(date +%s)"
ADMIN_URL="$PREFIX/postgres"
SCRATCH_URL="$PREFIX/$NAME"

drop_scratch() {
  psql "$ADMIN_URL" -v ON_ERROR_STOP=1 -q \
    -c "drop database if exists \"$NAME\" with (force)" >/dev/null 2>&1 || true
}
trap drop_scratch EXIT

psql "$ADMIN_URL" -v ON_ERROR_STOP=1 -q -c "create database \"$NAME\"" >/dev/null
# client-min-messages=warning silences the migrations' idempotent
# `drop ... if exists` NOTICEs (~45 lines of noise on a fresh DB); real
# warnings/errors still surface.
PGOPTIONS='-c client-min-messages=warning' \
  python3 "$ROOT/scripts/apply_memphant_migrations.py" --database-url "$SCRATCH_URL" >/dev/null

export "$ENV_VAR=$SCRATCH_URL"
"$@"

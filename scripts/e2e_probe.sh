#!/usr/bin/env bash
# End-to-end durability/auth/tri-domain probe (plan Task 12).
#
# Proves, against a real Postgres and the real binaries:
#   retain -> worker compiles -> recall -> restart -> recall persists,
#   cross-tenant trace denial, correct, forget (no resurrection), mark,
#   resource (code) ingest with revision identity, health reporting.
#
# Usage: DATABASE_URL=postgres://memphant:memphant@localhost:5432/memphant \
#          bash scripts/e2e_probe.sh
# Exits non-zero on the first failed assertion, printing the transcript.
#
# DATABASE_URL is the *base* campaign server; the probe runs against an
# ephemeral scratch database minted from it (created, migrated, and dropped
# here), NEVER the shared `memphant` DB directly. That isolation is what makes
# the probe immune to foreign job_state debris: the worker's global claim is
# oldest-first across all tenants, so debris from the contract tests or a
# killed bench in a shared DB would starve the probe's fresh job on a single
# tick. An ephemeral DB has no foreign rows, so it cannot be starved.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DATABASE_URL="${DATABASE_URL:-postgres://memphant:memphant@localhost:5432/memphant}"

# Re-exec once through the scratch-DB helper (which points DATABASE_URL at a
# fresh migrated DB and drops it on exit). MEMPHANT_SCRATCH_ACTIVE guards the
# recursion; set it to run the probe against DATABASE_URL as-is (e.g. an
# already-isolated DB).
if [ -z "${MEMPHANT_SCRATCH_ACTIVE:-}" ]; then
  exec env MEMPHANT_SCRATCH_ACTIVE=1 \
    bash "$ROOT/scripts/with_scratch_db.sh" "$DATABASE_URL" DATABASE_URL \
    bash "$ROOT/scripts/$(basename "$0")"
fi
PORT="${MEMPHANT_PROBE_PORT:-39411}"
BASE="http://127.0.0.1:${PORT}"
SERVER="$ROOT/target/debug/memphant-server"
WORKER="$ROOT/target/debug/memphant-worker"
CLI="$ROOT/target/debug/memphant-cli"
SCOPE="7c000000-0000-4000-8000-000000000001"
ACTOR="7c000000-0000-4000-8000-000000000002"
SERVER_PID=""

log()  { printf '\n### %s\n' "$*"; }
fail() { printf 'PROBE FAILED: %s\n' "$*" >&2; exit 1; }
cleanup() { [ -n "$SERVER_PID" ] && kill "$SERVER_PID" 2>/dev/null || true; }
trap cleanup EXIT

jget() { python3 -c "import json,sys;d=json.load(sys.stdin);print(d$1)"; }

start_server() {
  DATABASE_URL="$DATABASE_URL" MEMPHANT_BIND="127.0.0.1:${PORT}" "$SERVER" &
  SERVER_PID=$!
  for _ in $(seq 1 40); do
    curl -sf "$BASE/v1/health" >/dev/null 2>&1 && return 0
    sleep 0.25
  done
  fail "server did not become healthy on :$PORT"
}

worker_once() { DATABASE_URL="$DATABASE_URL" MEMPHANT_WORKER_ONCE=1 "$WORKER" >/dev/null; }

api() { # api KEY METHOD PATH [JSON]
  local key="$1" method="$2" path="$3" body="${4:-}"
  if [ -n "$body" ]; then
    curl -s -X "$method" -H "Authorization: Bearer $key" -H 'content-type: application/json' \
      -d "$body" "$BASE$path"
  else
    curl -s -X "$method" -H "Authorization: Bearer $key" "$BASE$path"
  fi
}
api_status() { # like api, but prints only the HTTP status
  local key="$1" method="$2" path="$3"
  curl -s -o /dev/null -w '%{http_code}' -X "$method" -H "Authorization: Bearer $key" "$BASE$path"
}

log "build binaries (debug)"
(cd "$ROOT" && cargo build -q -p memphant-server -p memphant-worker -p memphant-cli)

log "apply migrations (idempotent)"
python3 "$ROOT/scripts/apply_memphant_migrations.py" --database-url "$DATABASE_URL" | tail -1

log "provision tenants + keys via admin CLI"
TENANT_A=$("$CLI" admin create-tenant --name "probe-a-$RANDOM" --database-url "$DATABASE_URL" | sed -n 's/^tenant_created id=\([^ ]*\).*/\1/p')
TENANT_B=$("$CLI" admin create-tenant --name "probe-b-$RANDOM" --database-url "$DATABASE_URL" | sed -n 's/^tenant_created id=\([^ ]*\).*/\1/p')
KEY_A=$("$CLI" admin create-key --tenant "$TENANT_A" --database-url "$DATABASE_URL" | tail -1)
KEY_B=$("$CLI" admin create-key --tenant "$TENANT_B" --database-url "$DATABASE_URL" | tail -1)
[ -n "$TENANT_A" ] && [ -n "$KEY_A" ] && [ -n "$KEY_B" ] || fail "provisioning failed"
echo "tenant_a=$TENANT_A tenant_b=$TENANT_B"

start_server
log "health reports postgres"
api "$KEY_A" GET /v1/health | tee /dev/stderr | grep -q '"store":"postgres"' || fail "health lacks store=postgres"

log "retain episode (A, explicit subject)"
RETAIN=$(api "$KEY_A" POST /v1/episodes "{\"tenant_id\":\"$TENANT_A\",\"scope_id\":\"$SCOPE\",\"actor_id\":\"$ACTOR\",\"source_kind\":\"user\",\"source_trust\":\"trusted_user\",\"subject\":\"release region\",\"predicate\":\"value\",\"body\":\"Release region is Taipei.\"}")
EPISODE_ID=$(echo "$RETAIN" | jget "['episode_id']")
[ -n "$EPISODE_ID" ] || fail "retain returned no episode_id: $RETAIN"

log "read-your-own-writes: recall before worker runs -> degraded hit"
RECALL0=$(api "$KEY_A" POST /v1/recall "{\"tenant_id\":\"$TENANT_A\",\"scope_id\":\"$SCOPE\",\"actor_id\":\"$ACTOR\",\"query\":\"Where is the release region?\"}")
echo "$RECALL0" | jget "['degraded']" | grep -qi true || fail "expected degraded read-your-own-writes: $RECALL0"

log "worker tick compiles"
worker_once
RECALL1=$(api "$KEY_A" POST /v1/recall "{\"tenant_id\":\"$TENANT_A\",\"scope_id\":\"$SCOPE\",\"actor_id\":\"$ACTOR\",\"query\":\"Where is the release region?\"}")
echo "$RECALL1" | jget "['items'][0]['body']" | grep -q "Taipei" || fail "recall missed compiled unit: $RECALL1"
echo "$RECALL1" | jget "['degraded']" | grep -qi false || fail "recall still degraded after compile"
TRACE_ID=$(echo "$RECALL1" | jget "['trace_id']")
UNIT_ID=$(echo "$RECALL1" | jget "['items'][0]['unit_id']")

log "retain code resource (A) with commit revision"
RES=$(api "$KEY_A" POST /v1/episodes "{\"tenant_id\":\"$TENANT_A\",\"scope_id\":\"$SCOPE\",\"actor_id\":\"$ACTOR\",\"source_kind\":\"repo\",\"source_trust\":\"trusted_system\",\"resource\":{\"uri\":\"repo://demo/src/main.rs\",\"mime_type\":\"text/x-rust\",\"content_hash\":\"sha256:probe\",\"kind\":\"code\",\"revision\":\"abc123def\",\"body\":\"fn deploy() { /* canary first, then roll forward */ }\"}}")
echo "$RES" | jget "['enqueued'][0]" | grep -q reflect_resource || fail "resource retain not enqueued: $RES"
worker_once
RECALL_RES=$(api "$KEY_A" POST /v1/recall "{\"tenant_id\":\"$TENANT_A\",\"scope_id\":\"$SCOPE\",\"actor_id\":\"$ACTOR\",\"query\":\"canary deploy roll forward\"}")
echo "$RECALL_RES" | python3 -c "import json,sys;d=json.load(sys.stdin);assert any(i['kind']=='resource' for i in d['items']),d" || fail "resource-derived unit not recalled"

log "cross-tenant: B fetching A's trace must 404"
STATUS_B=$(api_status "$KEY_B" GET "/v1/traces/$TRACE_ID")
[ "$STATUS_B" = "404" ] || fail "tenant B got $STATUS_B for tenant A's trace (must be 404)"
STATUS_A=$(api_status "$KEY_A" GET "/v1/traces/$TRACE_ID")
[ "$STATUS_A" = "200" ] || fail "tenant A cannot read own trace ($STATUS_A)"

log "restart durability"
kill "$SERVER_PID"; wait "$SERVER_PID" 2>/dev/null || true; SERVER_PID=""
start_server
RECALL2=$(api "$KEY_A" POST /v1/recall "{\"tenant_id\":\"$TENANT_A\",\"scope_id\":\"$SCOPE\",\"actor_id\":\"$ACTOR\",\"query\":\"Where is the release region?\"}")
echo "$RECALL2" | jget "['items'][0]['body']" | grep -q "Taipei" || fail "memory lost across restart: $RECALL2"
[ "$(api_status "$KEY_A" GET "/v1/traces/$TRACE_ID")" = "200" ] || fail "trace lost across restart"

log "correct supersedes"
api "$KEY_A" POST /v1/correct "{\"tenant_id\":\"$TENANT_A\",\"scope_id\":\"$SCOPE\",\"actor_id\":\"$ACTOR\",\"selector\":{\"memory_unit_id\":\"$UNIT_ID\"},\"correction\":{\"value\":\"Release region is Osaka.\",\"reason\":\"probe correction\"}}" >/dev/null
RECALL3=$(api "$KEY_A" POST /v1/recall "{\"tenant_id\":\"$TENANT_A\",\"scope_id\":\"$SCOPE\",\"actor_id\":\"$ACTOR\",\"query\":\"Where is the release region?\"}")
echo "$RECALL3" | jget "['items'][0]['body']" | grep -q "Osaka" || fail "correction not reflected: $RECALL3"

log "forget episode + no resurrection"
FORGET=$(api "$KEY_A" POST /v1/forget "{\"tenant_id\":\"$TENANT_A\",\"scope_id\":\"$SCOPE\",\"actor_id\":\"$ACTOR\",\"selector\":{\"episode_id\":\"$EPISODE_ID\",\"scope_id\":\"$SCOPE\"},\"reason\":\"probe forget\"}")
echo "$FORGET" | jget "['verification']" | grep -q "probe_hits=0" || fail "forget verification not clean: $FORGET"
api "$KEY_A" POST /v1/reflect "{\"tenant_id\":\"$TENANT_A\",\"scope_id\":\"$SCOPE\",\"actor_id\":\"$ACTOR\"}" >/dev/null
worker_once
RECALL4=$(api "$KEY_A" POST /v1/recall "{\"tenant_id\":\"$TENANT_A\",\"scope_id\":\"$SCOPE\",\"actor_id\":\"$ACTOR\",\"query\":\"release region Taipei Osaka\"}")
echo "$RECALL4" | python3 -c "import json,sys;d=json.load(sys.stdin);assert not any('egion is' in i['body'] for i in d['items']),d" || fail "forgotten memory resurfaced: $RECALL4"

log "mark outcome feedback"
MARK=$(api "$KEY_A" POST /v1/mark "{\"tenant_id\":\"$TENANT_A\",\"trace_id\":\"$TRACE_ID\",\"caller_id\":\"e2e-probe\",\"used_ids\":[],\"outcome\":\"success\"}")
echo "$MARK" | jget "['accepted']" | grep -qi true || fail "mark rejected: $MARK"

log "unauthenticated request is refused"
STATUS_NOKEY=$(curl -s -o /dev/null -w '%{http_code}' -X POST -H 'content-type: application/json' -d '{}' "$BASE/v1/recall")
[ "$STATUS_NOKEY" = "401" ] || fail "missing key got $STATUS_NOKEY (must be 401)"

echo
echo "E2E PROBE: ALL CHECKS PASSED"

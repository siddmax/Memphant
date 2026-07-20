#!/usr/bin/env bash
set -euo pipefail

if test "$#" -lt 6 || test "$#" -gt 7
then
  echo "usage: $0 ARM TEST_JSONL OUT_ROOT ROWS LANES DB_TAG [START_OFFSET]" >&2
  exit 2
fi

REPO=/Users/sidsharma/Memphant
OFFICIAL="$REPO/docs/build-log/artifacts/unified-sota-20260714/memsyco-evidence-sota-20260715T172416Z/personalized-use/future-v2/shadow-v1/upstream/official"
ARM="$1"
TEST="$2"
ROOT_OUT="$3"
ROWS="$4"
LANES="$5"
DB_TAG="$6"
START_OFFSET="${7:-0}"
MODEL=deepseek/deepseek-v4-flash
ADMIN_URL=postgres://memphant:memphant@127.0.0.1:5432/postgres
DB_PREFIX=postgres://memphant:memphant@127.0.0.1:5432
CARGO_TARGET_DIR_VALUE="${CARGO_TARGET_DIR:-$REPO/target}"
CARGO_BUILD_JOBS_VALUE="${CARGO_BUILD_JOBS:-1}"
CARGO_INCREMENTAL_VALUE="${CARGO_INCREMENTAL:-0}"
PMU_ROW_POLICY="${PMU_ROW_POLICY:-strict}"

case "$ARM" in
  memphant|episode_only|raw_dialogue) ;;
  *) echo "unsupported arm: $ARM" >&2; exit 2 ;;
esac
case "$PMU_ROW_POLICY" in
  strict|aggregate) ;;
  *) echo "PMU_ROW_POLICY must be strict or aggregate" >&2; exit 2 ;;
esac
if ! [[ "$ROWS" =~ ^[1-9][0-9]*$ && "$LANES" =~ ^[1-9][0-9]*$ && "$START_OFFSET" =~ ^[0-9]+$ ]]
then
  echo "ROWS and LANES must be positive integers and START_OFFSET a nonnegative integer" >&2
  exit 2
fi
if test "$START_OFFSET" -ge "$ROWS"
then
  echo "START_OFFSET must be smaller than ROWS" >&2
  exit 2
fi
if test "$LANES" -gt 4
then
  echo "LANES exceeds the measured safe local maximum of 4" >&2
  exit 2
fi
if ! [[ "$DB_TAG" =~ ^[a-z0-9_]+$ ]]
then
  echo "DB_TAG must contain only lowercase letters, digits, and underscores" >&2
  exit 2
fi

memory_arm=false
if test "$ARM" != raw_dialogue
then
  memory_arm=true
fi

prepare_lane() {
  local lane="$1" name="memphant_${DB_TAG}_lane_$1" exists pending
  exists=$(psql "$ADMIN_URL" -Atc "select count(*) from pg_database where datname = '$name'")
  if test "$exists" = 1
  then
    pending=$(psql "$DB_PREFIX/$name" -Atc \
      "select count(*) from memphant.job_state where state in ('queued','running')" \
      2>/dev/null || echo invalid)
    if test "$pending" = 0
    then
      return
    fi
    dropdb --force --maintenance-db="$ADMIN_URL" "$name"
  fi
  createdb --maintenance-db="$ADMIN_URL" "$name"
  PGOPTIONS='-c client-min-messages=warning' \
    python3 "$REPO/scripts/apply_memphant_migrations.py" \
      --database-url "$DB_PREFIX/$name" >/dev/null
}

verify_row() {
  local directory="$1"
  doppler run --project syndai --config dev -- \
    uv run --python 3.10 --with-requirements "$OFFICIAL/requirements.txt" \
    python3 "$REPO/scripts/run_restraint_bench.py" verify-results \
      --run-dir "$directory"
  if test "$ARM" != episode_only && test "$PMU_ROW_POLICY" = strict
  then
    jq -e '
      .n_samples == 1 and
      .n_api_failed_samples == 0 and
      (.metrics.with_memory as $metrics |
        $metrics.n_judged == 1 and
        $metrics.answer_accuracy_sum == 1 and
        $metrics.preference_used_sum == 1 and
        $metrics.memory_use_pass_sum == 1 and
        $metrics.judge_parse_failed == 0 and
        $metrics.judge_error_count == 0)
    ' "$directory/report.json" >/dev/null
  fi
}

run_row() {
  local lane="$1" offset="$2" directory out port database_url
  for directory in "$ROOT_OUT/offset-$offset"-attempt-*
  do
    if test -f "$directory/report.json"
    then
      verify_row "$directory" >/dev/null
      return
    fi
    if test -e "$directory"
    then
      echo "offset $offset has an incomplete attempt; classify it before recovery" >&2
      return 1
    fi
  done

  out="$ROOT_OUT/offset-$offset-attempt-1"
  port=$((43000 + offset))
  if $memory_arm
  then
    database_url="$DB_PREFIX/memphant_${DB_TAG}_lane_$lane"
  else
    database_url="$DB_PREFIX/postgres"
  fi

  if test "$ARM" = memphant
  then
    doppler run --project syndai --config dev -- \
      env \
        DATABASE_URL="$database_url" \
        MEMPHANT_SCRATCH_ACTIVE=1 \
        MEMPHANT_STRUCTURED_STATE=on \
        MEMPHANT_STRUCTURED_STATE_MODEL="$MODEL" \
        MEMPHANT_STRUCTURED_STATE_PROMPT_PATH="$REPO/config/structured-state-v1.txt" \
        CARGO_TARGET_DIR="$CARGO_TARGET_DIR_VALUE" \
        CARGO_BUILD_JOBS="$CARGO_BUILD_JOBS_VALUE" \
        CARGO_INCREMENTAL="$CARGO_INCREMENTAL_VALUE" \
        uv run --python 3.10 --with-requirements "$OFFICIAL/requirements.txt" \
        python3 "$REPO/scripts/run_restraint_bench.py" run \
          --official-dir "$OFFICIAL" --out-dir "$out" \
          --task personalized_memory_use --arm "$ARM" --test-jsonl "$TEST" \
          --offset "$offset" --limit 1 --embed-model fastembed:bge-m3 \
          --model "$MODEL" --base-url https://openrouter.ai/api/v1 \
          --judge-model "$MODEL" --judge-base-url https://openrouter.ai/api/v1 \
          --database-url "$database_url" --port "$port"
  else
    doppler run --project syndai --config dev -- \
      env DATABASE_URL="$database_url" MEMPHANT_SCRATCH_ACTIVE=1 \
        MEMPHANT_STRUCTURED_STATE=off \
        CARGO_TARGET_DIR="$CARGO_TARGET_DIR_VALUE" \
        CARGO_BUILD_JOBS="$CARGO_BUILD_JOBS_VALUE" \
        CARGO_INCREMENTAL="$CARGO_INCREMENTAL_VALUE" \
        uv run --python 3.10 --with-requirements "$OFFICIAL/requirements.txt" \
        python3 "$REPO/scripts/run_restraint_bench.py" run \
          --official-dir "$OFFICIAL" --out-dir "$out" \
          --task personalized_memory_use --arm "$ARM" --test-jsonl "$TEST" \
          --offset "$offset" --limit 1 --embed-model fastembed:bge-m3 \
          --model "$MODEL" --base-url https://openrouter.ai/api/v1 \
          --judge-model "$MODEL" --judge-base-url https://openrouter.ai/api/v1 \
          --database-url "$database_url" --port "$port"
  fi
  verify_row "$out" >/dev/null
}

run_lane() {
  local lane="$1" offset
  for ((offset=START_OFFSET+lane; offset<ROWS; offset+=LANES))
  do
    run_row "$lane" "$offset"
  done
}

declare -a cleanup_pids=()
collect_process_tree() {
  local parent="$1" child
  while IFS= read -r child
  do
    collect_process_tree "$child"
  done < <(pgrep -P "$parent" 2>/dev/null || true)
  cleanup_pids+=("$parent")
}

terminate_process_tree() {
  local parent="$1" pid attempt live
  cleanup_pids=()
  collect_process_tree "$parent"
  for pid in "${cleanup_pids[@]}"
  do
    kill -TERM "$pid" 2>/dev/null || true
  done
  for attempt in {1..10}
  do
    live=false
    for pid in "${cleanup_pids[@]}"
    do
      if kill -0 "$pid" 2>/dev/null
      then
        live=true
        break
      fi
    done
    if ! $live
    then
      return
    fi
    sleep 0.1
  done
  for pid in "${cleanup_pids[@]}"
  do
    kill -KILL "$pid" 2>/dev/null || true
  done
}

declare -a pids=()
cleanup() {
  local exit_status="$?" pid
  trap - EXIT INT TERM
  for pid in "${pids[@]}"
  do
    terminate_process_tree "$pid"
  done
  for pid in "${pids[@]}"
  do
    wait "$pid" 2>/dev/null || true
  done
  exit "$exit_status"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

mkdir -p "$ROOT_OUT"
if $memory_arm
then
  for ((lane=0; lane<LANES; lane++))
  do
    prepare_lane "$lane"
  done
fi

for ((lane=0; lane<LANES; lane++))
do
  run_lane "$lane" &
  pids+=("$!")
done
status=0
for pid in "${pids[@]}"
do
  wait "$pid" || status=1
done
exit "$status"

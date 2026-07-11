#!/usr/bin/env python3
"""Coding-lane corpus extraction for the R0 embedder bakeoff (R0-T6).

Source: the Syndai dev Postgres table ``coding_execution_attempt_events``
(read-only — this script never writes to that database). Content lives in
event payloads: ``message_end`` -> ``.message.content[].text`` (role from
``.message.role``: assistant/user/toolResult; content-less message_end rows
— usage-metadata-only turns, e.g. a tool-use-only assistant turn — are
skipped) and ``tool_execution_end`` -> ``.result.content[].text`` (always
attributed role ``toolResult``; this DB event carries no role of its own).

Pipeline per attempt (all pure functions below, unit-tested in
``tests/test_code_lane_extract.py`` over fixture rows — no DB needed for
those tests):
1. **Event-gap exclusion**: an attempt whose event count doesn't equal
   ``max(sequence) + 1`` has holes in its recorded timeline (a replay/backfill
   artifact) and is excluded outright — ``has_event_gap``.
2. **Content extraction**: per surviving attempt, pull the real-text events
   (``build_content_events``), truncating any single event's text at
   ``--truncate-chars`` (default 4000; the truncation flag is recorded per
   event, never silently dropped).
3. **Eligibility**: an attempt qualifies for the sample only with at least
   ``--min-events`` content events AND at least ``--min-chars`` total
   characters (``is_eligible``).
4. **Deterministic sample**: a seeded shuffle (default seed 20260713) over
   the sorted eligible attempt ids, greedily accumulated until the running
   content-event total reaches ``--haystack-min`` (``sample_attempts``) — the
   corpus this run's golden questions get mined from and the runner ingests.

Output: ``benchmarks/data/coding_events_corpus.jsonl`` — one row per sampled
attempt: ``{attempt_id, run_id, started_at, events: [{sequence, role, text,
event_id, truncated}, ...]}``. This is the user's private coding content, so
it is gitignored (never committed) — see the corpus-privacy note in
``.gitignore``. A stats sidecar (``coding_events_corpus.stats.json``, also
gitignored — an intermediate handoff, not a committed artifact) carries the
extraction counts (no content) that ``code_lane_mine.py`` folds into the one
committed lock file, ``coding_events_golden.lock.json``.
"""

from __future__ import annotations

import argparse
import csv
import json
import random
import subprocess
import sys
import tempfile
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import gate_common as gc  # noqa: E402

DEFAULT_DATABASE_URL = "postgresql://syndai:syndai@127.0.0.1:55432/syndai_local"
ATTEMPT_TABLE = "coding_execution_attempts"
EVENT_TABLE = "coding_execution_attempt_events"
CONTENT_EVENT_TYPES = ("message_end", "tool_execution_end")

TRUNCATE_CHARS = 4000
MIN_CONTENT_EVENTS = 6
MIN_TOTAL_CHARS = 2000
SAMPLE_SEED = 20260713
HAYSTACK_MIN = 600
HAYSTACK_MAX = 1200

CORPUS_PATH = gc.MEMPHANT_ROOT / "benchmarks" / "data" / "coding_events_corpus.jsonl"
STATS_PATH = gc.MEMPHANT_ROOT / "benchmarks" / "data" / "coding_events_corpus.stats.json"


# --- pure functions (TDD'd in tests/test_code_lane_extract.py) --------------


def extract_event_text(event_type: str, payload: dict) -> tuple[str | None, str | None]:
    """(role, text) for one raw DB event's payload, or ``(None, None)`` when
    it carries no real text content. Multiple ``content[].text`` blocks on
    one event are joined with a blank line (rare; keeps one output row per
    source DB event, matching the corpus schema's one-event-per-entry
    contract)."""
    if event_type == "message_end":
        message = payload.get("message") or {}
        role = message.get("role")
        texts = [
            item.get("text")
            for item in (message.get("content") or [])
            if isinstance(item, dict) and isinstance(item.get("text"), str) and item["text"].strip()
        ]
        if not role or not texts:
            return None, None
        return role, "\n\n".join(texts)
    if event_type == "tool_execution_end":
        result = payload.get("result") or {}
        texts = [
            item.get("text")
            for item in (result.get("content") or [])
            if isinstance(item, dict) and isinstance(item.get("text"), str) and item["text"].strip()
        ]
        if not texts:
            return None, None
        return "toolResult", "\n\n".join(texts)
    return None, None


def truncate_text(text: str, limit: int) -> tuple[str, bool]:
    if len(text) <= limit:
        return text, False
    return text[:limit], True


def build_content_events(raw_events: list[dict], truncate_chars: int) -> list[dict]:
    """``raw_events``: ``[{sequence, event_type, payload, event_id}, ...]``
    for ONE attempt (any order, already restricted to
    ``CONTENT_EVENT_TYPES``). Returns content events ordered by sequence:
    ``{sequence, role, text, event_id, truncated}``; rows with no real text
    are dropped."""
    out = []
    for row in raw_events:
        role, text = extract_event_text(row["event_type"], row["payload"])
        if role is None or text is None:
            continue
        truncated_text, was_truncated = truncate_text(text, truncate_chars)
        out.append(
            {
                "sequence": row["sequence"],
                "role": role,
                "text": truncated_text,
                "event_id": row["event_id"],
                "truncated": was_truncated,
            }
        )
    out.sort(key=lambda e: e["sequence"])
    return out


def has_event_gap(n_events_total: int, max_sequence: int) -> bool:
    """R0-T6 exclusion rule: ``count(events) != max(sequence) + 1``."""
    return n_events_total != max_sequence + 1


def is_eligible(content_events: list[dict], min_events: int, min_chars: int) -> bool:
    total_chars = sum(len(e["text"]) for e in content_events)
    return len(content_events) >= min_events and total_chars >= min_chars


def sample_attempts(
    eligible_counts: list[tuple[str, int]],
    seed: int,
    haystack_min: int,
    haystack_max: int,
) -> tuple[list[str], int]:
    """Deterministic seeded sample of attempt ids: sort eligible attempt ids
    for a stable base ordering, seeded-shuffle, then greedily accumulate (in
    shuffle order) until the running content-event total reaches
    ``haystack_min``. Returns ``(chosen_attempt_ids, cumulative_event_count)``.
    Never splits an attempt, so the final total can land above
    ``haystack_min`` — the caller warns if it overshoots ``haystack_max``.
    Exhausts the whole pool (and returns whatever total that yields) if the
    pool can't reach ``haystack_min``."""
    ordered_ids = sorted(aid for aid, _ in eligible_counts)
    rng = random.Random(seed)
    shuffled = ordered_ids[:]
    rng.shuffle(shuffled)
    counts = dict(eligible_counts)
    chosen: list[str] = []
    cumulative = 0
    for attempt_id in shuffled:
        chosen.append(attempt_id)
        cumulative += counts[attempt_id]
        if cumulative >= haystack_min:
            break
    return chosen, cumulative


# --- DB I/O (psql \copy, CSV format so embedded newlines in code/text stay --
# --- inside their quoted field instead of breaking one-row-per-line parsing) -


def fetch_via_copy(psql_bin: str, database_url: str, select_sql: str) -> list[dict]:
    """Runs ``select_sql`` (must project a single ``row_to_json(...)::text``
    column) via ``\\copy ... TO '<tmp file>' WITH (FORMAT CSV)`` and returns
    the parsed JSON objects, one per source row. CSV format (not the plain
    COPY text format) so a payload's real embedded newlines/backslashes stay
    correctly quoted instead of needing manual COPY-text unescaping."""
    with tempfile.NamedTemporaryFile(suffix=".csv", delete=False) as tmp:
        tmp_path = Path(tmp.name)
    try:
        copy_cmd = f"\\copy ({select_sql}) to '{tmp_path}' with (format csv)"
        result = subprocess.run(
            [psql_bin, database_url, "-v", "ON_ERROR_STOP=1", "-c", copy_cmd],
            capture_output=True,
            text=True,
        )
        if result.returncode != 0:
            raise RuntimeError(f"psql \\copy failed: {result.stderr.strip()[:2000]}")
        rows: list[dict] = []
        with tmp_path.open(newline="", encoding="utf-8") as handle:
            for record in csv.reader(handle):
                if not record:
                    continue
                rows.append(json.loads(record[0]))
        return rows
    finally:
        tmp_path.unlink(missing_ok=True)


def fetch_attempt_sequence_stats(psql_bin: str, database_url: str) -> dict[str, dict]:
    """Per-attempt ``{run_id, started_at, n_events, max_sequence}`` for every
    attempt that has AT LEAST ONE event (an inner join — an attempt with zero
    events never appears here and is reported separately, distinct from the
    event-gap exclusion)."""
    sql = (
        "select row_to_json(t)::text as j from ("
        "  select ca.id as attempt_id, ca.coding_run_id as run_id, ca.started_at,"
        "         count(e.id) as n_events, max(e.sequence) as max_sequence"
        f"  from {ATTEMPT_TABLE} ca"
        f"  join {EVENT_TABLE} e on e.attempt_id = ca.id"
        "  group by ca.id, ca.coding_run_id, ca.started_at"
        ") t"
    )
    rows = fetch_via_copy(psql_bin, database_url, sql)
    return {row["attempt_id"]: row for row in rows}


def fetch_content_events_by_attempt(psql_bin: str, database_url: str) -> dict[str, list[dict]]:
    """All ``message_end``/``tool_execution_end`` raw rows, grouped by
    attempt_id, ordered by sequence within each attempt."""
    types_sql = ", ".join(f"'{t}'" for t in CONTENT_EVENT_TYPES)
    sql = (
        "select row_to_json(t)::text as j from ("
        "  select attempt_id, sequence, event_type, payload, id as event_id"
        f"  from {EVENT_TABLE}"
        f"  where event_type in ({types_sql})"
        "  order by attempt_id, sequence"
        ") t"
    )
    rows = fetch_via_copy(psql_bin, database_url, sql)
    by_attempt: dict[str, list[dict]] = {}
    for row in rows:
        by_attempt.setdefault(row["attempt_id"], []).append(row)
    return by_attempt


def fetch_total_attempt_count(psql_bin: str, database_url: str) -> int:
    sql = f"select row_to_json(t)::text as j from (select count(*) as n from {ATTEMPT_TABLE}) t"
    rows = fetch_via_copy(psql_bin, database_url, sql)
    return rows[0]["n"] if rows else 0


def rel_to_root(path: Path) -> str:
    """POSIX path relative to MEMPHANT_ROOT when possible, else the resolved
    absolute path unchanged (mirrors ``gate_mine_goldens.rel_to_root``)."""
    resolved = path.resolve()
    try:
        return resolved.relative_to(gc.MEMPHANT_ROOT).as_posix()
    except ValueError:
        return str(resolved)


# --- main --------------------------------------------------------------------


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--database-url", default=DEFAULT_DATABASE_URL)
    parser.add_argument("--psql-bin", default="psql")
    parser.add_argument("--seed", type=int, default=SAMPLE_SEED)
    parser.add_argument("--min-events", type=int, default=MIN_CONTENT_EVENTS)
    parser.add_argument("--min-chars", type=int, default=MIN_TOTAL_CHARS)
    parser.add_argument("--truncate-chars", type=int, default=TRUNCATE_CHARS)
    parser.add_argument("--haystack-min", type=int, default=HAYSTACK_MIN)
    parser.add_argument("--haystack-max", type=int, default=HAYSTACK_MAX)
    parser.add_argument("--out-corpus", default=str(CORPUS_PATH))
    parser.add_argument("--out-stats", default=str(STATS_PATH))
    args = parser.parse_args()

    t0 = time.time()
    total_attempts = fetch_total_attempt_count(args.psql_bin, args.database_url)
    attempt_stats = fetch_attempt_sequence_stats(args.psql_bin, args.database_url)
    attempts_with_events = len(attempt_stats)
    attempts_with_zero_events = total_attempts - attempts_with_events

    gap_attempt_ids = {
        aid
        for aid, d in attempt_stats.items()
        if has_event_gap(d["n_events"], d["max_sequence"])
    }
    ok_attempt_ids = set(attempt_stats) - gap_attempt_ids
    gap_rate = (len(gap_attempt_ids) / attempts_with_events) if attempts_with_events else 0.0
    print(
        f"attempts total={total_attempts} with_events={attempts_with_events} "
        f"zero_events={attempts_with_zero_events} gap_excluded={len(gap_attempt_ids)} "
        f"({gap_rate:.1%}) ok={len(ok_attempt_ids)}",
        file=sys.stderr,
    )

    content_by_attempt = fetch_content_events_by_attempt(args.psql_bin, args.database_url)

    eligible: dict[str, list[dict]] = {}
    for attempt_id in ok_attempt_ids:
        raw = content_by_attempt.get(attempt_id, [])
        content_events = build_content_events(raw, args.truncate_chars)
        if is_eligible(content_events, args.min_events, args.min_chars):
            eligible[attempt_id] = content_events
    print(
        f"eligible attempts (>= {args.min_events} events, >= {args.min_chars} chars): "
        f"{len(eligible)}/{len(ok_attempt_ids)}",
        file=sys.stderr,
    )
    if not eligible:
        print("no eligible attempts found; aborting", file=sys.stderr)
        return 1

    eligible_counts = [(aid, len(events)) for aid, events in eligible.items()]
    chosen, cumulative = sample_attempts(
        eligible_counts, args.seed, args.haystack_min, args.haystack_max
    )
    if cumulative > args.haystack_max:
        print(
            f"NOTE: sample overshoots --haystack-max ({cumulative} > {args.haystack_max}); "
            "a single sampled attempt's event count exceeds the window — accepted as-is "
            "(attempts are never split mid-sample)",
            file=sys.stderr,
        )
    print(
        f"sampled {len(chosen)} attempts, {cumulative} content events "
        f"(target [{args.haystack_min}, {args.haystack_max}]) seed={args.seed}",
        file=sys.stderr,
    )

    corpus_rows = []
    truncated_events = 0
    total_chars = 0
    for attempt_id in chosen:
        events = eligible[attempt_id]
        truncated_events += sum(1 for e in events if e["truncated"])
        total_chars += sum(len(e["text"]) for e in events)
        meta = attempt_stats[attempt_id]
        corpus_rows.append(
            {
                "attempt_id": attempt_id,
                "run_id": meta["run_id"],
                "started_at": meta["started_at"],
                "events": events,
            }
        )
    corpus_rows.sort(key=lambda row: row["attempt_id"])

    out_corpus = Path(args.out_corpus)
    gc.write_jsonl(out_corpus, corpus_rows)
    corpus_bytes = out_corpus.read_bytes()

    stats = {
        "source_table": EVENT_TABLE,
        "database_url_db": args.database_url.rsplit("/", 1)[-1],
        "total_attempts": total_attempts,
        "attempts_with_events": attempts_with_events,
        "attempts_with_zero_events": attempts_with_zero_events,
        "gap_excluded_attempts": len(gap_attempt_ids),
        "gap_exclusion_rate": round(gap_rate, 4),
        "ok_attempts": len(ok_attempt_ids),
        "eligible_attempts": len(eligible),
        "min_events": args.min_events,
        "min_chars": args.min_chars,
        "truncate_chars": args.truncate_chars,
        "sample_seed": args.seed,
        "haystack_min": args.haystack_min,
        "haystack_max": args.haystack_max,
        "sampled_attempts": len(chosen),
        "haystack_event_count": cumulative,
        "haystack_char_count": total_chars,
        "haystack_truncated_event_count": truncated_events,
        "corpus_path": rel_to_root(out_corpus),
        "corpus_sha256": gc.sha256_hex(corpus_bytes),
        "corpus_bytes": len(corpus_bytes),
        "extracted_at_unix": int(t0),
        "elapsed_seconds": round(time.time() - t0, 1),
    }
    Path(args.out_stats).write_text(json.dumps(stats, indent=2) + "\n")

    print(
        f"corpus written: {out_corpus} attempts={len(corpus_rows)} "
        f"events={cumulative} chars={total_chars} truncated={truncated_events} "
        f"sha256={stats['corpus_sha256'][:12]} stats={args.out_stats}",
        file=sys.stderr,
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

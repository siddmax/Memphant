#!/usr/bin/env python3
"""Golden-set miner for the R0 code-lane sub-bakeoff (R0-T6).

Pattern-matched on ``gate_mine_goldens.py`` (the W10 docs-gate miner) and
reuses its OpenRouter-calling machinery directly by import — ``MinerCli``
(the raised-``max_tokens`` ``ReaderCli`` subclass), ``parse_reply``,
``lexical_overlap`` — rather than re-implementing them (the brief's "reuse
gate_common helpers where they fit," extended here to the sibling miner
script itself since it is stdlib-only and side-effect-free to import).

Recipe:
- candidates are individual **content events** (not markdown sections) drawn
  from ``benchmarks/data/coding_events_corpus.jsonl`` (R0-T6 extraction),
  stratified by event role (assistant/toolResult/user) the same way the docs
  miner stratifies by doc-directory bucket;
- each generator call is given the TARGET event's full text plus a short
  preview of the immediately preceding events in that attempt (for
  "coding-continuity" phrasing — "what error did the build produce when
  X?"), and must return a question + a verbatim ``answer_span`` copied from
  the TARGET event ONLY;
- a candidate is kept only if its span (a) locates verbatim in the target
  event's text (exact, then whitespace-normalized fallback — never
  fabricated), (b) is 3-200 chars, and (c) is not "too generic" (appears in
  more than 3 distinct attempts across the sampled corpus);
- single-hop only (no multi-hop stratum for the code lane, per the brief).

Honesty / determinism contract (same as the docs miner): the generator model
(``google/gemini-3.1-pro-preview`` via OpenRouter) is neither the reader nor
the judge that will score this gate later; every reply is cached by
``sha256(engine+model+kind+system+prompt)`` so a rerun with a warm cache
re-emits byte-identical goldens at zero cost; candidate sampling is a seeded
shuffle over a sorted candidate list.

Outputs:
- ``benchmarks/data/coding_events_golden.jsonl`` — one golden per line
  (gitignored: private coding content, never committed);
- ``benchmarks/data/coding_events_golden.lock.json`` — the ONE committed
  artifact: sha256/count/strata of the golden set, the mining params
  (seed, model, span bounds, generic threshold), the mining stats
  (fresh/cached calls, rejects by reason), and the R0-T6 extraction stats
  folded in from the corpus stats sidecar (counts only — no content, per the
  binding privacy note in the brief).
"""

from __future__ import annotations

import argparse
import json
import random
import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import gate_common as gc  # noqa: E402
import gate_mine_goldens as gm  # noqa: E402

DEFAULT_GENERATOR_MODEL = "google/gemini-3.1-pro-preview"
TARGET_TOTAL = 40
CANDIDATE_MULTIPLIER = 4
SAMPLE_SEED = 20260713
EVENT_MIN_CHARS = 200
MIN_SPAN_CHARS = 3
MAX_SPAN_CHARS = 200
TOO_GENERIC_THRESHOLD = 3
PREVIEW_MAX_EVENTS = 3
PREVIEW_MAX_CHARS_EACH = 300

CORPUS_PATH = gc.MEMPHANT_ROOT / "benchmarks" / "data" / "coding_events_corpus.jsonl"
CORPUS_STATS_PATH = gc.MEMPHANT_ROOT / "benchmarks" / "data" / "coding_events_corpus.stats.json"
GOLDEN_PATH = gc.MEMPHANT_ROOT / "benchmarks" / "data" / "coding_events_golden.jsonl"

WORD_RE = re.compile(r"[a-z0-9]+")


# --- pure functions (TDD'd in tests/test_code_lane_mine.py) -----------------


def locate_span_in_event(event_text: str, span: str) -> tuple[int, int, str] | None:
    """Locate ``span`` in ``event_text``: verbatim first, then a
    whitespace-normalized fallback (the model may reflow whitespace across an
    embedded newline). Returns ``(start, end, exact_text)`` where
    ``event_text[start:end] == exact_text`` holds by construction — the
    canonical span recorded in the golden. ``None`` if unlocatable (never
    fabricated)."""
    idx = event_text.find(span)
    if idx != -1:
        return idx, idx + len(span), span
    span_norm = re.sub(r"\s+", " ", span.strip())
    if not span_norm:
        return None
    pattern = re.compile(r"\s+".join(re.escape(tok) for tok in span_norm.split(" ")))
    match = pattern.search(event_text)
    if match:
        return match.start(), match.end(), match.group(0)
    return None


def too_generic(span: str, corpus_index: dict[str, str], threshold: int) -> bool:
    """True when ``span`` (verbatim substring) appears in MORE than
    ``threshold`` distinct attempts' concatenated text in ``corpus_index``
    (``attempt_id -> full text``) — too generic a fact to be a good
    single-hop provenance probe."""
    count = sum(1 for text in corpus_index.values() if span in text)
    return count > threshold


def build_candidate_pool(corpus_rows: list[dict], min_chars: int) -> list[dict]:
    """Flattens the corpus into individual content-event candidates
    (``{attempt_id, sequence, role, text, event_id}``) substantial enough to
    mine a verbatim-span question from (``>= min_chars``)."""
    pool: list[dict] = []
    for row in corpus_rows:
        for event in row["events"]:
            if len(event["text"]) < min_chars:
                continue
            pool.append(
                {
                    "attempt_id": row["attempt_id"],
                    "sequence": event["sequence"],
                    "role": event["role"],
                    "text": event["text"],
                    "event_id": event["event_id"],
                }
            )
    return pool


def candidate_key(candidate: dict) -> str:
    return f"{candidate['attempt_id']}::{candidate['sequence']}"


def stratified_candidates(pool: list[dict], seed: int, want: int) -> list[dict]:
    """Round-robins across role buckets (sorted by candidate key, then
    seeded-shuffled within each bucket — deterministic given ``seed``) until
    ``want`` candidates are collected or the pool is exhausted. Mirrors the
    docs miner's bucket-round-robin sampling, with event role standing in for
    doc-directory bucket."""
    rng = random.Random(seed)
    by_role: dict[str, list[dict]] = {}
    for candidate in pool:
        by_role.setdefault(candidate["role"], []).append(candidate)
    for role in by_role:
        by_role[role].sort(key=candidate_key)
        rng.shuffle(by_role[role])
    roles_sorted = sorted(by_role)
    cursors = {role: 0 for role in roles_sorted}
    out: list[dict] = []
    while len(out) < want and any(cursors[r] < len(by_role[r]) for r in roles_sorted):
        for role in roles_sorted:
            if cursors[role] < len(by_role[role]):
                out.append(by_role[role][cursors[role]])
                cursors[role] += 1
                if len(out) >= want:
                    break
    return out


def build_context_preview(
    events: list[dict], target_index: int, max_events: int, max_chars_each: int
) -> str:
    """A short "role: clipped-text" preview of up to ``max_events``
    immediately-preceding events in the SAME attempt, for continuity flavor
    in the generator prompt. Never includes the target event or anything
    after it (the model must not lift the answer span from context)."""
    start = max(0, target_index - max_events)
    preceding = events[start:target_index]
    lines = []
    for event in preceding:
        clipped = event["text"][:max_chars_each]
        lines.append(f"{event['role']}: {clipped}")
    return "\n\n".join(lines)


def content_words(text: str) -> set[str]:
    return {w for w in WORD_RE.findall(text.lower()) if len(w) > 2}


def lexical_overlap(question: str, span: str) -> float:
    q = content_words(question)
    s = content_words(span)
    if not q or not s:
        return 0.0
    return len(q & s) / len(q | s)


# --- generator prompt ---------------------------------------------------


GEN_SYSTEM_SINGLE = (
    "You author one question-and-answer pair over a slice of an AI coding "
    "agent's execution transcript (an autonomous run: user task, assistant "
    "turns, tool results). Requirements, all mandatory: (1) the answer MUST "
    "be a short span (roughly 3 to 200 characters) copied VERBATIM, "
    "character-for-character, from the TARGET EVENT text only — never "
    "paraphrased, never invented, and never copied from the preceding "
    "context shown for background; (2) the question must have a "
    "'coding-continuity' flavor, asking about a concrete fact from this "
    "point in the run — an error message, a file that was changed, a "
    "command that was run, a test result, a decision made — for example "
    "'What error did the build produce when running the test suite?', "
    "'Which file was modified to fix the failing import?', 'What command "
    "was run to install the missing dependency?'; ground the question in "
    "the run's narrative using the preceding context, but its ANSWER must "
    "come only from the target event; never ask a meta question about the "
    "transcript format itself; (3) paraphrase the question so it shares as "
    "few words as possible with the answer span and its sentence — use "
    "synonyms and a different sentence shape. Output ONLY a JSON object "
    "with keys \"question\" and \"answer_span\". No markdown, no code "
    "fence, no commentary."
)


def single_prompt(candidate: dict, preview: str) -> str:
    parts = []
    if preview:
        parts.append(
            "Preceding context in this coding run (for background only — "
            "the answer_span must NOT come from here):\n" + preview + "\n"
        )
    parts.append(
        f"TARGET EVENT (role: {candidate['role']}, turn {candidate['sequence']} of "
        "this run) — the answer_span MUST be copied verbatim from HERE:\n"
        f"{candidate['text']}\n"
    )
    return "\n".join(parts)


# --- mining loop --------------------------------------------------------


def mine(
    cli,
    candidates: list[dict],
    events_by_attempt: dict[str, list[dict]],
    corpus_index: dict[str, str],
    n_target: int,
    id_prefix: str,
    min_span: int,
    max_span: int,
    generic_threshold: int,
) -> tuple[list[dict], dict[str, int]]:
    goldens: list[dict] = []
    rejects = {
        "parse_failed": 0,
        "span_not_located": 0,
        "span_length": 0,
        "too_generic": 0,
    }
    for candidate in candidates:
        if len(goldens) >= n_target:
            break
        attempt_events = events_by_attempt[candidate["attempt_id"]]
        target_index = next(
            i for i, e in enumerate(attempt_events) if e["sequence"] == candidate["sequence"]
        )
        preview = build_context_preview(
            attempt_events, target_index, PREVIEW_MAX_EVENTS, PREVIEW_MAX_CHARS_EACH
        )
        reply = cli.call("generate_single", GEN_SYSTEM_SINGLE, single_prompt(candidate, preview))
        obj = gm.parse_reply(reply, ("question", "answer_span"))
        if obj is None:
            rejects["parse_failed"] += 1
            continue
        span = obj["answer_span"].strip()
        located = locate_span_in_event(candidate["text"], span)
        if located is None:
            rejects["span_not_located"] += 1
            continue
        start, end, exact = located
        if not (min_span <= len(exact) <= max_span):
            rejects["span_length"] += 1
            continue
        if too_generic(exact, corpus_index, generic_threshold):
            rejects["too_generic"] += 1
            continue
        question = obj["question"].strip()
        goldens.append(
            {
                "question_id": f"{id_prefix}_{len(goldens) + 1:03d}_{candidate['role']}",
                "question_type": candidate["role"],
                "is_abstention": False,
                "question": question,
                "question_date": None,
                "gold_answer": exact,
                "multi_hop": False,
                "provenance": [
                    {
                        "role": "answer",
                        "attempt_id": candidate["attempt_id"],
                        "event_sequence": candidate["sequence"],
                        "event_role": candidate["role"],
                        "event_id": candidate["event_id"],
                        "span": exact,
                        "char_start": start,
                        "char_end": end,
                    }
                ],
                "lexical_overlap": round(lexical_overlap(question, exact), 4),
                "source_event_key": candidate_key(candidate),
            }
        )
    return goldens, rejects


# --- main -----------------------------------------------------------------


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--corpus", default=str(CORPUS_PATH))
    parser.add_argument("--corpus-stats", default=str(CORPUS_STATS_PATH))
    parser.add_argument("--engine", default="openrouter")
    parser.add_argument("--model", default=DEFAULT_GENERATOR_MODEL)
    parser.add_argument("--target", type=int, default=TARGET_TOTAL)
    parser.add_argument(
        "--cache-dir",
        default=str(gc.MEMPHANT_ROOT / "docs/build-log/artifacts/code-lane/miner-cache"),
    )
    parser.add_argument("--max-calls", type=int, default=200)
    parser.add_argument("--seed", type=int, default=SAMPLE_SEED)
    parser.add_argument("--event-min-chars", type=int, default=EVENT_MIN_CHARS)
    parser.add_argument("--min-span-chars", type=int, default=MIN_SPAN_CHARS)
    parser.add_argument("--max-span-chars", type=int, default=MAX_SPAN_CHARS)
    parser.add_argument("--too-generic-threshold", type=int, default=TOO_GENERIC_THRESHOLD)
    parser.add_argument("--out-golden", default=str(GOLDEN_PATH))
    parser.add_argument("--out-lock", default=None)
    parser.add_argument("--id-prefix", default="code_lane")
    args = parser.parse_args()

    out_golden = Path(args.out_golden)
    out_lock = Path(args.out_lock) if args.out_lock else out_golden.with_name(
        out_golden.stem + ".lock.json"
    )

    corpus_path = Path(args.corpus)
    if not corpus_path.exists():
        print(f"corpus not found: {corpus_path} (run code_lane_extract.py first)", file=sys.stderr)
        return 1
    corpus_rows = gc.load_goldens(corpus_path)  # any-JSONL loader; name is generic
    corpus_index = {
        row["attempt_id"]: "\n\n".join(e["text"] for e in row["events"]) for row in corpus_rows
    }
    events_by_attempt = {row["attempt_id"]: row["events"] for row in corpus_rows}

    pool = build_candidate_pool(corpus_rows, args.event_min_chars)
    if not pool:
        print("no candidates meet --event-min-chars; aborting", file=sys.stderr)
        return 1
    want = args.target * CANDIDATE_MULTIPLIER
    candidates = stratified_candidates(pool, args.seed, want)
    print(
        f"corpus attempts={len(corpus_rows)} candidate_pool={len(pool)} "
        f"candidates_drawn={len(candidates)} (want={want}) target={args.target}",
        file=sys.stderr,
    )

    cli = gm.MinerCli(
        args.engine, args.model, args.model, Path(args.cache_dir), args.max_calls
    )
    goldens, rejects = mine(
        cli,
        candidates,
        events_by_attempt,
        corpus_index,
        args.target,
        args.id_prefix,
        args.min_span_chars,
        args.max_span_chars,
        args.too_generic_threshold,
    )

    gc.write_jsonl(out_golden, goldens)
    golden_bytes = out_golden.read_bytes()
    strata: dict[str, int] = {}
    for row in goldens:
        strata[row["question_type"]] = strata.get(row["question_type"], 0) + 1

    extraction_stats = {}
    corpus_stats_path = Path(args.corpus_stats)
    if corpus_stats_path.exists():
        extraction_stats = json.loads(corpus_stats_path.read_text())

    lock = {
        "golden_path": "benchmarks/data/coding_events_golden.jsonl",
        "sha256": gc.sha256_hex(golden_bytes),
        "bytes": len(golden_bytes),
        "count": len(goldens),
        "strata": strata,
        "generator_engine": args.engine,
        "generator_model": args.model,
        "sample_seed": args.seed,
        "target": args.target,
        "candidate_pool_size": len(pool),
        "candidates_drawn": len(candidates),
        "event_min_chars": args.event_min_chars,
        "min_span_chars": args.min_span_chars,
        "max_span_chars": args.max_span_chars,
        "too_generic_threshold": args.too_generic_threshold,
        "fresh_calls": cli.fresh_calls,
        "cached_calls": cli.cached_calls,
        "rejects": rejects,
        "extraction": extraction_stats,
    }
    out_lock.write_text(json.dumps(lock, indent=2) + "\n")

    print(
        f"mined={len(goldens)} target={args.target} strata={strata} "
        f"fresh_calls={cli.fresh_calls} cached_calls={cli.cached_calls} "
        f"rejects={rejects} sha256={lock['sha256'][:12]} out={out_golden}",
        file=sys.stderr,
    )
    if len(goldens) < args.target:
        print(f"WARNING: mined {len(goldens)} < target {args.target}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

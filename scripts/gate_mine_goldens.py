#!/usr/bin/env python3
"""Golden-set miner for the Syndai replacement gate (W10).

Mines a version-pinned golden QA set from a real product-documentation corpus
(``/Users/sidsharma/Syndai/docs/**/*.md``, excluding the SDD process
scaffolding under ``docs/superpowers/`` which is not product documentation; see
``gate_common``).

Recipe (plan addendum W10, "engine-vs-engine gate"):
- stratified sample of markdown *sections* (leaf content under a heading),
  bucketed by top-level docs directory;
- for each section, generate exactly one QA pair whose answer is a **verbatim
  span** copied character-for-character from the section (recorded with file,
  heading path, and absolute char span), asking the generator to paraphrase the
  question so its lexical overlap with the span stays low;
- ~20% multi-hop: a question that needs two sections of the same file (the spec
  allows cross-file; same-file pairs read more coherently), recording a bridge
  span AND the answer span so a retrieval engine must surface both;
- target 60 questions total.

Honesty / determinism contract:
- the generator model is neither the reader (gpt-5.6-terra) nor the judge
  (claude-sonnet-5): no self-grading. Default ``google/gemini-3.1-pro-preview``
  via OpenRouter (``OPENROUTER_API_KEY`` in the environment; run through
  ``doppler run --project syndai --config dev --``).
- every generator reply is cached by ``sha256(engine + model + kind + system +
  prompt)`` — the exact ``run_reader.ReaderCli`` cache — so a rerun over the
  same corpus with a warm cache re-emits byte-identical goldens and spends zero
  budget;
- section sampling is a seeded shuffle over a *sorted* candidate list, so the
  sample is deterministic given the corpus;
- a generated pair is kept only if every claimed span is an exact substring of
  its section (verbatim), otherwise it is dropped and the next candidate tried —
  the miner never fabricates a span it could not locate.

Outputs (all committed):
- ``benchmarks/manifests/syndai_docs_gate.lock.json`` — the corpus manifest:
  per-file sha256 + byte length, the Syndai git commit, and the exclusion rule;
- ``benchmarks/data/syndai_docs_golden.jsonl`` — one golden per line;
- ``benchmarks/data/syndai_docs_golden.lock.json`` — sha256 of the golden JSONL
  plus counts, so the two engine runners can assert they scored the pinned set.
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

DEFAULT_SYNDAI_ROOT = Path("/Users/sidsharma/Syndai")
DEFAULT_GENERATOR_MODEL = "google/gemini-3.1-pro-preview"
TARGET_TOTAL = 60
MULTI_HOP_FRACTION = 0.20
CANDIDATE_MULTIPLIER = 4
SAMPLE_SEED = 20260711

WORD_RE = re.compile(r"[a-z0-9]+")

MANIFEST_PATH = gc.MEMPHANT_ROOT / "benchmarks" / "manifests" / "syndai_docs_gate.lock.json"
GOLDEN_PATH = gc.MEMPHANT_ROOT / "benchmarks" / "data" / "syndai_docs_golden.jsonl"


def content_words(text: str) -> set[str]:
    return {w for w in WORD_RE.findall(text.lower()) if len(w) > 2}


def lexical_overlap(question: str, span: str) -> float:
    q = content_words(question)
    s = content_words(span)
    if not q or not s:
        return 0.0
    return len(q & s) / len(q | s)


def strip_json(reply: str) -> str:
    text = reply.strip()
    if text.startswith("```"):
        text = text.split("\n", 1)[1] if "\n" in text else text
        if text.endswith("```"):
            text = text[:-3]
        text = text.strip()
    start = text.find("{")
    end = text.rfind("}")
    if start == -1 or end == -1 or end <= start:
        return text
    return text[start : end + 1]


def parse_reply(reply: str, required_keys: tuple[str, ...]) -> dict | None:
    try:
        obj = json.loads(strip_json(reply))
    except (json.JSONDecodeError, ValueError):
        return None
    if not isinstance(obj, dict):
        return None
    for key in required_keys:
        value = obj.get(key)
        if not isinstance(value, str) or not value.strip():
            return None
    return obj


def load_excluded_keys(path: Path) -> set[str]:
    """Flattened `source_section_key` values from an existing golden JSONL —
    single-hop rows contribute one key, multi-hop rows split on ``||`` so BOTH
    of their sections are excluded. Used by ``--exclude-golden`` so a second
    mining pass (v2) draws a fresh sample of the SAME pinned corpus with zero
    section overlap against the first (v1)."""
    keys: set[str] = set()
    for row in gc.load_goldens(path):
        keys.update(row["source_section_key"].split("||"))
    return keys


def filter_excluded(
    per_file: dict[str, list[gc.Section]],
    all_candidates: list[gc.Section],
    excluded_keys: set[str],
) -> tuple[dict[str, list[gc.Section]], list[gc.Section]]:
    """Drops sections whose ``.key()`` is in ``excluded_keys`` from both the
    per-file map (feeds multi-hop pairing) and the flat candidate list (feeds
    single-hop sampling), so an excluded section can never be drawn by either
    path. A no-op (returns the inputs unchanged) when ``excluded_keys`` is
    empty."""
    if not excluded_keys:
        return per_file, all_candidates
    filtered_per_file = {
        rel: [s for s in secs if s.key() not in excluded_keys]
        for rel, secs in per_file.items()
    }
    filtered_candidates = [s for s in all_candidates if s.key() not in excluded_keys]
    return filtered_per_file, filtered_candidates


def assert_no_overlap(goldens: list[dict], excluded_goldens: list[dict]) -> None:
    """Sanity check for an exclusion-mined golden set: zero `question_id`
    collisions and zero `source_section_key` overlap against the golden set it
    was mined to exclude. Raises loudly (never silently drops rows) since
    either would mean the exclusion filtering failed to do its job."""
    excluded_qids = {g["question_id"] for g in excluded_goldens}
    excluded_keys: set[str] = set()
    for g in excluded_goldens:
        excluded_keys.update(g["source_section_key"].split("||"))
    new_qids = {g["question_id"] for g in goldens}
    new_keys: set[str] = set()
    for g in goldens:
        new_keys.update(g["source_section_key"].split("||"))
    qid_overlap = sorted(excluded_qids & new_qids)
    if qid_overlap:
        raise RuntimeError(f"question_id collision with excluded golden set: {qid_overlap}")
    key_overlap = excluded_keys & new_keys
    if key_overlap:
        raise RuntimeError(
            f"source_section_key overlap with excluded golden set: {len(key_overlap)} keys"
        )


def verify_corpus_pin(manifest_path: Path, root: Path, files: list[str]) -> list[str]:
    """Corpus-pin integrity check: recomputes sha256 for every file the
    CURRENTLY-COMMITTED manifest at ``manifest_path`` pins and diffs against
    the current corpus. Returns a list of human-readable drift descriptions
    (empty = the pin holds). Run BEFORE the manifest is rebuilt/overwritten —
    otherwise this would always compare a file against itself."""
    old = json.loads(manifest_path.read_text())
    old_files: dict = old.get("files", {})
    pinned = set(old_files)
    current = set(files)
    problems: list[str] = []
    for rel in sorted(pinned - current):
        problems.append(f"{rel}: MISSING (file no longer present at pinned path)")
    for rel in sorted(current - pinned):
        problems.append(f"{rel}: NEW file not in lock (corpus changed since pin)")
    for rel in sorted(pinned & current):
        actual_sha = gc.sha256_hex((root / rel).read_bytes())
        expected_sha = old_files[rel]["sha256"]
        if actual_sha != expected_sha:
            problems.append(
                f"{rel}: sha256 mismatch lock={expected_sha[:12]} current={actual_sha[:12]}"
            )
    return problems


def locate_span(section: gc.Section, span: str) -> tuple[int, int, str] | None:
    """Locate ``span`` in the section's content: verbatim first, then a
    whitespace-normalized fallback. Returns (abs_start, abs_end, exact_text)
    where exact_text is the RAW corpus text at those offsets — the canonical
    span recorded in the golden, so ``corpus[start:end] == span`` holds by
    construction even when the generator reflowed whitespace. None if
    unlocatable (the pair is then dropped)."""
    content = section.content
    base = section.content_char_start
    idx = content.find(span)
    if idx != -1:
        return base + idx, base + idx + len(span), span
    span_norm = re.sub(r"\s+", " ", span.strip())
    if not span_norm:
        return None
    pattern = re.compile(r"\s+".join(re.escape(tok) for tok in span_norm.split(" ")))
    match = pattern.search(content)
    if match:
        return base + match.start(), base + match.end(), match.group(0)
    return None


def provenance_entry(section: gc.Section, span: str, role: str) -> dict | None:
    located = locate_span(section, span)
    if located is None:
        return None
    start, end, exact = located
    return {
        "role": role,
        "file": section.rel_path,
        "bucket": section.bucket,
        "heading_path": section.heading_path,
        "span": exact,
        "char_start": start,
        "char_end": end,
    }


GEN_SYSTEM_SINGLE = (
    "You author one reading-comprehension question and answer from a single "
    "documentation section. Requirements, all mandatory: (1) the answer MUST be "
    "a short span (a few words to one sentence) copied VERBATIM, "
    "character-for-character, from the section text — never paraphrased, never "
    "invented; (2) the question must be answerable ONLY from this section's "
    "substantive content — ask about a concrete fact, value, name, rule, or "
    "definition, never a meta question about the document itself; (3) paraphrase "
    "the question so it shares as few words as possible with the answer span and "
    "its sentence — use synonyms and a different sentence shape. Output ONLY a "
    "JSON object with keys \"question\" and \"answer_span\". No markdown, no "
    "code fence, no commentary."
)

GEN_SYSTEM_MULTI = (
    "You author one multi-hop question that can be answered ONLY by combining "
    "two documentation sections. Requirements, all mandatory: (1) the question "
    "genuinely requires a fact from EACH section — neither section alone "
    "suffices; (2) \"answer_span\" MUST be copied VERBATIM from SECTION B and is "
    "the final answer; (3) \"bridge_span\" MUST be copied VERBATIM from SECTION A "
    "and is the linking fact the question forces the reader to find in A; both "
    "spans are short (a few words to one sentence), never paraphrased or "
    "invented; (4) paraphrase the question so it shares as few words as possible "
    "with either span. Keep any internal deliberation brief. Output ONLY a "
    "JSON object with keys \"question\", \"bridge_span\" (from A), and "
    "\"answer_span\" (from B). No markdown, no code fence, no commentary."
)


def single_prompt(section: gc.Section) -> str:
    return (
        f"Section heading path: {' > '.join(section.heading_path)}\n"
        f"Source file: {section.rel_path}\n\n"
        "Section text:\n"
        f"{section.body}\n"
    )


def multi_prompt(a: gc.Section, b: gc.Section) -> str:
    return (
        "SECTION A\n"
        f"heading path: {' > '.join(a.heading_path)}\n"
        f"source file: {a.rel_path}\n"
        f"text:\n{a.body}\n\n"
        "SECTION B\n"
        f"heading path: {' > '.join(b.heading_path)}\n"
        f"source file: {b.rel_path}\n"
        f"text:\n{b.body}\n"
    )


def mine(cli, single_sections, multi_pairs, n_single, n_multi, id_prefix: str = "syndai_docs") -> list[dict]:
    goldens: list[dict] = []
    for section in single_sections:
        if len(goldens) >= n_single:
            break
        reply = cli.call("generate_single", GEN_SYSTEM_SINGLE, single_prompt(section))
        obj = parse_reply(reply, ("question", "answer_span"))
        if obj is None:
            continue
        span = obj["answer_span"].strip()
        prov = provenance_entry(section, span, "answer")
        if prov is None:
            continue
        question = obj["question"].strip()
        goldens.append(
            {
                "question_id": f"{id_prefix}_s{len(goldens) + 1:03d}_{section.bucket}",
                "question_type": section.bucket,
                "is_abstention": False,
                "question": question,
                "question_date": None,
                "gold_answer": prov["span"],
                "multi_hop": False,
                "provenance": [prov],
                "lexical_overlap": round(lexical_overlap(question, span), 4),
                "source_section_key": section.key(),
            }
        )

    multi_count = 0
    for a, b in multi_pairs:
        if multi_count >= n_multi:
            break
        reply = cli.call("generate_multi", GEN_SYSTEM_MULTI, multi_prompt(a, b))
        obj = parse_reply(reply, ("question", "bridge_span", "answer_span"))
        if obj is None:
            continue
        bridge, answer = obj["bridge_span"].strip(), obj["answer_span"].strip()
        prov_bridge = provenance_entry(a, bridge, "bridge")
        prov_answer = provenance_entry(b, answer, "answer")
        if prov_bridge is None or prov_answer is None:
            continue
        question = obj["question"].strip()
        multi_count += 1
        goldens.append(
            {
                "question_id": f"{id_prefix}_m{multi_count:03d}",
                "question_type": "multi-hop",
                "is_abstention": False,
                "question": question,
                "question_date": None,
                "gold_answer": prov_answer["span"],
                "multi_hop": True,
                "provenance": [prov_bridge, prov_answer],
                "lexical_overlap": round(
                    max(lexical_overlap(question, bridge), lexical_overlap(question, answer)),
                    4,
                ),
                "source_section_key": f"{a.key()}||{b.key()}",
            }
        )
    return goldens


def build_manifest(root: Path, files: list[str]) -> dict:
    entries = {}
    total_bytes = 0
    for rel in files:
        data = (root / rel).read_bytes()
        entries[rel] = {"sha256": gc.sha256_hex(data), "bytes": len(data)}
        total_bytes += len(data)
    sections = gc.all_sections(root, files)
    return {
        "corpus": "syndai_product_docs",
        "syndai_repo": root.name,
        "git_commit": gc.git_commit(root),
        "globs": list(gc.CORPUS_GLOBS),
        "excluded_prefixes": list(gc.EXCLUDE_PREFIXES),
        "exclusion_rationale": (
            "docs/superpowers/ is SDD process scaffolding (plans, audits, "
            "briefs, research), not product documentation; the gate corpus is "
            "the real product/architecture/spec docs."
        ),
        "file_count": len(files),
        "total_bytes": total_bytes,
        "sectionizer": "markdown_heading_leaf_v1",
        "section_count": len(sections),
        "section_chars": sum(len(section.body) for section in sections),
        "section_revision": gc.corpus_revision(sections),
        "mining_candidate_section_count": len(gc.candidate_sections(sections)),
        "mining_candidate_rule": (
            "content chars >= 240 and section body chars <= 3200; "
            "mining only, never indexing"
        ),
        "files": entries,
    }


def rel_to_root(path_str: str) -> str:
    """POSIX path relative to MEMPHANT_ROOT when possible (for lock-file
    fields), else the input unchanged (e.g. an already-relative path)."""
    resolved = Path(path_str).resolve()
    try:
        return resolved.relative_to(gc.MEMPHANT_ROOT).as_posix()
    except ValueError:
        return path_str


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--syndai-root", default=str(DEFAULT_SYNDAI_ROOT))
    parser.add_argument("--engine", default="openrouter")
    parser.add_argument("--model", default=DEFAULT_GENERATOR_MODEL)
    parser.add_argument("--target", type=int, default=TARGET_TOTAL)
    parser.add_argument(
        "--cache-dir",
        default=str(gc.MEMPHANT_ROOT / "docs/build-log/artifacts/syndai-gate/miner-cache"),
    )
    parser.add_argument("--max-calls", type=int, default=400)
    parser.add_argument("--seed", type=int, default=SAMPLE_SEED)
    parser.add_argument("--manifest-only", action="store_true")
    parser.add_argument(
        "--exclude-golden",
        default=None,
        help=(
            "existing golden JSONL whose source_section_key values (both "
            "single- and multi-hop, split on '||') are excluded from the "
            "candidate pool — mines a fresh sample of the SAME pinned corpus "
            "with zero section overlap against it (e.g. v2 vs v1)"
        ),
    )
    parser.add_argument(
        "--out-golden",
        default=str(GOLDEN_PATH),
        help="golden JSONL output path (default: the v1 path)",
    )
    parser.add_argument(
        "--out-lock",
        default=None,
        help="golden lock JSON output path (default: derived from --out-golden's stem, e.g. foo.jsonl -> foo.lock.json)",
    )
    parser.add_argument(
        "--id-prefix",
        default="syndai_docs",
        help=(
            "question_id prefix (default: syndai_docs); set distinctly for an "
            "--exclude-golden run (e.g. syndai_docs_v2) so question_ids can "
            "never collide with the excluded set even before the sanity check runs"
        ),
    )
    args = parser.parse_args()

    out_golden = Path(args.out_golden)
    out_lock = Path(args.out_lock) if args.out_lock else out_golden.with_name(out_golden.stem + ".lock.json")

    root = Path(args.syndai_root)
    files = gc.list_corpus_files(root)
    if not files:
        print("no corpus files found", file=sys.stderr)
        return 1

    if MANIFEST_PATH.exists() and not args.manifest_only:
        drift = verify_corpus_pin(MANIFEST_PATH, root, files)
        if drift:
            print(f"BLOCKED: corpus-pin drift detected against {MANIFEST_PATH}", file=sys.stderr)
            for line in drift:
                print(f"  {line}", file=sys.stderr)
            return 1
        print(f"corpus-pin verified: {len(files)} files match {MANIFEST_PATH}", file=sys.stderr)

    manifest = build_manifest(root, files)
    MANIFEST_PATH.parent.mkdir(parents=True, exist_ok=True)
    MANIFEST_PATH.write_text(json.dumps(manifest, indent=2) + "\n")
    print(
        f"manifest={MANIFEST_PATH} files={len(files)} "
        f"commit={manifest['git_commit'][:12]}",
        file=sys.stderr,
    )
    if args.manifest_only:
        return 0

    per_file: dict[str, list[gc.Section]] = {}
    all_candidates: list[gc.Section] = []
    for rel in files:
        text = (root / rel).read_text(encoding="utf-8", errors="replace")
        sections = gc.candidate_sections(gc.parse_sections(rel, text))
        per_file[rel] = sections
        all_candidates.extend(sections)

    excluded_goldens: list[dict] = []
    excluded_keys: set[str] = set()
    if args.exclude_golden:
        excluded_goldens = gc.load_goldens(Path(args.exclude_golden))
        excluded_keys = load_excluded_keys(Path(args.exclude_golden))
        per_file, all_candidates = filter_excluded(per_file, all_candidates, excluded_keys)
        print(
            f"exclude_golden={args.exclude_golden} excluded_sections={len(excluded_keys)} "
            f"remaining_candidates={len(all_candidates)}",
            file=sys.stderr,
        )

    n_multi = round(args.target * MULTI_HOP_FRACTION)
    n_single = args.target - n_multi

    rng = random.Random(args.seed)
    by_bucket: dict[str, list[gc.Section]] = {}
    for section in all_candidates:
        by_bucket.setdefault(section.bucket, []).append(section)
    for bucket in by_bucket:
        by_bucket[bucket].sort(key=lambda s: s.key())
        rng.shuffle(by_bucket[bucket])
    buckets_sorted = sorted(by_bucket)
    single_candidates: list[gc.Section] = []
    cursors = {b: 0 for b in buckets_sorted}
    want = n_single * CANDIDATE_MULTIPLIER
    while len(single_candidates) < want and any(
        cursors[b] < len(by_bucket[b]) for b in buckets_sorted
    ):
        for bucket in buckets_sorted:
            if cursors[bucket] < len(by_bucket[bucket]):
                single_candidates.append(by_bucket[bucket][cursors[bucket]])
                cursors[bucket] += 1

    multi_pairs: list[tuple[gc.Section, gc.Section]] = []
    files_with_two = sorted(rel for rel, secs in per_file.items() if len(secs) >= 2)
    rng.shuffle(files_with_two)
    want_pairs = n_multi * CANDIDATE_MULTIPLIER
    for rel in files_with_two:
        if len(multi_pairs) >= want_pairs:
            break
        local = list(per_file[rel])
        rng.shuffle(local)
        a, b = local[0], local[1]
        if a.char_start > b.char_start:
            a, b = b, a
        multi_pairs.append((a, b))

    cli = gc._RUN_READER.ReaderCli(
        args.engine, args.model, args.model, Path(args.cache_dir), args.max_calls
    )
    goldens = mine(cli, single_candidates, multi_pairs, n_single, n_multi, args.id_prefix)

    if args.exclude_golden:
        assert_no_overlap(goldens, excluded_goldens)
        print(
            f"overlap check vs {args.exclude_golden}: question_id_overlap=0 "
            f"source_section_key_overlap=0 (excluded_sections={len(excluded_keys)})",
            file=sys.stderr,
        )

    gc.write_jsonl(out_golden, goldens)
    golden_bytes = out_golden.read_bytes()
    n_multi_out = sum(1 for row in goldens if row["multi_hop"])
    strata: dict[str, int] = {}
    for row in goldens:
        strata[row["question_type"]] = strata.get(row["question_type"], 0) + 1
    lock = {
        "golden_path": rel_to_root(str(out_golden)),
        "sha256": gc.sha256_hex(golden_bytes),
        "bytes": len(golden_bytes),
        "count": len(goldens),
        "multi_hop_count": n_multi_out,
        "strata": strata,
        "generator_engine": args.engine,
        "generator_model": args.model,
        "sample_seed": args.seed,
        "corpus_manifest": "benchmarks/manifests/syndai_docs_gate.lock.json",
        "corpus_git_commit": manifest["git_commit"],
        "exclude_golden": rel_to_root(args.exclude_golden) if args.exclude_golden else None,
        "excluded_section_count": len(excluded_keys),
    }
    out_lock.write_text(json.dumps(lock, indent=2) + "\n")

    print(
        f"mined={len(goldens)} (single={len(goldens) - n_multi_out} "
        f"multi={n_multi_out}) target={args.target} "
        f"fresh_calls={cli.fresh_calls} cached_calls={cli.cached_calls} "
        f"sha256={lock['sha256'][:12]} out={out_golden}",
        file=sys.stderr,
    )
    if len(goldens) < args.target:
        print(
            f"WARNING: mined {len(goldens)} < target {args.target}",
            file=sys.stderr,
        )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

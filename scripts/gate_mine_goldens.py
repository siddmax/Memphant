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
GOLDEN_LOCK_PATH = gc.MEMPHANT_ROOT / "benchmarks" / "data" / "syndai_docs_golden.lock.json"


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


class MinerCli(gc._RUN_READER.ReaderCli):
    """ReaderCli with a raised OpenRouter completion cap for GENERATION.

    run_reader's ``_call_openrouter`` caps ``max_tokens`` at 1024 — right for
    terse reader/judge replies, but Gemini 3.1's reasoning tokens count against
    that cap and the two-section multi-hop prompts got truncated mid-JSON
    (0/48 accepted on the first full mine). run_reader must stay byte-unchanged
    for scoring (brief req 4), so the miner overrides the one method here.
    Cache keys are inherited unchanged, so previously mined replies stay valid.
    """

    OPENROUTER_MAX_TOKENS = 8192

    def _call_openrouter(self, kind: str, system_prompt: str, prompt: str) -> str:
        rr = gc._RUN_READER
        payload = {
            "model": self.model_for(kind),
            "messages": [
                {"role": "system", "content": system_prompt},
                {"role": "user", "content": prompt},
            ],
            "temperature": 0,
            "max_tokens": self.OPENROUTER_MAX_TOKENS,
        }
        if self.reasoning_effort is not None:
            payload["reasoning"] = {"effort": self.reasoning_effort}
        body = json.dumps(payload).encode()
        request = rr.urllib.request.Request(
            rr.OPENROUTER_URL,
            data=body,
            method="POST",
            headers={
                "Authorization": f"Bearer {self._openrouter_api_key}",
                "Content-Type": "application/json",
                "HTTP-Referer": "https://github.com/memphant",
                "X-Title": "memphant-gate-miner",
            },
        )
        last_error: Exception | None = None
        for attempt, delay in enumerate((0, *rr.OPENROUTER_RETRY_DELAYS)):
            if delay:
                rr.time.sleep(delay)
            try:
                with rr.urllib.request.urlopen(
                    request, timeout=rr.OPENROUTER_TIMEOUT
                ) as response:
                    data = json.loads(response.read())
                content = (
                    (data.get("choices") or [{}])[0].get("message", {}).get("content")
                )
                if not content:
                    last_error = RuntimeError(
                        f"openrouter returned empty content (attempt "
                        f"{attempt + 1}/4): {json.dumps(data)[:500]}"
                    )
                    continue
                return content.strip()
            except rr.urllib.error.HTTPError as error:
                body_text = error.read().decode(errors="replace")[:500]
                last_error = RuntimeError(
                    f"openrouter request failed (HTTP {error.code}, attempt "
                    f"{attempt + 1}/4): {body_text}"
                )
                if error.code != 429 and error.code < 500:
                    raise last_error from error
            except (
                rr.urllib.error.URLError,
                TimeoutError,
                OSError,
                ValueError,
            ) as error:
                last_error = RuntimeError(
                    f"openrouter request failed (attempt {attempt + 1}/4): {error}"
                )
        assert last_error is not None
        raise last_error


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


def mine(cli, single_sections, multi_pairs, n_single, n_multi) -> list[dict]:
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
                "question_id": f"syndai_docs_s{len(goldens) + 1:03d}_{section.bucket}",
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
                "question_id": f"syndai_docs_m{multi_count:03d}",
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
    for rel in files:
        data = (root / rel).read_bytes()
        entries[rel] = {"sha256": gc.sha256_hex(data), "bytes": len(data)}
    return {
        "corpus": "syndai_product_docs",
        "syndai_root": str(root),
        "git_commit": gc.git_commit(root),
        "globs": list(gc.CORPUS_GLOBS),
        "excluded_prefixes": list(gc.EXCLUDE_PREFIXES),
        "exclusion_rationale": (
            "docs/superpowers/ is SDD process scaffolding (plans, audits, "
            "briefs, research), not product documentation; the gate corpus is "
            "the real product/architecture/spec docs."
        ),
        "file_count": len(files),
        "files": entries,
    }


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
    args = parser.parse_args()

    root = Path(args.syndai_root)
    files = gc.list_corpus_files(root)
    if not files:
        print("no corpus files found", file=sys.stderr)
        return 1

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

    cli = MinerCli(
        args.engine, args.model, args.model, Path(args.cache_dir), args.max_calls
    )
    goldens = mine(cli, single_candidates, multi_pairs, n_single, n_multi)

    gc.write_jsonl(GOLDEN_PATH, goldens)
    golden_bytes = GOLDEN_PATH.read_bytes()
    n_multi_out = sum(1 for row in goldens if row["multi_hop"])
    strata: dict[str, int] = {}
    for row in goldens:
        strata[row["question_type"]] = strata.get(row["question_type"], 0) + 1
    lock = {
        "golden_path": "benchmarks/data/syndai_docs_golden.jsonl",
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
    }
    GOLDEN_LOCK_PATH.write_text(json.dumps(lock, indent=2) + "\n")

    print(
        f"mined={len(goldens)} (single={len(goldens) - n_multi_out} "
        f"multi={n_multi_out}) target={args.target} "
        f"fresh_calls={cli.fresh_calls} cached_calls={cli.cached_calls} "
        f"sha256={lock['sha256'][:12]} out={GOLDEN_PATH}",
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

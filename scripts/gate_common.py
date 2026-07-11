#!/usr/bin/env python3
"""Shared corpus + provenance helpers for the Syndai replacement gate (W10).

Imported by the miner (``gate_mine_goldens.py``) and BOTH engine runners
(``gate_run_syndai.py``, ``gate_run_memphant.py``) so the corpus is sectionized
identically, the golden set is read identically, and provenance is graded
identically for both engines (the brief's requirement 4). ``gate_run_syndai.py``
runs under the Syndai backend's interpreter and imports this by inserting
``/Users/sidsharma/Memphant/scripts`` on ``sys.path`` — everything here is
stdlib-only (plus ``run_reader`` for containment, which is itself stdlib-only).

Ingest granularity decision (recorded here because both runners depend on it):
Syndai ingests each markdown FILE and its real sectionizer/chunker splits it;
MemPhant's resource channel does NOT auto-chunk (one resource body -> one
whole-document unit). To compare the two engines at the SAME section
granularity, the gate pre-splits the corpus into markdown sections with the one
parser below and ingests each section as one unit in EACH engine. Provenance is
then "does a retrieved item's body contain the gold span" — identical for both
engines, independent of either engine's internal provenance metadata.
"""

from __future__ import annotations

import hashlib
import importlib.util
import json
import re
import subprocess
from pathlib import Path

MEMPHANT_ROOT = Path(__file__).resolve().parents[1]

# The SDD process scaffolding (plans/audits/briefs/research) under
# docs/superpowers/ is not product documentation; the gate corpus is the real
# product/architecture/spec docs.
EXCLUDE_PREFIXES = ("docs/superpowers/",)
CORPUS_GLOBS = ("docs/*.md", "docs/**/*.md")

# A leaf section must have at least this much of its own content to seed a
# verbatim-span question (miner). The haystack (runner ingest) keeps every
# section with any non-empty content so no gold section is ever missing.
SECTION_MIN_CHARS = 240
SECTION_MAX_CHARS = 3200

HEADING_RE = re.compile(r"^(#{1,6})[ \t]+(.*?)[ \t]*#*[ \t]*$")


def _load_run_reader():
    spec = importlib.util.spec_from_file_location(
        "run_reader", MEMPHANT_ROOT / "scripts" / "run_reader.py"
    )
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(module)
    return module


_RUN_READER = _load_run_reader()
normalize = _RUN_READER.normalize
contains_gold = _RUN_READER.contains_gold


def sha256_hex(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def git_commit(root: Path) -> str:
    result = subprocess.run(
        ["git", "-C", str(root), "rev-parse", "HEAD"],
        capture_output=True,
        text=True,
    )
    return result.stdout.strip() if result.returncode == 0 else "unknown"


def list_corpus_files(root: Path) -> list[str]:
    """Sorted git-tracked corpus paths relative to root; tracked-only
    auto-excludes gitignored/generated files, superpowers dropped explicitly."""
    result = subprocess.run(
        ["git", "-C", str(root), "ls-files", *CORPUS_GLOBS],
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        raise RuntimeError(f"git ls-files failed in {root}: {result.stderr.strip()}")
    files = [
        line
        for line in result.stdout.splitlines()
        if line.strip()
        and not any(line.startswith(prefix) for prefix in EXCLUDE_PREFIXES)
    ]
    return sorted(set(files))


def bucket_of(rel_path: str) -> str:
    """Top-level docs directory bucket (docs/FOO.md -> 'root',
    docs/specs/... -> 'specs')."""
    parts = rel_path.split("/")
    if len(parts) <= 2:
        return "root"
    return parts[1]


def slugify(text: str) -> str:
    slug = re.sub(r"[^a-z0-9]+", "-", text.lower()).strip("-")
    return slug or "section"


class Section:
    __slots__ = ("rel_path", "bucket", "heading_path", "char_start", "char_end", "body")

    def __init__(self, rel_path, bucket, heading_path, char_start, char_end, body):
        self.rel_path = rel_path
        self.bucket = bucket
        self.heading_path = heading_path
        self.char_start = char_start
        self.char_end = char_end
        self.body = body

    @property
    def content(self) -> str:
        """Section text without the heading line itself."""
        newline = self.body.find("\n")
        return self.body[newline + 1 :] if newline != -1 else ""

    @property
    def content_char_start(self) -> int:
        newline = self.body.find("\n")
        return self.char_start + (newline + 1 if newline != -1 else 0)

    def key(self) -> str:
        return f"{self.rel_path}::{' > '.join(self.heading_path)}::{self.char_start}"

    def uri(self) -> str:
        """Stable per-section URI used as the MemPhant resource uri and the
        Syndai source name: doc://path#h1--h2--h3."""
        anchor = "--".join(slugify(h) for h in self.heading_path)
        return f"doc://{self.rel_path}#{anchor}::{self.char_start}"


def parse_sections(rel_path: str, text: str) -> list[Section]:
    """Split a markdown file into leaf sections: each heading plus its own text
    up to the next heading of ANY level. Records absolute char spans and the
    full ancestor heading breadcrumb. Fenced code blocks are respected so a
    ``#`` inside a fence is never read as a heading. Any preamble before the
    first heading is emitted as a headless '(preamble)' section so no corpus
    text is dropped from the haystack."""
    bucket = bucket_of(rel_path)
    lines = text.split("\n")
    offsets: list[int] = []
    running = 0
    for line in lines:
        offsets.append(running)
        running += len(line) + 1

    heading_stack: list[tuple[int, str]] = []
    sections: list[Section] = []
    in_fence = False
    fence_marker = ""
    pending_start_line: int | None = None
    pending_path: list[str] = []

    def emit(start_line: int, end_line: int, path: list[str]) -> None:
        char_start = offsets[start_line]
        char_end = offsets[end_line] if end_line < len(offsets) else len(text)
        body = text[char_start:char_end].rstrip("\n")
        if body.strip():
            sections.append(
                Section(rel_path, bucket, list(path), char_start, char_end, body)
            )

    # Preamble before the first heading.
    first_heading_line = None
    scan_fence = False
    scan_marker = ""
    for i, line in enumerate(lines):
        stripped = line.lstrip()
        if stripped.startswith("```") or stripped.startswith("~~~"):
            marker = stripped[:3]
            if not scan_fence:
                scan_fence, scan_marker = True, marker
            elif marker == scan_marker:
                scan_fence, scan_marker = False, ""
            continue
        if scan_fence:
            continue
        if HEADING_RE.match(line):
            first_heading_line = i
            break
    if first_heading_line is None:
        emit(0, len(lines), ["(preamble)"])
        return sections
    if first_heading_line > 0:
        emit(0, first_heading_line, ["(preamble)"])

    for i, line in enumerate(lines):
        stripped = line.lstrip()
        if stripped.startswith("```") or stripped.startswith("~~~"):
            marker = stripped[:3]
            if not in_fence:
                in_fence, fence_marker = True, marker
            elif marker == fence_marker:
                in_fence, fence_marker = False, ""
            continue
        if in_fence:
            continue
        match = HEADING_RE.match(line)
        if not match:
            continue
        level = len(match.group(1))
        title = match.group(2).strip()
        if pending_start_line is not None:
            emit(pending_start_line, i, pending_path)
        while heading_stack and heading_stack[-1][0] >= level:
            heading_stack.pop()
        heading_stack.append((level, title))
        pending_start_line = i
        pending_path = [t for _, t in heading_stack]

    if pending_start_line is not None:
        emit(pending_start_line, len(lines), pending_path)
    return sections


def all_sections(root: Path, files: list[str]) -> list[Section]:
    out: list[Section] = []
    for rel in files:
        text = (root / rel).read_text(encoding="utf-8", errors="replace")
        out.extend(parse_sections(rel, text))
    return out


def candidate_sections(sections: list[Section]) -> list[Section]:
    """Sections substantial enough to mine a verbatim-span question from."""
    out = []
    for section in sections:
        content = section.content.strip()
        if len(content) < SECTION_MIN_CHARS:
            continue
        if len(section.body) > SECTION_MAX_CHARS:
            continue
        out.append(section)
    return out


# --- golden set + provenance -------------------------------------------------


def load_goldens(path: Path) -> list[dict]:
    return [
        json.loads(line)
        for line in path.read_text().split("\n")
        if line.strip()
    ]


def required_spans(golden: dict) -> list[str]:
    """The verbatim spans a retrieval engine must surface for this question:
    the single answer span, or (multi-hop) the bridge span AND the answer
    span."""
    return [entry["span"] for entry in golden["provenance"]]


def provenance_hit(golden: dict, evidence_bodies: list[str], k: int) -> bool:
    """Span-level provenance hit within the top-k retrieved bodies, graded
    identically for every engine: a required span counts as covered if ANY
    top-k body contains it (normalized word-boundary containment, the same
    function run_reader uses to grade answers). Single-hop hits when its one
    span is covered; multi-hop hits only when BOTH required spans are covered
    across the union of the top-k bodies."""
    top = evidence_bodies[:k]
    for span in required_spans(golden):
        if not any(contains_gold(body, span) for body in top):
            return False
    return True


def evidence_row(golden: dict, evidence_bodies: list[str], k: int) -> dict:
    """Assemble the run_reader.py evidence-row for a golden + retrieved bodies.
    Matches the QaEvidenceRow contract that bench_lme emits (question_id,
    question_type, is_abstention, question, question_date, gold_answer,
    abstained, granularity, k, evidence[{rank, session_id, body}])."""
    items = [
        {"rank": rank + 1, "session_id": None, "body": body}
        for rank, body in enumerate(evidence_bodies[:k])
    ]
    return {
        "question_id": golden["question_id"],
        "question_type": golden["question_type"],
        "is_abstention": golden["is_abstention"],
        "question": golden["question"],
        "question_date": golden.get("question_date"),
        "gold_answer": golden["gold_answer"],
        "abstained": len(items) == 0,
        "granularity": "section",
        "k": k,
        "evidence": items,
    }


def write_jsonl(path: Path, rows: list[dict]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    lines = [json.dumps(row, ensure_ascii=False) for row in rows]
    path.write_text("\n".join(lines) + ("\n" if lines else ""))

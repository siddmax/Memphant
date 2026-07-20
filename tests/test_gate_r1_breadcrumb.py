"""Unit tests for the R1-T1 breadcrumb lever (docs-gate harness) and the
Syndai v2 golden-set lock-path fix.

Breadcrumb: ``gate_run_memphant.py --breadcrumb`` prefixes each ingested
section body with Syndai's deterministic context-prefix convention, exactly
as implemented at ``/Users/sidsharma/Syndai/backend/src/features/knowledge/
processing_chunks.py:84`` (``_deterministic_context_prefix``):

    if not chunk.heading_hierarchy:
        return None
    return "Section path: " + " > ".join(chunk.heading_hierarchy)

(then joined to the body with a blank line by the caller). ``gate_common.
breadcrumb_prefix`` mirrors that exact truthiness check and string shape.

All pure functions here, no network, no DB, no Syndai backend dependency (the
real ``gate_run_syndai.py`` module imports the Syndai backend at MODULE level
and can't be loaded under this repo's plain ``pytest tests/`` — its fix is
pinned via a source-text check instead of an import, same constraint the
existing ``test_syndai_gate_contract.py`` already works around).
"""

from __future__ import annotations

import hashlib
import importlib.util
import json
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[1]


def _load(name: str, rel: str):
    spec = importlib.util.spec_from_file_location(name, ROOT / rel)
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(module)
    return module


@pytest.fixture(scope="module")
def gc():
    return _load("gate_common", "scripts/gate_common.py")


@pytest.fixture(scope="module")
def gr():
    return _load("gate_run_memphant", "scripts/gate_run_memphant.py")


class FakeApiClient:
    """Records the payload of the last POST; stands in for gate_runtime's
    real ApiClient (no HTTP)."""

    def __init__(self):
        self.tenant_id = "11111111-1111-4111-8111-111111111111"
        self.posts: list[tuple[str, dict]] = []

    def post(self, path: str, payload: dict) -> dict:
        self.posts.append((path, payload))
        return {"resource_id": "res_fake"}


# --- gc.golden_lock_path ------------------------------------------------


def test_golden_lock_path_derives_v1_lock_from_v1_golden(gc):
    golden = ROOT / "benchmarks" / "data" / "syndai_docs_golden.jsonl"
    assert gc.golden_lock_path(golden) == ROOT / "benchmarks" / "data" / "syndai_docs_golden.lock.json"


def test_golden_lock_path_derives_v2_lock_from_v2_golden(gc):
    golden = ROOT / "benchmarks" / "data" / "syndai_docs_golden_v2.jsonl"
    assert gc.golden_lock_path(golden) == ROOT / "benchmarks" / "data" / "syndai_docs_golden_v2.lock.json"


def test_golden_lock_path_generalizes_to_any_stem(gc, tmp_path):
    golden = tmp_path / "some_future_golden_v7.jsonl"
    assert gc.golden_lock_path(golden) == tmp_path / "some_future_golden_v7.lock.json"


# --- gc.breadcrumb_prefix ------------------------------------------------


def test_breadcrumb_prefix_exact_string_for_nonempty_heading_path(gc):
    # Byte-identical to Syndai's "Section path: " + " > ".join(...) + the
    # "\n\n" join separator _build_embedding_texts applies between prefix and
    # content.
    assert gc.breadcrumb_prefix(["Firebase Test Lab Blaze", "Cost model"]) == (
        "Section path: Firebase Test Lab Blaze > Cost model\n\n"
    )


def test_breadcrumb_prefix_single_heading(gc):
    assert gc.breadcrumb_prefix(["Overview"]) == "Section path: Overview\n\n"


def test_breadcrumb_prefix_empty_heading_path_prepends_nothing(gc):
    """Mirrors Syndai's `if not chunk.heading_hierarchy: return None` (no
    prefix at all, not even a bare "Section path: "\\n\\n)."""
    assert gc.breadcrumb_prefix([]) == ""


# --- Section.heading_path is never actually empty in this harness --------


def test_parse_sections_preamble_heading_path_is_a_nonempty_sentinel_not_empty(gc):
    """The empty-heading-path finding: gate_common's own sectionizer never
    emits a literal `[]` heading_path. Headerless content before the first
    heading (or a headingless file entirely) gets the placeholder
    `["(preamble)"]` instead (so no corpus text is ever dropped from the
    haystack) — so gc.breadcrumb_prefix's `not heading_path` branch is
    unreachable from a real Section, and preamble sections DO get a
    "Section path: (preamble)" breadcrumb under --breadcrumb, since Syndai's
    own check is a plain truthiness test on the list, not a check against any
    particular sentinel string."""
    sections = gc.parse_sections("docs/x.md", "no heading here, just text.\n")
    assert len(sections) == 1
    assert sections[0].heading_path == ["(preamble)"]
    assert gc.breadcrumb_prefix(sections[0].heading_path) == "Section path: (preamble)\n\n"


# --- gr.ingest_section ----------------------------------------------------


def _make_section(gc, heading_path, body="Some section body text."):
    return gc.Section("docs/x.md", "root", heading_path, 0, len(body), body)


def test_ingest_section_breadcrumb_true_prefixes_body(gc, gr):
    section = _make_section(gc, ["Firebase Test Lab Blaze", "Cost model"])
    client = FakeApiClient()
    gr.ingest_section(client, section, breadcrumb=True)
    assert len(client.posts) == 1
    _, payload = client.posts[0]
    expected_body = "Section path: Firebase Test Lab Blaze > Cost model\n\n" + section.body
    assert payload["resource"]["body"] == expected_body
    assert payload["resource"]["content_hash"] == "sha256:" + hashlib.sha256(expected_body.encode()).hexdigest()


def test_ingest_section_breadcrumb_false_leaves_body_verbatim(gc, gr):
    section = _make_section(gc, ["Firebase Test Lab Blaze", "Cost model"])
    client = FakeApiClient()
    gr.ingest_section(client, section, breadcrumb=False)
    _, payload = client.posts[0]
    assert payload["resource"]["body"] == section.body


def test_ingest_section_defaults_to_no_breadcrumb(gc, gr):
    """breadcrumb defaults False so existing call sites without the flag are
    unaffected."""
    section = _make_section(gc, ["Heading"])
    client = FakeApiClient()
    gr.ingest_section(client, section)
    _, payload = client.posts[0]
    assert payload["resource"]["body"] == section.body


def test_ingest_section_breadcrumb_true_with_truly_empty_heading_path_prepends_nothing(gc, gr):
    section = _make_section(gc, [])
    client = FakeApiClient()
    gr.ingest_section(client, section, breadcrumb=True)
    _, payload = client.posts[0]
    assert payload["resource"]["body"] == section.body


def test_ingest_section_breadcrumb_true_with_preamble_sentinel_gets_prefixed(gc, gr):
    section = _make_section(gc, ["(preamble)"])
    client = FakeApiClient()
    gr.ingest_section(client, section, breadcrumb=True)
    _, payload = client.posts[0]
    assert payload["resource"]["body"] == "Section path: (preamble)\n\n" + section.body


def test_ingest_section_uri_unaffected_by_breadcrumb(gc, gr):
    """uri() is derived from heading_path/rel_path/char_start, never body —
    must be identical with breadcrumb on or off."""
    section = _make_section(gc, ["Heading"])
    client = FakeApiClient()
    gr.ingest_section(client, section, breadcrumb=True)
    uri_on = client.posts[0][1]["resource"]["uri"]
    client2 = FakeApiClient()
    gr.ingest_section(client2, section, breadcrumb=False)
    uri_off = client2.posts[0][1]["resource"]["uri"]
    assert uri_on == uri_off == section.uri()


# --- provenance grading is unaffected by the breadcrumb prefix -----------


def test_provenance_hit_unaffected_by_breadcrumb_prefix(gc):
    """The IMPORTANT grading consideration from the brief: the prefix ADDS
    text in front of the body, never removes any — so a golden span that
    would hit against the raw body must still hit against the breadcrumbed
    body."""
    golden = {
        "multi_hop": False,
        "provenance": [{"role": "answer", "span": "Stripe and Payoneer"}],
    }
    raw_body = "MemPhant uses Stripe and Payoneer for payouts."
    breadcrumbed_body = gc.breadcrumb_prefix(["Billing", "Payout providers"]) + raw_body
    assert gc.provenance_hit(golden, [raw_body], 10)
    assert gc.provenance_hit(golden, [breadcrumbed_body], 10)


# --- provenance-header records the breadcrumb flag ------------------------


def test_build_provenance_report_records_breadcrumb_true(gr):
    report = gr.build_provenance_report(
        embed_model="small",
        label="r1-docs-small+bc",
        breadcrumb=True,
        golden_path=Path("benchmarks/data/syndai_docs_golden.jsonl"),
        database_url="postgres://memphant:memphant@localhost:5432/memphant_scratch_1_2",
        k=10,
        mode="deep",
        budget_tokens=8192,
        haystack_len=42,
        golden_sha="deadbeef",
        provenance_rows=[],
    )
    assert report["breadcrumb"] is True
    assert report["golden_count"] == 0
    assert report["recall_at_5"] == 0.0
    assert report["recall_at_10"] == 0.0


def test_build_provenance_report_records_breadcrumb_false(gr):
    report = gr.build_provenance_report(
        embed_model="small",
        label="r1-docs-small",
        breadcrumb=False,
        golden_path=Path("benchmarks/data/syndai_docs_golden.jsonl"),
        database_url="postgres://memphant:memphant@localhost:5432/memphant_scratch_1_2",
        k=10,
        mode="deep",
        budget_tokens=8192,
        haystack_len=42,
        golden_sha="deadbeef",
        corpus_revision_id="sha256:test-corpus",
        expected_n=2,
        provenance_rows=[
            {
                "hit_at_5": True,
                "hit_at_10": True,
                "degraded": False,
                "fallback": False,
                "skipped": False,
                "recall_e2e_ms": 100,
            },
            {
                "hit_at_5": False,
                "hit_at_10": True,
                "degraded": False,
                "fallback": False,
                "skipped": False,
                "recall_e2e_ms": 200,
            },
        ],
    )
    assert report["breadcrumb"] is False
    assert report["golden_count"] == 2
    assert report["recall_at_5"] == pytest.approx(0.5)
    assert report["recall_at_10"] == pytest.approx(1.0)


# --- --breadcrumb argparse wiring -----------------------------------------


def test_breadcrumb_flag_defaults_false(gr):
    parser = gr.build_arg_parser()
    args = parser.parse_args(["--out-evidence", "e.jsonl", "--out-provenance", "p.json"])
    assert args.breadcrumb is False


def test_breadcrumb_flag_settable(gr):
    parser = gr.build_arg_parser()
    args = parser.parse_args(
        ["--breadcrumb", "--out-evidence", "e.jsonl", "--out-provenance", "p.json"]
    )
    assert args.breadcrumb is True


# --- gate_run_syndai.py lock-path fix (source-pinned; module needs the ----
# --- Syndai backend venv to import, so it can't be dynamically loaded here)


def test_gate_run_syndai_derives_lock_path_from_golden_not_a_hardcoded_v1_constant():
    source = (ROOT / "scripts" / "gate_run_syndai.py").read_text()
    assert "GOLDEN_LOCK_PATH" not in source, (
        "gate_run_syndai.py must derive the golden lock path from --golden "
        "via gc.golden_lock_path, not a hardcoded v1-only module constant "
        "(a --golden syndai_docs_golden_v2.jsonl run would otherwise verify "
        "against the WRONG (v1) lock file)"
    )
    assert "gc.golden_lock_path(" in source


def test_gate_run_syndai_lock_derivation_matches_shared_helper(gc):
    """Same assertion as the memphant runner's contract: whatever path
    gate_run_syndai.py's --golden points at, the lock file it verifies
    against is the shared gc.golden_lock_path(...) derivation."""
    v2_golden = ROOT / "benchmarks" / "data" / "syndai_docs_golden_v2.jsonl"
    lock_path = gc.golden_lock_path(v2_golden)
    assert lock_path.name == "syndai_docs_golden_v2.lock.json"
    assert json.loads(lock_path.read_text())["sha256"]

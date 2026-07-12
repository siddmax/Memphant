"""Unit tests for the R1.5-T1 ``--cross-rerank`` lever (docs-gate harness).

``gate_run_memphant.py --cross-rerank`` sets ``MEMPHANT_CROSS_RERANK=1`` for
BOTH the server and worker subprocess env (so recall reorders the top
``recall_pool_depth`` fused candidates via the W8 cross-encoder,
``bge-reranker-base``, before packing), and records ``cross_rerank`` in the
self-describing provenance header. Mirrors
``test_gate_r1_resource_chunks.py`` exactly — this is a DIFFERENT, distinctly
named lever from the retired heuristic rerank (never exposed by this script).

All pure functions here (argparse + header assembly): no network, no DB, no
subprocess spawn — same constraint as ``test_gate_r1_resource_chunks.py``.
"""

from __future__ import annotations

import importlib.util
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
def gr():
    return _load("gate_run_memphant", "scripts/gate_run_memphant.py")


# --- --cross-rerank argparse wiring ----------------------------------------


def test_cross_rerank_flag_defaults_false(gr):
    parser = gr.build_arg_parser()
    args = parser.parse_args(["--out-evidence", "e.jsonl", "--out-provenance", "p.json"])
    assert args.cross_rerank is False


def test_cross_rerank_flag_sets_true(gr):
    parser = gr.build_arg_parser()
    args = parser.parse_args(
        ["--out-evidence", "e.jsonl", "--out-provenance", "p.json", "--cross-rerank"]
    )
    assert args.cross_rerank is True


# --- server / worker env pass-through --------------------------------------


def test_server_carries_cross_rerank_flag(gr):
    server = gr.Server("srv", "postgres://x/db", 39412, cross_rerank=True)
    assert server.cross_rerank is True
    default = gr.Server("srv", "postgres://x/db", 39412)
    assert default.cross_rerank is False


# --- provenance-header records the cross_rerank flag -----------------------


def test_build_provenance_report_records_cross_rerank_true(gr):
    report = gr.build_provenance_report(
        embed_model="small",
        label="r15-docs-small+xr",
        breadcrumb=False,
        resource_chunks=False,
        cross_rerank=True,
        golden_path=Path("benchmarks/data/syndai_docs_golden.jsonl"),
        database_url="postgres://memphant:memphant@localhost:5432/memphant_scratch_1_2",
        k=10,
        mode="exhaustive",
        budget_tokens=8192,
        haystack_len=42,
        golden_sha="deadbeef",
        provenance_rows=[],
    )
    assert report["cross_rerank"] is True
    # The independently-set levers are still recorded independently.
    assert report["breadcrumb"] is False
    assert report["resource_chunks"] is False


def test_build_provenance_report_defaults_cross_rerank_false(gr):
    report = gr.build_provenance_report(
        embed_model="small",
        label="r15-docs-small",
        breadcrumb=False,
        golden_path=Path("benchmarks/data/syndai_docs_golden.jsonl"),
        database_url="postgres://memphant:memphant@localhost:5432/memphant_scratch_1_2",
        k=10,
        mode="exhaustive",
        budget_tokens=8192,
        haystack_len=42,
        golden_sha="deadbeef",
        provenance_rows=[],
    )
    assert report["cross_rerank"] is False

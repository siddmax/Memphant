"""Unit tests for the R1-T3 ``--resource-chunks`` lever (docs-gate harness).

``gate_run_memphant.py --resource-chunks`` sets ``MEMPHANT_RESOURCE_CHUNKS=1``
for BOTH the server and worker subprocess env (so reflect mints per-resource
contextual chunks for ``kind=document`` sections — the flag-gated, default-off
twin of the promoted episode chunks), and records ``resource_chunks`` in the
self-describing provenance header.

All pure functions here (argparse + header assembly): no network, no DB, no
subprocess spawn — same constraint as ``test_gate_r1_breadcrumb.py``.
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


# --- --resource-chunks argparse wiring ------------------------------------


def test_resource_chunks_flag_defaults_false(gr):
    parser = gr.build_arg_parser()
    args = parser.parse_args(["--out-evidence", "e.jsonl", "--out-provenance", "p.json"])
    assert args.resource_chunks is False


def test_resource_chunks_flag_sets_true(gr):
    parser = gr.build_arg_parser()
    args = parser.parse_args(
        ["--out-evidence", "e.jsonl", "--out-provenance", "p.json", "--resource-chunks"]
    )
    assert args.resource_chunks is True


# --- server / worker env pass-through -------------------------------------


def test_server_carries_resource_chunks_flag(gr):
    server = gr.Server("srv", "postgres://x/db", 39412, resource_chunks=True)
    assert server.resource_chunks is True
    default = gr.Server("srv", "postgres://x/db", 39412)
    assert default.resource_chunks is False


# --- provenance-header records the resource_chunks flag -------------------


def test_build_provenance_report_records_resource_chunks_true(gr):
    report = gr.build_provenance_report(
        embed_model="small",
        label="r1-docs-small+rc",
        breadcrumb=False,
        resource_chunks=True,
        golden_path=Path("benchmarks/data/syndai_docs_golden.jsonl"),
        database_url="postgres://memphant:memphant@localhost:5432/memphant_scratch_1_2",
        k=10,
        mode="deep",
        budget_tokens=8192,
        haystack_len=42,
        golden_sha="deadbeef",
        provenance_rows=[],
    )
    assert report["resource_chunks"] is True
    # The R1-T1 breadcrumb field is still recorded independently.
    assert report["breadcrumb"] is False


def test_build_provenance_report_defaults_resource_chunks_false(gr):
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
        provenance_rows=[],
    )
    assert report["resource_chunks"] is False

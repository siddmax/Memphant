"""Contract tests for the shared gate-runner runtime module
(``scripts/gate_runtime.py``), centered on the API-arm key map: the map is
the SINGLE source of truth for every gate runner (per-script copies drifted
once — voyage-4-large landed in ``gate_run_memphant.py``'s copy but not
``code_lane_run_memphant.py``'s, silently disabling that arm's fail-fast),
so these tests pin (1) the map's keys against the API-arm ids accepted by
the Rust ``embedder_from_id`` grammar — both as an explicit constant and by
scraping the grammar's match arms out of the Rust source, so the NEXT arm
added to the grammar fails a test here until the map learns it — and
(2) that both runner scripts genuinely share the one map object rather than
carrying copies. Also pins ``reexec_through_scratch_db`` — the shared scratch-DB
isolation linchpin both runners call — since a broken recursion guard or a
reordered helper invocation would silently un-isolate every bench run (or
infinite-loop) with nothing else catching it. No DB, no network, no server
process.
"""

from __future__ import annotations

import importlib.util
import re
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[1]
RUST_GRAMMAR = ROOT / "crates" / "memphant-runtime" / "src" / "lib.rs"

# The API-arm ids accepted by `embedder_from_id`
# (crates/memphant-runtime/src/lib.rs) — the `"<id>" => api(...)` match arms.
# Local arms (small/base/modernbert/gemma/qwen3) and off/noop need no key and
# are deliberately NOT in the key map.
EXPECTED_API_ARMS = {
    "voyage-4",
    "voyage-4-lite",
    "voyage-4-large",
    "voyage-code-3",
    "voyage-context-4",
    "gemini-embedding-001",
    "openai-text-embedding-3-small",
}

# `"<id>" => api(` — the exact shape of an API-arm match arm in
# embedder_from_id (local arms route through `local(...)`/`Ok(...)` instead).
RUST_API_ARM_RE = re.compile(r'"([a-z0-9.-]+)"\s*=>\s*api\(')


def _load(name: str, rel: str):
    spec = importlib.util.spec_from_file_location(name, ROOT / rel)
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(module)
    return module


@pytest.fixture(scope="module")
def grt():
    return _load("gate_runtime", "scripts/gate_runtime.py")


def test_api_key_map_covers_exactly_the_expected_api_arms(grt):
    assert set(grt.API_KEY_ENV_BY_ARM) == EXPECTED_API_ARMS


def test_expected_api_arms_match_the_rust_grammar_source():
    """Scrapes the `\"<id>\" => api(...)` match arms out of embedder_from_id's
    Rust source, so adding an API arm to the grammar without extending
    API_KEY_ENV_BY_ARM (or this pin) fails here instead of silently
    no-op'ing the runners' fail-fast key check."""
    source = RUST_GRAMMAR.read_text(encoding="utf-8")
    scraped = set(RUST_API_ARM_RE.findall(source))
    assert scraped == EXPECTED_API_ARMS, (
        "Rust embedder_from_id API arms drifted from the pinned set — "
        "update EXPECTED_API_ARMS and gate_runtime.API_KEY_ENV_BY_ARM together"
    )


def test_every_arm_maps_to_a_provider_key_env_var(grt):
    for arm, var in grt.API_KEY_ENV_BY_ARM.items():
        assert var.endswith("_API_KEY"), f"{arm}: unexpected env var name {var!r}"


def test_both_runner_scripts_share_the_one_map_object():
    """The centralization pin: gate_run_memphant.py and
    code_lane_run_memphant.py must expose the SAME dict object imported from
    gate_runtime — not per-script copies (which is exactly how the
    voyage-4-large drift happened)."""
    docs_runner = _load("gate_run_memphant", "scripts/gate_run_memphant.py")
    code_runner = _load("code_lane_run_memphant", "scripts/code_lane_run_memphant.py")
    assert docs_runner.API_KEY_ENV_BY_ARM is code_runner.gr.API_KEY_ENV_BY_ARM


def test_check_embed_model_key_noop_for_none_and_local_arms(grt, monkeypatch):
    monkeypatch.delenv("VOYAGE_API_KEY", raising=False)
    grt.check_embed_model_key(None)  # must not raise
    grt.check_embed_model_key("small")  # local arm, must not raise
    grt.check_embed_model_key("qwen3")  # local arm, must not raise


@pytest.mark.parametrize(
    "arm,var",
    [
        ("voyage-4-large", "VOYAGE_API_KEY"),
        ("gemini-embedding-001", "GEMINI_API_KEY"),
        ("openai-text-embedding-3-small", "OPENAI_API_KEY"),
    ],
)
def test_check_embed_model_key_fails_fast_when_key_missing(grt, monkeypatch, arm, var):
    monkeypatch.delenv(var, raising=False)
    with pytest.raises(RuntimeError, match=re.escape(f"--embed-model {arm}: {var} is not set")):
        grt.check_embed_model_key(arm)


def test_check_embed_model_key_passes_when_key_present(grt, monkeypatch):
    monkeypatch.setenv("VOYAGE_API_KEY", "test-key")
    grt.check_embed_model_key("voyage-4-large")  # must not raise


def test_check_embed_model_key_rejects_whitespace_only_key(grt, monkeypatch):
    monkeypatch.setenv("VOYAGE_API_KEY", "   ")
    with pytest.raises(RuntimeError, match="VOYAGE_API_KEY is not set"):
        grt.check_embed_model_key("voyage-4")


def test_reexec_through_scratch_db_is_noop_when_already_active(grt, monkeypatch):
    """The recursion guard: inside a scratch DB (``MEMPHANT_SCRATCH_ACTIVE``
    set), the runner must NOT re-exec again — else `with_scratch_db.sh` nests
    forever, minting a DB per level."""
    monkeypatch.setenv("MEMPHANT_SCRATCH_ACTIVE", "1")
    monkeypatch.setattr(grt.os, "execvp", lambda *a: pytest.fail(f"re-exec'd despite guard: {a}"))
    grt.reexec_through_scratch_db("postgres://memphant:memphant@localhost:5432/memphant")


def test_reexec_through_scratch_db_execs_through_helper(grt, monkeypatch):
    """First entry re-execs the CURRENT argv through with_scratch_db.sh with
    ENV_VAR=DATABASE_URL and the guard set, so the child mints/migrates/drops a
    fresh DB and the runner continues against it. Pins the exact argv order the
    helper's ``<base_url> <ENV_VAR> <cmd...>`` contract requires."""
    monkeypatch.delenv("MEMPHANT_SCRATCH_ACTIVE", raising=False)
    monkeypatch.setattr(grt.sys, "argv", ["scripts/gate_run_memphant.py", "--label", "x"])
    captured = {}

    def fake_execvp(file, argv):
        captured["file"], captured["argv"] = file, argv
        raise _Execed  # os.execvp never returns; stop the function here

    monkeypatch.setattr(grt.os, "execvp", fake_execvp)
    with pytest.raises(_Execed):
        grt.reexec_through_scratch_db("postgres://memphant:memphant@localhost:5432/memphant")

    assert captured["file"] == "bash"
    argv = captured["argv"]
    assert argv[0] == "bash"
    assert argv[1] == str(ROOT / "scripts" / "with_scratch_db.sh")
    assert argv[2] == "postgres://memphant:memphant@localhost:5432/memphant"  # base url
    assert argv[3] == "DATABASE_URL"  # env var the scratch url lands in
    assert argv[4] == grt.sys.executable
    assert argv[5:] == ["scripts/gate_run_memphant.py", "--label", "x"]  # current argv, verbatim
    # Guard set BEFORE exec so the re-exec'd child sees it and doesn't recurse.
    assert grt.os.environ["MEMPHANT_SCRATCH_ACTIVE"] == "1"


class _Execed(Exception):
    """Sentinel: stands in for os.execvp replacing the process (never returns)."""

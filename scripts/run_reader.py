#!/usr/bin/env python3
"""Reader-scored QA lane over bench-lme evidence JSONL.

Input: the ``--emit-qa`` JSONL written by ``memphant-eval bench-lme`` (one row
per question: question, question_date, gold answer, top-k evidence bodies with
provenance). This script drives an external reader and judge through a
headless CLI engine and writes a labeled QA report.

Engines (``--engine``):
- ``claude`` (default): ``claude -p`` headless, no tools, no session
  persistence (the original lane).
- ``codex``: ``codex exec - -m <model> -s read-only --ephemeral
  --skip-git-repo-check --ignore-user-config -o <file>`` with the prompt on
  stdin; only the agent's final message is read (``-o``), so any tool use is
  stripped by construction, and the read-only sandbox plus an explicit
  "answer directly, no commands" instruction suppress it at the source.
- ``openrouter``: direct HTTPS POST to
  ``https://openrouter.ai/api/v1/chat/completions`` (no CLI, no quota tied to
  a coding-agent subscription). ``--model``/``--judge-model`` must be full
  OpenRouter model ids (e.g. ``openai/gpt-5.6-terra``,
  ``anthropic/claude-sonnet-5``). Requires ``OPENROUTER_API_KEY`` in the
  environment (never read from a flag, never printed, never persisted); run
  via Doppler so the key stays out of shell history and process args:
  ``doppler run --project syndai --config dev -- python3 scripts/run_reader.py
  --engine openrouter ...``.

``--judge-model`` lets the judge use a different (stronger) model than the
reader; both model ids and the engine are recorded in the report header.

Honesty contract:
- engine, reader model and judge method are recorded in the output header;
- containment judging runs first (gold answer normalized-contained in the
  reply); only non-matches spend one LLM judge call;
- abstention questions (``_abs`` in the question id) score correct only when
  the reader replies "I don't know" (normalized containment);
- a hard call budget aborts with partial results recorded (n is explicit);
- every reply is cached by sha256(engine + model + kind + prompt) so reruns
  and identical evidence packs across runs never re-spend budget (legacy
  pre-engine cache entries stay readable for the claude engine).

This script never fabricates: any CLI failure is recorded per question as
``reader_error`` and that question is excluded from accuracy (n_scored drops).
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import random
import re
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request
from pathlib import Path

DEFAULT_MODEL = "claude-haiku-4-5-20251001"
ENGINES = ("claude", "codex", "openrouter")
OPENROUTER_URL = "https://openrouter.ai/api/v1/chat/completions"
OPENROUTER_TIMEOUT = 180
OPENROUTER_RETRY_DELAYS = (2, 8, 30)  # 4 tries total: 3 backoff sleeps between them
# The codex engine has no separate system-prompt channel; the system prompt is
# prepended to the user prompt with this no-tool-use guard.
CODEX_NO_TOOLS_GUARD = (
    "Do not run any commands or use any tools; answer directly from this "
    "prompt in your final message."
)
READER_SYSTEM_PROMPT = (
    "You answer questions using ONLY the evidence provided in the prompt. "
    "Be terse: reply with the answer itself, a short phrase, no preamble. "
    "If the evidence is insufficient to answer, reply exactly: I don't know."
)
JUDGE_SYSTEM_PROMPT = (
    "You are a strict grader. Reply with exactly one word: yes or no."
)
BOOTSTRAP_RESAMPLES = 1000


def normalize(text: str) -> str:
    """Lowercase, strip punctuation, collapse whitespace."""
    text = text.lower()
    text = re.sub(r"[^\w\s]", " ", text)
    return re.sub(r"\s+", " ", text).strip()


def contains_gold(reply: str, gold: str) -> bool:
    """Word-boundary containment: short numeric golds (e.g. "2") must appear
    as whole tokens in the reply, never inside another token (e.g. "32")."""
    gold_norm = normalize(gold)
    if not gold_norm:
        return False
    pattern = r"(?<!\w)" + re.escape(gold_norm) + r"(?!\w)"
    return re.search(pattern, normalize(reply)) is not None


def is_abstention_reply(reply: str) -> bool:
    return "i don t know" in normalize(reply)


class CallBudgetExceeded(Exception):
    pass


class ReaderCli:
    """Serialized, cached headless CLI calls with a hard budget shared across
    reader and judge (which may use different models on the same engine)."""

    def __init__(
        self,
        engine: str,
        model: str,
        judge_model: str,
        cache_dir: Path,
        max_calls: int,
        reasoning_effort: str | None = None,
    ) -> None:
        if engine not in ENGINES:
            raise ValueError(f"unknown engine: {engine} (known: {ENGINES})")
        if reasoning_effort is not None and engine not in ("codex", "openrouter"):
            raise ValueError("--reasoning-effort is codex/openrouter-only")
        self.engine = engine
        self.model = model
        self.judge_model = judge_model
        self.reasoning_effort = reasoning_effort
        self.cache_dir = cache_dir
        self.max_calls = max_calls
        self.fresh_calls = 0
        self.cached_calls = 0
        self._openrouter_api_key = None
        if engine == "openrouter":
            api_key = os.environ.get("OPENROUTER_API_KEY")
            if not api_key:
                raise RuntimeError(
                    "--engine openrouter requires OPENROUTER_API_KEY in the "
                    "environment; run via: doppler run --project syndai "
                    "--config dev -- python3 scripts/run_reader.py --engine "
                    "openrouter ..."
                )
            self._openrouter_api_key = api_key
        cache_dir.mkdir(parents=True, exist_ok=True)

    def model_for(self, kind: str) -> str:
        return self.judge_model if kind == "judge" else self.model

    def cache_model_for(self, kind: str) -> str:
        """Cache identity of the model: reasoning effort changes replies, so
        it is part of the key (None = the engine's configured default)."""
        model = self.model_for(kind)
        if self.reasoning_effort is not None:
            return f"{model}@{self.reasoning_effort}"
        return model

    def _cache_path(self, kind: str, system_prompt: str, prompt: str) -> Path:
        key = hashlib.sha256(
            "\x1e".join(
                [
                    self.engine,
                    self.cache_model_for(kind),
                    kind,
                    system_prompt,
                    prompt,
                ]
            ).encode()
        ).hexdigest()
        return self.cache_dir / f"{key}.json"

    def _legacy_cache_path(self, kind: str, system_prompt: str, prompt: str) -> Path:
        """Pre-engine cache key (claude engine only): sha256(model+kind+prompts)."""
        key = hashlib.sha256(
            "\x1e".join(
                [self.model_for(kind), kind, system_prompt, prompt]
            ).encode()
        ).hexdigest()
        return self.cache_dir / f"{key}.json"

    def call(self, kind: str, system_prompt: str, prompt: str) -> str:
        cache_path = self._cache_path(kind, system_prompt, prompt)
        if cache_path.exists():
            self.cached_calls += 1
            return json.loads(cache_path.read_text())["reply"]
        if self.engine == "claude":
            legacy = self._legacy_cache_path(kind, system_prompt, prompt)
            if legacy.exists():
                self.cached_calls += 1
                return json.loads(legacy.read_text())["reply"]
        if self.fresh_calls >= self.max_calls:
            raise CallBudgetExceeded(
                f"{self.engine} CLI call budget exhausted ({self.max_calls})"
            )
        self.fresh_calls += 1
        if self.engine == "claude":
            reply = self._call_claude(kind, system_prompt, prompt)
        elif self.engine == "codex":
            reply = self._call_codex(kind, system_prompt, prompt)
        else:
            reply = self._call_openrouter(kind, system_prompt, prompt)
        cache_path.write_text(
            json.dumps({"kind": kind, "prompt": prompt, "reply": reply}) + "\n"
        )
        return reply

    def _call_claude(self, kind: str, system_prompt: str, prompt: str) -> str:
        result = subprocess.run(
            [
                "claude",
                "-p",
                prompt,
                "--model",
                self.model_for(kind),
                "--system-prompt",
                system_prompt,
                "--tools",
                "",
                "--no-session-persistence",
                "--setting-sources",
                "",
            ],
            capture_output=True,
            text=True,
            timeout=180,
        )
        if result.returncode != 0:
            raise RuntimeError(
                f"claude -p failed (exit {result.returncode}): "
                f"{result.stderr.strip()[:500]}"
            )
        return result.stdout.strip()

    def _call_codex(self, kind: str, system_prompt: str, prompt: str) -> str:
        full_prompt = f"Instructions: {system_prompt} {CODEX_NO_TOOLS_GUARD}\n\n{prompt}"
        effort_args = (
            ["-c", f'model_reasoning_effort="{self.reasoning_effort}"']
            if self.reasoning_effort is not None
            else []
        )
        with tempfile.NamedTemporaryFile(
            mode="r", suffix=".txt", prefix="codex-last-msg-"
        ) as last_message:
            result = subprocess.run(
                [
                    "codex",
                    "exec",
                    "-",
                    "-m",
                    self.model_for(kind),
                    *effort_args,
                    "-s",
                    "read-only",
                    "--ephemeral",
                    "--skip-git-repo-check",
                    "--ignore-user-config",
                    "--color",
                    "never",
                    "-o",
                    last_message.name,
                ],
                input=full_prompt,
                capture_output=True,
                text=True,
                timeout=300,
            )
            if result.returncode != 0:
                raise RuntimeError(
                    f"codex exec failed (exit {result.returncode}): "
                    f"{result.stderr.strip()[:500]}"
                )
            reply = Path(last_message.name).read_text().strip()
        if not reply:
            raise RuntimeError("codex exec returned an empty final message")
        return reply

    def _call_openrouter(self, kind: str, system_prompt: str, prompt: str) -> str:
        payload = {
            "model": self.model_for(kind),
            "messages": [
                {"role": "system", "content": system_prompt},
                {"role": "user", "content": prompt},
            ],
            "temperature": 0,
            "max_tokens": 1024,
        }
        if self.reasoning_effort is not None:
            payload["reasoning"] = {"effort": self.reasoning_effort}
        body = json.dumps(payload).encode()
        request = urllib.request.Request(
            OPENROUTER_URL,
            data=body,
            method="POST",
            headers={
                "Authorization": f"Bearer {self._openrouter_api_key}",
                "Content-Type": "application/json",
                "HTTP-Referer": "https://github.com/memphant",
                "X-Title": "memphant-bench-reader",
            },
        )
        last_error: Exception | None = None
        for attempt, delay in enumerate((0, *OPENROUTER_RETRY_DELAYS)):
            if delay:
                time.sleep(delay)
            try:
                with urllib.request.urlopen(
                    request, timeout=OPENROUTER_TIMEOUT
                ) as response:
                    data = json.loads(response.read())
                content = (
                    data.get("choices", [{}])[0].get("message", {}).get("content")
                )
                if not content:
                    last_error = RuntimeError(
                        f"openrouter returned empty content (attempt "
                        f"{attempt + 1}/4): {json.dumps(data)[:500]}"
                    )
                    continue
                return content.strip()
            except urllib.error.HTTPError as error:
                body_text = error.read().decode(errors="replace")[:500]
                last_error = RuntimeError(
                    f"openrouter request failed (HTTP {error.code}, attempt "
                    f"{attempt + 1}/4): {body_text}"
                )
                if error.code != 429 and error.code < 500:
                    raise last_error from error
            except (urllib.error.URLError, TimeoutError, OSError, ValueError) as error:
                # OSError covers ssl.SSLError/socket resets that urlopen can
                # surface raw; ValueError covers malformed JSON bodies. Both
                # must retry, then land as RuntimeError so the per-question
                # handler records the row instead of killing the run.
                last_error = RuntimeError(
                    f"openrouter request failed (attempt {attempt + 1}/4): {error}"
                )
        assert last_error is not None
        raise last_error


def build_reader_prompt(row: dict) -> str:
    lines = ["Evidence (retrieved memory items, most relevant first):", ""]
    for item in row["evidence"]:
        lines.append(f"--- evidence item {item['rank']} ---")
        lines.append(item["body"].strip())
        lines.append("")
    if not row["evidence"]:
        lines.append("(no evidence was retrieved)")
        lines.append("")
    question_date = row.get("question_date") or "unknown"
    lines.append(f"Question date: {question_date}")
    lines.append(f"Question: {row['question']}")
    return "\n".join(lines)


def build_judge_prompt(question: str, gold: str, reply: str) -> str:
    return (
        f"Question: {question}\n"
        f"Gold answer: {gold}\n"
        f"Model answer: {reply}\n\n"
        "Does the model answer convey the gold answer? "
        "Reply with exactly one word: yes or no."
    )


def judge_row(cli: ReaderCli, row: dict, reply: str) -> tuple[bool, str]:
    """Returns (correct, judge_method)."""
    gold = str(row["gold_answer"])
    if row["is_abstention"]:
        return is_abstention_reply(reply), "abstention_exact"
    if is_abstention_reply(reply):
        return False, "containment"
    if contains_gold(reply, gold):
        return True, "containment"
    verdict = cli.call(
        "judge",
        JUDGE_SYSTEM_PROMPT,
        build_judge_prompt(row["question"], gold, reply),
    )
    return normalize(verdict).startswith("yes"), "llm_judge"


def bootstrap_ci(deltas: list[float], resamples: int, seed: int) -> dict:
    n = len(deltas)
    mean = sum(deltas) / n if n else 0.0
    if n == 0:
        return {
            "mean": mean,
            "ci95_low": 0.0,
            "ci95_high": 0.0,
            "ci_excludes_zero": False,
        }
    rng = random.Random(seed)
    means = sorted(
        sum(deltas[rng.randrange(n)] for _ in range(n)) / n
        for _ in range(resamples)
    )
    low = means[min(int(resamples * 0.025), resamples - 1)]
    high = means[min(max(int(resamples * 0.975 + 0.999999) - 1, 0), resamples - 1)]
    return {
        "mean": mean,
        "ci95_low": low,
        "ci95_high": high,
        "ci_excludes_zero": low > 0.0 or high < 0.0,
    }


def accuracy(rows: list[dict]) -> dict:
    scored = [r for r in rows if r.get("correct") is not None]
    correct = [r for r in scored if r["correct"]]
    return {
        "n": len(rows),
        "n_scored": len(scored),
        "qa_accuracy": (len(correct) / len(scored)) if scored else None,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--evidence", required=True, help="bench-lme --emit-qa JSONL")
    parser.add_argument("--out", required=True, help="output reader report JSON")
    parser.add_argument("--label", required=True, help="run label, e.g. session-rerank-off")
    parser.add_argument("--retrieval-report", help="path of the paired bench-lme retrieval report (recorded in header)")
    parser.add_argument("--baseline", help="baseline reader report JSON for paired QA deltas")
    parser.add_argument(
        "--engine",
        choices=ENGINES,
        default="claude",
        help="headless CLI engine driving reader and judge calls",
    )
    parser.add_argument("--model", default=DEFAULT_MODEL)
    parser.add_argument(
        "--judge-model",
        default=None,
        help="judge model id (defaults to --model; lets a stronger model judge)",
    )
    parser.add_argument(
        "--reasoning-effort",
        default=None,
        help=(
            "codex/openrouter-only: reasoning effort override (low|medium|"
            "high|...) — codex model_reasoning_effort or OpenRouter "
            "reasoning.effort; part of the cache key"
        ),
    )
    parser.add_argument("--cache-dir", default="docs/build-log/artifacts/real-retrieval-20260710/reader-cache")
    parser.add_argument("--max-calls", type=int, default=150, help="hard fresh-call budget for this invocation")
    parser.add_argument("--limit", type=int, help="only process the first N evidence rows (smoke)")
    parser.add_argument("--seed", type=int, default=20260710, help="bootstrap seed")
    args = parser.parse_args()

    # Split on '\n' only: chat bodies can embed U+2028/U+2029, which
    # str.splitlines() would treat as line breaks mid-JSON-record.
    rows = [
        json.loads(line)
        for line in Path(args.evidence).read_text().split("\n")
        if line.strip()
    ]
    if args.limit:
        rows = rows[: args.limit]

    judge_model = args.judge_model or args.model
    cli = ReaderCli(
        args.engine,
        args.model,
        judge_model,
        Path(args.cache_dir),
        args.max_calls,
        reasoning_effort=args.reasoning_effort,
    )
    per_question: list[dict] = []
    aborted_reason = None
    for index, row in enumerate(rows):
        record = {
            "question_id": row["question_id"],
            "question_type": row["question_type"],
            "is_abstention": row["is_abstention"],
            "gold_answer": row["gold_answer"],
            "reply": None,
            "judge_method": None,
            "correct": None,
            "reader_error": None,
        }
        try:
            reply = cli.call("reader", READER_SYSTEM_PROMPT, build_reader_prompt(row))
            record["reply"] = reply
            correct, method = judge_row(cli, row, reply)
            record["correct"] = correct
            record["judge_method"] = method
        except CallBudgetExceeded as error:
            aborted_reason = str(error)
            per_question.append(record)
            print(f"ABORT at row {index + 1}/{len(rows)}: {error}", file=sys.stderr)
            break
        except (RuntimeError, subprocess.TimeoutExpired) as error:
            record["reader_error"] = str(error)
        per_question.append(record)
        print(
            f"reader [{index + 1}/{len(rows)}] {row['question_id']} "
            f"correct={record['correct']} method={record['judge_method']}",
            file=sys.stderr,
        )

    strata = sorted({r["question_type"] for r in per_question})
    engine_desc = {
        "claude": "claude -p headless",
        "codex": "codex exec headless (read-only sandbox, final message only)",
        "openrouter": "openrouter chat/completions API",
    }[args.engine]
    report = {
        "benchmark": "longmemeval_reader_qa",
        "engine": args.engine,
        "reader": f"{args.model} ({engine_desc})",
        "judge": (
            f"containment+{judge_model} (normalized containment first; one LLM "
            "judge call on non-match; abstention = exact 'I don't know' "
            "containment)"
        ),
        "reader_model_id": args.model,
        "judge_model_id": judge_model,
        "reasoning_effort": args.reasoning_effort,
        "runtime": "postgres",
        "label": args.label,
        "evidence_path": args.evidence,
        "retrieval_report": args.retrieval_report,
        "command": " ".join(sys.argv),
        "generated_at_unix": int(time.time()),
        "aborted": aborted_reason,
        "fresh_calls": cli.fresh_calls,
        "cached_calls": cli.cached_calls,
        "overall": accuracy(per_question),
        "strata": {
            stratum: accuracy(
                [r for r in per_question if r["question_type"] == stratum]
            )
            for stratum in strata
        },
        "per_question": per_question,
        "paired_vs_baseline": None,
    }

    if args.baseline:
        baseline = json.loads(Path(args.baseline).read_text())
        base_rows = {
            r["question_id"]: r
            for r in baseline["per_question"]
            if r.get("correct") is not None
        }
        deltas = [
            float(r["correct"]) - float(base_rows[r["question_id"]]["correct"])
            for r in per_question
            if r.get("correct") is not None and r["question_id"] in base_rows
        ]
        report["paired_vs_baseline"] = {
            "baseline_path": args.baseline,
            "baseline_label": baseline.get("label"),
            "n_paired": len(deltas),
            "delta_qa_accuracy": bootstrap_ci(deltas, BOOTSTRAP_RESAMPLES, args.seed),
            "bootstrap_resamples": BOOTSTRAP_RESAMPLES,
            "bootstrap_seed": args.seed,
        }

    Path(args.out).write_text(json.dumps(report, indent=2) + "\n")
    overall = report["overall"]
    print(
        f"reader=done label={args.label} n={overall['n']} "
        f"n_scored={overall['n_scored']} qa_accuracy={overall['qa_accuracy']} "
        f"fresh_calls={cli.fresh_calls} cached_calls={cli.cached_calls} "
        f"aborted={aborted_reason} out={args.out}"
    )
    return 1 if aborted_reason else 0


if __name__ == "__main__":
    raise SystemExit(main())

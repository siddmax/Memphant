#!/usr/bin/env python3
"""Audit calibration overlap without emitting official or calibration text."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
from pathlib import Path


def normalized_text(row: dict) -> str:
    dialogue = row.get("dialogue") or row.get("dialogue_context_turns") or []
    question = row.get("question")
    if question is None:
        question = (row.get("generated_question") or {}).get("question", "")
    material = " ".join(
        [*(str(turn.get("content", "")) for turn in dialogue), str(question)]
    ).lower()
    return " ".join(re.findall(r"[a-z0-9]+", material))


def fivegrams(value: str) -> set[tuple[str, ...]]:
    words = value.split()
    return {tuple(words[index : index + 5]) for index in range(len(words) - 4)}


def load(path: Path) -> list[dict]:
    return [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines() if line.strip()]


def audit(official: list[dict], calibration: list[dict]) -> dict:
    official_hashes = {hashlib.sha256(normalized_text(row).encode()).hexdigest() for row in official}
    official_grams = [fivegrams(normalized_text(row)) for row in official]
    exact = 0
    suspicious = 0
    overlap_grams = 0
    for case in calibration:
        text = normalized_text(case)
        exact += hashlib.sha256(text.encode()).hexdigest() in official_hashes
        grams = fivegrams(text)
        maximum = 0.0
        case_overlap = 0
        for candidate in official_grams:
            shared = len(grams & candidate)
            case_overlap += shared
            maximum = max(maximum, shared / max(1, len(grams | candidate)))
        overlap_grams += case_overlap
        suspicious += maximum >= 0.15
    return {
        "calibration_rows": len(calibration),
        "official_rows": len(official),
        "exact_normalized_hash_matches": exact,
        "normalized_fivegram_overlap_count": overlap_grams,
        "suspicious_row_matches": suspicious,
        "pass": exact == suspicious == overlap_grams == 0,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--official", type=Path, required=True)
    parser.add_argument("--calibration", type=Path, action="append", required=True)
    args = parser.parse_args()
    official = load(args.official)
    calibration = [row for path in args.calibration for row in load(path)]
    result = audit(official, calibration)
    print(json.dumps(result, indent=2, sort_keys=True))
    return 0 if result["pass"] else 1


if __name__ == "__main__":
    raise SystemExit(main())

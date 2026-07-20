#!/usr/bin/env python3
"""Execute LongMemEval-V2's actual Qwen memory-token counting path."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import sys


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--official-dir", type=Path, required=True)
    args = parser.parse_args()
    official_dir = args.official_dir.resolve()
    if not (official_dir / "evaluation/harness.py").is_file():
        raise RuntimeError(f"pinned upstream harness is missing: {official_dir}")
    sys.path.insert(0, str(official_dir))

    from evaluation.harness import count_memory_context_tokens

    token_count = count_memory_context_tokens(
        [{"type": "text", "value": "MemPhant processor preflight"}],
        [None],
    )
    if not isinstance(token_count, int) or isinstance(token_count, bool) or token_count <= 0:
        raise RuntimeError("official processor returned an invalid token count")
    print(
        json.dumps(
            {
                "memory_context_tokens": token_count,
                "processor_preflight": "passed",
            },
            sort_keys=True,
        )
    )


if __name__ == "__main__":
    main()

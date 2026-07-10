#!/usr/bin/env python3
"""Fetch the LongMemEval dataset from Hugging Face and pin it by sha256.

Downloads `longmemeval_s` (preferred; ~40-50 haystack sessions per question)
and `longmemeval_oracle` (answer sessions only; reduced distractor pressure)
from the `xiaowu0162/longmemeval` dataset repo into `benchmarks/data/`
(gitignored) and records {url, filename, sha256, bytes, fetched_at} for each
file in the committed lock manifest `benchmarks/manifests/longmemeval_s.lock.json`.

If the manifest already exists, existing files are verified against it and
re-downloaded only on hash mismatch or absence.

Usage: python3 scripts/fetch_longmemeval.py [--oracle-only]
"""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import sys
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DATA_DIR = ROOT / "benchmarks" / "data"
MANIFEST = ROOT / "benchmarks" / "manifests" / "longmemeval_s.lock.json"
REPO = "xiaowu0162/longmemeval"
FILES = ["longmemeval_s", "longmemeval_oracle"]


def resolve_url(name: str) -> str:
    return f"https://huggingface.co/datasets/{REPO}/resolve/main/{name}"


def sha256_of(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def download(name: str) -> dict:
    url = resolve_url(name)
    dest = DATA_DIR / f"{name}.json"
    print(f"downloading {url} -> {dest}")
    request = urllib.request.Request(url, headers={"User-Agent": "memphant-fetch/1.0"})
    with urllib.request.urlopen(request) as response, dest.open("wb") as out:
        while True:
            chunk = response.read(1 << 20)
            if not chunk:
                break
            out.write(chunk)
    entry = {
        "url": url,
        "filename": dest.name,
        "sha256": sha256_of(dest),
        "bytes": dest.stat().st_size,
        "fetched_at": dt.datetime.now(dt.timezone.utc).isoformat(),
    }
    print(f"  sha256={entry['sha256']} bytes={entry['bytes']}")
    return entry


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--oracle-only", action="store_true")
    args = parser.parse_args()
    names = ["longmemeval_oracle"] if args.oracle_only else FILES

    DATA_DIR.mkdir(parents=True, exist_ok=True)
    MANIFEST.parent.mkdir(parents=True, exist_ok=True)

    manifest = {"repo": REPO, "files": {}}
    if MANIFEST.exists():
        manifest = json.loads(MANIFEST.read_text(encoding="utf-8"))
        manifest.setdefault("files", {})

    for name in names:
        dest = DATA_DIR / f"{name}.json"
        pinned = manifest["files"].get(name)
        if dest.exists() and pinned and sha256_of(dest) == pinned["sha256"]:
            print(f"{dest} verified against pinned sha256, skipping download")
            continue
        manifest["files"][name] = download(name)

    MANIFEST.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    print(f"manifest written: {MANIFEST}")
    return 0


if __name__ == "__main__":
    sys.exit(main())

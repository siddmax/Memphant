#!/usr/bin/env python3
"""Acquire and audit the pinned official SWE-Explore release.

The July 2026 public bundle is not yet a self-contained executable benchmark:
its 848 records omit both issue text and snapshot commits. This script pins and
verifies what is public, then fails closed before running an explorer. Once the
authors publish immutable execution inputs, MemPhant can emit ranked regions
through its existing REST service and delegate every score to their ``eval.py``.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import shutil
import tarfile
import tempfile
import urllib.request
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_MANIFEST = ROOT / "benchmarks/manifests/swe_explore.lock.json"


def release_urls(lock: dict) -> dict[str, str]:
    commit = lock["code"]["commit"]
    dataset = lock["dataset"]
    filename = dataset["file"]["path"]
    return {
        "code_archive": (
            f"https://github.com/Qiushao-E/SWE-Explore-Bench/archive/{commit}.tar.gz"
        ),
        "dataset": (
            "https://huggingface.co/datasets/"
            f"{dataset['repository']}/resolve/{dataset['revision']}/{filename}?download=true"
        ),
    }


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def verify_dataset(path: Path, expected: dict) -> dict[str, int]:
    actual_sha = _sha256(path)
    if actual_sha != expected["sha256"]:
        raise RuntimeError(
            f"dataset sha256 mismatch: expected {expected['sha256']}, got {actual_sha}"
        )
    if path.stat().st_size != expected["bytes"]:
        raise RuntimeError("dataset byte count mismatch")

    rows = []
    with path.open() as handle:
        for line_number, line in enumerate(handle, 1):
            if not line.strip():
                continue
            try:
                rows.append(json.loads(line))
            except json.JSONDecodeError as error:
                raise RuntimeError(f"invalid dataset JSON at line {line_number}") from error
    if len(rows) != expected["rows"]:
        raise RuntimeError("dataset row count mismatch")
    return {
        "rows": len(rows),
        "problem_statement_rows": sum(bool(row.get("problem_statement")) for row in rows),
        "base_commit_rows": sum(bool(row.get("base_commit")) for row in rows),
    }


def verify_code(path: Path, expected_files: dict[str, str]) -> None:
    for relative, expected_sha in expected_files.items():
        file_path = path / relative
        if not file_path.is_file():
            raise RuntimeError(f"official code file missing: {relative}")
        actual_sha = _sha256(file_path)
        if actual_sha != expected_sha:
            raise RuntimeError(f"official code drift: {relative}")


def require_execution_inputs(audit: dict[str, int]) -> None:
    rows = audit["rows"]
    if (
        audit["problem_statement_rows"] != rows
        or audit["base_commit_rows"] != rows
    ):
        raise RuntimeError(
            "SWE-Explore is not publicly executable from the pinned release: "
            "every row needs issue text and an immutable repository snapshot commit"
        )


def _download(url: str, destination: Path) -> None:
    request = urllib.request.Request(url, headers={"User-Agent": "MemPhant-SWE-Explore"})
    with urllib.request.urlopen(request) as response, destination.open("wb") as output:
        shutil.copyfileobj(response, output)


def _extract_archive(archive: Path, destination: Path) -> None:
    with tarfile.open(archive, "r:gz") as bundle:
        bundle.extractall(destination, filter="data")


def acquire(directory: Path, lock: dict) -> dict[str, int]:
    directory.mkdir(parents=True, exist_ok=True)
    code_dir = directory / "official"
    dataset_path = directory / lock["dataset"]["file"]["path"]
    urls = release_urls(lock)

    with tempfile.TemporaryDirectory(dir=directory) as temp_name:
        temp = Path(temp_name)
        if not code_dir.exists():
            archive = temp / "official.tar.gz"
            extracted = temp / "extracted"
            extracted.mkdir()
            _download(urls["code_archive"], archive)
            _extract_archive(archive, extracted)
            roots = list(extracted.iterdir())
            if len(roots) != 1 or not roots[0].is_dir():
                raise RuntimeError("unexpected official code archive layout")
            verify_code(roots[0], lock["code"]["files"])
            roots[0].replace(code_dir)
        else:
            verify_code(code_dir, lock["code"]["files"])

        if not dataset_path.exists():
            downloaded = temp / dataset_path.name
            _download(urls["dataset"], downloaded)
            audit = verify_dataset(downloaded, lock["dataset"]["file"])
            downloaded.replace(dataset_path)
        else:
            audit = verify_dataset(dataset_path, lock["dataset"]["file"])
    return audit


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("acquire", "verify", "run"))
    parser.add_argument("--directory", type=Path, required=True)
    parser.add_argument("--manifest", type=Path, default=DEFAULT_MANIFEST)
    args = parser.parse_args()

    lock = json.loads(args.manifest.read_text())
    if args.command == "acquire":
        audit = acquire(args.directory, lock)
    else:
        verify_code(args.directory / "official", lock["code"]["files"])
        audit = verify_dataset(
            args.directory / lock["dataset"]["file"]["path"],
            lock["dataset"]["file"],
        )
    print(json.dumps({"verified": True, "public_execution_ready": False, **audit}))
    if args.command == "run":
        require_execution_inputs(audit)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

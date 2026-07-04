from __future__ import annotations

import filecmp
import json
import os
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
MANIFEST = ROOT / ".codex" / "linked-repos.json"
ALLOW_MISSING_PRIVATE_ENV = "MEMPHANT_ALLOW_MISSING_PRIVATE_REPO"


def collect_diffs(comparison: filecmp.dircmp[str], prefix: str = "") -> list[str]:
    differences: list[str] = []
    for name in comparison.left_only:
        differences.append(f"{prefix}{name}:public_only")
    for name in comparison.right_only:
        differences.append(f"{prefix}{name}:private_only")
    for name in comparison.diff_files:
        differences.append(f"{prefix}{name}:content")
    for name, subdir in comparison.subdirs.items():
        differences.extend(collect_diffs(subdir, f"{prefix}{name}/"))
    return differences


def main() -> int:
    manifest = json.loads(MANIFEST.read_text(encoding="utf-8"))
    public_dir = ROOT / manifest["source_docs"]["path"]
    private_dir = Path(manifest["private_repo"]["path"]) / manifest["source_docs"]["path"]
    if not public_dir.exists():
        print(f"public_specs_missing={public_dir}", file=sys.stderr)
        return 2
    if not private_dir.exists():
        if os.environ.get(ALLOW_MISSING_PRIVATE_ENV) == "1":
            print(
                "spec_drift=skipped "
                f"reason=private_specs_missing private={private_dir}"
            )
            return 0
        print(f"private_specs_missing={private_dir}", file=sys.stderr)
        return 2

    differences = collect_diffs(filecmp.dircmp(public_dir, private_dir))
    if differences:
        print("spec_drift=dirty")
        for item in sorted(differences):
            print(item)
        return 1

    print(f"spec_drift=clean public={public_dir} private={private_dir}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

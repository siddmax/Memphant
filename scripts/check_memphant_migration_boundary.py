from __future__ import annotations

import argparse
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
MIGRATIONS = ROOT / "memphant_migrations" / "versions"

HEADER_SCAN_LINES = 5


def declares_rewrite(text: str) -> bool:
    """A migration may drop tables/indexes only when its header declares
    `-- migration_kind: rewrite` within the first few lines."""
    for line in text.splitlines()[:HEADER_SCAN_LINES]:
        if line.strip().lower() == "-- migration_kind: rewrite":
            return True
    return False


def check_file(path: Path, root: Path) -> list[str]:
    findings: list[str] = []
    raw = path.read_text(encoding="utf-8")
    text = raw.lower()
    rewrite = declares_rewrite(raw)
    rel = path.relative_to(root) if path.is_relative_to(root) else path.name
    if "drop table" in text and not rewrite:
        findings.append(f"{rel}:drop_table_without_rewrite_header")
    if "drop index" in text and not rewrite:
        findings.append(f"{rel}:drop_index_without_rewrite_header")
    if "public." in text:
        findings.append(f"{rel}:public_schema_reference")
    if "syndai." in text:
        findings.append(f"{rel}:syndai_schema_reference")
    return findings


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Check MemPhant migration boundary rules."
    )
    parser.add_argument(
        "--migrations-dir",
        default=str(MIGRATIONS),
        help="Directory of *.sql migrations (default: repo migrations).",
    )
    args = parser.parse_args()
    migrations_dir = Path(args.migrations_dir)

    findings: list[str] = []
    for path in sorted(migrations_dir.glob("*.sql")):
        findings.extend(check_file(path, ROOT))

    if findings:
        print("migration_boundary=dirty")
        for finding in findings:
            print(finding)
        return 1

    print("migration_boundary=clean")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

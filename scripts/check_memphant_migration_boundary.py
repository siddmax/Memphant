from __future__ import annotations

import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
MIGRATIONS = ROOT / "memphant_migrations" / "versions"


def main() -> int:
    findings: list[str] = []
    for path in sorted(MIGRATIONS.glob("*.sql")):
        text = path.read_text(encoding="utf-8").lower()
        if "drop table" in text:
            findings.append(f"{path.relative_to(ROOT)}:drop_table")
        if "public." in text:
            findings.append(f"{path.relative_to(ROOT)}:public_schema_reference")
        if "syndai." in text:
            findings.append(f"{path.relative_to(ROOT)}:syndai_schema_reference")

    if findings:
        print("migration_boundary=dirty")
        for finding in findings:
            print(finding)
        return 1

    print("migration_boundary=clean")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

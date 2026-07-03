from __future__ import annotations

import re
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
MIGRATIONS = ROOT / "memphant_migrations" / "versions"

BREAKING_PATTERNS = [
    r"\balter\s+table\b.*\bdrop\s+column\b",
    r"\bdrop\s+constraint\b",
    r"\bdrop\s+index\b",
    r"\bdrop\s+type\b",
    r"\brename\s+column\b",
    r"\brename\s+to\b",
]


def classify(sql: str) -> str:
    lowered = sql.lower()
    if "drop table" in lowered:
        return "rewrite"
    if any(re.search(pattern, lowered, re.S) for pattern in BREAKING_PATTERNS):
        return "breaking"
    return "additive"


def declared_kind(sql: str) -> str | None:
    match = re.search(
        r"insert\s+into\s+memphant\.schema_migrations\s*\([^)]*migration_kind[^)]*\)\s*values\s*\([^)]*'(?P<kind>additive|breaking|rewrite)'",
        sql.lower(),
        re.S,
    )
    if match:
        return match.group("kind")
    return None


def main() -> int:
    findings: list[str] = []
    for path in sorted(MIGRATIONS.glob("*.sql")):
        sql = path.read_text(encoding="utf-8")
        computed = classify(sql)
        declared = declared_kind(sql)
        if declared is None:
            findings.append(f"{path.relative_to(ROOT)}:missing_migration_kind")
            continue
        if computed != declared:
            findings.append(
                f"{path.relative_to(ROOT)}:class_mismatch declared={declared} computed={computed}"
            )
        if computed in {"breaking", "rewrite"} and "schema_compat_revision" not in sql.lower():
            findings.append(f"{path.relative_to(ROOT)}:missing_schema_compat_revision_bump")

    if findings:
        print("migration_class=dirty")
        for finding in findings:
            print(finding)
        return 1

    print("migration_class=clean")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

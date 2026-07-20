from __future__ import annotations

import argparse
import subprocess
import sys
from pathlib import Path
from urllib.parse import urlsplit


ROOT = Path(__file__).resolve().parents[1]
MIGRATIONS = ROOT / "memphant_migrations" / "versions"


def migration_files() -> list[Path]:
    return sorted(MIGRATIONS.glob("*.sql"))


def psql(database_url: str, *args: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [
            "psql",
            "--no-psqlrc",
            "--set",
            "ON_ERROR_STOP=1",
            "--dbname",
            database_url,
            *args,
        ],
        cwd=ROOT,
        text=True,
        capture_output=True,
        check=False,
    )


def applied_versions(database_url: str) -> set[str]:
    """Read the memphant.schema_migrations ledger; empty when it does not
    exist yet (fresh database — the bootstrap migration creates it)."""
    result = psql(
        database_url,
        "--quiet",
        "--tuples-only",
        "--no-align",
        "--command",
        "select version from memphant.schema_migrations",
    )
    if result.returncode != 0:
        return set()
    return {line.strip() for line in result.stdout.splitlines() if line.strip()}


def apply_migration(database_url: str, path: Path) -> None:
    version = path.stem
    subprocess.run(
        [
            "psql",
            "--no-psqlrc",
            "--set",
            "ON_ERROR_STOP=1",
            "--dbname",
            database_url,
            "--single-transaction",
            "--file",
            str(path),
            "--command",
            "insert into memphant.schema_migrations (version, schema_compat_revision) "
            f"values ('{version}', '{version}') on conflict (version) do nothing",
        ],
        cwd=ROOT,
        check=True,
    )


def validate_database_url(database_url: str) -> None:
    try:
        port = urlsplit(database_url).port
    except ValueError as error:
        raise ValueError(f"invalid database URL: {error}") from error
    if port == 6543:
        raise ValueError(
            "transaction pooler port 6543 is not supported for migrations; "
            "use a direct or session connection on port 5432"
        )


def main() -> int:
    parser = argparse.ArgumentParser(description="Apply MemPhant SQL migrations in order.")
    parser.add_argument("--database-url", required=True)
    parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args()
    try:
        validate_database_url(args.database_url)
    except ValueError as error:
        parser.error(str(error))

    files = migration_files()
    print(f"migration_plan={len(files)}")
    for path in files:
        print(path.relative_to(ROOT))

    if args.dry_run:
        return 0

    applied = applied_versions(args.database_url)
    applied_count = 0
    for path in files:
        version = path.stem
        if version in applied:
            print(f"migration_skip={version} reason=already_applied")
            continue
        apply_migration(args.database_url, path)
        applied_count += 1
    print(f"migration_apply=complete applied={applied_count} skipped={len(files) - applied_count}")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except subprocess.CalledProcessError as error:
        print(f"migration_apply=failed command={error.cmd}", file=sys.stderr)
        raise SystemExit(error.returncode)

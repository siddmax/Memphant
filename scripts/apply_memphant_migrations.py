from __future__ import annotations

import argparse
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
MIGRATIONS = ROOT / "memphant_migrations" / "versions"


def migration_files() -> list[Path]:
    return sorted(MIGRATIONS.glob("*.sql"))


def apply_migration(database_url: str, path: Path) -> None:
    subprocess.run(
        [
            "psql",
            "--set",
            "ON_ERROR_STOP=1",
            "--dbname",
            database_url,
            "--file",
            str(path),
        ],
        cwd=ROOT,
        check=True,
    )


def main() -> int:
    parser = argparse.ArgumentParser(description="Apply MemPhant SQL migrations in order.")
    parser.add_argument("--database-url", required=True)
    parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args()

    files = migration_files()
    print(f"migration_plan={len(files)}")
    for path in files:
        print(path.relative_to(ROOT))

    if args.dry_run:
        return 0

    for path in files:
        apply_migration(args.database_url, path)
    print("migration_apply=complete")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except subprocess.CalledProcessError as error:
        print(f"migration_apply=failed command={error.cmd}", file=sys.stderr)
        raise SystemExit(error.returncode)

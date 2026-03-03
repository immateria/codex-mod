#!/usr/bin/env python3
"""
Skill Packager - Creates a distributable .skill file (zip) for a skill folder.

Usage:
    package_skill.py <path/to/skill-folder> [output-directory]

Examples:
    package_skill.py ~/.codex/skills/my-skill
    package_skill.py ./my-skill ./dist
"""

from __future__ import annotations

import argparse
import sys
import zipfile
from pathlib import Path

from quick_validate import validate_skill

IGNORE_DIR_NAMES = {
    ".git",
    ".hg",
    ".svn",
    "__pycache__",
    ".pytest_cache",
    ".mypy_cache",
    ".ruff_cache",
    ".venv",
    "node_modules",
}

IGNORE_FILE_NAMES = {
    ".DS_Store",
}


def should_include(path: Path) -> bool:
    if path.name in IGNORE_FILE_NAMES:
        return False
    if path.suffix == ".pyc":
        return False
    if any(part in IGNORE_DIR_NAMES for part in path.parts):
        return False
    return True


def package_skill(skill_path: str | Path, output_dir: str | Path | None = None) -> Path | None:
    skill_path = Path(skill_path).resolve()

    if not skill_path.exists():
        print(f"ERROR: skill folder not found: {skill_path}")
        return None
    if not skill_path.is_dir():
        print(f"ERROR: path is not a directory: {skill_path}")
        return None

    valid, message = validate_skill(skill_path)
    if not valid:
        print(f"ERROR: validation failed: {message}")
        print("   Fix validation errors before packaging.")
        return None

    output_path = Path(output_dir).resolve() if output_dir else Path.cwd()
    output_path.mkdir(parents=True, exist_ok=True)

    skill_name = skill_path.name
    out_file = output_path / f"{skill_name}.skill"

    files: list[Path] = [
        p for p in skill_path.rglob("*") if p.is_file() and should_include(p)
    ]
    files.sort()

    try:
        with zipfile.ZipFile(out_file, "w", zipfile.ZIP_DEFLATED) as zipf:
            for file_path in files:
                arcname = file_path.relative_to(skill_path.parent)
                zipf.write(file_path, arcname)
        print(f"OK: packaged skill to: {out_file}")
        return out_file
    except Exception as e:
        print(f"ERROR: error creating .skill file: {e}")
        return None


def main() -> None:
    parser = argparse.ArgumentParser(description="Package a skill directory into a .skill file.")
    parser.add_argument("skill_directory", help="Path to the skill directory")
    parser.add_argument(
        "output_directory",
        nargs="?",
        default=None,
        help="Optional output directory (default: current directory)",
    )
    args = parser.parse_args()

    result = package_skill(args.skill_directory, args.output_directory)
    raise SystemExit(0 if result else 1)


if __name__ == "__main__":
    main()

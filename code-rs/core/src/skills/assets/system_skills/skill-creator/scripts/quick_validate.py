#!/usr/bin/env python3
"""
Quick validation script for skills.

Checks:
- SKILL.md exists and starts with YAML frontmatter (--- ... ---)
- Frontmatter has `name` and `description`
- Name format: lowercase letters/digits/hyphens, <= 64 chars
- Description: non-empty string, <= 1024 chars
- Folder name matches frontmatter name (common footgun)
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

try:
    import yaml
except ImportError:  # pragma: no cover
    print("ERROR: missing dependency: PyYAML")
    print("   Install with: python3 -m pip install pyyaml")
    raise SystemExit(1)

MAX_NAME_LEN = 64
MAX_DESCRIPTION_LEN = 1024


def validate_skill(skill_path: str | Path) -> tuple[bool, str]:
    skill_path = Path(skill_path)

    if not skill_path.exists():
        return False, f"skill path not found: {skill_path}"
    if not skill_path.is_dir():
        return False, f"skill path is not a directory: {skill_path}"

    skill_md = skill_path / "SKILL.md"
    if not skill_md.exists():
        return False, f"SKILL.md not found in {skill_path}"

    try:
        content = skill_md.read_text(encoding="utf-8")
    except Exception as e:
        return False, f"failed to read SKILL.md: {e}"

    if not content.startswith("---"):
        return False, "SKILL.md must start with YAML frontmatter (---)"

    parts = content.split("---", 2)
    if len(parts) < 3:
        return False, "SKILL.md frontmatter must be closed with ---"

    try:
        frontmatter = yaml.safe_load(parts[1])
    except yaml.YAMLError as e:
        return False, f"invalid YAML in frontmatter: {e}"

    if not isinstance(frontmatter, dict):
        return False, "frontmatter must be a YAML mapping"

    if "name" not in frontmatter:
        return False, "missing required field: name"
    if "description" not in frontmatter:
        return False, "missing required field: description"

    name = frontmatter.get("name", "")
    if not isinstance(name, str) or not name.strip():
        return False, "name must be a non-empty string"

    name = name.strip()
    if len(name) > MAX_NAME_LEN:
        return False, f"name is too long ({len(name)} characters). Maximum is {MAX_NAME_LEN}."
    if not all(c.islower() or c.isdigit() or c == "-" for c in name):
        return False, "name must be lowercase with only letters, digits, and hyphens"
    if name.startswith("-") or name.endswith("-") or "--" in name:
        return False, f"name '{name}' cannot start/end with hyphen or contain consecutive hyphens"

    description = frontmatter.get("description", "")
    if not isinstance(description, str):
        return False, "description must be a string"
    if not description.strip():
        return False, "description must be non-empty"
    if description.lstrip().lower().startswith("todo"):
        return False, "description still looks like a placeholder (starts with 'TODO')"
    if len(description) > MAX_DESCRIPTION_LEN:
        return (
            False,
            f"description is too long ({len(description)} characters). Maximum is {MAX_DESCRIPTION_LEN}.",
        )

    if skill_path.name != name:
        return (
            False,
            f"folder name '{skill_path.name}' does not match frontmatter name '{name}'",
        )

    return True, "skill validation passed"


def main() -> None:
    parser = argparse.ArgumentParser(description="Quickly validate a skill folder.")
    parser.add_argument("skill_directory", help="Path to the skill directory")
    args = parser.parse_args()

    valid, message = validate_skill(args.skill_directory)
    if valid:
        print(f"OK: {message}")
        raise SystemExit(0)
    print(f"ERROR: {message}")
    raise SystemExit(1)


if __name__ == "__main__":
    main()

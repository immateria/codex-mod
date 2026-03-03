#!/usr/bin/env python3
"""Shared helpers for plan scripts.

This fork supports both `CODE_HOME` and upstream `CODEX_HOME`.

Defaults:
- If neither env var is set, prefer `~/.code` (fork default).
- If legacy `~/.codex/plans` exists and `~/.code` does not, use `~/.codex`.
"""

from __future__ import annotations

import os
import re
from pathlib import Path

_NAME_RE = re.compile(r"^[a-z0-9]+(-[a-z0-9]+)*$")


def get_code_home() -> Path:
    """Return CODE_HOME/CODEX_HOME if set, else default to ~/.code (with legacy fallback)."""
    for key in ("CODE_HOME", "CODEX_HOME"):
        raw = os.environ.get(key)
        if raw and raw.strip():
            return Path(raw).expanduser()

    home = Path.home()
    default = home / ".code"
    legacy = home / ".codex"

    # Prefer legacy only when it looks like it's the active home already.
    if not default.exists() and (legacy / "plans").exists():
        return legacy

    return default


def get_plans_dir() -> Path:
    return get_code_home() / "plans"


def validate_plan_name(name: str) -> None:
    if not name or not _NAME_RE.match(name):
        raise ValueError(
            "Invalid plan name. Use short, lower-case, hyphen-delimited names "
            "(e.g., codex-rate-limit-overview)."
        )


def parse_frontmatter(path: Path) -> dict:
    """Parse YAML frontmatter from a markdown file without reading the body."""
    with path.open("r", encoding="utf-8") as handle:
        first = handle.readline()
        if first.strip() != "---":
            raise ValueError("Frontmatter must start with '---'.")

        data: dict[str, str] = {}
        for line in handle:
            stripped = line.strip()
            if stripped == "---":
                return data
            if not stripped or stripped.startswith("#"):
                continue
            if ":" not in line:
                raise ValueError(f"Invalid frontmatter line: {line.rstrip()}")
            key, value = line.split(":", 1)
            key = key.strip()
            value = value.strip()
            if value and len(value) >= 2 and value[0] == value[-1] and value[0] in ('"', "'"):
                value = value[1:-1]
            data[key] = value

    raise ValueError("Frontmatter must end with '---'.")

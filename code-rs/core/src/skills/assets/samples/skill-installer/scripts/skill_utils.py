#!/usr/bin/env python3
"""Shared helpers for skill installer scripts.

This fork supports both `CODE_HOME` and upstream `CODEX_HOME`.

Defaults:
- If neither env var is set, prefer `~/.code` (fork default).
- If legacy `~/.codex/skills` exists and `~/.code` does not, use `~/.codex`.
"""

from __future__ import annotations

import os
from pathlib import Path


def get_code_home() -> Path:
    """Return CODE_HOME/CODEX_HOME if set, else default to ~/.code (with legacy fallback)."""
    for key in ("CODE_HOME", "CODEX_HOME"):
        raw = os.environ.get(key)
        if raw and raw.strip():
            return Path(raw).expanduser()

    home = Path.home()
    default = home / ".code"
    legacy = home / ".codex"

    if not default.exists() and (legacy / "skills").exists():
        return legacy

    return default


def get_skills_dir(code_home: Path | None = None) -> Path:
    code_home = code_home or get_code_home()
    return code_home / "skills"


def get_config_path(code_home: Path | None = None) -> Path:
    code_home = code_home or get_code_home()
    return code_home / "config.toml"


def _tomllib():
    try:
        import tomllib  # Python 3.11+
    except ModuleNotFoundError:
        try:
            import tomli as tomllib  # type: ignore[no-redef]
        except ModuleNotFoundError:
            return None
    return tomllib


def load_config(code_home: Path | None = None) -> dict:
    """Best-effort config reader.

    Returns {} when config is missing or a TOML parser isn't available.
    """
    code_home = code_home or get_code_home()
    path = get_config_path(code_home)
    if not path.is_file():
        return {}

    tomllib = _tomllib()
    if tomllib is None:
        return {}

    try:
        with path.open("rb") as handle:
            return tomllib.load(handle)  # type: ignore[no-any-return]
    except Exception:
        # Best-effort: callers decide if missing config should be fatal.
        return {}


def resolve_style_skill_roots(style: str, code_home: Path | None = None) -> list[Path]:
    """Return configured skill roots for a shell style profile.

    Roots are resolved relative to CODE_HOME/CODEX_HOME when not absolute.
    """
    code_home = code_home or get_code_home()
    cfg = load_config(code_home)
    profiles = cfg.get("shell_style_profiles")
    if not isinstance(profiles, dict):
        return []

    profile = profiles.get(style)
    if not isinstance(profile, dict):
        return []

    roots = profile.get("skill_roots")
    if not isinstance(roots, list):
        return []

    out: list[Path] = []
    for root in roots:
        if not isinstance(root, str) or not root.strip():
            continue
        p = Path(root).expanduser()
        if not p.is_absolute():
            p = code_home / p
        out.append(p)
    return out


def installed_skill_names(roots: list[Path]) -> set[str]:
    """Return installed skill names across one or more roots."""
    out: set[str] = set()
    for root in roots:
        if not root.is_dir():
            continue
        for entry in root.iterdir():
            if entry.is_dir():
                out.add(entry.name)
    return out


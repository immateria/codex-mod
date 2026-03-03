#!/usr/bin/env python3
"""Install one or more curated skills by name.

This is a small wrapper around `install-skill-from-github.py` that expands skill
names into repo paths like `skills/.curated/<skill-name>`.
"""

from __future__ import annotations

import argparse
from pathlib import Path
import subprocess
import sys


DEFAULT_REPO = "openai/skills"
DEFAULT_PATH = "skills/.curated"
DEFAULT_REF = "main"


def _validate_skill_name(name: str) -> None:
    if not name or "/" in name or "\\" in name:
        raise SystemExit(f"Invalid skill name: {name!r} (must be a single directory name)")
    if name in (".", ".."):
        raise SystemExit(f"Invalid skill name: {name!r}")


def _parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Install curated skills by name (wraps install-skill-from-github.py)."
    )
    parser.add_argument("skills", nargs="+", help="Skill name(s) to install.")
    parser.add_argument("--repo", default=DEFAULT_REPO, help="Source repo (default: openai/skills).")
    parser.add_argument(
        "--path",
        default=DEFAULT_PATH,
        help="Base repo path (default: skills/.curated).",
    )
    parser.add_argument(
        "--experimental",
        action="store_true",
        help="Install from skills/.experimental (overrides --path).",
    )
    parser.add_argument("--ref", default=DEFAULT_REF, help="Git ref (default: main).")
    parser.add_argument("--dest", help="Destination skills directory.")
    parser.add_argument(
        "--style",
        help="Shell style profile name (uses shell_style_profiles.<style>.skill_roots[0]).",
    )
    parser.add_argument(
        "--method",
        choices=["auto", "download", "git"],
        default="auto",
        help="Install method (default: auto).",
    )
    parser.add_argument(
        "--overwrite",
        action="store_true",
        help="Replace destination skill directory if it already exists.",
    )
    parser.add_argument(
        "--name",
        help="Destination skill name (single install only; defaults to basename).",
    )
    return parser.parse_args(argv)


def main(argv: list[str]) -> int:
    args = _parse_args(argv)
    if args.name and len(args.skills) != 1:
        raise SystemExit("--name is only valid when installing a single skill.")

    base_path = "skills/.experimental" if args.experimental else args.path
    base_path = base_path.rstrip("/")

    for name in args.skills:
        _validate_skill_name(name)

    script_dir = Path(__file__).resolve().parent
    installer = script_dir / "install-skill-from-github.py"
    if not installer.is_file():
        raise SystemExit(f"Missing script: {installer}")

    repo_paths = [f"{base_path}/{name}" for name in args.skills]

    cmd: list[str] = [
        sys.executable,
        str(installer),
        "--repo",
        args.repo,
        "--ref",
        args.ref,
        "--path",
        *repo_paths,
        "--method",
        args.method,
    ]
    if args.dest:
        cmd.extend(["--dest", args.dest])
    if args.style:
        cmd.extend(["--style", args.style])
    if args.overwrite:
        cmd.append("--overwrite")
    if args.name:
        cmd.extend(["--name", args.name])

    proc = subprocess.run(cmd)
    return proc.returncode


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))


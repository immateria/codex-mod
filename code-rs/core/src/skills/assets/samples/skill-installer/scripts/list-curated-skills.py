#!/usr/bin/env python3
"""List skills from a GitHub repo path with local install annotations."""

from __future__ import annotations

import argparse
import json
import sys
import urllib.error
from pathlib import Path

from github_utils import github_api_contents_url, github_request
from skill_utils import (
    get_config_path,
    get_skills_dir,
    installed_skill_names,
    resolve_style_skill_roots,
)

DEFAULT_REPO = "openai/skills"
DEFAULT_PATH = "skills/.curated"
DEFAULT_REF = "main"


class ListError(Exception):
    pass


class Args(argparse.Namespace):
    repo: str
    path: str
    ref: str
    format: str
    query: str | None
    limit: int | None
    dest: str | None
    style: str | None


def _request(url: str) -> bytes:
    return github_request(url, "codex-skill-list")


def _installed_roots(args: Args) -> list[Path]:
    if args.dest:
        return [Path(args.dest).expanduser()]

    if args.style:
        roots = resolve_style_skill_roots(args.style)
        if roots:
            return roots

        cfg_path = get_config_path()
        if not cfg_path.exists():
            raise ListError(
                f"No config found at {cfg_path}. Use --dest to pick an install root, "
                "or create config with shell_style_profiles.<style>.skill_roots."
            )
        raise ListError(
            f"No skill_roots configured for style '{args.style}' in {cfg_path}. "
            "Use --dest to pick an install root, or update config."
        )

    return [get_skills_dir()]


def _list_skills(repo: str, path: str, ref: str) -> list[str]:
    api_url = github_api_contents_url(repo, path, ref)
    try:
        payload = _request(api_url)
    except urllib.error.HTTPError as exc:
        if exc.code == 404:
            raise ListError(
                "Skills path not found: "
                f"https://github.com/{repo}/tree/{ref}/{path}"
            ) from exc
        raise ListError(f"Failed to fetch skills: HTTP {exc.code}") from exc
    data = json.loads(payload.decode("utf-8"))
    if not isinstance(data, list):
        raise ListError("Unexpected skills listing response.")
    skills = [item["name"] for item in data if item.get("type") == "dir"]
    return sorted(skills)


def _parse_args(argv: list[str]) -> Args:
    parser = argparse.ArgumentParser(description="List skills.")
    parser.add_argument("--repo", default=DEFAULT_REPO)
    parser.add_argument(
        "--path",
        default=DEFAULT_PATH,
        help="Repo path to list (default: skills/.curated)",
    )
    parser.add_argument("--ref", default=DEFAULT_REF)
    parser.add_argument(
        "--query",
        help="Case-insensitive substring to filter skill names.",
    )
    parser.add_argument("--limit", type=int, help="Limit the number of results.")
    parser.add_argument("--dest", help="Skills directory to check for installed skills.")
    parser.add_argument(
        "--style",
        help="Shell style profile name (uses shell_style_profiles.<style>.skill_roots).",
    )
    parser.add_argument(
        "--format",
        choices=["text", "json"],
        default="text",
        help="Output format",
    )
    return parser.parse_args(argv, namespace=Args())


def main(argv: list[str]) -> int:
    args = _parse_args(argv)
    try:
        skills = _list_skills(args.repo, args.path, args.ref)
        query = args.query.lower() if args.query else None
        if query:
            skills = [name for name in skills if query in name.lower()]

        if args.limit is not None and args.limit > 0:
            skills = skills[: args.limit]

        installed = installed_skill_names(_installed_roots(args))
        if args.format == "json":
            base_path = args.path.rstrip("/")
            payload = [
                {
                    "name": name,
                    "installed": name in installed,
                    "repo_path": f"{base_path}/{name}",
                }
                for name in skills
            ]
            print(json.dumps(payload))
        else:
            for idx, name in enumerate(skills, start=1):
                suffix = " (already installed)" if name in installed else ""
                print(f"{idx}. {name}{suffix}")
        return 0
    except ListError as exc:
        print(f"Error: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))

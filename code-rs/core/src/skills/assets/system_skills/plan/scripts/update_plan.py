#!/usr/bin/env python3
"""Update an existing plan markdown file under $CODE_HOME/plans (or $CODEX_HOME/plans)."""

from __future__ import annotations

import argparse
import os
import sys
from pathlib import Path

from plan_utils import get_plans_dir, read_plan_file, validate_plan_name


DEFAULT_APPEND_SEPARATOR = "\n\n"


def _read_text(path: str) -> str:
    return Path(path).read_text(encoding="utf-8")


def _read_replacement_body(args: argparse.Namespace) -> str | None:
    if args.body_file:
        return _read_text(args.body_file)
    if args.template:
        return args.template
    if not sys.stdin.isatty():
        data = sys.stdin.read()
        if not data or not data.strip():
            return None
        return data
    return None


def _validate_body(body: str) -> str:
    body = body.strip()
    if not body:
        raise SystemExit("Plan body cannot be empty.")
    if body.lstrip().startswith("---"):
        raise SystemExit("Plan body should not include frontmatter.")
    return body


def _render_frontmatter(frontmatter: dict[str, str]) -> str:
    lines = ["---"]
    for key, value in frontmatter.items():
        if "\n" in value:
            raise SystemExit(f"Frontmatter field '{key}' must be a single line.")
        lines.append(f"{key}: {value}")
    lines.append("---")
    return "\n".join(lines)


def _atomic_write(path: Path, contents: str) -> None:
    tmp = path.with_suffix(path.suffix + ".tmp")
    tmp.write_text(contents, encoding="utf-8")
    os.replace(tmp, path)


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Update a plan file under $CODE_HOME/plans (or $CODEX_HOME/plans).",
    )
    parser.add_argument("name", help="Plan name (lower-case, hyphen-delimited).")
    parser.add_argument("--description", help="Replace the plan description (single line).")
    parser.add_argument(
        "--body-file",
        help="Replace the plan body with this markdown file (no frontmatter).",
    )
    parser.add_argument(
        "--append-file",
        help="Append this markdown snippet to the existing body.",
    )
    parser.add_argument(
        "--template",
        help="Replace the plan body with this literal string (useful for quick resets).",
    )
    parser.add_argument(
        "--overwrite",
        action="store_true",
        help="Required when replacing the body (via --body-file/--template/stdin).",
    )
    args = parser.parse_args()

    name = args.name.strip()
    try:
        validate_plan_name(name)
    except ValueError as e:
        raise SystemExit(str(e)) from None

    if args.description is not None:
        description = args.description.strip()
        if not description or "\n" in description:
            raise SystemExit("Description must be a single line.")
    else:
        description = None

    replace_body = _read_replacement_body(args)
    append_body = _read_text(args.append_file) if args.append_file else None

    if replace_body is not None and append_body is not None:
        raise SystemExit("Choose either a replacement body or --append-file, not both.")

    if replace_body is not None and not args.overwrite:
        raise SystemExit("Refusing to overwrite plan body without --overwrite.")

    if replace_body is None and append_body is None and description is None:
        raise SystemExit("No changes requested. Use --description, --append-file, or a body source.")

    plans_dir = get_plans_dir()
    plans_dir.mkdir(parents=True, exist_ok=True)
    plan_path = plans_dir / f"{name}.md"

    if not plan_path.exists():
        raise SystemExit(f"Plan not found: {plan_path}")

    try:
        frontmatter, existing_body = read_plan_file(plan_path)
    except ValueError as e:
        raise SystemExit(str(e)) from None

    fm_name = frontmatter.get("name")
    fm_desc = frontmatter.get("description")
    if not fm_name or not fm_desc:
        raise SystemExit("Frontmatter must include name and description.")

    if fm_name != name:
        raise SystemExit(
            f"Frontmatter name '{fm_name}' does not match requested name '{name}'."
        )
    if plan_path.stem != name:
        raise SystemExit(f"Filename '{plan_path.name}' does not match name '{name}'.")

    new_desc = description if description is not None else fm_desc

    new_body = existing_body
    if replace_body is not None:
        new_body = _validate_body(replace_body) + "\n"
    elif append_body is not None:
        snippet = append_body.strip()
        if not snippet:
            raise SystemExit("Append body cannot be empty.")
        if not new_body:
            new_body = snippet + "\n"
        else:
            body = new_body.rstrip()
            new_body = body + DEFAULT_APPEND_SEPARATOR + snippet + "\n"

    # Preserve any extra frontmatter keys, but keep name/description first.
    out_frontmatter: dict[str, str] = {"name": fm_name, "description": new_desc}
    for key, value in frontmatter.items():
        if key in ("name", "description"):
            continue
        out_frontmatter[key] = value

    content = _render_frontmatter(out_frontmatter) + "\n\n" + new_body.lstrip("\n")
    _atomic_write(plan_path, content)
    print(str(plan_path))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

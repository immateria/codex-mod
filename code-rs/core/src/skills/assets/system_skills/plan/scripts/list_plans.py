#!/usr/bin/env python3
"""List plan summaries by reading frontmatter only."""

from __future__ import annotations

import argparse
import datetime as _dt
import json

from plan_utils import get_plans_dir, parse_frontmatter


def main() -> int:
    parser = argparse.ArgumentParser(
        description="List plan summaries from $CODE_HOME/plans (or $CODEX_HOME/plans)."
    )
    parser.add_argument("--query", help="Case-insensitive substring to filter name/description.")
    parser.add_argument("--json", action="store_true", help="Emit JSON output.")
    parser.add_argument(
        "--sort",
        choices=("name", "mtime"),
        default="name",
        help="Sort by name (default) or mtime (newest first).",
    )
    parser.add_argument("--limit", type=int, help="Limit the number of results.")
    parser.add_argument("--paths", action="store_true", help="Print only paths (one per line).")
    parser.add_argument(
        "--long",
        action="store_true",
        help="Include file metadata (mtime, size).",
    )
    parser.add_argument(
        "--include-broken",
        action="store_true",
        help="Include files with invalid/missing frontmatter in output.",
    )
    args = parser.parse_args()

    plans_dir = get_plans_dir()
    plans_dir.mkdir(parents=True, exist_ok=True)

    query = args.query.lower() if args.query else None
    items = []
    paths = list(plans_dir.glob("*.md"))
    if args.sort == "mtime":
        paths.sort(key=lambda p: p.stat().st_mtime, reverse=True)
    else:
        paths.sort()

    for path in paths:
        stat = path.stat()
        try:
            data = parse_frontmatter(path)
        except ValueError as e:
            if not args.include_broken:
                continue
            payload = {
                "broken": True,
                "error": str(e),
                "path": str(path),
            }
            if args.long:
                payload["mtime_epoch_sec"] = int(stat.st_mtime)
                payload["size_bytes"] = int(stat.st_size)
            if query:
                if query not in str(path).lower():
                    continue
            items.append(payload)
            if args.limit is not None and args.limit > 0 and len(items) >= args.limit:
                break
            continue
        name = data.get("name")
        description = data.get("description")
        if not name or not description:
            if not args.include_broken:
                continue
            payload = {
                "broken": True,
                "error": "frontmatter missing name/description",
                "path": str(path),
            }
            if args.long:
                payload["mtime_epoch_sec"] = int(stat.st_mtime)
                payload["size_bytes"] = int(stat.st_size)
            if query:
                if query not in str(path).lower():
                    continue
            items.append(payload)
            if args.limit is not None and args.limit > 0 and len(items) >= args.limit:
                break
            continue
        if query:
            haystack = f"{name} {description}".lower()
            if query not in haystack:
                continue
        payload = {"name": name, "description": description, "path": str(path)}
        if args.long:
            payload["mtime_epoch_sec"] = int(stat.st_mtime)
            payload["size_bytes"] = int(stat.st_size)
        items.append(payload)

        if args.limit is not None and args.limit > 0 and len(items) >= args.limit:
            break

    if args.json:
        print(json.dumps(items))
    else:
        for item in items:
            if args.paths:
                print(item["path"])
                continue

            prefix = ""
            if args.long:
                mtime = _dt.datetime.fromtimestamp(
                    item.get("mtime_epoch_sec", 0)
                ).isoformat(timespec="seconds")
                size = item.get("size_bytes", 0)
                prefix = f"{mtime}\t{size}\t"

            if item.get("broken"):
                error = item.get("error", "invalid frontmatter")
                print(f"{prefix}BROKEN\t{error}\t{item['path']}")
            else:
                print(f"{prefix}{item['name']}\t{item['description']}\t{item['path']}")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())

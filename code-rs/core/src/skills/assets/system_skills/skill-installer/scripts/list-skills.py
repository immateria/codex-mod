#!/usr/bin/env python3
"""Compatibility wrapper for listing curated skills.

Upstream skill-installer uses `list-skills.py`. This fork keeps the more explicit
`list-curated-skills.py` name, but provides this alias for convenience.
"""

from __future__ import annotations

from pathlib import Path
import runpy
import sys


def main() -> int:
    target = Path(__file__).resolve().with_name("list-curated-skills.py")
    if not target.is_file():
        raise SystemExit(f"Missing script: {target}")

    sys.argv[0] = str(target)
    runpy.run_path(str(target), run_name="__main__")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())


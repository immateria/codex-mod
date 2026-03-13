#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)"
# Prefer git to locate the repository root when available, but gracefully fall
# back to the script directory so source tarballs without .git still work.
ROOT_DIR="$(cd -- "${SCRIPT_DIR}/.." >/dev/null 2>&1 && pwd)"
if command -v git >/dev/null 2>&1; then
  if git -C "$ROOT_DIR" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    ROOT_DIR="$(git -C "$ROOT_DIR" rev-parse --show-toplevel)"
  fi
fi

OVERLAY_CONTENTS_DIR="$ROOT_DIR/code-rs/tui/src/chatwidget/settings_overlay/contents"

if [[ ! -d "$OVERLAY_CONTENTS_DIR" ]]; then
  echo "ERROR: Expected directory not found: $OVERLAY_CONTENTS_DIR" >&2
  exit 1
fi

if ! command -v rg >/dev/null 2>&1; then
  echo "(rg not found; skipping settings overlay chrome guard)"
  exit 0
fi

violations=0

echo "Running settings overlay chrome guard…"

checks_fixed=(
  # Overlay content must not depend on BottomPaneView; it should render via
  # content_only wrappers so nested frames can't sneak back in.
  "BottomPaneView"
)

checks_regex=(
  # Ban calling framed render directly on overlay content views.
  "\\.view\\.render\\s*\\("
  # Ban framed wrappers in overlay content.
  "\\.framed_mut\\s*\\("
  "\\.framed\\s*\\("
)

for pat in "${checks_fixed[@]}"; do
  raw_matches="$(cd "$ROOT_DIR" && rg -n -F "$pat" "$OVERLAY_CONTENTS_DIR" --glob '*.rs' || true)"
  # Avoid false positives for reminders in comments like:
  #   // Don't use BottomPaneView here.
  matches="$(printf '%s' "$raw_matches" | rg -v ":[0-9]+:\\s*(//|/\\*|\\*)" || true)"
  if [[ -n "$matches" ]]; then
    echo "ERROR: Forbidden token in settings overlay contents: $pat" >&2
    echo "$matches" >&2
    violations=1
  fi
done

for pat in "${checks_regex[@]}"; do
  matches="$(cd "$ROOT_DIR" && rg -n -e "$pat" "$OVERLAY_CONTENTS_DIR" --glob '*.rs' || true)"
  if [[ -n "$matches" ]]; then
    echo "ERROR: Forbidden pattern in settings overlay contents: $pat" >&2
    echo "$matches" >&2
    violations=1
  fi
done

if [[ $violations -ne 0 ]]; then
  echo "" >&2
  echo "ERROR: Overlay contents must render content-only (no nested frames)." >&2
  echo "Use view.content_only().render(...) and view.content_only_mut() for mouse." >&2
  exit 1
fi

echo "OK: No forbidden overlay chrome patterns found."

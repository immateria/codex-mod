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

TUI_SRC_DIR="$ROOT_DIR/code-rs/tui/src"
SEARCH_ROOT="code-rs/tui/src"
SETTINGS_UI_GLOB="${SEARCH_ROOT}/bottom_pane/settings_ui/**"

if [[ ! -d "$TUI_SRC_DIR" ]]; then
  echo "ERROR: Expected directory not found: $TUI_SRC_DIR" >&2
  exit 1
fi

if ! command -v rg >/dev/null 2>&1; then
  echo "(rg not found; skipping settings_ui chrome API guard)"
  exit 0
fi

violations=0

echo "Running settings_ui chrome API guard…"

patterns=(
  '.layout_content('
  '.render_content_shell('
  '.render_content_menu_rows('
  '.render_content_runs('
  '.render_content_with_standard_actions_end('
  '.render_content_with_standard_actions('
)

for pat in "${patterns[@]}"; do
  matches="$(cd "$ROOT_DIR" && rg -n -F "$pat" "$SEARCH_ROOT" --glob '*.rs' --glob "!${SETTINGS_UI_GLOB}" || true)"
  if [[ -n "$matches" ]]; then
    echo "ERROR: Forbidden chrome split API used outside settings_ui/: $pat" >&2
    echo "$matches" >&2
    violations=1
  fi
done

if [[ $violations -ne 0 ]]; then
  echo "" >&2
  echo "ERROR: Use mode-bound wrappers (framed()/content_only()) instead of raw split helpers." >&2
  exit 1
fi

echo "OK: No forbidden settings_ui chrome split APIs used outside settings_ui/."

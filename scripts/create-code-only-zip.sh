#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: scripts/create-code-only-zip.sh [-o output.zip]

Creates a code-focused .zip archive from tracked, modified, and untracked
non-ignored files, while excluding common build/cache directories.

Options:
  -o, --output PATH   Output zip path (default: ../<repo>-code-only-<timestamp>.zip)
  -h, --help          Show this help

Environment:
  ZIP_CODE_ONLY_EXCLUDE_NAMES
    Comma-separated extra path segment names to exclude.
    Example: ZIP_CODE_ONLY_EXCLUDE_NAMES="vendor,tmp"
USAGE
}

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/.." >/dev/null 2>&1 && pwd)"

if ! command -v git >/dev/null 2>&1; then
  echo "ERROR: git is required." >&2
  exit 1
fi

if ! command -v python3 >/dev/null 2>&1; then
  echo "ERROR: python3 is required." >&2
  exit 1
fi

if ! git -C "$REPO_ROOT" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  echo "ERROR: ${REPO_ROOT} is not inside a git working tree." >&2
  exit 1
fi

timestamp="$(date +%Y%m%d-%H%M%S)"
repo_name="$(basename "$REPO_ROOT")"
default_output="${REPO_ROOT%/*}/${repo_name}-code-only-${timestamp}.zip"
output_path="$default_output"

while [[ $# -gt 0 ]]; do
  case "$1" in
    -o|--output)
      shift
      if [[ $# -eq 0 ]]; then
        echo "ERROR: --output requires a path." >&2
        exit 1
      fi
      output_path="$1"
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "ERROR: Unknown argument: $1" >&2
      usage
      exit 1
      ;;
  esac
  shift
done

exclude_names=".git,.code,target,incremental,_target-cache,node_modules,.next,.cache,.turbo,dist,build,coverage,out"
if [[ -n "${ZIP_CODE_ONLY_EXCLUDE_NAMES:-}" ]]; then
  exclude_names="${exclude_names},${ZIP_CODE_ONLY_EXCLUDE_NAMES}"
fi

python3 - "$REPO_ROOT" "$output_path" "$exclude_names" <<'PY'
import subprocess
import sys
import zipfile
from pathlib import Path
repo_root = Path(sys.argv[1]).resolve()
output_path = Path(sys.argv[2]).expanduser().resolve()
output_path = Path(sys.argv[2]).expanduser()
exclude_names_raw = sys.argv[3]
exclude_names = {name.strip() for name in exclude_names_raw.split(",") if name.strip()}

archive_rel_path = None
try:
    archive_rel_path = output_path.relative_to(repo_root).as_posix()
except ValueError:
    archive_rel_path = None

proc = subprocess.run(
    [
        "git",
        "--no-pager",
        "ls-files",
        "--cached",
        "--modified",
        "--others",
        "--exclude-standard",
        "-z",
    ],
    cwd=repo_root,
    check=True,
    stdout=subprocess.PIPE,
)

all_files = [p for p in proc.stdout.decode("utf-8", errors="surrogateescape").split("\x00") if p]
selected_files = []
seen_files = set()
for rel in all_files:
    if archive_rel_path and rel == archive_rel_path:
        continue
    parts = Path(rel).parts
    if any(part in exclude_names for part in parts):
        continue
    if rel in seen_files:
        continue
    seen_files.add(rel)
    selected_files.append(rel)

output_path.parent.mkdir(parents=True, exist_ok=True)

written_files = 0
non_file_skipped = 0
with zipfile.ZipFile(output_path, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as zf:
    for rel in selected_files:
        full_path = repo_root / rel
        if full_path.is_file():
            zf.write(full_path, arcname=rel)
            written_files += 1
        else:
            non_file_skipped += 1

with zipfile.ZipFile(output_path, "r") as zf:
    archive_entries = zf.namelist()

forbidden_entries = []
for entry in archive_entries:
    parts = Path(entry).parts
    if any(part in exclude_names for part in parts):
        forbidden_entries.append(entry)

print(f"ZIP_PATH={output_path}")
print(f"INPUT_FILES={len(all_files)}")
print(f"SELECTED_FILES={len(selected_files)}")
print(f"ZIPPED_FILES={written_files}")
print(f"NON_FILE_SKIPPED={non_file_skipped}")
print(f"ARCHIVE_ENTRIES={len(archive_entries)}")
print("EXCLUDE_NAMES=" + ",".join(sorted(exclude_names)))
if forbidden_entries:
    print("FORBIDDEN_MATCHES_FOUND=1")
    print("FORBIDDEN_EXAMPLES_START")
    for line in forbidden_entries[:50]:
        print(line)
    print("FORBIDDEN_EXAMPLES_END")
    sys.exit(2)
print("FORBIDDEN_MATCHES_FOUND=0")
PY

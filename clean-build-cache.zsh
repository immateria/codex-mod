#!/usr/bin/env zsh
# Clean Rust build artifacts for code-rs plus build-fast.sh target caches.
set -euo pipefail
emulate -L zsh

SCRIPT_DIR="$(cd -- "$(dirname -- "${(%):-%x}")" >/dev/null 2>&1 && pwd)"
REPO_ROOT="$SCRIPT_DIR"

log_info() {
  print "[clean-build-cache] $*"
}

if [[ "$SCRIPT_DIR" == */.code/working/*/branches/* ]]; then
  WORKTREE_PARENT="${SCRIPT_DIR%/branches/*}"
  REPO_NAME="${WORKTREE_PARENT:t}"
else
  REPO_NAME="${SCRIPT_DIR:t}"
fi

if [[ ! -d "$REPO_ROOT/code-rs" ]]; then
  print -u2 "[clean-build-cache] error: code-rs directory not found under $REPO_ROOT"
  exit 1
fi

log_info "repo root: $REPO_ROOT"
log_info "repo name key: $REPO_NAME"

log_info "running cargo clean in code-rs"
(
  cd "$REPO_ROOT/code-rs"
  cargo clean
)

# build-fast.sh chooses CACHE_HOME from:
# - CODEX_HOME (if set)
# - /mnt/data/.code (if writable)
# - repo-local .code
# We clean all candidate roots so cache location changes do not leave stale artifacts.
typeset -a cache_roots
if [[ -n "${CODEX_HOME:-}" ]]; then
  if [[ "$CODEX_HOME" == /* ]]; then
    cache_roots+=("${CODEX_HOME%/}")
  else
    cache_roots+=("${REPO_ROOT}/${CODEX_HOME#./}")
  fi
fi
cache_roots+=("/mnt/data/.code")
cache_roots+=("${REPO_ROOT}/.code")

typeset -a unique_roots
typeset -A seen_root
for root in "${cache_roots[@]}"; do
  [[ -z "$root" ]] && continue
  if [[ -z "${seen_root[$root]-}" ]]; then
    unique_roots+=("$root")
    seen_root[$root]=1
  fi
done

removed_any=0
for root in "${unique_roots[@]}"; do
  cache_dir="$root/working/_target-cache/$REPO_NAME"
  if [[ -e "$cache_dir" ]]; then
    log_info "removing $cache_dir"
    rm -rf -- "$cache_dir"
    removed_any=1
  else
    log_info "skip (not present): $cache_dir"
  fi
done

if [[ $removed_any -eq 0 ]]; then
  log_info "no build-fast target-cache directories found"
else
  log_info "done removing build-fast target caches"
fi

log_info "cleanup complete"

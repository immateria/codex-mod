#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage:
  ./scripts/upstream-merge/report-upstream-delta.sh [--out PATH] [UPSTREAM_REF] [LOCAL_REF]

Defaults:
  UPSTREAM_REF = upstream/main
  LOCAL_REF   = HEAD

This prints a concise merge-readiness report:
- ahead/behind counts
- merge-base
- churn + dirstat on each side since merge-base
- files changed on both sides (likely conflict hotspots)
USAGE
}

ROOT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." >/dev/null 2>&1 && pwd)"
cd "$ROOT_DIR"

OUT_PATH=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    -h|--help)
      usage
      exit 0
      ;;
    --out)
      OUT_PATH="${2:-}"
      shift 2
      ;;
    *)
      break
      ;;
  esac
done

UPSTREAM_REF="${1:-upstream/main}"
LOCAL_REF="${2:-HEAD}"

git rev-parse --verify "$UPSTREAM_REF^{commit}" >/dev/null 2>&1 || {
  echo "Error: upstream ref not found: $UPSTREAM_REF" >&2
  exit 1
}
git rev-parse --verify "$LOCAL_REF^{commit}" >/dev/null 2>&1 || {
  echo "Error: local ref not found: $LOCAL_REF" >&2
  exit 1
}

BASE_REF="$(git merge-base "$LOCAL_REF" "$UPSTREAM_REF")"
LOCAL_SHA="$(git rev-parse --short "$LOCAL_REF")"
UPSTREAM_SHA="$(git rev-parse --short "$UPSTREAM_REF")"
BASE_SHA="$(git rev-parse --short "$BASE_REF")"

ahead_count="$(git rev-list --count "${LOCAL_REF}..${UPSTREAM_REF}")"
behind_count="$(git rev-list --count "${UPSTREAM_REF}..${LOCAL_REF}")"

worktree_dirty="no"
if [ -n "$(git status --porcelain)" ]; then
  worktree_dirty="yes"
fi

top_churn() {
  local from="$1"
  local to="$2"
  git diff --numstat "$from..$to" \
    | awk '{
        add=$1; del=$2; file=$3;
        if (add=="-" || del=="-") next;
        churn=add+del;
        printf("%8d %6d %6d %s\n", churn, add, del, file)
      }' \
    | sort -nr \
    | head -n 25
}

top_dirs() {
  local from="$1"
  local to="$2"
  git diff --dirstat=files,0 "$from..$to" | head -n 25
}

both_changed() {
  local tmp_a
  local tmp_b
  tmp_a="$(mktemp)"
  tmp_b="$(mktemp)"
  git diff --name-only "$BASE_REF..$LOCAL_REF" | sort > "$tmp_a"
  git diff --name-only "$BASE_REF..$UPSTREAM_REF" | sort > "$tmp_b"
  comm -12 "$tmp_a" "$tmp_b"
  rm -f "$tmp_a" "$tmp_b"
}

both_list="$(both_changed || true)"
both_count="$(printf "%s" "$both_list" | sed '/^$/d' | wc -l | tr -d ' ')"

report() {
  cat <<EOF
# Upstream Merge Delta Report

- Local:   $LOCAL_REF ($LOCAL_SHA)
- Upstream: $UPSTREAM_REF ($UPSTREAM_SHA)
- Merge-base: $BASE_REF ($BASE_SHA)
- Upstream ahead: $ahead_count commits
- Local ahead:    $behind_count commits
- Worktree dirty: $worktree_dirty

## Likely Conflict Hotspots (Changed On Both Sides)

Count: $both_count
$(printf "%s\n" "$both_list" | sed '/^$/d' | head -n 120)

## Local Churn Since Merge-Base (Top Files)

$(top_churn "$BASE_REF" "$LOCAL_REF")

## Upstream Churn Since Merge-Base (Top Files)

$(top_churn "$BASE_REF" "$UPSTREAM_REF")

## Local Dir Share Since Merge-Base

$(top_dirs "$BASE_REF" "$LOCAL_REF")

## Upstream Dir Share Since Merge-Base

$(top_dirs "$BASE_REF" "$UPSTREAM_REF")
EOF
}

if [ -n "$OUT_PATH" ]; then
  mkdir -p "$(dirname "$OUT_PATH")" 2>/dev/null || true
  report > "$OUT_PATH"
  echo "OK: wrote $OUT_PATH"
else
  report
fi


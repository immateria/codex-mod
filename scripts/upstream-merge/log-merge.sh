#!/usr/bin/env bash
set -euo pipefail

# log-merge.sh: Log upstream merge activity and decisions
#
# This script helps track merge decisions, conflicts, and resolutions
# for future reference. It creates structured logs that can be referenced
# when doing subsequent merges.
#
# Usage:
#   ./scripts/upstream-merge/log-merge.sh init <upstream-ref>
#   ./scripts/upstream-merge/log-merge.sh note <category> <message>
#   ./scripts/upstream-merge/log-merge.sh decision <crate> <action> <reason>
#   ./scripts/upstream-merge/log-merge.sh finalize
#
# Examples:
#   ./scripts/upstream-merge/log-merge.sh init upstream/main
#   ./scripts/upstream-merge/log-merge.sh note conflict "core/src/codex.rs has protocol changes"
#   ./scripts/upstream-merge/log-merge.sh decision core preserve "Keep fork's QueueUserInput support"
#   ./scripts/upstream-merge/log-merge.sh finalize

ROOT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." >/dev/null 2>&1 && pwd)"
cd "$ROOT_DIR"

LOGS_DIR="docs/maintenance/upstream-merge-logs"
mkdir -p "$LOGS_DIR"

# Get current timestamp
timestamp() {
    date -u +%Y-%m-%d\ %H:%M:%S\ UTC
}

# Get current date for log filename
log_date() {
    date -u +%Y%m%d
}

# Initialize a new merge log
init_merge_log() {
    local upstream_ref="$1"
    local log_file="${LOGS_DIR}/merge-$(log_date).md"

    if [[ -f "$log_file" ]]; then
        echo "WARNING: Log file already exists: $log_file"
        read -p "Overwrite? (y/N) " -n 1 -r
        echo
        if [[ ! $REPLY =~ ^[Yy]$ ]]; then
            echo "Aborted."
            exit 1
        fi
    fi

    # Get upstream commit info
    local upstream_commit=$(git rev-parse "$upstream_ref" 2>/dev/null || echo "unknown")
    local upstream_date=$(git show -s --format=%ci "$upstream_ref" 2>/dev/null || echo "unknown")
    local fork_commit=$(git rev-parse HEAD)
    local fork_branch=$(git branch --show-current)

    cat > "$log_file" <<HEADER
# Upstream Merge Log - $(log_date)

**Started:** $(timestamp)
**Upstream Ref:** ${upstream_ref} (${upstream_commit})
**Upstream Date:** ${upstream_date}
**Fork Branch:** ${fork_branch} (${fork_commit})

## Overview

This log tracks the merge of upstream changes into the fork.

## Merge Strategy

- [ ] Review diff summary (\`diff-crates.sh --all\`)
- [ ] Identify critical changes (\`highlight-critical-changes.sh --all\`)
- [ ] Plan integration approach
- [ ] Apply changes crate-by-crate
- [ ] Verify build and tests
- [ ] Update parity tracker

## Critical Changes

HEADER

    echo "OK: Initialized merge log: $log_file"
    echo "$log_file"
}

# Add a note to the current merge log
add_note() {
    local category="$1"
    shift
    local message="$*"

    local log_file=$(ls -t "${LOGS_DIR}"/merge-*.md 2>/dev/null | head -1)

    if [[ -z "$log_file" ]]; then
        echo "ERROR: No active merge log found. Run 'init' first."
        exit 1
    fi

    echo "### ${category} - $(timestamp)" >> "$log_file"
    echo "" >> "$log_file"
    echo "${message}" >> "$log_file"
    echo "" >> "$log_file"

    echo "NOTE: Added note to $log_file"
}

# Log a merge decision for a specific crate
log_decision() {
    local crate="$1"
    local action="$2"
    local reason="$3"

    local log_file=$(ls -t "${LOGS_DIR}"/merge-*.md 2>/dev/null | head -1)

    if [[ -z "$log_file" ]]; then
        echo "ERROR: No active merge log found. Run 'init' first."
        exit 1
    fi

    # Check if Decisions section exists, create if not
    if ! grep -q "^## Decisions$" "$log_file"; then
        echo "" >> "$log_file"
        echo "## Decisions" >> "$log_file"
        echo "" >> "$log_file"
        echo "| Crate | Action | Reason | Timestamp |" >> "$log_file"
        echo "|-------|--------|--------|-----------|" >> "$log_file"
    fi

    # Add decision row
    echo "| ${crate} | ${action} | ${reason} | $(timestamp) |" >> "$log_file"

    echo "NOTE: Logged decision for ${crate}: ${action}"
}

# Finalize the merge log
finalize_log() {
    local log_file=$(ls -t "${LOGS_DIR}"/merge-*.md 2>/dev/null | head -1)

    if [[ -z "$log_file" ]]; then
        echo "ERROR: No active merge log found. Run 'init' first."
        exit 1
    fi

    # Add completion timestamp
    cat >> "$log_file" <<FOOTER

## Completion

**Finished:** $(timestamp)

### Post-Merge Checklist

- [ ] All critical changes reviewed and integrated
- [ ] Build passes (\`./build-fast.sh\`)
- [ ] Tests pass (\`./scripts/ci-tests.sh\`)
- [ ] Verification guards pass (\`./scripts/upstream-merge/verify.sh\`)
- [ ] Parity tracker updated (\`docs/code-crate-parity-tracker.md\`)
- [ ] Changelog updated if user-facing changes
- [ ] Commit created with merge details

### Files Modified

\`\`\`
$(git status --short)
\`\`\`

### Next Upstream Sync

Recommended: Check upstream weekly for new changes
Command: \`git fetch upstream && git log HEAD..upstream/main --oneline\`

FOOTER

    echo "OK: Finalized merge log: $log_file"
    echo ""
    echo "Next steps:"
    echo "  1. Complete post-merge checklist in the log"
    echo "  2. Commit changes with reference to log file"
    echo "  3. Update docs/code-crate-parity-tracker.md"
}

# Generate a summary of all merge logs
summarize_logs() {
    echo "# Historical Merge Summary"
    echo ""
    echo "All upstream merge logs:"
    echo ""

    for log_file in "${LOGS_DIR}"/merge-*.md; do
        if [[ -f "$log_file" ]]; then
            local filename=$(basename "$log_file")
            local date=$(echo "$filename" | sed 's/merge-\(.*\)\.md/\1/')
            local started=$(grep "^\*\*Started:\*\*" "$log_file" | sed 's/\*\*Started:\*\* //')
            local upstream=$(grep "^\*\*Upstream Ref:\*\*" "$log_file" | sed 's/\*\*Upstream Ref:\*\* //')

            echo "## ${date}"
            echo ""
            echo "- File: [\`${filename}\`](./upstream-merge-logs/${filename})"
            echo "- Started: ${started}"
            echo "- Upstream: ${upstream}"
            echo ""
        fi
    done
}

# Main logic
main() {
    if [[ $# -eq 0 ]]; then
        echo "Usage: $0 <command> [args...]"
        echo ""
        echo "Commands:"
        echo "  init <upstream-ref>           Initialize new merge log"
        echo "  note <category> <message>     Add a note to current log"
        echo "  decision <crate> <action> <reason>  Log a merge decision"
        echo "  finalize                      Finalize current merge log"
        echo "  summary                       Show all merge logs"
        echo ""
        echo "Examples:"
        echo "  $0 init upstream/main"
        echo "  $0 note conflict 'core/src/codex.rs has protocol changes'"
        echo "  $0 decision core preserve 'Keep fork QueueUserInput support'"
        echo "  $0 finalize"
        exit 1
    fi

    local command="$1"
    shift

    case "$command" in
        init)
            if [[ $# -lt 1 ]]; then
                echo "ERROR: Usage: $0 init <upstream-ref>"
                exit 1
            fi
            init_merge_log "$@"
            ;;
        note)
            if [[ $# -lt 2 ]]; then
                echo "ERROR: Usage: $0 note <category> <message>"
                exit 1
            fi
            add_note "$@"
            ;;
        decision)
            if [[ $# -lt 3 ]]; then
                echo "ERROR: Usage: $0 decision <crate> <action> <reason>"
                exit 1
            fi
            log_decision "$@"
            ;;
        finalize)
            finalize_log
            ;;
        summary)
            summarize_logs
            ;;
        *)
            echo "ERROR: Unknown command: $command"
            exit 1
            ;;
    esac
}

main "$@"

#!/usr/bin/env bash
set -euo pipefail

# diff-crates.sh: Compare codex-rs vs code-rs per crate
#
# Usage:
#   ./scripts/upstream-merge/diff-crates.sh [crate-name]
#   ./scripts/upstream-merge/diff-crates.sh --all
#   ./scripts/upstream-merge/diff-crates.sh --summary
#
# Examples:
#   ./scripts/upstream-merge/diff-crates.sh core
#   ./scripts/upstream-merge/diff-crates.sh --all
#   ./scripts/upstream-merge/diff-crates.sh --summary

ROOT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." >/dev/null 2>&1 && pwd)"
cd "$ROOT_DIR"

OUTPUT_DIR=".github/auto/upstream-diffs"
mkdir -p "$OUTPUT_DIR"

# List of crates that exist in both codex-rs and code-rs
SHARED_CRATES=(
    "ansi-escape"
    "app-server"
    "app-server-protocol"
    "apply-patch"
    "arg0"
    "backend-client"
    "chatgpt"
    "cli"
    "cloud-tasks"
    "cloud-tasks-client"
    "common"
    "core"
    "exec"
    "execpolicy"
    "file-search"
    "git-apply"
    "git-tooling"
    "linux-sandbox"
    "login"
    "mcp-client"
    "mcp-server"
    "mcp-types"
    "ollama"
    "otel"
    "protocol"
    "protocol-ts"
    "process-hardening"
    "rmcp-client"
    "responses-api-proxy"
    "tui"
)

# Function to diff a single crate
diff_crate() {
    local crate_name="$1"
    local codex_path="codex-rs/${crate_name}"
    local code_path="code-rs/${crate_name}"

    # Check if both directories exist
    if [[ ! -d "$codex_path" ]]; then
        echo "WARNING: $codex_path does not exist"
        return 1
    fi

    if [[ ! -d "$code_path" ]]; then
        echo "WARNING: $code_path does not exist (fork-only crate)"
        return 1
    fi

    local output_file="${OUTPUT_DIR}/${crate_name}.diff"

    echo "Comparing ${crate_name}..."

    # Prepare sanitized copy of the fork crate so simple `code-*/code_*`
    # branding changes do not overwhelm the diff output.
    local temp_dir
    temp_dir=$(mktemp -d)
    local sanitized_path="${temp_dir}/${crate_name}"

    rsync -a --delete "$code_path/" "$sanitized_path/"
    find "$sanitized_path" -name '.DS_Store' -delete

    # Rewrite code-* / code_* identifiers back to codex-* so the diff highlights
    # real behavioural changes instead of fork re-branding noise. Limit to
    # textual sources we care about (.toml/.rs).
    find "$sanitized_path" -type f \( -name '*.toml' -o -name '*.rs' -o -name '*.md' -o -name '*.json' \) -print0 |
        xargs -0 perl -pi -e 's/\bcode-([a-z0-9_]+)/codex-$1/g; s/\bcode_([a-z0-9_]+)/codex_$1/g'

    # Generate diff with context using the sanitized fork copy
    if diff -Naur --exclude="target" --exclude="*.lock" --exclude="node_modules" --exclude=".DS_Store" \
        "$codex_path" "$sanitized_path" > "$output_file" 2>&1; then
        echo "   OK: No differences found"
        rm "$output_file"
        rm -rf "$temp_dir"
        return 0
    else
        local line_count=$(wc -l < "$output_file")
        echo "   NOTE: Differences found: ${line_count} lines written to ${output_file}"
        rm -rf "$temp_dir"
        return 0
    fi
}

# Function to generate summary
generate_summary() {
    echo "Generating diff summary..."
    local summary_file="${OUTPUT_DIR}/SUMMARY.md"

    local timestamp
    timestamp=$(date -u +"%Y-%m-%d %H:%M:%S UTC")

    cat > "$summary_file" <<HEADER
# Upstream Diff Summary

Generated: ${timestamp}

This report compares \`codex-rs\` (upstream baseline) vs \`code-rs\` (fork) for each shared crate.

## Overview

HEADER

    echo "| Crate | Status | Diff Size | Notes |" >> "$summary_file"
    echo "|-------|--------|-----------|-------|" >> "$summary_file"

    for crate in "${SHARED_CRATES[@]}"; do
        local diff_file="${OUTPUT_DIR}/${crate}.diff"

        if [[ ! -f "$diff_file" ]]; then
            echo "| ${crate} | Identical | 0 lines | - |" >> "$summary_file"
        else
            local line_count=$(wc -l < "$diff_file")
            echo "| ${crate} | Differs | ${line_count} lines | See \`${crate}.diff\` |" >> "$summary_file"
        fi
    done

    echo "" >> "$summary_file"
    echo "## Crates with Differences" >> "$summary_file"
    echo "" >> "$summary_file"

    for crate in "${SHARED_CRATES[@]}"; do
        local diff_file="${OUTPUT_DIR}/${crate}.diff"

        if [[ -f "$diff_file" ]]; then
            echo "### ${crate}" >> "$summary_file"
            echo "" >> "$summary_file"
            echo "Diff file: \`${crate}.diff\`" >> "$summary_file"
            echo "" >> "$summary_file"

            # Extract added/removed line counts
            local added=$(grep -cE '^\+[^+]' "$diff_file" || echo "0")
            local removed=$(grep -cE '^-[^-]' "$diff_file" || echo "0")
            echo "- Lines added: ${added}" >> "$summary_file"
            echo "- Lines removed: ${removed}" >> "$summary_file"
            echo "" >> "$summary_file"
        fi
    done

    echo "OK: Summary written to ${summary_file}"
    cat "$summary_file"
}

# Main logic
main() {
    if [[ $# -eq 0 ]]; then
        echo "Usage: $0 [crate-name | --all | --summary]"
        echo ""
        echo "Available crates:"
        printf "  %s\n" "${SHARED_CRATES[@]}"
        exit 1
    fi

    case "$1" in
        --all)
            echo "Comparing all shared crates..."
            rm -f "${OUTPUT_DIR}"/*.diff
            for crate in "${SHARED_CRATES[@]}"; do
                diff_crate "$crate"
            done
            generate_summary
            ;;
        --summary)
            generate_summary
            ;;
        *)
            diff_crate "$1"
            ;;
    esac
}

main "$@"

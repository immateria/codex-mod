#!/usr/bin/env python3
"""
Skill Initializer - Creates a new skill from template.

Usage:
    init_skill.py <skill-name> [--path <dir>] [--resources scripts,references,assets] [--examples]
                 [--shell-style <style>] [--attach-mode <mode>] [--config <config.toml>] [--apply-config]

Examples:
    # Create a normal skill under CODE_HOME/skills/<skill-name>
    init_skill.py my-skill

    # Create a skill in a custom root
    init_skill.py my-skill --path ./skills/public

    # Create a Zsh-specific skill, stored outside the default skill roots, and attach it
    init_skill.py zsh-arrays --shell-style zsh --apply-config

Notes:
    - When --shell-style is set and --path is omitted, this script defaults to:
        CODE_HOME/skills-style/<style>/<skill-name>
      This keeps the skill style-scoped (it won't be discovered outside that style unless you add the root elsewhere).
    - Config updates are best-effort text edits. If your config already has a complex
      shell_style_profiles table, the script will print a TOML snippet instead of guessing.
"""

from __future__ import annotations

import argparse
import os
import re
import sys
from dataclasses import dataclass
from pathlib import Path

MAX_SKILL_NAME_LENGTH = 64
ALLOWED_RESOURCES = {"scripts", "references", "assets"}


SKILL_TEMPLATE = """---
name: {skill_name}
description: "TODO: Describe what this skill does and, critically, when to use it. Include concrete triggers: file types, tools, workflows, or recurring tasks."
---

# {skill_title}

[TODO: 1-2 sentences on what this skill enables.]

## Overview

[TODO: Add the core workflow/capabilities. Prefer short, actionable guidance over long explanations.]

## Resources (optional)

Create only the resource directories this skill actually needs. Delete this section if no resources are required.

### scripts/
Executable code (Python/Bash/etc.) that can be run directly.

### references/
Documentation intended to be loaded into context when needed.

### assets/
Templates or other static files used in outputs (not meant to be loaded into context).
"""

EXAMPLE_SCRIPT = '''#!/usr/bin/env python3
"""
Example helper script for {skill_name}
"""

def main() -> None:
    print("This is an example script for {skill_name}")

if __name__ == "__main__":
    main()
'''

EXAMPLE_REFERENCE = """# Reference Documentation for {skill_title}

This is an example reference document. Replace with real reference content or delete.
"""

EXAMPLE_ASSET = """This is an example asset placeholder.
Replace with actual templates/data files or delete.
"""


class ConfigPatchError(Exception):
    pass


@dataclass(frozen=True)
class ShellStyle:
    canonical_key: str

    @staticmethod
    def parse(value: str) -> "ShellStyle":
        v = value.strip().lower()
        if v in ("posix-sh", "posix", "sh"):
            return ShellStyle("posix-sh")
        if v in ("bash-zsh-compatible", "bash-zsh"):
            return ShellStyle("bash-zsh-compatible")
        if v in ("zsh", "zsh-idiomatic"):
            return ShellStyle("zsh")
        raise ValueError(
            "unknown style (expected one of: posix-sh, bash-zsh-compatible, zsh)"
        )


def normalize_skill_name(skill_name: str) -> str:
    """Normalize a skill name to lowercase hyphen-case."""
    normalized = skill_name.strip().lower()
    normalized = re.sub(r"[ _]+", "-", normalized)
    normalized = re.sub(r"[^a-z0-9-]", "", normalized)
    normalized = re.sub(r"-+", "-", normalized)
    normalized = normalized.strip("-")
    return normalized


def title_case_skill_name(skill_name: str) -> str:
    """Convert hyphenated skill name to Title Case for display."""
    return " ".join(word.capitalize() for word in skill_name.split("-"))


def parse_resources(raw_resources: str) -> list[str]:
    if not raw_resources:
        return []
    resources = [item.strip() for item in raw_resources.split(",") if item.strip()]
    invalid = sorted({item for item in resources if item not in ALLOWED_RESOURCES})
    if invalid:
        allowed = ", ".join(sorted(ALLOWED_RESOURCES))
        raise ValueError(f"unknown resource type(s): {', '.join(invalid)} (allowed: {allowed})")

    deduped: list[str] = []
    seen: set[str] = set()
    for resource in resources:
        if resource not in seen:
            deduped.append(resource)
            seen.add(resource)
    return deduped


def find_code_home(explicit: str | None) -> Path:
    if explicit:
        return Path(explicit).expanduser()

    for key in ("CODE_HOME", "CODEX_HOME"):
        value = os.environ.get(key, "").strip()
        if value:
            return Path(value).expanduser()

    return Path.home() / ".codex"


def default_config_path(code_home: Path) -> Path:
    return code_home / "config.toml"


def _toml_escape_string(value: str) -> str:
    return value.replace("\\", "\\\\").replace('"', '\\"')


def _table_header_regex(style_key: str) -> re.Pattern[str]:
    # Accept both bare and quoted keys:
    #   [shell_style_profiles.zsh]
    #   [shell_style_profiles."bash-zsh-compatible"]
    escaped = re.escape(style_key)
    return re.compile(
        r"^\s*\[shell_style_profiles\.(?:"
        + escaped
        + r"|\""
        + escaped
        + r"\")\]\s*$"
    )


def _is_table_header(line: str) -> bool:
    return bool(re.match(r"^\s*\[.*\]\s*$", line))


def _find_style_table_span(lines: list[str], style_key: str) -> tuple[int, int] | None:
    header_re = _table_header_regex(style_key)
    start: int | None = None
    for i, line in enumerate(lines):
        if header_re.match(line):
            start = i
            break
    if start is None:
        return None
    end = len(lines)
    for j in range(start + 1, len(lines)):
        if _is_table_header(lines[j]):
            end = j
            break
    return (start, end)


def _find_assignment_span(
    lines: list[str],
    start: int,
    end: int,
    key: str,
) -> tuple[int, int] | None:
    # Returns [span_start, span_end) indices for the assignment block.
    key_re = re.compile(r"^\s*" + re.escape(key) + r"\s*=")
    for i in range(start, end):
        if not key_re.match(lines[i]):
            continue
        # Single-line array?
        if "[" in lines[i] and "]" in lines[i]:
            return (i, i + 1)
        # Multi-line array: scan forward until closing bracket.
        for j in range(i + 1, end):
            if "]" in lines[j]:
                return (i, j + 1)
        return (i, i + 1)
    return None


def _extract_toml_string_literals(text: str) -> list[str]:
    # Extracts double-quoted strings, tolerating basic escapes.
    pattern = re.compile(r'"((?:[^"\\\\]|\\\\.)*)"')
    return [m.group(1) for m in pattern.finditer(text)]

def _toml_unescape_basic(value: str) -> str:
    # Minimal unescape for our needs: handles \\ and \"
    out: list[str] = []
    i = 0
    while i < len(value):
        ch = value[i]
        if ch == "\\" and i + 1 < len(value):
            nxt = value[i + 1]
            if nxt in ("\\", '"'):
                out.append(nxt)
                i += 2
                continue
        out.append(ch)
        i += 1
    return "".join(out)


def _ensure_list_contains_path(
    lines: list[str],
    span: tuple[int, int],
    value_to_add: str,
    key: str,
) -> tuple[bool, list[str]]:
    """
    Returns (changed, new_lines_for_span).
    Best-effort TOML editing for string arrays.
    """
    span_start, span_end = span
    block = lines[span_start:span_end]

    block_text = "\n".join(block)
    existing_escaped = _extract_toml_string_literals(block_text)
    existing = [_toml_unescape_basic(s) for s in existing_escaped]
    if value_to_add in existing:
        return (False, block)

    # Heuristic: if the whole assignment is on one line, rewrite it cleanly.
    if span_end - span_start == 1:
        items = existing + [value_to_add]
        rendered = ", ".join(f"\"{_toml_escape_string(v)}\"" for v in items)
        m_indent = re.match(r"^(\s*)", block[0])
        indent = m_indent.group(1) if m_indent else ""
        return (True, [f"{indent}{key} = [{rendered}]\n"])

    # Multi-line: insert before the closing bracket line, keep indentation.
    close_idx = None
    for i in range(len(block) - 1, -1, -1):
        if "]" in block[i]:
            close_idx = i
            break
    if close_idx is None:
        # Fallback: rewrite as a single-line array.
        items = existing + [value_to_add]
        rendered = ", ".join(f"\"{_toml_escape_string(v)}\"" for v in items)
        m_indent = re.match(r"^(\s*)", block[0])
        indent = m_indent.group(1) if m_indent else ""
        return (True, [f"{indent}{key} = [{rendered}]\n"])

    entry_indent = "  "
    for line in block[1:]:
        m = re.match(r"^(\s*)\"", line)
        if m:
            entry_indent = m.group(1)
            break

    new_block = block[:close_idx] + [f"{entry_indent}\"{_toml_escape_string(value_to_add)}\",\n"] + block[close_idx:]
    return (True, new_block)


def patch_config_shell_style(
    config_path: Path,
    style_key: str,
    mode: str,
    skill_name: str,
    skill_root: Path,
) -> tuple[str, str]:
    """
    Best-effort patcher.

    Return values:
        ("write", <full file contents>)  - write this to config.toml
        ("noop", "")                     - config already had the desired entry
        ("snippet", <toml snippet>)      - could not safely patch; print snippet
    """
    key_map = {
        "skill-roots": ("skill_roots", str(skill_root)),
        "allowlist": ("skills", skill_name),
        "disable": ("disabled_skills", skill_name),
    }
    if mode == "none":
        return ("noop", "")

    key, value = key_map[mode]

    lines: list[str] = []
    if config_path.exists():
        lines = config_path.read_text(encoding="utf-8").splitlines(keepends=True)

    # If the file contains multiple definitions for the same table/key, we bail.
    span = _find_style_table_span(lines, style_key)
    if span is None:
        snippet = build_shell_style_snippet(style_key, mode, skill_name, skill_root)
        if not config_path.exists():
            # Safe to create a new config file with just our snippet.
            return ("write", snippet)
        # Safe to append a new style table at end.
        contents = "".join(lines)
        if contents and not contents.endswith("\n"):
            contents += "\n"
        if contents and not contents.endswith("\n\n"):
            contents += "\n"
        contents += snippet
        return ("write", contents)

    table_start, table_end = span
    assign_span = _find_assignment_span(lines, table_start + 1, table_end, key)

    if assign_span is None:
        # Insert a simple assignment after the header line.
        insert_at = table_start + 1
        rendered_value = f"\"{_toml_escape_string(value)}\""
        new_line = f"{key} = [{rendered_value}]\n"
        lines = lines[:insert_at] + [new_line] + lines[insert_at:]
        return ("write", "".join(lines))

    # Update existing array
    changed, new_block = _ensure_list_contains_path(lines, assign_span, value, key)
    if not changed:
        return ("noop", "")
    span_start, span_end = assign_span
    lines = lines[:span_start] + new_block + lines[span_end:]
    return ("write", "".join(lines))


def build_shell_style_snippet(
    style_key: str,
    mode: str,
    skill_name: str,
    skill_root: Path,
) -> str:
    if mode == "skill-roots":
        key = "skill_roots"
        value = str(skill_root)
    elif mode == "allowlist":
        key = "skills"
        value = skill_name
    elif mode == "disable":
        key = "disabled_skills"
        value = skill_name
    else:
        key = "skill_roots"
        value = str(skill_root)

    value_rendered = f"\"{_toml_escape_string(value)}\""
    return (
        f"[shell_style_profiles.{style_key}]\n"
        f"{key} = [{value_rendered}]\n"
    )


def init_skill(
    skill_name: str,
    output_root: Path,
    resources: list[str],
    include_examples: bool,
) -> Path:
    skill_dir = output_root.resolve() / skill_name
    if skill_dir.exists():
        raise FileExistsError(f"skill directory already exists: {skill_dir}")

    skill_dir.mkdir(parents=True, exist_ok=False)

    skill_title = title_case_skill_name(skill_name)
    (skill_dir / "SKILL.md").write_text(
        SKILL_TEMPLATE.format(skill_name=skill_name, skill_title=skill_title),
        encoding="utf-8",
    )

    if resources:
        for resource in resources:
            (skill_dir / resource).mkdir(exist_ok=True)

        if include_examples:
            if "scripts" in resources:
                example_script = skill_dir / "scripts" / "example.py"
                example_script.write_text(
                    EXAMPLE_SCRIPT.format(skill_name=skill_name),
                    encoding="utf-8",
                )
                example_script.chmod(0o755)
            if "references" in resources:
                (skill_dir / "references" / "api_reference.md").write_text(
                    EXAMPLE_REFERENCE.format(skill_title=skill_title),
                    encoding="utf-8",
                )
            if "assets" in resources:
                (skill_dir / "assets" / "example_asset.txt").write_text(
                    EXAMPLE_ASSET,
                    encoding="utf-8",
                )

    return skill_dir


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Create a new skill directory with a SKILL.md template.",
    )
    parser.add_argument("skill_name", help="Skill name (normalized to hyphen-case)")
    parser.add_argument(
        "--path",
        default=None,
        help="Output root directory. Defaults to CODE_HOME/skills or CODE_HOME/skills-style/<style> when --shell-style is set.",
    )
    parser.add_argument(
        "--resources",
        default="scripts,references,assets",
        help="Comma-separated list: scripts,references,assets (default: all). Use empty string for none.",
    )
    parser.add_argument(
        "--examples",
        action="store_true",
        help="Create example files inside the selected resource directories",
    )
    parser.add_argument(
        "--shell-style",
        default=None,
        help="Attach this skill root to a shell style profile: posix-sh, bash-zsh-compatible, zsh",
    )
    parser.add_argument(
        "--attach-mode",
        default="skill-roots",
        choices=("skill-roots", "allowlist", "disable", "none"),
        help="How to attach when --shell-style is set (default: skill-roots). Use 'none' to avoid any config output/changes.",
    )
    parser.add_argument(
        "--code-home",
        default=None,
        help="Override CODE_HOME/CODEX_HOME for locating config.toml and default output roots.",
    )
    parser.add_argument(
        "--config",
        default=None,
        help="Path to config.toml (defaults to CODE_HOME/config.toml).",
    )
    parser.add_argument(
        "--apply-config",
        action="store_true",
        help="Best-effort update of config.toml. If omitted, prints the TOML snippet to add manually.",
    )
    args = parser.parse_args()

    raw_skill_name = args.skill_name
    skill_name = normalize_skill_name(raw_skill_name)
    if not skill_name:
        print("ERROR: Skill name must include at least one letter or digit.")
        sys.exit(1)
    if len(skill_name) > MAX_SKILL_NAME_LENGTH:
        print(
            f"ERROR: Skill name '{skill_name}' is too long ({len(skill_name)} characters). "
            f"Maximum is {MAX_SKILL_NAME_LENGTH} characters."
        )
        sys.exit(1)
    if skill_name != raw_skill_name:
        print(f"Note: Normalized skill name from '{raw_skill_name}' to '{skill_name}'.")

    try:
        resources = parse_resources(args.resources)
    except ValueError as e:
        print(f"ERROR: {e}")
        sys.exit(1)

    if args.examples and not resources:
        print("ERROR: --examples requires --resources to be set.")
        sys.exit(1)

    style: ShellStyle | None = None
    if args.shell_style:
        try:
            style = ShellStyle.parse(args.shell_style)
        except ValueError as e:
            print(f"ERROR: {e}")
            sys.exit(1)

    code_home = find_code_home(args.code_home)
    config_path = Path(args.config).expanduser() if args.config else default_config_path(code_home)

    if args.path:
        output_root = Path(args.path).expanduser()
    else:
        if style is None:
            output_root = code_home / "skills"
        else:
            if args.attach_mode == "skill-roots" or args.attach_mode == "none":
                output_root = code_home / "skills-style" / style.canonical_key
            else:
                # allowlist/disable usually assumes the skill is discoverable from default roots
                output_root = code_home / "skills"

    print(f"Initializing skill: {skill_name}")
    print(f"   Output root: {output_root}")
    if style is not None:
        print(f"   Shell style: {style.canonical_key} (attach-mode: {args.attach_mode})")
        if args.attach_mode == "allowlist":
            print("   Note: allowlist mode makes style skills filtering stricter when non-empty.")
    if resources:
        print(f"   Resources: {', '.join(resources)}")
        if args.examples:
            print("   Examples: enabled")
    else:
        print("   Resources: none")
    print()

    try:
        skill_dir = init_skill(skill_name, output_root, resources, args.examples)
    except Exception as e:
        print(f"ERROR: {e}")
        sys.exit(1)

    print(f"OK: Created skill directory: {skill_dir}")
    print("OK: Created SKILL.md")
    if resources:
        print(f"OK: Created resources: {', '.join(resources)}")
        if args.examples:
            print("OK: Created example files")

    if style is not None and args.attach_mode != "none":
        mode = args.attach_mode
        if args.apply_config:
            # For allowlist/disable, also ensure the style can discover the root.
            modes_to_apply = ["skill-roots", mode] if mode in ("allowlist", "disable") else [mode]
            last_status: str | None = None
            last_result: str = ""
            for patch_mode in modes_to_apply:
                try:
                    status, result = patch_config_shell_style(
                        config_path=config_path,
                        style_key=style.canonical_key,
                        mode=patch_mode,
                        skill_name=skill_name,
                        skill_root=output_root.resolve(),
                    )
                except Exception:
                    status = "snippet"
                    result = build_shell_style_snippet(
                        style.canonical_key,
                        patch_mode,
                        skill_name,
                        output_root.resolve(),
                    )

                last_status = status
                last_result = result

                if status == "write":
                    config_path.parent.mkdir(parents=True, exist_ok=True)
                    config_path.write_text(result, encoding="utf-8")
                elif status == "noop":
                    continue
                else:
                    print(
                        "\nNOTE: Could not safely auto-edit your config. Add this snippet manually:\n"
                    )
                    print(result)
                    break

            if last_status in ("write", "noop"):
                print(f"\nOK: Updated config: {config_path}")
        else:
            print("\nTo attach this skill to the shell style profile, add:\n")
            if mode in ("allowlist", "disable"):
                key = "skills" if mode == "allowlist" else "disabled_skills"
                root_value = f"\"{_toml_escape_string(str(output_root.resolve()))}\""
                name_value = f"\"{_toml_escape_string(skill_name)}\""
                print(f"[shell_style_profiles.{style.canonical_key}]")
                print(f"skill_roots = [{root_value}]")
                print(f"{key} = [{name_value}]\n")
            else:
                print(
                    build_shell_style_snippet(
                        style.canonical_key,
                        mode,
                        skill_name,
                        output_root.resolve(),
                    )
                )

    print("\nNext steps:")
    print("1. Edit SKILL.md frontmatter: replace the TODO description with real trigger wording")
    print("2. Delete any unused resource directories/files")
    print("3. Validate with: scripts/quick_validate.py <path-to-skill-dir>")
    print("4. Package (optional): scripts/package_skill.py <path-to-skill-dir> [out-dir]")


if __name__ == "__main__":
    main()

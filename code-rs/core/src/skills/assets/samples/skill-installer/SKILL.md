---
name: skill-installer
description: Install Codex skills into $CODEX_HOME/skills from a curated list or a GitHub repo path. Use when a user asks to list installable skills, install a curated skill, or install a skill from another repo (including private repos).
metadata:
  short-description: Install curated skills from openai/skills or other repos
---

# Skill Installer

Helps install skills. By default these are from https://github.com/openai/skills/tree/main/skills/.curated, but users can also provide other locations.

Use the helper scripts based on the task:
- List curated skills when the user asks what is available, or if the user uses this skill without specifying what to do.
- Install from the curated list when the user provides a skill name.
- Install from another repo when the user provides a GitHub repo/path (including private repos).

Install skills with the helper scripts.

## Communication

When listing curated skills, output approximately as follows, depending on the context of the user's request:
"""
Skills from {repo}:
1. skill-1
2. skill-2 (already installed)
3. ...
Which ones would you like installed?
"""

After installing a skill, tell the user: "Restart Codex to pick up new skills."

## Scripts

All of these scripts use network, so when running in the sandbox, request escalation when running them.

- `scripts/list-curated-skills.py` (prints curated list with installed annotations)
- `scripts/list-curated-skills.py --format json`
- `scripts/install-skill-from-github.py --repo <owner>/<repo> --path <path/to/skill> [<path/to/skill> ...]`
- `scripts/install-skill-from-github.py --url https://github.com/<owner>/<repo>/tree/<ref>/<path>`

## Behavior and Options

- Defaults to direct download for public GitHub repos.
- If download fails with auth/permission errors, falls back to git sparse checkout.
- Aborts if the destination skill directory already exists.
- Installs into `$CODEX_HOME/skills/<skill-name>` (defaults to `~/.codex/skills`).
- Multiple `--path` values install multiple skills in one run, each named from the path basename unless `--name` is supplied.
- Options: `--ref <ref>` (default `main`), `--dest <path>`, `--method auto|download|git`.

## Shell-style-aware installs

When users have shell-style profiles configured, do not assume a global install location is correct.

- If the user asks for style-specific behavior, install into the style root referenced by `shell_style_profiles.<style>.skill_roots` (use `--dest`).
- If no style root exists yet, install to a user-chosen path and provide a config snippet that adds it to `shell_style_profiles.<style>.skill_roots`.
- If the user wants stricter control, recommend:
  - `shell_style_profiles.<style>.skills` as an allow-list
  - `shell_style_profiles.<style>.disabled_skills` for explicit removals
- Mention precedence clearly: `disabled_skills` overrides allow-list and loaded roots.

Example:

```toml
[shell_style_profiles.zsh]
skill_roots = [".codex/skills/zsh"]
skills = ["termux-zsh", "zsh-arrays"]
disabled_skills = ["legacy-zsh-skill"]
```

## Notes

- Curated listing is fetched from `https://github.com/openai/skills/tree/main/skills/.curated` via the GitHub API. If it is unavailable, explain the error and exit.
- Private GitHub repos can be accessed via existing git credentials or optional `GITHUB_TOKEN`/`GH_TOKEN` for download.
- Git fallback tries HTTPS first, then SSH.
- The skills at https://github.com/openai/skills/tree/main/skills/.system are preinstalled, so no need to help users install those. If they ask, just explain this. If they insist, you can download and overwrite.
- Installed annotations come from `$CODEX_HOME/skills`.

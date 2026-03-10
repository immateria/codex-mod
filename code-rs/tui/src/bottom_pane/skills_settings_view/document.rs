use super::*;

pub(super) fn skill_slug(skill: &Skill) -> String {
    skill
        .path
        .parent()
        .and_then(|path| path.file_name())
        .and_then(|name| name.to_str())
        .map(str::to_string)
        .unwrap_or_else(|| skill.name.clone())
}

pub(super) fn extract_frontmatter(body: &str) -> Option<String> {
    let mut lines = body.lines();
    if lines.next()?.trim() != "---" {
        return None;
    }
    let mut frontmatter = String::new();
    for line in lines {
        if line.trim() == "---" {
            return Some(frontmatter);
        }
        frontmatter.push_str(line);
        frontmatter.push('\n');
    }
    None
}

pub(super) fn strip_frontmatter(body: &str) -> String {
    let mut lines = body.lines();
    if lines.next().map(str::trim) != Some("---") {
        return body.to_string();
    }

    for line in lines.by_ref() {
        if line.trim() == "---" {
            let rest: String = lines.collect::<Vec<_>>().join("\n");
            let rest = rest.trim_start_matches('\n').to_string();
            return if body.ends_with('\n') && !rest.ends_with('\n') {
                format!("{rest}\n")
            } else {
                rest
            };
        }
    }

    body.to_string()
}

fn yaml_escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

pub(super) fn compose_skill_document(
    name: &str,
    description: &str,
    shell_style: &str,
    extra_frontmatter: &str,
    body: &str,
) -> String {
    debug_assert!(
        !extra_frontmatter.lines().any(|line| line.trim() == "---"),
        "extra_frontmatter must not contain bare YAML frontmatter delimiters"
    );
    let escaped_name = yaml_escape(name);
    let escaped_description = yaml_escape(description);
    let mut header = format!(
        "---\nname: \"{escaped_name}\"\ndescription: \"{escaped_description}\"\n"
    );
    let shell_style = shell_style.trim();
    if !shell_style.is_empty() {
        let escaped_style = yaml_escape(shell_style);
        header.push_str(&format!("shell_style: \"{escaped_style}\"\n"));
    }
    let extra_frontmatter = extra_frontmatter.trim();
    if !extra_frontmatter.is_empty() {
        header.push_str(extra_frontmatter.trim_end_matches('\n'));
        header.push('\n');
    }
    header.push_str("---\n\n");

    let body = body.trim_start_matches('\n');
    if body.is_empty() {
        header
    } else {
        format!("{header}{body}")
    }
}

pub(super) fn filter_frontmatter_excluding_keys(frontmatter: &str, excluded_keys: &[&str]) -> String {
    if excluded_keys.is_empty() {
        return frontmatter.to_string();
    }

    let mut out: Vec<&str> = Vec::new();
    let lines: Vec<&str> = frontmatter.lines().collect();
    let mut idx = 0;
    while idx < lines.len() {
        let line = lines[idx];
        let is_top_level = !line.starts_with(|c: char| c.is_whitespace());
        if is_top_level {
            let trimmed = line.trim_start();
            let matches_excluded = excluded_keys.iter().any(|key| {
                let needle = format!("{key}:");
                trimmed.starts_with(needle.as_str())
            });
            if matches_excluded {
                idx += 1;
                let mut pending_blanks = 0usize;
                while idx < lines.len() {
                    let next = lines[idx];
                    if next.trim().is_empty() || next.starts_with(|c: char| c.is_whitespace()) {
                        if next.trim().is_empty() {
                            pending_blanks += 1;
                        }
                        idx += 1;
                        continue;
                    }
                    break;
                }
                for _ in 0..pending_blanks {
                    out.push("");
                }
                continue;
            }
        }

        out.push(line);
        idx += 1;
    }

    out.join("\n")
}

pub(super) fn frontmatter_value(body: &str, key: &str) -> Option<String> {
    let frontmatter = extract_frontmatter(body)?;
    let needle = format!("{key}:");
    for line in frontmatter.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix(needle.as_str()) {
            let value = rest.trim();
            let value = value
                .strip_prefix('"')
                .and_then(|s| s.strip_suffix('"'))
                .or_else(|| value.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')))
                .unwrap_or(value)
                .trim();
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

fn normalize_profile_skill_name(name: &str) -> String {
    name.trim().to_ascii_lowercase()
}

pub(super) fn remove_profile_skill(values: &mut Vec<String>, skill_name: &str) {
    let normalized_target = normalize_profile_skill_name(skill_name);
    values.retain(|entry| normalize_profile_skill_name(entry) != normalized_target);
}

pub(super) fn profile_list_contains_any(values: &[String], identifiers: &[&str]) -> bool {
    let normalized_values: HashSet<String> = values
        .iter()
        .map(|entry| normalize_profile_skill_name(entry))
        .collect();
    identifiers
        .iter()
        .map(|identifier| normalize_profile_skill_name(identifier))
        .any(|normalized| normalized_values.contains(&normalized))
}

pub(super) fn unique_profile_identifiers<'a, I>(identifiers: I) -> Vec<String>
where
    I: IntoIterator<Item = &'a str>,
{
    let mut deduped: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for identifier in identifiers {
        let trimmed = identifier.trim();
        if trimmed.is_empty() {
            continue;
        }
        let normalized = normalize_profile_skill_name(trimmed);
        if seen.insert(normalized) {
            deduped.push(trimmed.to_string());
        }
    }
    deduped
}

pub(super) fn format_path_list(paths: &[PathBuf]) -> String {
    // NOTE: this editor stores path lists in text fields, so non-UTF-8 paths
    // are displayed lossy and cannot round-trip byte-exactly through the UI.
    paths
        .iter()
        .map(|path| path.to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join("\n")
}

pub(super) fn format_string_list(values: &[String]) -> String {
    values
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>()
        .join("\n")
}

pub(super) fn parse_path_list(text: &str) -> Vec<PathBuf> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(PathBuf::from)
        .collect()
}

pub(super) fn parse_string_list(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect()
}

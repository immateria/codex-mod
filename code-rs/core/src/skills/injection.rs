use std::collections::HashMap;
use std::collections::HashSet;
use std::path::PathBuf;

use code_protocol::models::ResponseItem;
use serde::Deserialize;
use tokio::fs;

use crate::mentions;
use crate::mcp::ids::McpServerId;
use crate::mcp::ids::McpToolId;
use crate::skills::frontmatter::extract_frontmatter;
use crate::skills::model::SkillMetadata;
use crate::user_instructions::SkillInstructions;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MentionedSkill {
    pub(crate) name: String,
    pub(crate) path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct SkillMcpDependency {
    pub(crate) skill_name: String,
    pub(crate) server: String,
    pub(crate) tool: Option<String>,
}

pub(crate) struct SkillMentionOutcome {
    pub(crate) mentioned: Vec<MentionedSkill>,
    pub(crate) warnings: Vec<String>,
}

#[derive(Debug, Default)]
pub(crate) struct SkillInjections {
    pub(crate) items: Vec<ResponseItem>,
    pub(crate) warnings: Vec<String>,
    pub(crate) mcp_dependencies: Vec<SkillMcpDependency>,
}

pub(crate) fn collect_explicit_skill_mentions(
    messages: &[String],
    skills: &[SkillMetadata],
) -> SkillMentionOutcome {
    if messages.is_empty() || skills.is_empty() {
        return SkillMentionOutcome {
            mentioned: Vec::new(),
            warnings: Vec::new(),
        };
    }

    let mentions = mentions::collect_tool_mentions_from_messages(messages);
    if mentions.is_empty() {
        return SkillMentionOutcome {
            mentioned: Vec::new(),
            warnings: Vec::new(),
        };
    }

    let mut skill_name_counts: HashMap<String, usize> = HashMap::new();
    for skill in skills {
        *skill_name_counts
            .entry(skill.name.to_ascii_lowercase())
            .or_insert(0) += 1;
    }

    let mention_skill_paths: HashSet<String> = mentions
        .paths
        .iter()
        .copied()
        .filter(|path| is_skill_path_like(path))
        .map(|path| normalize_skill_path(path).replace('\\', "/"))
        .collect();

    let mut selected: Vec<MentionedSkill> = Vec::new();
    let mut seen_paths: HashSet<PathBuf> = HashSet::new();
    let mut warned_ambiguous: HashSet<String> = HashSet::new();
    let mut warnings: Vec<String> = Vec::new();

    // Prefer explicit path mentions first.
    if !mention_skill_paths.is_empty() {
        for skill in skills {
            let path_str = normalize_path_for_compare(skill.path.as_path());
            if mention_skill_paths.contains(path_str.as_str()) && seen_paths.insert(skill.path.clone()) {
                selected.push(MentionedSkill {
                    name: skill.name.clone(),
                    path: skill.path.clone(),
                });
            }
        }
    }

    let mention_plain_names_lower: HashSet<String> = mentions
        .plain_names
        .iter()
        .copied()
        .map(str::to_ascii_lowercase)
        .collect();

    if mention_plain_names_lower.is_empty() {
        return SkillMentionOutcome {
            mentioned: selected,
            warnings: Vec::new(),
        };
    }

    for skill in skills {
        if seen_paths.contains(&skill.path) {
            continue;
        }

        let skill_lower = skill.name.to_ascii_lowercase();
        if !mention_plain_names_lower.contains(skill_lower.as_str()) {
            continue;
        }

        let count = skill_name_counts.get(skill_lower.as_str()).copied().unwrap_or(0);
        if count != 1 {
            if warned_ambiguous.insert(skill_lower.clone()) {
                let mut paths = skills
                    .iter()
                    .filter(|candidate| candidate.name.to_ascii_lowercase() == skill_lower)
                    .map(|candidate| candidate.path.to_string_lossy().into_owned())
                    .collect::<Vec<_>>();
                paths.sort();
                let joined = paths.join(", ");
                warnings.push(format!(
                    "Ambiguous skill mention `${skill_lower}` matched multiple skills: {joined}. Use a linked mention to disambiguate: `[$skill_lower](skill://<full path>)`."
                ));
            }
            continue;
        }

        if seen_paths.insert(skill.path.clone()) {
            selected.push(MentionedSkill {
                name: skill.name.clone(),
                path: skill.path.clone(),
            });
        }
    }

    SkillMentionOutcome {
        mentioned: selected,
        warnings,
    }
}

pub(crate) async fn build_skill_injections(skills: &[MentionedSkill]) -> SkillInjections {
    if skills.is_empty() {
        return SkillInjections::default();
    }

    let mut items: Vec<ResponseItem> = Vec::with_capacity(skills.len());
    let mut warnings: Vec<String> = Vec::new();
    let mut mcp_dependencies: Vec<SkillMcpDependency> = Vec::new();

    for skill in skills {
        match fs::read_to_string(&skill.path).await {
            Ok(contents) => {
                match parse_skill_mcp_dependencies(skill.name.as_str(), contents.as_str()) {
                    Ok(deps) => mcp_dependencies.extend(deps),
                    Err(err) => warnings.push(format!(
                        "Failed to parse MCP dependencies for skill `{}` at {}: {err}",
                        skill.name,
                        skill.path.display()
                    )),
                }

                let path = skill.path.to_string_lossy().replace('\\', "/");
                items.push(
                    SkillInstructions {
                        name: skill.name.clone(),
                        path,
                        contents,
                    }
                    .into(),
                );
            }
            Err(err) => {
                warnings.push(format!(
                    "Failed to load skill `{}` at {}: {err:#}",
                    skill.name,
                    skill.path.display()
                ));
            }
        }
    }

    SkillInjections {
        items,
        warnings,
        mcp_dependencies,
    }
}

#[derive(Debug, Deserialize, Default)]
struct SkillFrontmatterMcpDeps {
    #[serde(default)]
    mcp_servers: Vec<String>,
    #[serde(default)]
    mcp_tools: Vec<McpToolDepSpec>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum McpToolDepSpec {
    String(String),
    Map { server: String, tool: String },
}

fn parse_skill_mcp_dependencies(
    skill_name: &str,
    contents: &str,
) -> Result<Vec<SkillMcpDependency>, String> {
    let Some(frontmatter) = extract_frontmatter(contents) else {
        return Ok(Vec::new());
    };

    let parsed: SkillFrontmatterMcpDeps = serde_yaml::from_str(frontmatter)
        .map_err(|err| format!("invalid YAML frontmatter: {err}"))?;

    let mut dedupe: HashSet<(String, Option<String>)> = HashSet::new();
    let mut out: Vec<SkillMcpDependency> = Vec::new();

    for server in parsed.mcp_servers {
        let Some(server) = McpServerId::parse(server.as_str()) else {
            continue;
        };
        if dedupe.insert((server.as_str().to_string(), None)) {
            out.push(SkillMcpDependency {
                skill_name: skill_name.to_string(),
                server: server.as_str().to_string(),
                tool: None,
            });
        }
    }

    for entry in parsed.mcp_tools {
        let (server, tool) = match entry {
            McpToolDepSpec::String(spec) => match McpToolId::parse_spec(spec.as_str()) {
                Some(pair) => pair.into_parts(),
                None => {
                    return Err(format!(
                        "invalid mcp_tools entry `{spec}` (expected `server/tool` or `server::tool`)",
                    ));
                }
            },
            McpToolDepSpec::Map { server, tool } => {
                let server = McpServerId::parse(server.as_str())
                    .ok_or_else(|| "mcp_tools.server cannot be empty".to_string())?;
                McpToolId::parse(server.as_str(), tool.as_str())
                    .ok_or_else(|| "mcp_tools.tool cannot be empty".to_string())?
                    .into_parts()
            }
        };

        if dedupe.insert((server.clone(), Some(tool.clone()))) {
            out.push(SkillMcpDependency {
                skill_name: skill_name.to_string(),
                server,
                tool: Some(tool),
            });
        }
    }

    Ok(out)
}

fn normalize_skill_path(path: &str) -> &str {
    path.strip_prefix("skill://").unwrap_or(path)
}

fn normalize_path_for_compare(path: &std::path::Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn is_skill_path_like(path: &str) -> bool {
    path.starts_with("skill://") || path.ends_with("SKILL.md") || path.ends_with("skill.md")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skills::model::SkillMetadata;
    use crate::skills::model::SkillScope;
    use std::collections::HashSet;
    use std::path::PathBuf;

    fn meta(name: &str, path: &str) -> SkillMetadata {
        SkillMetadata {
            name: name.to_string(),
            description: format!("desc for {name}"),
            path: PathBuf::from(path),
            scope: SkillScope::Repo,
        }
    }

    #[test]
    fn ambiguous_plain_name_emits_warning_and_selects_nothing() {
        let skills = vec![meta("dup", "/a/SKILL.md"), meta("dup", "/b/SKILL.md")];
        let messages = vec!["please use $dup".to_string()];

        let outcome = collect_explicit_skill_mentions(&messages, &skills);

        assert!(outcome.mentioned.is_empty());
        assert_eq!(outcome.warnings.len(), 1);
        let warning = &outcome.warnings[0];
        assert!(warning.contains("Ambiguous skill mention `$dup`"));
        assert!(warning.contains("/a/SKILL.md"));
        assert!(warning.contains("/b/SKILL.md"));
    }

    #[test]
    fn linked_skill_path_matches_after_normalization() {
        let skills = vec![meta("my-skill", r"C:\skills\dup\SKILL.md")];
        let messages = vec![r"use [$x](skill://C:\skills\dup\SKILL.md)".to_string()];

        let outcome = collect_explicit_skill_mentions(&messages, &skills);

        assert_eq!(outcome.mentioned.len(), 1);
        assert_eq!(outcome.mentioned[0].name, "my-skill");
        assert_eq!(outcome.mentioned[0].path, PathBuf::from(r"C:\skills\dup\SKILL.md"));
    }

    #[test]
    fn linked_plugin_mention_does_not_trigger_skill_by_name() {
        let skills = vec![meta("sample", "/skills/sample/SKILL.md")];
        let messages = vec!["use [$sample](plugin://sample@test)".to_string()];

        let outcome = collect_explicit_skill_mentions(&messages, &skills);

        assert!(outcome.mentioned.is_empty());
        assert!(outcome.warnings.is_empty());
    }

    #[test]
    fn parses_mcp_dependencies_from_frontmatter() {
        let contents = r#"---
name: example
description: demo
mcp_servers:
  - Brave
  - brave
mcp_tools:
  - brave/web_search
  - mcp://BRAVE::image_search
  - { server: brave, tool: Web_Search }
---
body
"#;

        let deps = parse_skill_mcp_dependencies("example", contents).expect("deps parse ok");
        let set: HashSet<SkillMcpDependency> = deps.into_iter().collect();

        assert!(set.contains(&SkillMcpDependency {
            skill_name: "example".to_string(),
            server: "brave".to_string(),
            tool: None,
        }));
        assert!(set.contains(&SkillMcpDependency {
            skill_name: "example".to_string(),
            server: "brave".to_string(),
            tool: Some("web_search".to_string()),
        }));
        assert!(set.contains(&SkillMcpDependency {
            skill_name: "example".to_string(),
            server: "brave".to_string(),
            tool: Some("image_search".to_string()),
        }));

        // Ensure duplicates are deduped (server listed twice; web_search listed twice).
        assert_eq!(set.iter().filter(|dep| dep.tool.is_none()).count(), 1);
        assert_eq!(
            set.iter()
                .filter(|dep| dep.tool.as_deref() == Some("web_search"))
                .count(),
            1
        );
    }

    #[test]
    fn invalid_mcp_tool_spec_errors() {
        let contents = r#"---
mcp_tools:
  - brave
---
"#;
        let err = parse_skill_mcp_dependencies("example", contents).expect_err("should fail");
        assert!(err.contains("invalid mcp_tools entry"));
    }
}

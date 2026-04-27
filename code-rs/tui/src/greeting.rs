use chrono::Datelike;
use chrono::Local;
use std::collections::BTreeMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::Hash;
use std::hash::Hasher;
use std::path::Path;

/// All candidate composer placeholder prompts.
///
/// Selection rotates by calendar day so the text changes daily but is
/// stable within a single session. Set `CODEX_TUI_GREETING_INDEX` to a
/// specific integer to pin a prompt in tests or custom builds.
const DEFAULT_PROMPTS: &[&str] = &[
    "Where do you want to start?",
    "What needs attention?",
    "What's the next move?",
    "Which problem are we solving?",
    "What should we dig into?",
    "What's blocking you?",
    "What are we untangling?",
    "What deserves a closer look?",
    "What should we ship?",
    "What are we picking up?",
    "What needs fixing?",
    "Ready when you are.",
];

pub(crate) const GREETINGS_FILE: &str = "greetings.txt";
const MAX_EXPANSION_DEPTH: u8 = 4;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GreetingConfig {
    prompts: Vec<String>,
    slots: BTreeMap<String, String>,
}

#[derive(Clone, Copy)]
struct RenderContext<'a> {
    day_ordinal: usize,
    time_of_day: &'a str,
}

pub(crate) fn load_config(code_home: Option<&Path>) -> GreetingConfig {
    code_home
        .and_then(load_user_config)
        .unwrap_or_else(default_config)
}

fn load_user_config(code_home: &Path) -> Option<GreetingConfig> {
    let contents = std::fs::read_to_string(code_home.join(GREETINGS_FILE)).ok()?;
    Some(parse_config(&contents))
}

fn default_config() -> GreetingConfig {
    GreetingConfig {
        prompts: DEFAULT_PROMPTS
            .iter()
            .map(|prompt| (*prompt).to_owned())
            .collect(),
        slots: BTreeMap::new(),
    }
}

fn parse_config(contents: &str) -> GreetingConfig {
    let mut slots = BTreeMap::new();
    let mut prompts = Vec::new();

    for raw_line in contents.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if let Some((key, value)) = parse_slot_definition(line) {
            slots.insert(key, value);
            continue;
        }

        prompts.push(line.to_owned());
    }

    if prompts.is_empty() {
        prompts = default_config().prompts;
    }

    GreetingConfig { prompts, slots }
}

fn parse_slot_definition(line: &str) -> Option<(String, String)> {
    let rest = line.strip_prefix('@')?;

    let (key, value) = if let Some((key, value)) = rest.split_once('=') {
        (key.trim(), value.trim())
    } else {
        let split_at = rest.find(char::is_whitespace)?;
        let (key, value) = rest.split_at(split_at);
        (key.trim(), value.trim())
    };

    if key.is_empty() || value.is_empty() {
        return None;
    }

    Some((key.to_ascii_lowercase(), value.to_owned()))
}

fn prompt_index(prompt_count: usize, day_ordinal: usize) -> usize {
    if prompt_count == 0 {
        return 0;
    }
    if let Ok(raw) = std::env::var("CODEX_TUI_GREETING_INDEX")
        && let Ok(idx) = raw.parse::<usize>()
    {
        return idx % prompt_count;
    }
    day_ordinal % prompt_count
}

fn prompt_at(prompts: &[String], index: usize) -> &str {
    prompts
        .get(index)
        .map(String::as_str)
        .or_else(|| prompts.first().map(String::as_str))
        .unwrap_or(DEFAULT_PROMPTS[0])
}

fn current_context() -> RenderContext<'static> {
    RenderContext {
        day_ordinal: Local::now().ordinal0() as usize,
        time_of_day: crate::time_of_day::current_label(),
    }
}

fn resolve_name(config: &GreetingConfig) -> Option<String> {
    if let Some(name) = config.slots.get("name")
        && !name.trim().is_empty()
    {
        return Some(name.clone());
    }

    for key in ["CODEX_TUI_GREETING_NAME", "USER", "LOGNAME", "USERNAME"] {
        if let Ok(value) = std::env::var(key) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_owned());
            }
        }
    }

    None
}

fn stable_seed(template: &str, placeholder_ordinal: usize, day_ordinal: usize) -> usize {
    let mut hasher = DefaultHasher::new();
    template.hash(&mut hasher);
    placeholder_ordinal.hash(&mut hasher);
    day_ordinal.hash(&mut hasher);
    hasher.finish() as usize
}

fn select_choice(token: &str, template: &str, placeholder_ordinal: usize, day_ordinal: usize) -> String {
    let options = token
        .split('|')
        .map(str::trim)
        .filter(|option| !option.is_empty())
        .collect::<Vec<_>>();

    if options.is_empty() {
        return format!("<{token}>");
    }

    let seed = stable_seed(template, placeholder_ordinal, day_ordinal);
    options[seed % options.len()].to_owned()
}

fn render_template(
    template: &str,
    config: &GreetingConfig,
    context: RenderContext<'_>,
    depth: u8,
) -> String {
    let mut rendered = String::with_capacity(template.len());
    let mut remaining = template;
    let mut placeholder_ordinal = 0usize;

    while let Some(start) = remaining.find('<') {
        rendered.push_str(&remaining[..start]);
        let after_start = &remaining[start + 1..];
        let Some(end) = after_start.find('>') else {
            rendered.push_str(&remaining[start..]);
            return rendered;
        };

        let token = after_start[..end].trim();
        let expansion = expand_placeholder(token, template, config, context, depth, placeholder_ordinal);
        rendered.push_str(&expansion);
        placeholder_ordinal += 1;
        remaining = &after_start[end + 1..];
    }

    rendered.push_str(remaining);
    rendered
}

fn expand_placeholder(
    token: &str,
    template: &str,
    config: &GreetingConfig,
    context: RenderContext<'_>,
    depth: u8,
    placeholder_ordinal: usize,
) -> String {
    if token.is_empty() {
        return "<>".to_owned();
    }

    if token.contains('|') {
        return select_choice(token, template, placeholder_ordinal, context.day_ordinal);
    }

    let key = token.to_ascii_lowercase();
    if let Some(value) = config.slots.get(&key) {
        if depth >= MAX_EXPANSION_DEPTH {
            return value.clone();
        }
        return render_template(value, config, context, depth + 1);
    }

    match key.as_str() {
        "time_of_day" | "timeofday" | "time" => context.time_of_day.to_owned(),
        "name" => resolve_name(config).unwrap_or_else(|| "there".to_owned()),
        _ => format!("<{token}>"),
    }
}

pub(crate) fn greeting_placeholder(config: &GreetingConfig) -> String {
    let context = current_context();
    let template = prompt_at(&config.prompts, prompt_index(config.prompts.len(), context.day_ordinal));
    render_template(template, config, context, 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn test_context() -> RenderContext<'static> {
        RenderContext {
            day_ordinal: 3,
            time_of_day: "this evening",
        }
    }

    #[test]
    fn all_prompts_are_non_empty() {
        for p in DEFAULT_PROMPTS {
            assert!(!p.is_empty());
        }
    }

    #[test]
    fn parse_config_reads_slots_and_prompts() {
        let config = parse_config(
            "\n# comment\n@name = Casey\nWhere next?\n@tone curious\nWhat broke?\n",
        );

        assert_eq!(config.prompts, vec!["Where next?", "What broke?"]);
        assert_eq!(config.slots.get("name"), Some(&"Casey".to_owned()));
        assert_eq!(config.slots.get("tone"), Some(&"curious".to_owned()));
    }

    #[test]
    fn load_config_uses_user_configured_file_when_present() {
        let code_home = tempdir().expect("tempdir");
        std::fs::write(
            code_home.path().join(GREETINGS_FILE),
            "# preferred greetings\n@name = Casey\nFocus this.\nUntangle that.\n",
        )
        .expect("write greeting file");

        let config = load_config(Some(code_home.path()));

        assert_eq!(config.prompts, vec!["Focus this.", "Untangle that."]);
        assert_eq!(config.slots.get("name"), Some(&"Casey".to_owned()));
    }

    #[test]
    fn render_template_expands_slots_builtins_and_choices() {
        let config = parse_config(
            "@name = Casey\n@nickname = <dude|buddy|captain>\nHey <name>, what are you tryin' to build <time_of_day>, <nickname>?",
        );
        let template = config.prompts.first().expect("prompt");
        let result = render_template(template, &config, test_context(), 0);

        assert!(result.contains("Casey"));
        assert!(result.contains("this evening"));
        assert!(
            result.contains("dude") || result.contains("buddy") || result.contains("captain"),
            "unexpected choice expansion: {result}"
        );
    }

    #[test]
    fn greeting_placeholder_returns_known_prompt() {
        let config = load_config(None);
        let result = greeting_placeholder(&config);

        assert!(config.prompts.contains(&result), "unexpected prompt: {result}");
    }

    #[test]
    fn prompt_at_falls_back_to_first_prompt() {
        let prompts = vec!["Where next?".to_owned(), "What broke?".to_owned()];

        assert_eq!(prompt_at(&prompts, 99), "Where next?");
    }

    #[test]
    fn unknown_placeholder_is_left_visible() {
        let config = parse_config("Hello <mystery>.");
        let template = config.prompts.first().expect("prompt");

        assert_eq!(render_template(template, &config, test_context(), 0), "Hello <mystery>.");
    }
}

pub(crate) fn normalize_agent_name(name: Option<String>) -> Option<String> {
    let name = name.map(|value| value.trim().to_string())?;

    if name.is_empty() {
        return None;
    }

    let canonicalized = canonicalize_agent_word_boundaries(&name);
    let words: Vec<&str> = canonicalized.split_whitespace().collect();
    if words.is_empty() {
        return None;
    }

    Some(
        words
            .into_iter()
            .map(format_agent_word)
            .collect::<Vec<_>>()
            .join(" "),
    )
}

fn canonicalize_agent_word_boundaries(input: &str) -> String {
    let mut tokens: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut chars = input.chars().peekable();
    let mut prev_char: Option<char> = None;
    let mut uppercase_run: usize = 0;

    while let Some(ch) = chars.next() {
        if ch.is_whitespace() || matches!(ch, '_' | '-' | '/' | ':' | '.') {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
            prev_char = None;
            uppercase_run = 0;
            continue;
        }

        let next_char = chars.peek().copied();
        let mut split = false;

        if !current.is_empty()
            && let Some(prev) = prev_char
            && ((prev.is_ascii_lowercase() && ch.is_ascii_uppercase())
                || (prev.is_ascii_uppercase()
                    && ch.is_ascii_uppercase()
                    && uppercase_run > 0
                    && next_char.is_some_and(|c| c.is_ascii_lowercase())))
        {
            split = true;
        }

        if split {
            tokens.push(std::mem::take(&mut current));
            uppercase_run = 0;
        }

        current.push(ch);

        if ch.is_ascii_uppercase() {
            uppercase_run += 1;
        } else {
            uppercase_run = 0;
        }

        prev_char = Some(ch);
    }

    if !current.is_empty() {
        tokens.push(current);
    }

    tokens.join(" ")
}

const AGENT_NAME_ACRONYMS: &[&str] = &[
    "AI",
    "API",
    "CLI",
    "CPU",
    "DB",
    "GPU",
    "HTTP",
    "HTTPS",
    "ID",
    "LLM",
    "SDK",
    "SQL",
    "TUI",
    "UI",
    "UX",
];

fn format_agent_word(word: &str) -> String {
    if word.is_empty() {
        return String::new();
    }

    let uppercase = word.to_ascii_uppercase();
    if AGENT_NAME_ACRONYMS.contains(&uppercase.as_str()) {
        return uppercase;
    }

    let mut chars = word.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };

    let mut formatted = String::new();
    formatted.extend(first.to_uppercase());
    formatted.push_str(&chars.flat_map(char::to_lowercase).collect::<String>());
    formatted
}

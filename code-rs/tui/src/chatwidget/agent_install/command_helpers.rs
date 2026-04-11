fn collect_command_output(
    controller_rx: &Receiver<TerminalRunEvent>,
) -> Result<Option<(String, Option<i32>)>> {
    let mut buf: Vec<u8> = Vec::new();
    let exit_code = loop {
        match controller_rx.recv() {
            Ok(TerminalRunEvent::Chunk { data, _is_stderr: _ }) => buf.extend_from_slice(&data),
            Ok(TerminalRunEvent::Exit { exit_code, _duration: _ }) => break exit_code,
            Err(_) => return Ok(None),
        }
    };
    let text = String::from_utf8_lossy(&buf).into_owned();
    Ok(Some((text, exit_code)))
}

pub(crate) fn simplify_command(raw: &str) -> &str {
    let trimmed = raw.trim();
    let lower = trimmed.to_ascii_lowercase();
    if let Some(rest) = lower.strip_prefix("bash -lc ") {
        let original = &trimmed[trimmed.len() - rest.len()..];
        return original.trim_matches(|c| c == '\'' || c == '"').trim();
    }
    trimmed
}

pub(crate) fn wrap_command(raw: &str) -> Vec<String> {
    let simplified = simplify_command(raw);
    if simplified.is_empty() {
        return Vec::new();
    }
    if cfg!(target_os = "windows") {
        vec![
            "powershell.exe".to_owned(),
            "-NoProfile".to_owned(),
            "-ExecutionPolicy".to_owned(),
            "Bypass".to_owned(),
            "-Command".to_owned(),
            simplified.to_owned(),
        ]
    } else {
        vec!["/bin/bash".to_owned(), "-lc".to_owned(), simplified.to_owned()]
    }
}

fn tail_chars(text: &str, max_chars: usize) -> String {
    let total = text.chars().count();
    if total <= max_chars {
        return text.to_owned();
    }
    let mut idx = text.len();
    let mut count = 0usize;
    for (i, _) in text.char_indices().rev() {
        count += 1;
        if count >= max_chars {
            idx = i;
            break;
        }
    }
    text[idx..].to_string()
}

fn make_message(role: &str, text: String) -> ResponseItem {
    let content = if role.eq_ignore_ascii_case("assistant") {
        ContentItem::OutputText { text }
    } else {
        ContentItem::InputText { text }
    };

    ResponseItem::Message {
        id: None,
        role: role.to_owned(),
        content: vec![content],
        end_turn: None,
        phase: None,
    }
}

fn extract_first_json_object(input: &str) -> Option<String> {
    let mut depth = 0usize;
    let mut in_str = false;
    let mut escape = false;
    let mut start: Option<usize> = None;
    for (idx, ch) in input.char_indices() {
        if in_str {
            if escape {
                escape = false;
                continue;
            }
            match ch {
                '"' => in_str = false,
                '\\' => escape = true,
                _ => {}
            }
            continue;
        }
        match ch {
            '"' => in_str = true,
            '{' => {
                if depth == 0 {
                    start = Some(idx);
                }
                depth += 1;
            }
            '}' => {
                if depth == 0 {
                    continue;
                }
                depth -= 1;
                if depth == 0 {
                    let s = start?;
                    return Some(input[s..=idx].to_string());
                }
            }
            _ => {}
        }
    }
    None
}

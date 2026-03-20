use std::io::Write;

pub(super) fn format_notification_message_inner(title: &str, body: Option<&str>) -> Option<String> {
    let title = sanitize_notification_text(title);
    let body = body.map(sanitize_notification_text);
    let mut message = match body {
        Some(b) if !b.is_empty() => {
            if title.is_empty() {
                b
            } else {
                format!("{title}: {b}")
            }
        }
        _ => title,
    };

    if message.is_empty() {
        return None;
    }

    const MAX_LEN: usize = 160;
    if message.chars().count() > MAX_LEN {
        let mut truncated = String::new();
        for ch in message.chars() {
            if truncated.chars().count() >= MAX_LEN.saturating_sub(3) {
                break;
            }
            truncated.push(ch);
        }
        truncated.push_str("...");
        message = truncated;
    }

    Some(message)
}

pub(super) fn emit_osc9_notification_inner(message: &str) {
    let payload = format!("\u{1b}]9;{message}\u{7}");
    let mut stdout = std::io::stdout();
    let _ = stdout.write_all(payload.as_bytes());
    let _ = stdout.flush();
}

fn sanitize_notification_text(input: &str) -> String {
    let mut sanitized = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '\u{00}'..='\u{08}' | '\u{0B}' | '\u{0C}' | '\u{0E}'..='\u{1F}' | '\u{7F}' => {}
            '\n' | '\r' | '\t' => {
                if !sanitized.ends_with(' ') {
                    sanitized.push(' ');
                }
            }
            _ => sanitized.push(ch),
        }
    }
    sanitized
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}


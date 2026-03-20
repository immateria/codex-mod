mod js;
mod python;
mod shell;

pub(crate) fn format_inline_script_for_display(command_escaped: &str) -> String {
    if let Some(formatted) = python::try_format_inline_python(command_escaped) {
        return formatted;
    }
    if let Some(formatted) = js::format_inline_node_for_display(command_escaped) {
        return formatted;
    }
    if let Some(formatted) = shell::format_inline_shell_for_display(command_escaped) {
        return formatted;
    }
    command_escaped.to_string()
}

fn escape_token_for_display(token: &str) -> String {
    if is_shell_word(token) {
        token.to_string()
    } else {
        let mut escaped = String::from("'");
        for ch in token.chars() {
            if ch == '\'' {
                escaped.push_str("'\\''");
            } else {
                escaped.push(ch);
            }
        }
        escaped.push('\'');
        escaped
    }
}

fn is_shell_word(token: &str) -> bool {
    token.chars().all(|ch| {
        matches!(
            ch,
            'a'..='z'
                | 'A'..='Z'
                | '0'..='9'
                | '_'
                | '-'
                | '.'
                | '/'
                | ':'
                | ','
                | '@'
                | '%'
                | '+'
                | '='
                | '['
                | ']'
        )
    })
}


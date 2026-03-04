use crate::bash::try_parse_bash;
use crate::bash::try_parse_word_only_commands_sequence;

/// Parse a script into a sequence of "word-only" commands using the bash AST
/// parser. This rejects constructs we cannot prove safe (subshells, redirects,
/// expansions that require evaluation, etc.).
pub(crate) fn parse_word_only_commands(script: &str) -> Option<Vec<Vec<String>>> {
    let tree = try_parse_bash(script)?;
    let commands = try_parse_word_only_commands_sequence(&tree, script)?;
    if commands.is_empty() {
        None
    } else {
        Some(commands)
    }
}

/// Parse a script into "word-only" commands, falling back to a very
/// conservative `shlex` split when structured parsing fails.
///
/// This is intended for dangerous-command detection (spotting destructive
/// operations) rather than proving something is safe.
pub(crate) fn parse_word_only_commands_with_fallback(script: &str) -> Option<Vec<Vec<String>>> {
    parse_word_only_commands(script).or_else(|| parse_plain_word_commands_fallback(script))
}

fn parse_plain_word_commands_fallback(script: &str) -> Option<Vec<Vec<String>>> {
    let tokens = shlex::split(script)?;
    if tokens.is_empty() {
        return None;
    }

    let mut all_commands: Vec<Vec<String>> = Vec::new();
    let mut current: Vec<String> = Vec::new();
    for token in tokens {
        if matches!(token.as_str(), "|" | "||" | "&&" | ";") {
            if current.is_empty() {
                return None;
            }
            all_commands.push(std::mem::take(&mut current));
        } else {
            current.push(token);
        }
    }

    if current.is_empty() {
        return None;
    }
    all_commands.push(current);
    Some(all_commands)
}

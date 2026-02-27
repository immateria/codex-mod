use super::prelude::*;

impl ChatWidget<'_> {
    pub(in super::super) fn split_leading_slash_command(text: &str) -> Option<(String, String)> {
        if !text.starts_with('/') {
            return None;
        }
        let mut parts = text.splitn(2, '\n');
        let first_line = parts.next().unwrap_or_default();
        let rest = parts.next().unwrap_or("");
        if rest.is_empty() {
            return None;
        }
        let command = first_line.trim_end_matches('\r');
        if command.is_empty() {
            return None;
        }
        if rest.trim().is_empty() {
            return None;
        }
        Some((command.to_string(), rest.to_string()))
    }

    pub(in super::super) fn slash_command_from_line(line: &str) -> Option<SlashCommand> {
        let trimmed = line.trim();
        let command_portion = trimmed.strip_prefix('/')?;
        let name = command_portion.split_whitespace().next()?;
        let canonical = name.to_ascii_lowercase();
        SlashCommand::from_str(&canonical).ok()
    }

    pub(in super::super) fn multiline_slash_command_requires_split(command_line: &str) -> bool {
        Self::slash_command_from_line(command_line)
            .map(|cmd| !cmd.is_prompt_expanding())
            .unwrap_or(true)
    }
}

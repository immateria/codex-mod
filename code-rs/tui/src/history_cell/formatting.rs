use crate::history::state::{
    ExecRecord,
    ExecStatus,
    ExecStreamChunk,
};
use crate::sanitize::Mode as SanitizeMode;
use crate::sanitize::Options as SanitizeOptions;
use crate::sanitize::sanitize_for_tui;
use crate::text_formatting::format_json_compact;
use code_ansi_escape::ansi_escape_line;
use code_core::history::state::MAX_EXEC_STREAM_RETAINED_BYTES;
use ratatui::style::{Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use std::cell::Cell;
use std::time::Instant;

use super::core::CommandOutput;
use super::layout_cache::LayoutCache;

// Unified preview format: show first 2 and last 5 non-empty lines with an ellipsis between.
const PREVIEW_HEAD_LINES: usize = 2;
const PREVIEW_TAIL_LINES: usize = 5;
const EXEC_PREVIEW_MAX_CHARS: usize = 16_000;
pub(crate) const STREAMING_EXIT_CODE: i32 = i32::MIN;
pub(crate) const OUTPUT_FOLD_THRESHOLD: usize = 40;

pub(crate) fn describe_exit_code(code: i32) -> &'static str {
    match code {
        1 => "general error",
        2 => "shell built-in misuse",
        126 => "not executable",
        127 => "command not found",
        130 => "interrupted",
        137 => "killed",
        139 => "segfault",
        _ => "",
    }
}

pub(crate) fn clean_wait_command(raw: &str) -> String {
    let trimmed = raw.trim();
    let Some((first_token, rest)) = split_token(trimmed) else {
        return trimmed.to_string();
    };
    if !looks_like_shell(first_token) {
        return trimmed.to_string();
    }
    let rest = rest.trim_start();
    let Some((second_token, remainder)) = split_token(rest) else {
        return trimmed.to_string();
    };
    if second_token != "-lc" {
        return trimmed.to_string();
    }
    let mut command = remainder.trim_start();
    if command.len() >= 2 {
        let bytes = command.as_bytes();
        let first_char = bytes[0] as char;
        let last_char = bytes[bytes.len().saturating_sub(1)] as char;
        if (first_char == '"' && last_char == '"') || (first_char == '\'' && last_char == '\'') {
            command = &command[1..command.len().saturating_sub(1)];
        }
    }
    if command.is_empty() {
        trimmed.to_string()
    } else {
        command.to_string()
    }
}

pub(crate) fn left_border_span() -> Span<'static> {
    Span::styled(
        "│ ",
        Style::default()
            .fg(crate::colors::border_dim())
            .bg(crate::colors::background()),
    )
}

fn split_token(input: &str) -> Option<(&str, &str)> {
    let s = input.trim_start();
    if s.is_empty() {
        return None;
    }
    if let Some(idx) = s.find(char::is_whitespace) {
        let (token, rest) = s.split_at(idx);
        Some((token, rest))
    } else {
        Some((s, ""))
    }
}

fn looks_like_shell(token: &str) -> bool {
    let trimmed = token.trim_matches('"').trim_matches('\'');
    let basename = trimmed
        .rsplit('/')
        .next()
        .unwrap_or(trimmed)
        .to_ascii_lowercase();
    matches!(
        basename.as_str(),
        "bash"
            | "bash.exe"
            | "sh"
            | "sh.exe"
            | "zsh"
            | "zsh.exe"
            | "dash"
            | "dash.exe"
            | "ksh"
            | "ksh.exe"
            | "busybox"
    )
}

/// Normalize common TTY overwrite sequences within a text block so that
/// progress lines using carriage returns, backspaces, or ESC[K erase behave as
/// expected when rendered in a pure-buffered UI (no cursor movement).
pub(crate) fn normalize_overwrite_sequences(input: &str) -> String {
    // Process per line, but keep CR/BS/CSI semantics within logical lines.
    // Treat "\n" as committing a line and resetting the cursor.
    let mut out = String::with_capacity(input.len());
    let mut line: Vec<char> = Vec::new(); // visible chars only
    let mut cursor: usize = 0; // column in visible chars

    // Helper to flush current line to out
    let flush_line = |line: &mut Vec<char>, cursor: &mut usize, out: &mut String| {
        if !line.is_empty() {
            out.push_str(&line.iter().collect::<String>());
        }
        out.push('\n');
        line.clear();
        *cursor = 0;
    };

    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        match ch {
            '\n' => {
                flush_line(&mut line, &mut cursor, &mut out);
                i += 1;
            }
            '\r' => {
                // Carriage return: move cursor to column 0
                cursor = 0;
                i += 1;
            }
            '\u{0008}' => {
                // Backspace: move left one column if possible
                cursor = cursor.saturating_sub(1);
                i += 1;
            }
            '\u{001B}' => {
                // CSI: ESC [ ... <cmd>
                if i + 1 < chars.len() && chars[i + 1] == '[' {
                    // Find final byte (alphabetic)
                    let mut j = i + 2;
                    while j < chars.len() && !chars[j].is_alphabetic() {
                        j += 1;
                    }
                    if j < chars.len() {
                        let cmd = chars[j];
                        // Extract numeric prefix (first parameter only)
                        let num: usize = chars[i + 2..j]
                            .iter()
                            .take_while(|c| c.is_ascii_digit())
                            .collect::<String>()
                            .parse()
                            .unwrap_or(0);

                        match cmd {
                            // Erase in Line: 0/None = cursor..end, 1 = start..cursor, 2 = entire line
                            'K' => {
                                let n = num; // default 0 when absent
                                match n {
                                    0 => {
                                        if cursor < line.len() {
                                            line.truncate(cursor);
                                        }
                                    }
                                    1 => {
                                        // Replace from start to cursor with spaces to keep remaining columns stable
                                        let end = cursor.min(line.len());
                                        for ch in line.iter_mut().take(end) {
                                            *ch = ' ';
                                        }
                                        // Trim leading spaces if the whole line became spaces
                                        while line.last().is_some_and(|c| *c == ' ') {
                                            line.pop();
                                        }
                                    }
                                    2 => {
                                        line.clear();
                                        cursor = 0;
                                    }
                                    _ => {}
                                }
                                i = j + 1;
                                continue;
                            }
                            // Cursor horizontal absolute (1-based)
                            'G' => {
                                let pos = num.saturating_sub(1);
                                cursor = pos.min(line.len());
                                i = j + 1;
                                continue;
                            }
                            // Cursor forward/backward
                            'C' => {
                                cursor = cursor.saturating_add(num);
                                i = j + 1;
                                continue;
                            }
                            'D' => {
                                cursor = cursor.saturating_sub(num);
                                i = j + 1;
                                continue;
                            }
                            _ => {
                                // Unknown/unsupported CSI (incl. SGR 'm'): keep styling intact by
                                // copying the entire sequence verbatim into the output so ANSI
                                // parsing can apply later, but do not affect cursor position.
                                // First, splice current visible buffer into out to preserve order
                                if !line.is_empty() {
                                    out.push_str(&line.iter().collect::<String>());
                                    line.clear();
                                    cursor = 0;
                                }
                                for ch in chars.iter().take(j + 1).skip(i) {
                                    out.push(*ch);
                                }
                                i = j + 1;
                                continue;
                            }
                        }
                    } else {
                        // Malformed CSI: drop it entirely by exiting the loop
                        break;
                    }
                } else {
                    // Other ESC sequences (e.g., OSC): pass through verbatim without affecting cursor
                    // Copy ESC and advance one; do not attempt to parse full OSC payload here.
                    if !line.is_empty() {
                        out.push_str(&line.iter().collect::<String>());
                        line.clear();
                        cursor = 0;
                    }
                    out.push(ch);
                    i += 1;
                }
            }
            _ => {
                // Put visible char at cursor, expanding with spaces if needed
                if cursor < line.len() {
                    line[cursor] = ch;
                } else {
                    while line.len() < cursor {
                        line.push(' ');
                    }
                    line.push(ch);
                }
                cursor += 1;
                i += 1;
            }
        }
    }
    // Flush any remaining visible text
    if !line.is_empty() {
        out.push_str(&line.iter().collect::<String>());
    }
    out
}

pub(crate) fn build_preview_lines(text: &str) -> Vec<Line<'static>> {
    build_preview_lines_windowed(
        text,
        PREVIEW_HEAD_LINES,
        PREVIEW_TAIL_LINES,
        EXEC_PREVIEW_MAX_CHARS,
    )
}

fn build_preview_lines_windowed(
    text: &str,
    head: usize,
    tail: usize,
    max_chars: usize,
) -> Vec<Line<'static>> {
    // Prefer UI‑themed JSON highlighting when the (ANSI‑stripped) text parses as JSON.
    let stripped_plain = sanitize_for_tui(
        text,
        SanitizeMode::Plain,
        SanitizeOptions {
            expand_tabs: true,
            tabstop: 4,
            debug_markers: false,
        },
    );
    if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&stripped_plain) {
        let pretty =
            serde_json::to_string_pretty(&json_val).unwrap_or_else(|_| json_val.to_string());
        let highlighted = crate::syntax_highlight::highlight_code_block(&pretty, Some("json"));
        return select_preview_from_lines(&highlighted, head, tail);
    }

    // Otherwise, compact valid JSON (without ANSI) to improve wrap, or pass original through.
    let processed = format_json_compact(text).unwrap_or_else(|| text.to_string());
    let processed = normalize_overwrite_sequences(&processed);
    let (processed, clipped) = clip_preview_text(&processed, max_chars);
    let processed = sanitize_for_tui(
        &processed,
        SanitizeMode::AnsiPreserving,
        SanitizeOptions {
            expand_tabs: true,
            tabstop: 4,
            debug_markers: false,
        },
    );
    let non_empty: Vec<&str> = processed.lines().filter(|line| !line.is_empty()).collect();

    enum Seg<'a> {
        Line(&'a str),
        Ellipsis,
    }
    let segments: Vec<Seg> = if non_empty.len() <= head + tail {
        non_empty.iter().map(|s| Seg::Line(s)).collect()
    } else {
        let mut v: Vec<Seg> = Vec::with_capacity(head + tail + 1);
        // Head
        for line in non_empty.iter().take(head) {
            v.push(Seg::Line(line));
        }
        v.push(Seg::Ellipsis);
        // Tail
        let start = non_empty.len().saturating_sub(tail);
        for s in &non_empty[start..] {
            v.push(Seg::Line(s));
        }
        v
    };

    fn ansi_line_with_theme_bg(s: &str) -> Line<'static> {
        let mut ln = ansi_escape_line(s);
        for sp in ln.spans.iter_mut() {
            sp.style.bg = None;
        }
        ln
    }

    let mut out: Vec<Line<'static>> = Vec::new();
    if clipped {
        out.push(Line::styled(
            format!("… output truncated to last {max_chars} chars"),
            Style::default().fg(crate::colors::text_dim()),
        ));
    }
    for seg in segments {
        match seg {
            Seg::Line(line) => out.push(ansi_line_with_theme_bg(line)),
            Seg::Ellipsis => out.push(Line::from("⋮".dim())),
        }
    }
    out
}

fn clip_preview_text(text: &str, limit: usize) -> (String, bool) {
    let char_count = text.chars().count();
    if char_count <= limit {
        return (text.to_string(), false);
    }
    let tail: String = text
        .chars()
        .rev()
        .take(limit)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    (tail, true)
}

pub(crate) fn output_lines(
    output: Option<&CommandOutput>,
    only_err: bool,
    include_angle_pipe: bool,
) -> Vec<Line<'static>> {
    let CommandOutput {
        exit_code,
        stdout,
        stderr,
    } = match output {
        Some(o) => o,
        None => return Vec::new(),
    };

    let mut lines: Vec<Line<'static>> = Vec::new();
    let is_streaming_preview = *exit_code == STREAMING_EXIT_CODE;

    if !only_err && !stdout.is_empty() {
        let mut stdout_lines = build_preview_lines(stdout);
        if include_angle_pipe {
            let angle_style = Style::default()
                .fg(crate::colors::text_dim())
                .add_modifier(Modifier::DIM);
            for line in stdout_lines.iter_mut() {
                line.spans.insert(0, Span::styled("> ", angle_style));
            }
        }
        lines.extend(stdout_lines);
    }

    if !stderr.is_empty() && (is_streaming_preview || *exit_code != 0) {
        if !lines.is_empty() {
            lines.push(Line::from(""));
        }
        if !is_streaming_preview {
            let description = describe_exit_code(*exit_code);
            let msg = if description.is_empty() {
                format!("Error (exit {exit_code})")
            } else {
                format!("Error (exit {exit_code}: {description})")
            };
            lines.push(Line::styled(msg, Style::default().fg(crate::colors::error())));
        }
        let stderr_norm = sanitize_for_tui(
            &normalize_overwrite_sequences(stderr),
            SanitizeMode::AnsiPreserving,
            SanitizeOptions {
                expand_tabs: true,
                tabstop: 4,
                debug_markers: false,
            },
        );
        for line in stderr_norm.lines().filter(|line| !line.is_empty()) {
            lines.push(ansi_escape_line(line).style(Style::default().fg(crate::colors::error())));
        }
    }

    if !lines.is_empty() {
        lines.push(Line::from(""));
    }

    lines
}

pub(crate) fn pretty_provider_name(id: &str) -> String {
    // Special case common providers with human-friendly names
    match id {
        "brave-search" => "brave",
        "screenshot-website-fast" => "screenshot",
        "read-website-fast" => "readweb",
        "sequential-thinking" => "think",
        "discord-bot" => "discord",
        _ => id,
    }
    .to_string()
}

pub(crate) fn lines_to_plain_text(lines: &[Line<'_>]) -> String {
    lines
        .iter()
        .map(line_to_plain_text)
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) fn line_to_plain_text(line: &Line<'_>) -> String {
    line
        .spans
        .iter()
        .map(|sp| sp.content.as_ref())
        .collect::<String>()
}

// Helper: choose first `head` and last `tail` non-empty lines from a styled line list
pub(crate) fn select_preview_from_lines(
    lines: &[Line<'static>],
    head: usize,
    tail: usize,
) -> Vec<Line<'static>> {
    fn is_non_empty(l: &Line<'_>) -> bool {
        let s: String = l.spans.iter().map(|sp| sp.content.as_ref()).collect();
        !s.trim().is_empty()
    }
    let non_empty_idx: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter_map(|(i, l)| if is_non_empty(l) { Some(i) } else { None })
        .collect();
    if non_empty_idx.len() <= head + tail {
        return lines.to_vec();
    }
    let mut out: Vec<Line<'static>> = Vec::new();
    for &i in non_empty_idx.iter().take(head) {
        out.push(lines[i].clone());
    }
    out.push(Line::from("⋮".dim()));
    for &i in non_empty_idx
        .iter()
        .rev()
        .take(tail)
        .collect::<Vec<_>>()
        .iter()
        .rev()
    {
        out.push(lines[*i].clone());
    }
    out
}

// Helper: build a preview window with custom head/tail sizes.
pub(crate) fn select_preview_from_plain_text(text: &str, head: usize, tail: usize) -> Vec<Line<'static>> {
    build_preview_lines_windowed(text, head, tail, EXEC_PREVIEW_MAX_CHARS)
}

/// Check if a line appears to be a title/header (like "codex", "user", "thinking", etc.)
fn is_title_line(line: &Line) -> bool {
    // Check if the line has special formatting that indicates it's a title
    if line.spans.is_empty() {
        return false;
    }

    // Get the text content of the line
    let text: String = line
        .spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect::<String>()
        .trim()
        .to_lowercase();

    // Check for common title patterns (fallback heuristic only; primary logic uses explicit cell types)
    matches!(
        text.as_str(),
        "codex"
            | "user"
            | "thinking"
            | "event"
            | "tool"
            | "/diff"
            | "/status"
            | "/prompts"
            | "/skills"
            | "reasoning effort"
            | "error"
    ) || text.starts_with("…")
        || text.starts_with("✓")
        || text.starts_with("✗")
        || text.starts_with("↯")
        || text.starts_with("proposed patch")
        || text.starts_with("applying patch")
        || text.starts_with("updating")
        || text.starts_with("updated")
}

/// Check if a line is empty (no content or just whitespace)
fn is_empty_line(line: &Line) -> bool {
    if line.spans.is_empty() {
        return true;
    }
    // Consider a line empty when all spans have only whitespace
    line.spans
        .iter()
        .all(|s| s.content.as_ref().trim().is_empty())
}

/// Trim empty lines from the beginning and end of a Vec<Line>.
/// Also normalizes internal spacing - no more than 1 empty line between content.
/// This ensures consistent spacing when cells are rendered together.
pub(crate) fn trim_empty_lines(mut lines: Vec<Line<'static>>) -> Vec<Line<'static>> {
    // Remove ALL leading empty lines
    while lines.first().is_some_and(is_empty_line) {
        lines.remove(0);
    }

    // Remove ALL trailing empty lines
    while lines.last().is_some_and(is_empty_line) {
        lines.pop();
    }

    // Normalize internal spacing - no more than 1 empty line in a row
    let mut result = Vec::new();
    let mut prev_was_empty = false;

    for line in lines {
        let is_empty = is_empty_line(&line);

        // Skip consecutive empty lines
        if is_empty && prev_was_empty {
            continue;
        }

        // Special case: If this is an empty line right after a title, skip it
        if is_empty && result.len() == 1 && result.first().is_some_and(is_title_line) {
            continue;
        }

        result.push(line);
        prev_was_empty = is_empty;
    }

    result
}

// ---------------------------------------------------------------------------
// Shared exec/js-repl output helpers
// ---------------------------------------------------------------------------

fn chunks_to_string(chunks: &[ExecStreamChunk]) -> String {
    if chunks.is_empty() {
        return String::new();
    }
    let mut sorted = chunks.to_vec();
    sorted.sort_by_key(|chunk| chunk.offset);
    let mut combined = String::new();
    for chunk in sorted {
        combined.push_str(&chunk.content);
    }
    combined
}

pub(crate) fn render_exec_stream(chunks: &[ExecStreamChunk], stream_name: &str) -> String {
    let mut body = chunks_to_string(chunks);
    if let Some(first) = chunks.first()
        && first.offset > 0 {
            let mut notice = String::new();
            notice.push_str(&format!(
                "… clipped {} from the start of {} (showing last {}).\n\n",
                code_core::util::format_bytes(first.offset),
                stream_name,
                code_core::util::format_bytes(MAX_EXEC_STREAM_RETAINED_BYTES),
            ));
            notice.push_str(&body);
            body = notice;
        }
    body
}

pub(crate) fn record_output(record: &ExecRecord) -> Option<CommandOutput> {
    if !matches!(record.status, ExecStatus::Running) {
        let stdout = render_exec_stream(&record.stdout_chunks, "stdout");
        let stderr = render_exec_stream(&record.stderr_chunks, "stderr");
        let exit_code = record.exit_code.unwrap_or_default();
        return Some(CommandOutput {
            exit_code,
            stdout,
            stderr,
        });
    }
    None
}

pub(crate) fn should_auto_collapse_output(output: Option<&CommandOutput>) -> bool {
    let Some(out) = output else { return false; };
    out.stdout
        .lines()
        .count()
        .saturating_add(out.stderr.lines().count())
        > OUTPUT_FOLD_THRESHOLD
}

/// Shared output/streaming state sync used by ExecCell and JsReplCell.
///
/// Callers are responsible for capturing `was_running` before overwriting their
/// stored record so we only auto-collapse on the running -> completed
/// transition.
pub(crate) fn sync_exec_output_state<L: Default>(
    record: &ExecRecord,
    was_running: bool,
    output: &mut Option<CommandOutput>,
    stream_preview: &mut Option<CommandOutput>,
    start_time: &mut Option<Instant>,
    collapsed_output: &Cell<bool>,
    layout_cache: &LayoutCache<L>,
) {
    *output = record_output(record);
    *stream_preview = build_streaming_preview(record);

    if matches!(record.status, ExecStatus::Running) {
        if start_time.is_none() {
            *start_time = Some(Instant::now());
        }
    } else {
        *start_time = None;
    }

    if was_running
        && !matches!(record.status, ExecStatus::Running)
        && should_auto_collapse_output(output.as_ref())
    {
        collapsed_output.set(true);
    }

    layout_cache.invalidate();
}

/// Build a streaming preview `CommandOutput` from a running exec record,
/// or `None` if the record is not running or has no output yet.
pub(crate) fn build_streaming_preview(record: &ExecRecord) -> Option<CommandOutput> {
    if !matches!(record.status, ExecStatus::Running) {
        return None;
    }
    let stdout = render_exec_stream(&record.stdout_chunks, "stdout");
    let stderr = render_exec_stream(&record.stderr_chunks, "stderr");
    if stdout.is_empty() && stderr.is_empty() {
        None
    } else {
        Some(CommandOutput::streaming_preview(stdout, stderr))
    }
}

/// Configuration for folding (collapsing) a block of lines.
pub(crate) struct FoldConfig {
    /// Maximum visible lines when collapsed.
    pub threshold: usize,
}

/// Per-section limits for structured folds (e.g. ToolCallCell with args + result + error).
pub(crate) struct FoldSectionLimits {
    pub args: usize,
    pub result: usize,
    pub error: usize,
}

impl FoldConfig {
    /// Standard output fold (ExecCell, JsReplCell, WebFetchToolCell).
    pub(crate) fn output() -> Self {
        Self { threshold: OUTPUT_FOLD_THRESHOLD }
    }

    /// Custom threshold (e.g. WebFetchToolCell body preview).
    pub(crate) fn with_threshold(threshold: usize) -> Self {
        Self { threshold }
    }
}

/// Fold indicator line used by all fold types.
pub(crate) fn fold_indicator(hidden_count: usize) -> Line<'static> {
    Line::from(Span::styled(
        format!("… {hidden_count} more lines (use Fold Output to expand)"),
        Style::default()
            .fg(crate::colors::text_dim())
            .add_modifier(Modifier::DIM),
    ))
}

/// If `collapsed` is true and `lines` exceeds the fold threshold,
/// truncate in-place and append a fold indicator.
pub(crate) fn maybe_fold_output(lines: &mut Vec<Line<'static>>, collapsed: bool) {
    fold_lines(lines, collapsed, &FoldConfig::output());
}

/// Generic fold: if `collapsed` is true and `lines` exceeds `config.threshold`,
/// truncate in-place and append a fold indicator.
pub(crate) fn fold_lines(lines: &mut Vec<Line<'static>>, collapsed: bool, config: &FoldConfig) {
    if collapsed && lines.len() > config.threshold {
        let folded_count = lines.len() - config.threshold;
        lines.truncate(config.threshold);
        lines.push(fold_indicator(folded_count));
    }
}

/// Fold multiple sections (args, result, error) into a collapsed preview.
/// Returns the folded lines and the total hidden count.
pub(crate) fn fold_sections(
    args: Vec<Line<'static>>,
    result: Vec<Line<'static>>,
    error: Vec<Line<'static>>,
    limits: &FoldSectionLimits,
) -> Vec<Line<'static>> {
    let total = args.len()
        .saturating_add(result.len())
        .saturating_add(error.len());

    let mut shown: Vec<Line<'static>> = Vec::new();
    shown.extend(args.into_iter().take(limits.args));
    shown.extend(result.into_iter().take(limits.result));
    shown.extend(error.into_iter().take(limits.error));

    let hidden = total.saturating_sub(shown.len());
    if hidden > 0 {
        shown.push(fold_indicator(hidden));
    }
    shown
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_preview_from_plain_text_inserts_ellipsis() {
        let text = (1..=10)
            .map(|n| n.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        let lines = select_preview_from_plain_text(&text, 2, 2);
        assert_eq!(lines.len(), 5);
        assert_eq!(line_to_plain_text(&lines[2]), "⋮");
        assert!(lines_to_plain_text(&lines).contains("1"));
        assert!(lines_to_plain_text(&lines).contains("10"));
        assert!(!lines_to_plain_text(&lines).contains("5"));
    }
}

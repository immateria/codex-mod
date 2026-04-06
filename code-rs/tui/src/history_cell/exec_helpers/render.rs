use std::time::{Duration, Instant};

use code_common::elapsed::format_duration;
use code_core::parse_command::ParsedCommand;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::exec_command::strip_bash_lc_and_escape;
use crate::history::compat::ExecAction;

use super::super::core::CommandOutput;
use super::super::exec::ParsedExecMetadata;
use super::super::formatting::output_lines;

use super::inline_scripts::format_inline_script_for_display;
use super::read_annotation::{coalesce_read_ranges_in_lines_local, parse_read_line_annotation};
use super::shell_display::{
    emphasize_shell_command_name,
    insert_line_breaks_after_double_ampersand,
    normalize_shell_command_display,
};
pub(crate) fn exec_command_lines(
    command: &[String],
    parsed: &[ParsedCommand],
    output: Option<&CommandOutput>,
    stream_preview: Option<&CommandOutput>,
    start_time: Option<Instant>,
) -> Vec<Line<'static>> {
    match parsed.is_empty() {
        true => new_exec_command_generic(command, output, stream_preview, start_time),
        false => new_parsed_command(parsed, output, stream_preview, start_time),
    }
}

pub(crate) fn exec_render_parts_parsed_with_meta(
    parsed_commands: &[ParsedCommand],
    meta: &ParsedExecMetadata,
    output: Option<&CommandOutput>,
    stream_preview: Option<&CommandOutput>,
    elapsed_since_start: Option<Duration>,
    status_label: &str,
) -> (
    Vec<Line<'static>>,
    Vec<Line<'static>>,
    Option<Line<'static>>,
) {
    let action = meta.action;
    let ctx_path = meta.ctx_path.as_deref();
    let suppress_run_header = matches!(action, ExecAction::Run) && output.is_some();
    let mut pre: Vec<Line<'static>> = Vec::new();
    let mut running_status: Option<Line<'static>> = None;
    if !suppress_run_header {
        match output {
            None => match action {
                ExecAction::Read => pre.push(Line::styled(
                    "Read",
                    Style::default().fg(crate::colors::text()),
                )),
                ExecAction::Search => pre.push(Line::styled(
                    "Search",
                    Style::default().fg(crate::colors::text_dim()),
                )),
                ExecAction::List => pre.push(Line::styled(
                    "List",
                    Style::default().fg(crate::colors::text()),
                )),
                ExecAction::Run => {
                    let mut message = match &ctx_path {
                        Some(p) => format!("{status_label}... in {p}"),
                        None => format!("{status_label}..."),
                    };
                    if let Some(elapsed) = elapsed_since_start {
                        message = format!("{message} ({})", format_duration(elapsed));
                    }
                    running_status = Some(running_status_line(message));
                }
            },
            Some(o) if o.exit_code == 0 => {
                let done = match action {
                    ExecAction::Read => "Read".to_string(),
                    ExecAction::Search => "Search".to_string(),
                    ExecAction::List => "List".to_string(),
                    ExecAction::Run => match &ctx_path {
                        Some(p) => format!("Ran in {p}"),
                        None => "Ran".to_string(),
                    },
                };
                if matches!(
                    action,
                    ExecAction::Read | ExecAction::Search | ExecAction::List
                ) {
                    pre.push(Line::styled(
                        done,
                        Style::default().fg(crate::colors::text_dim()),
                    ));
                } else {
                    pre.push(Line::styled(
                        done,
                        Style::default()
                            .fg(crate::colors::text_bright())
                            .add_modifier(Modifier::BOLD),
                    ));
                }
            }
            Some(_) => {
                let done = match action {
                    ExecAction::Read => "Read".to_string(),
                    ExecAction::Search => "Search".to_string(),
                    ExecAction::List => "List".to_string(),
                    ExecAction::Run => match &ctx_path {
                        Some(p) => format!("Ran in {p}"),
                        None => "Ran".to_string(),
                    },
                };
                if matches!(
                    action,
                    ExecAction::Read | ExecAction::Search | ExecAction::List
                ) {
                    pre.push(Line::styled(
                        done,
                        Style::default().fg(crate::colors::text_dim()),
                    ));
                } else {
                    pre.push(Line::styled(
                        done,
                        Style::default()
                            .fg(crate::colors::text_bright())
                            .add_modifier(Modifier::BOLD),
                    ));
                }
            }
        }
    }

    // Reuse the same parsed-content rendering as new_parsed_command
    let search_paths = &meta.search_paths;
    // Compute output preview first to know whether to draw the downward corner.
    let show_stdout = matches!(action, ExecAction::Run);
    let display_output = output.or(stream_preview);
    let mut out = output_lines(display_output, !show_stdout, false);
    let mut any_content_emitted = false;
    // Determine allowed label(s) for this cell's primary action
    let expected_label: Option<&'static str> = match action {
        ExecAction::Read => Some("Read"),
        ExecAction::Search => Some("Search"),
        ExecAction::List => Some("List"),
        ExecAction::Run => None, // run: allow a set of labels
    };
    let use_content_connectors = !(matches!(action, ExecAction::Run) && output.is_none());

    for parsed in parsed_commands.iter() {
        let (label, content) = match parsed {
            ParsedCommand::Read { name, cmd, .. } => {
                let mut c = name.clone();
                if let Some(ann) = parse_read_line_annotation(cmd) {
                    c = format!("{c} {ann}");
                }
                ("Read".to_string(), c)
            }
            ParsedCommand::ListFiles { cmd: _, path } => match path {
                Some(p) => {
                    if search_paths.contains(p) {
                        (String::new(), String::new())
                    } else {
                        let display_p = if p.ends_with('/') {
                            p.clone()
                        } else {
                            format!("{p}/")
                        };
                        ("List".to_string(), display_p.clone())
                    }
                }
                None => ("List".to_string(), "./".to_string()),
            },
            ParsedCommand::Search { query, path, cmd } => {
                // Make search terms human-readable:
                // - Unescape any backslash-escaped character (e.g., "\?" -> "?")
                // - Close unbalanced pairs for '(' and '{' to avoid dangling text in UI
                let prettify_term = |s: &str| -> String {
                    // General unescape: remove backslashes that escape the next char
                    let mut out = String::with_capacity(s.len());
                    let mut iter = s.chars();
                    while let Some(ch) = iter.next() {
                        if ch == '\\' {
                            if let Some(next) = iter.next() {
                                out.push(next);
                            } else {
                                out.push('\\');
                            }
                        } else {
                            out.push(ch);
                        }
                    }

                    // Balance parentheses
                    let opens_paren = out.matches("(").count();
                    let closes_paren = out.matches(")").count();
                    for _ in 0..opens_paren.saturating_sub(closes_paren) {
                        out.push(')');
                    }

                    // Balance curly braces
                    let opens_curly = out.matches("{").count();
                    let closes_curly = out.matches("}").count();
                    for _ in 0..opens_curly.saturating_sub(closes_curly) {
                        out.push('}');
                    }

                    out
                };
                let fmt_query = |q: &str| -> String {
                    let mut parts: Vec<String> = q
                        .split('|')
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(prettify_term)
                        .collect();
                    match parts.len() {
                        0 => String::new(),
                        1 => parts.remove(0),
                        2 => format!("{} and {}", parts[0], parts[1]),
                        _ => {
                            let last = parts.last().cloned().unwrap_or_default();
                            let head = &parts[..parts.len() - 1];
                            format!("{} and {}", head.join(", "), last)
                        }
                    }
                };
                match (query, path) {
                    (Some(q), Some(p)) => {
                        let display_p = if p.ends_with('/') {
                            p.clone()
                        } else {
                            format!("{p}/")
                        };
                        (
                            "Search".to_string(),
                            format!("{} in {}", fmt_query(q), display_p),
                        )
                    }
                    (Some(q), None) => ("Search".to_string(), fmt_query(q).clone()),
                    (None, Some(p)) => {
                        let display_p = if p.ends_with('/') {
                            p.clone()
                        } else {
                            format!("{p}/")
                        };
                        ("Search".to_string(), format!(" in {display_p}"))
                    }
                    (None, None) => ("Search".to_string(), cmd.clone()),
                }
            }
            ParsedCommand::ReadCommand { cmd } => ("Run".to_string(), cmd.clone()),
            // Upstream variants not present in our core parser are ignored or treated as generic runs
            ParsedCommand::Unknown { cmd } => {
                // Suppress separator helpers like `echo ---` which are used
                // internally to delimit chunks when reading files.
                let t = cmd.trim();
                let lower = t.to_lowercase();
                if lower.starts_with("echo") && lower.contains("---") {
                    (String::new(), String::new()) // drop from preamble
                } else {
                    ("Run".to_string(), format_inline_script_for_display(cmd))
                }
            } // Noop variant not present in our core parser
              // ParsedCommand::Noop { .. } => continue,
        };
        // Enforce per-action grouping: only keep entries matching this cell's action.
        if let Some(exp) = expected_label {
            if label != exp {
                continue;
            }
        } else if !(label == "Run" || label == "Search") {
            // For generic "run" header, keep common run-like labels only.
            continue;
        }
        if label.is_empty() && content.is_empty() {
            continue;
        }
        for line_text in content.lines() {
            if line_text.is_empty() {
                continue;
            }
            let prefix = if !any_content_emitted {
                if suppress_run_header || !use_content_connectors {
                    ""
                } else {
                    "└ "
                }
            } else if suppress_run_header || !use_content_connectors {
                ""
            } else {
                "  "
            };
            let mut spans: Vec<Span<'static>> = Vec::new();
            if !prefix.is_empty() {
                spans.push(Span::styled(
                    prefix,
                    Style::default().add_modifier(Modifier::DIM),
                ));
            }
            match label.as_str() {
                "Search" => {
                    let remaining = line_text.to_string();
                    let (terms_part, path_part) = if let Some(idx) = remaining.rfind(" (in ") {
                        (
                            remaining[..idx].to_string(),
                            Some(remaining[idx..].to_string()),
                        )
                    } else if let Some(idx) = remaining.rfind(" in ") {
                        let suffix = &remaining[idx + 1..];
                        if suffix.trim_end().ends_with('/') {
                            (
                                remaining[..idx].to_string(),
                                Some(remaining[idx..].to_string()),
                            )
                        } else {
                            (remaining.clone(), None)
                        }
                    } else {
                        (remaining.clone(), None)
                    };
                    let tmp = terms_part.clone();
                    let chunks: Vec<String> = if tmp.contains(", ") {
                        tmp.split(", ").map(std::string::ToString::to_string).collect()
                    } else {
                        vec![tmp.clone()]
                    };
                    for (i, chunk) in chunks.iter().enumerate() {
                        if i > 0 {
                            spans.push(Span::styled(
                                ", ",
                                Style::default().fg(crate::colors::text_dim()),
                            ));
                        }
                        if let Some((left, right)) = chunk.rsplit_once(" and ") {
                            if !left.is_empty() {
                                spans.push(Span::styled(
                                    left.to_string(),
                                    Style::default().fg(crate::colors::text()),
                                ));
                                spans.push(Span::styled(
                                    " and ",
                                    Style::default().fg(crate::colors::text_dim()),
                                ));
                                spans.push(Span::styled(
                                    right.to_string(),
                                    Style::default().fg(crate::colors::text()),
                                ));
                            } else {
                                spans.push(Span::styled(
                                    chunk.clone(),
                                    Style::default().fg(crate::colors::text()),
                                ));
                            }
                        } else {
                            spans.push(Span::styled(
                                chunk.clone(),
                                Style::default().fg(crate::colors::text()),
                            ));
                        }
                    }
                    if let Some(p) = path_part {
                        spans.push(Span::styled(
                            p,
                            Style::default().fg(crate::colors::text_dim()),
                        ));
                    }
                }
                "Read" => {
                    if let Some(idx) = line_text.find(" (") {
                        let (fname, rest) = line_text.split_at(idx);
                        spans.push(Span::styled(
                            fname.to_string(),
                            Style::default().fg(crate::colors::text()),
                        ));
                        spans.push(Span::styled(
                            rest.to_string(),
                            Style::default().fg(crate::colors::text_dim()),
                        ));
                    } else {
                        spans.push(Span::styled(
                            line_text.to_string(),
                            Style::default().fg(crate::colors::text()),
                        ));
                    }
                }
                "List" => {
                    spans.push(Span::styled(
                        line_text.to_string(),
                        Style::default().fg(crate::colors::text()),
                    ));
                }
                _ => {
                    // Apply shell syntax highlighting to executed command lines.
                    // We highlight the single logical line as bash and append its spans inline.
                    let normalized = normalize_shell_command_display(line_text);
                    let display_line = insert_line_breaks_after_double_ampersand(&normalized);
                    let mut hl =
                        crate::syntax_highlight::highlight_code_block(&display_line, Some("bash"));
                    if let Some(mut first_line) = hl.pop() {
                        emphasize_shell_command_name(&mut first_line);
                        spans.extend(first_line.spans.into_iter());
                    } else {
                        spans.push(Span::styled(
                            display_line,
                            Style::default().fg(crate::colors::text()),
                        ));
                    }
                }
            }
            pre.push(Line::from(spans));
            any_content_emitted = true;
        }
    }

    // If this is a List cell and nothing emitted (e.g., suppressed due to matching Search path),
    // still show a single contextual line so users can see where we listed.
    if matches!(action, ExecAction::List) && !any_content_emitted {
        let display_p = match &ctx_path {
            Some(p) if !p.is_empty() => {
                if p.ends_with('/') {
                    p.to_string()
                } else {
                    format!("{p}/")
                }
            }
            _ => "./".to_string(),
        };
        pre.push(Line::from(vec![
            Span::styled("└ ", Style::default().add_modifier(Modifier::DIM)),
            Span::styled(
                display_p,
                Style::default().fg(crate::colors::text()),
            ),
        ]));
    }

    // Collapse adjacent Read ranges for the same file inside a single exec's preamble
    coalesce_read_ranges_in_lines_local(&mut pre);

    // Output: show stdout only for real run commands; errors always included
    // Collapse adjacent Read ranges for the same file inside a single exec's preamble
    coalesce_read_ranges_in_lines_local(&mut pre);

    if running_status.is_some()
        && let Some(last) = out.last() {
            let is_blank = last
                .spans
                .iter()
                .all(|sp| sp.content.as_ref().trim().is_empty());
            if is_blank {
                out.pop();
            }
        }

    (pre, out, running_status)
}

pub(crate) fn exec_render_parts_parsed(
    parsed_commands: &[ParsedCommand],
    output: Option<&CommandOutput>,
    stream_preview: Option<&CommandOutput>,
    elapsed_since_start: Option<Duration>,
    status_label: &str,
) -> (
    Vec<Line<'static>>,
    Vec<Line<'static>>,
    Option<Line<'static>>,
) {
    let meta = ParsedExecMetadata::from_commands(parsed_commands);
    exec_render_parts_parsed_with_meta(
        parsed_commands,
        &meta,
        output,
        stream_preview,
        elapsed_since_start,
        status_label,
    )
}

pub(crate) fn running_status_line(message: String) -> Line<'static> {
    Line::from(vec![
        Span::styled("└ ", Style::default().fg(crate::colors::border_dim())),
        Span::styled(message, Style::default().fg(crate::colors::text_dim())),
    ])
}

fn new_parsed_command(
    parsed_commands: &[ParsedCommand],
    output: Option<&CommandOutput>,
    stream_preview: Option<&CommandOutput>,
    start_time: Option<Instant>,
) -> Vec<Line<'static>> {
    let meta = ParsedExecMetadata::from_commands(parsed_commands);
    let action = meta.action;
    let ctx_path = meta.ctx_path.as_deref();
    let suppress_run_header = matches!(action, ExecAction::Run) && output.is_some();
    let mut lines: Vec<Line> = Vec::new();
    let mut running_status: Option<Line<'static>> = None;
    if !suppress_run_header {
        match output {
            None => {
                if matches!(action, ExecAction::Run) {
                    let mut message = match &ctx_path {
                        Some(p) => format!("Running... in {p}"),
                        None => "Running...".to_string(),
                    };
                    if let Some(start) = start_time {
                        let elapsed = start.elapsed();
                        message = format!("{message} ({})", format_duration(elapsed));
                    }
                    running_status = Some(running_status_line(message));
                } else {
                    let duration_suffix = if let Some(start) = start_time {
                        let elapsed = start.elapsed();
                        format!(" ({})", format_duration(elapsed))
                    } else {
                        String::new()
                    };
                    let header = match action {
                        ExecAction::Read => "Read",
                        ExecAction::Search => "Search",
                        ExecAction::List => "List",
                        ExecAction::Run => unreachable!(),
                    };
                    lines.push(Line::styled(
                        format!("{header}{duration_suffix}"),
                        Style::default().fg(crate::colors::text_dim()),
                    ));
                }
            }
            Some(o) if o.exit_code == 0 => {
                if matches!(
                    action,
                    ExecAction::Read | ExecAction::Search | ExecAction::List
                ) {
                    lines.push(Line::styled(
                        match action {
                            ExecAction::Read => "Read",
                            ExecAction::Search => "Search",
                            ExecAction::List => "List",
                            ExecAction::Run => unreachable!(),
                        },
                        Style::default().fg(crate::colors::text()),
                    ));
                } else {
                    let done = match ctx_path {
                        Some(p) => format!("Ran in {p}"),
                        None => "Ran".to_string(),
                    };
                    lines.push(Line::styled(
                        done,
                        Style::default()
                            .fg(crate::colors::text_bright())
                            .add_modifier(Modifier::BOLD),
                    ));
                }
            }
            Some(_o) => {
                if matches!(
                    action,
                    ExecAction::Read | ExecAction::Search | ExecAction::List
                ) {
                    lines.push(Line::styled(
                        match action {
                            ExecAction::Read => "Read",
                            ExecAction::Search => "Search",
                            ExecAction::List => "List",
                            ExecAction::Run => unreachable!(),
                        },
                        Style::default().fg(crate::colors::text()),
                    ));
                } else {
                    let done = match ctx_path {
                        Some(p) => format!("Ran in {p}"),
                        None => "Ran".to_string(),
                    };
                    lines.push(Line::styled(
                        done,
                        Style::default()
                            .fg(crate::colors::text_bright())
                            .add_modifier(Modifier::BOLD),
                    ));
                }
            }
        }
    }

    // Collect any paths referenced by search commands to suppress redundant directory lines
    let search_paths = &meta.search_paths;

    // We'll emit only content lines here; the header above already communicates the action.
    // Use a single leading "└ " for the very first content line, then indent subsequent ones,
    // except when we're showing an inline running status for ExecAction::Run.
    let mut any_content_emitted = false;
    let use_content_connectors = !(matches!(action, ExecAction::Run) && output.is_none());

    // Restrict displayed entries to the primary action for this cell.
    // For the generic "run" header, allow Run/Test/Lint/Format entries.
    let expected_label: Option<&'static str> = match action {
        ExecAction::Read => Some("Read"),
        ExecAction::Search => Some("Search"),
        ExecAction::List => Some("List"),
        ExecAction::Run => None,
    };

    for parsed in parsed_commands.iter() {
        // Produce a logical label and content string without icons
        let (label, content) = match parsed {
            ParsedCommand::Read { name, cmd, .. } => {
                let mut c = name.clone();
                if let Some(ann) = parse_read_line_annotation(cmd) {
                    c = format!("{c} {ann}");
                }
                ("Read".to_string(), c)
            }
            ParsedCommand::ListFiles { cmd: _, path } => match path {
                Some(p) => {
                    if search_paths.contains(p) {
                        (String::new(), String::new()) // suppressed
                    } else {
                        let display_p = if p.ends_with('/') {
                            p.clone()
                        } else {
                            format!("{p}/")
                        };
                        ("List".to_string(), display_p.clone())
                    }
                }
                None => ("List".to_string(), "./".to_string()),
            },
            ParsedCommand::Search { query, path, cmd } => {
                // Format query for display: unescape backslash-escapes and close common unbalanced delimiters
                let prettify_term = |s: &str| -> String {
                    // General unescape: turn "\X" into "X" for any X
                    let mut out = String::with_capacity(s.len());
                    let mut iter = s.chars();
                    while let Some(ch) = iter.next() {
                        if ch == '\\' {
                            if let Some(next) = iter.next() {
                                out.push(next);
                            } else {
                                out.push('\\');
                            }
                        } else {
                            out.push(ch);
                        }
                    }
                    // Balance parentheses
                    let opens_paren = out.matches("(").count();
                    let closes_paren = out.matches(")").count();
                    for _ in 0..opens_paren.saturating_sub(closes_paren) {
                        out.push(')');
                    }
                    // Balance curly braces
                    let opens_curly = out.matches("{").count();
                    let closes_curly = out.matches("}").count();
                    for _ in 0..opens_curly.saturating_sub(closes_curly) {
                        out.push('}');
                    }
                    out
                };
                let fmt_query = |q: &str| -> String {
                    let mut parts: Vec<String> = q
                        .split('|')
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(prettify_term)
                        .collect();
                    match parts.len() {
                        0 => String::new(),
                        1 => parts.remove(0),
                        2 => format!("{} and {}", parts[0], parts[1]),
                        _ => {
                            let last = parts.last().cloned().unwrap_or_default();
                            let head = &parts[..parts.len() - 1];
                            format!("{} and {}", head.join(", "), last)
                        }
                    }
                };
                match (query, path) {
                    (Some(q), Some(p)) => {
                        let display_p = if p.ends_with('/') {
                            p.clone()
                        } else {
                            format!("{p}/")
                        };
                        (
                            "Search".to_string(),
                            format!("{} in {}", fmt_query(q), display_p),
                        )
                    }
                    (Some(q), None) => ("Search".to_string(), fmt_query(q).clone()),
                    (None, Some(p)) => {
                        let display_p = if p.ends_with('/') {
                            p.clone()
                        } else {
                            format!("{p}/")
                        };
                        ("Search".to_string(), format!(" in {display_p}"))
                    }
                    (None, None) => ("Search".to_string(), cmd.clone()),
                }
            }
            ParsedCommand::ReadCommand { cmd } => ("Run".to_string(), cmd.clone()),
            // Upstream-only variants handled as generic runs in this fork
            ParsedCommand::Unknown { cmd } => {
                let t = cmd.trim();
                let lower = t.to_lowercase();
                if lower.starts_with("echo") && lower.contains("---") {
                    (String::new(), String::new())
                } else {
                    ("Run".to_string(), format_inline_script_for_display(cmd))
                }
            } // ParsedCommand::Noop { .. } => continue,
        };

        // Keep only entries that match the primary action grouping.
        if let Some(exp) = expected_label {
            if label != exp {
                continue;
            }
        } else if !(label == "Run" || label == "Search") {
            continue;
        }

        // Skip suppressed entries
        if label.is_empty() && content.is_empty() {
            continue;
        }

        // Split content into lines and push without repeating the action label
        for line_text in content.lines() {
            if line_text.is_empty() {
                continue;
            }
            let prefix = if !any_content_emitted {
                if suppress_run_header || !use_content_connectors {
                    ""
                } else {
                    "└ "
                }
            } else if suppress_run_header || !use_content_connectors {
                ""
            } else {
                "  "
            };
            let mut spans: Vec<Span<'static>> = Vec::new();
            if !prefix.is_empty() {
                spans.push(Span::styled(
                    prefix,
                    Style::default().add_modifier(Modifier::DIM),
                ));
            }

            match label.as_str() {
                // Highlight searched terms in normal text color; keep connectors/path dim
                "Search" => {
                    let remaining = line_text.to_string();
                    // Split off optional path suffix. Support both " (in ...)" and " in <dir>/" forms.
                    let (terms_part, path_part) = if let Some(idx) = remaining.rfind(" (in ") {
                        (
                            remaining[..idx].to_string(),
                            Some(remaining[idx..].to_string()),
                        )
                    } else if let Some(idx) = remaining.rfind(" in ") {
                        let suffix = &remaining[idx + 1..]; // keep leading space for styling
                        // Heuristic: treat as path if it ends with '/'
                        if suffix.trim_end().ends_with('/') {
                            (
                                remaining[..idx].to_string(),
                                Some(remaining[idx..].to_string()),
                            )
                        } else {
                            (remaining.clone(), None)
                        }
                    } else {
                        (remaining.clone(), None)
                    };
                    // Tokenize terms by ", " and " and " while preserving separators
                    let tmp = terms_part.clone();
                    // First, split by ", "
                    let chunks: Vec<String> = if tmp.contains(", ") {
                        tmp.split(", ").map(std::string::ToString::to_string).collect()
                    } else {
                        vec![tmp.clone()]
                    };
                    for (i, chunk) in chunks.iter().enumerate() {
                        if i > 0 {
                            // Add comma separator between items (dim)
                            spans.push(Span::styled(
                                ", ",
                                Style::default().fg(crate::colors::text_dim()),
                            ));
                        }
                        // Within each chunk, if it contains " and ", split into left and right with dimmed " and "
                        if let Some((left, right)) = chunk.rsplit_once(" and ") {
                            if !left.is_empty() {
                                spans.push(Span::styled(
                                    left.to_string(),
                                    Style::default().fg(crate::colors::text()),
                                ));
                                spans.push(Span::styled(
                                    " and ",
                                    Style::default().fg(crate::colors::text_dim()),
                                ));
                                spans.push(Span::styled(
                                    right.to_string(),
                                    Style::default().fg(crate::colors::text()),
                                ));
                            } else {
                                spans.push(Span::styled(
                                    chunk.clone(),
                                    Style::default().fg(crate::colors::text()),
                                ));
                            }
                        } else {
                            spans.push(Span::styled(
                                chunk.clone(),
                                Style::default().fg(crate::colors::text()),
                            ));
                        }
                    }
                    if let Some(p) = path_part {
                        // Dim the entire path portion including the " in " or " (in " prefix
                        spans.push(Span::styled(
                            p,
                            Style::default().fg(crate::colors::text_dim()),
                        ));
                    }
                }
                // Highlight filenames in Read; keep line ranges dim
                "Read" => {
                    if let Some(idx) = line_text.find(" (") {
                        let (fname, rest) = line_text.split_at(idx);
                        spans.push(Span::styled(
                            fname.to_string(),
                            Style::default().fg(crate::colors::text()),
                        ));
                        spans.push(Span::styled(
                            rest.to_string(),
                            Style::default().fg(crate::colors::text_dim()),
                        ));
                    } else {
                        spans.push(Span::styled(
                            line_text.to_string(),
                            Style::default().fg(crate::colors::text()),
                        ));
                    }
                }
                // List: highlight directory names
                "List" => {
                    spans.push(Span::styled(
                        line_text.to_string(),
                        Style::default().fg(crate::colors::text()),
                    ));
                }
                _ => {
                    // For executed commands (Run/Test/Lint/etc.), use shell syntax highlighting.
                    let normalized = normalize_shell_command_display(line_text);
                    let display_line = insert_line_breaks_after_double_ampersand(&normalized);
                    let mut hl =
                        crate::syntax_highlight::highlight_code_block(&display_line, Some("bash"));
                    if let Some(mut first_line) = hl.pop() {
                        emphasize_shell_command_name(&mut first_line);
                        spans.extend(first_line.spans.into_iter());
                    } else {
                        spans.push(Span::styled(
                            display_line,
                            Style::default().fg(crate::colors::text()),
                        ));
                    }
                }
            }

            lines.push(Line::from(spans));
            any_content_emitted = true;
        }
    }

    // If this is a List cell and the loop above produced no content (e.g.,
    // the list path was suppressed because a Search referenced the same path),
    // emit a single contextual line so the location is always visible.
    if matches!(action, ExecAction::List) && !any_content_emitted {
        let display_p = match ctx_path {
            Some(p) if !p.is_empty() => {
                if p.ends_with('/') {
                    p.to_string()
                } else {
                    format!("{p}/")
                }
            }
            _ => "./".to_string(),
        };
        lines.push(Line::from(vec![
            Span::styled("└ ", Style::default().add_modifier(Modifier::DIM)),
            Span::styled(
                display_p,
                Style::default().fg(crate::colors::text()),
            ),
        ]));
        // no-op: avoid unused assignment warning; the variable's value is not consumed later
    }

    // Show stdout for real run commands; keep read/search/list concise unless error
    let show_stdout = matches!(action, ExecAction::Run);
    let use_angle_pipe = show_stdout; // add "> " prefix for run output
    let display_output = output.or(stream_preview);
    let mut preview_lines = output_lines(display_output, !show_stdout, use_angle_pipe);
    if let Some(status_line) = running_status {
        if let Some(last) = preview_lines.last() {
            let is_blank = last
                .spans
                .iter()
                .all(|sp| sp.content.as_ref().trim().is_empty());
            if is_blank {
                preview_lines.pop();
            }
        }
        preview_lines.push(status_line);
    }
    lines.extend(preview_lines);
    lines.push(Line::from(""));
    lines
}

fn new_exec_command_generic(
    command: &[String],
    output: Option<&CommandOutput>,
    stream_preview: Option<&CommandOutput>,
    start_time: Option<Instant>,
) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    let command_escaped = strip_bash_lc_and_escape(command);
    let normalized = normalize_shell_command_display(&command_escaped);
    let command_display = insert_line_breaks_after_double_ampersand(&normalized);
    // Highlight the command as bash and then append a dimmed duration to the
    // first visual line while running.
    let mut highlighted_cmd =
        crate::syntax_highlight::highlight_code_block(&command_display, Some("bash"));

    for (idx, line) in highlighted_cmd.iter_mut().enumerate() {
        emphasize_shell_command_name(line);
        if idx > 0 {
            line.spans.insert(
                0,
                Span::styled("  ", Style::default().fg(crate::colors::text())),
            );
        }
    }

    let render_running_header = output.is_none();
    let display_output = output.or(stream_preview);
    let mut running_status = None;
    if render_running_header {
        let mut message = "Running...".to_string();
        if let Some(start) = start_time {
            let elapsed = start.elapsed();
            message = format!("{message} ({})", format_duration(elapsed));
        }
        running_status = Some(running_status_line(message));
    }

    if output.is_some() {
        for line in highlighted_cmd.iter_mut() {
            for span in line.spans.iter_mut() {
                span.style = span.style.fg(crate::colors::text_bright());
            }
        }
    }

    lines.extend(highlighted_cmd);

    let mut preview_lines = output_lines(display_output, false, true);
    if let Some(status_line) = running_status {
        if let Some(last) = preview_lines.last() {
            let is_blank = last
                .spans
                .iter()
                .all(|sp| sp.content.as_ref().trim().is_empty());
            if is_blank {
                preview_lines.pop();
            }
        }
        preview_lines.push(status_line);
    }

    lines.extend(preview_lines);
    lines
}


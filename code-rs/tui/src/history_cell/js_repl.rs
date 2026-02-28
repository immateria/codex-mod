use std::cell::Cell;
use std::time::Instant;

use ratatui::prelude::*;

use crate::history::state::ExecRecord;
use crate::history::state::ExecStatus;
use crate::history::state::HistoryId;

use super::CommandOutput;
use super::HistoryCell;
use super::HistoryCellType;
use super::exec::record_output;
use super::exec::render_exec_stream;
use super::formatting::output_lines;
use super::formatting::trim_empty_lines;
use code_common::elapsed::format_duration;

const STREAMING_EXIT_CODE: i32 = i32::MIN;
const OUTPUT_FOLD_THRESHOLD: usize = 40;

/// A history cell that represents a JavaScript REPL execution.
/// Unlike the generic `ExecCell`, this stores the JS source code and
/// runtime metadata, making it possible to render the code in history.
pub(crate) struct JsReplCell {
    pub(crate) record: ExecRecord,
    /// JS source code that was executed.
    pub(crate) code: String,
    /// Runtime kind string: "node" or "deno".
    pub(crate) runtime_kind: String,
    /// Resolved runtime version string, e.g. "v20.11.0".
    pub(crate) runtime_version: String,
    /// Completed output (set on exec end).
    pub(crate) output: Option<CommandOutput>,
    /// Streaming output preview while the kernel is executing.
    pub(crate) stream_preview: Option<CommandOutput>,
    /// Instant when execution started (for elapsed-time display).
    pub(crate) start_time: Option<Instant>,
    /// When true the code block shows only the first non-empty line + "…".
    pub(crate) code_collapsed: Cell<bool>,
    /// When true the output block is capped at OUTPUT_FOLD_THRESHOLD lines.
    pub(crate) collapsed_output: Cell<bool>,
}

impl JsReplCell {
    pub(crate) fn new_active(
        record: ExecRecord,
        code: String,
        runtime_kind: String,
        runtime_version: String,
    ) -> Self {
        Self {
            record,
            code,
            runtime_kind,
            runtime_version,
            output: None,
            stream_preview: None,
            start_time: Some(Instant::now()),
            code_collapsed: Cell::new(true),
            collapsed_output: Cell::new(false),
        }
    }

    pub(crate) fn toggle_output_collapsed(&self) {
        self.collapsed_output.set(!self.collapsed_output.get());
    }

    /// Update output data from an `ExecRecord` produced by the history domain.
    /// Called by the TUI when an `ExecCommandEnd` arrives for this cell's call_id.
    pub(crate) fn sync_from_exec_record(&mut self, record: &ExecRecord) {
        let was_running = matches!(self.record.status, ExecStatus::Running);
        self.record = record.clone();
        self.output = record_output(record);

        if matches!(record.status, ExecStatus::Running) {
            let stdout = render_exec_stream(&record.stdout_chunks, "stdout");
            let stderr = render_exec_stream(&record.stderr_chunks, "stderr");
            if stdout.is_empty() && stderr.is_empty() {
                self.stream_preview = None;
            } else {
                self.stream_preview = Some(CommandOutput {
                    exit_code: STREAMING_EXIT_CODE,
                    stdout,
                    stderr,
                });
            }
            if self.start_time.is_none() {
                self.start_time = Some(Instant::now());
            }
        } else {
            self.stream_preview = None;
            self.start_time = None;
        }
        if was_running && !matches!(record.status, ExecStatus::Running) {
            let line_count = self
                .output
                .as_ref()
                .map(|o| {
                    o.stdout
                        .lines()
                        .count()
                        .saturating_add(o.stderr.lines().count())
                })
                .unwrap_or(0);
            if line_count > OUTPUT_FOLD_THRESHOLD {
                self.collapsed_output.set(true);
            }
        }
    }

    pub(crate) fn set_history_id(&mut self, id: HistoryId) {
        self.record.id = id;
    }

    fn header_line(&self) -> Line<'static> {
        let runtime_str = if self.runtime_version.is_empty() {
            self.runtime_kind.clone()
        } else {
            format!("{} {}", self.runtime_kind, self.runtime_version)
        };
        Line::from(vec![
            Span::styled(
                "js",
                Style::default()
                    .fg(crate::colors::text_dim())
                    .add_modifier(Modifier::DIM),
            ),
            Span::styled(
                format!(" {runtime_str}"),
                Style::default()
                    .fg(crate::colors::text_dim())
                    .add_modifier(Modifier::DIM),
            ),
        ])
    }

    fn code_lines(&self) -> Vec<Line<'static>> {
        let border_span = Span::styled(
            "│ ",
            Style::default()
                .fg(crate::colors::border_dim())
                .bg(crate::colors::background()),
        );
        let code_style = Style::default()
            .fg(crate::colors::text_dim())
            .bg(crate::colors::background());

        if self.code_collapsed.get() {
            // Show first non-empty line with "…" suffix
            let first = self
                .code
                .lines()
                .find(|l| !l.trim().is_empty())
                .unwrap_or("")
                .to_string();
            let preview = if first.chars().count() > 60 {
                let mut s: String = first.chars().take(60).collect();
                s.push('…');
                s
            } else {
                format!("{first} …")
            };
            vec![Line::from(vec![
                border_span,
                Span::styled(preview, code_style),
            ])]
        } else {
            self.code
                .lines()
                .map(|l| {
                    Line::from(vec![
                        border_span.clone(),
                        Span::styled(l.to_string(), code_style),
                    ])
                })
                .collect()
        }
    }

    fn status_line(&self) -> Option<Line<'static>> {
        if self.output.is_some() {
            return None;
        }
        let elapsed = self.start_time.map(|s| s.elapsed());
        let mut msg = "Running…".to_string();
        if let Some(dur) = elapsed
            && !dur.is_zero()
        {
            msg = format!("Running… ({})", format_duration(dur));
        }
        Some(Line::from(Span::styled(
            msg,
            Style::default()
                .fg(crate::colors::text_dim())
                .add_modifier(Modifier::DIM),
        )))
    }

}

impl HistoryCell for JsReplCell {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn kind(&self) -> HistoryCellType {
        HistoryCellType::JsRepl {
            status: self.record.status,
        }
    }

    fn gutter_symbol(&self) -> Option<&'static str> {
        match self.record.status {
            ExecStatus::Error => Some("✗"),
            _ => Some("❯"),
        }
    }

    fn is_animating(&self) -> bool {
        matches!(self.record.status, ExecStatus::Running) && self.start_time.is_some()
    }

    fn display_lines(&self) -> Vec<Line<'static>> {
        let mut lines = Vec::new();

        lines.push(self.header_line());

        // Code block
        lines.extend(self.code_lines());

        // Output block
        let display_output = self.output.as_ref().or(self.stream_preview.as_ref());
        let out = output_lines(display_output, false, false);
        let trimmed_out = trim_empty_lines(out.clone());
        let has_output = !trimmed_out.is_empty();
        if has_output {
            if self.output.is_some()
                && self.collapsed_output.get()
                && trimmed_out.len() > OUTPUT_FOLD_THRESHOLD
            {
                let folded_count = trimmed_out.len() - OUTPUT_FOLD_THRESHOLD;
                let mut capped: Vec<Line<'static>> =
                    trimmed_out.into_iter().take(OUTPUT_FOLD_THRESHOLD).collect();
                capped.push(Line::from(Span::styled(
                    format!("… {folded_count} more lines  [ to expand"),
                    Style::default()
                        .fg(crate::colors::text_dim())
                        .add_modifier(Modifier::DIM),
                )));
                lines.extend(capped);
            } else {
                lines.extend(out);
            }
        }

        // Status line when running
        if let Some(status) = self.status_line() {
            lines.push(status);
        }

        lines
    }
}

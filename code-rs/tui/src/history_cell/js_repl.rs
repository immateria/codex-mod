use std::cell::Cell;
use std::collections::HashSet;
use std::time::Instant;

use ratatui::prelude::*;
use unicode_width::UnicodeWidthStr as _;

use crate::history::state::ExecRecord;
use crate::history::state::ExecStatus;
use crate::history::state::HistoryId;
use crate::insert_history::word_wrap_lines;
use crate::util::buffer::{fill_rect, write_line};

use super::CommandOutput;
use super::HistoryCell;
use super::HistoryCellType;
use super::formatting::{
    OUTPUT_FOLD_THRESHOLD,
    describe_exit_code,
    output_lines,
    trim_empty_lines,
};
use code_common::elapsed::format_duration;

#[derive(Default)]
struct JsReplRenderLayout {
    lines: Vec<Line<'static>>,
    total: u16,
}


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
    child_call_ids: HashSet<String>,
    last_child_call_id: Option<String>,
    layout_cache: super::layout_cache::LayoutCache<JsReplRenderLayout>,
}

impl JsReplCell {
    pub(crate) fn new_active(
        record: ExecRecord,
        code: String,
        runtime_kind: String,
        runtime_version: String,
    ) -> Self {
        let non_empty_lines = code.lines().filter(|l| !l.trim().is_empty()).count();
        // Default to showing the full code for small snippets. Large scripts are
        // collapsed to keep the history readable.
        let collapse_code_by_default = non_empty_lines > 6;
        Self {
            record,
            code,
            runtime_kind,
            runtime_version,
            output: None,
            stream_preview: None,
            start_time: Some(Instant::now()),
            code_collapsed: Cell::new(collapse_code_by_default),
            collapsed_output: Cell::new(false),
            child_call_ids: HashSet::new(),
            last_child_call_id: None,
            layout_cache: super::layout_cache::LayoutCache::new(),
        }
    }

    pub(crate) fn record_child_call_id(&mut self, call_id: &str) -> bool {
        let call_id = call_id.to_string();
        let inserted = self.child_call_ids.insert(call_id.clone());
        self.last_child_call_id = Some(call_id);
        inserted
    }

    pub(crate) fn latest_child_call_id(&self) -> Option<&str> {
        self.last_child_call_id.as_deref()
    }

    pub(crate) fn toggle_code_collapsed(&self) {
        self.code_collapsed.set(!self.code_collapsed.get());
        self.layout_cache.invalidate();
    }

    pub(crate) fn toggle_output_collapsed(&self) {
        self.collapsed_output.set(!self.collapsed_output.get());
        self.layout_cache.invalidate();
    }

    /// Update output data from an `ExecRecord` produced by the history domain.
    /// Called by the TUI when an `ExecCommandEnd` arrives for this cell's call_id.
    pub(crate) fn sync_from_exec_record(&mut self, record: &ExecRecord) {
        let was_running = matches!(self.record.status, ExecStatus::Running);
        self.record = record.clone();
        super::formatting::sync_exec_output_state(
            record,
            was_running,
            &mut self.output,
            &mut self.stream_preview,
            &mut self.start_time,
            &self.collapsed_output,
            &self.layout_cache,
        );
    }

    pub(crate) fn set_history_id(&mut self, id: HistoryId) {
        self.record.id = id;
    }

    pub(crate) fn spawned_click_target(&self, width: u16) -> Option<(String, usize, u16, u16)> {
        if self.child_call_ids.is_empty() {
            return None;
        }
        let call_id = self.last_child_call_id.clone()?;
        let layout = self.layout_for_width(width);
        for (idx, line) in layout.lines.iter().enumerate() {
            let text: String = line
                .spans
                .iter()
                .map(|sp| sp.content.as_ref())
                .collect();
            // Header lines come first; stop once we reach the bordered code block.
            if text.starts_with("│ ") {
                break;
            }
            let Some(start) = text.find("spawned ") else {
                continue;
            };
            let rest = &text[start..];
            let end_rel = rest.find(" • ").unwrap_or(rest.len());
            let segment = &rest[..end_rel];

            let start_col = text[..start].width().min(u16::MAX as usize) as u16;
            let seg_width = segment.width().min(u16::MAX as usize) as u16;
            if seg_width == 0 {
                continue;
            }
            return Some((call_id, idx, start_col, seg_width));
        }
        None
    }

    fn layout_for_width(&self, width: u16) -> std::cell::Ref<'_, JsReplRenderLayout> {
        self.layout_cache.get_or_compute(width, |w| self.compute_layout_for_width(w))
    }

    fn compute_layout_for_width(&self, width: u16) -> JsReplRenderLayout {
        let raw_lines = self.build_display_lines();
        let trimmed = trim_empty_lines(raw_lines);
        if width == 0 {
            return JsReplRenderLayout::default();
        }
        let wrapped = word_wrap_lines(&trimmed, width);
        let total = wrapped.len().min(u16::MAX as usize) as u16;
        JsReplRenderLayout {
            lines: wrapped,
            total,
        }
    }

    fn build_display_lines(&self) -> Vec<Line<'static>> {
        let mut lines = Vec::new();

        lines.push(self.header_line());

        // Code block
        lines.extend(self.code_lines());

        // Output block
        let display_output = self.output.as_ref().or(self.stream_preview.as_ref());
        let mut out = trim_empty_lines(output_lines(display_output, false, false));
        if !out.is_empty() {
            if self.output.is_some() {
                super::formatting::maybe_fold_output(&mut out, self.collapsed_output.get());
            }
            lines.extend(out);
        }

        // Status line when running
        if let Some(status) = self.status_line() {
            lines.push(status);
        }

        lines
    }

    fn header_line(&self) -> Line<'static> {
        let runtime_str = if self.runtime_version.is_empty() {
            self.runtime_kind.clone()
        } else {
            format!("{} {}", self.runtime_kind, self.runtime_version)
        };
        let dim_style = Style::default()
            .fg(crate::colors::text_dim())
            .add_modifier(Modifier::DIM);
        let mut spans = vec![
            Span::styled("js", dim_style),
            Span::styled(format!(" {runtime_str}"), dim_style),
        ];

        if !matches!(self.record.status, ExecStatus::Running)
            && let Some(completed_at) = self.record.completed_at
            && let Ok(duration) = completed_at.duration_since(self.record.started_at)
            && !duration.is_zero()
        {
            spans.push(Span::styled(
                format!(" • {}", format_duration(duration)),
                dim_style,
            ));
        }

        // Exit code indicator for failed executions
        if let Some(exit_code) = self.record.exit_code
            && exit_code != 0
            && !matches!(self.record.status, ExecStatus::Running)
        {
            let description = describe_exit_code(exit_code);
            let msg = if description.is_empty() {
                format!(" • exit {exit_code}")
            } else {
                format!(" • exit {exit_code}: {description}")
            };
            spans.push(Span::styled(
                msg,
                Style::default().fg(crate::colors::error()),
            ));
        }

        let child_count = self.child_call_ids.len();
        if child_count > 0 {
            spans.push(Span::styled(
                format!(" • spawned {child_count} (}})"),
                dim_style,
            ));
        }

        let has_hidden_code = self.code_collapsed.get()
            && self
                .code
                .lines()
                .filter(|line| !line.trim().is_empty())
                .nth(1)
                .is_some();
        if has_hidden_code {
            spans.push(Span::styled(" • code (\\)", dim_style));
        }

        let has_hidden_output = self.output.is_some()
            && self.collapsed_output.get()
            && self
                .output
                .as_ref()
                .is_some_and(|o| {
                    o.stdout
                        .lines()
                        .count()
                        .saturating_add(o.stderr.lines().count())
                        > OUTPUT_FOLD_THRESHOLD
                });
        if has_hidden_output {
            spans.push(Span::styled(" • output ([)", dim_style));
        }

        Line::from(spans)
    }

    fn code_lines(&self) -> Vec<Line<'static>> {
        let border_span = super::formatting::left_border_span();
        let code_style = Style::default()
            .fg(crate::colors::text_dim())
            .bg(crate::colors::background());

        if self.code_collapsed.get() {
            // Show first non-empty line with "…" suffix
            let mut first_non_empty = None;
            let mut non_empty_count = 0usize;
            for line in self.code.lines() {
                if line.trim().is_empty() {
                    continue;
                }
                non_empty_count += 1;
                if first_non_empty.is_none() {
                    first_non_empty = Some(line);
                }
            }
            let first = first_non_empty.unwrap_or("");
            let hidden_lines = non_empty_count.saturating_sub(1);

            let mut preview = if first.is_empty() {
                "…".to_string()
            } else if first.chars().count() > 60 {
                let mut s: String = first.chars().take(60).collect();
                s.push('…');
                s
            } else {
                first.to_string()
            };

            if hidden_lines > 0 {
                let suffix = if preview.ends_with('…') {
                    format!(" (+{hidden_lines} lines)")
                } else {
                    format!(" … (+{hidden_lines} lines)")
                };
                preview.push_str(&suffix);
            }

            let mut lines = Vec::new();
            lines.push(Line::from(vec![
                border_span.clone(),
                Span::styled(preview, code_style),
            ]));
            if hidden_lines > 0 {
                lines.push(Line::from(vec![
                    border_span,
                    Span::styled(
                        format!("… {hidden_lines} more lines (press \\ to expand)"),
                        code_style.add_modifier(Modifier::DIM),
                    ),
                ]));
            }
            lines
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
    impl_as_any!();

    fn kind(&self) -> HistoryCellType {
        HistoryCellType::JsRepl {
            status: self.record.status,
        }
    }

    fn call_id(&self) -> Option<&str> {
        self.record.call_id.as_deref()
    }

    fn is_animating(&self) -> bool {
        matches!(self.record.status, ExecStatus::Running) && self.start_time.is_some()
    }

    fn has_custom_render(&self) -> bool {
        true
    }

    fn desired_height(&self, width: u16) -> u16 {
        self.layout_for_width(width).total
    }

    fn display_lines(&self) -> Vec<Line<'static>> {
        self.build_display_lines()
    }

    fn custom_render_with_skip(&self, area: Rect, buf: &mut Buffer, skip_rows: u16) {
        let layout = self.layout_for_width(area.width);
        let total = layout.total;

        let visible_start = skip_rows.min(total) as usize;
        let visible_count = (total.saturating_sub(skip_rows)).min(area.height) as usize;

        if visible_count == 0 {
            return;
        }

        let bg_style = Style::default()
            .bg(crate::colors::background())
            .fg(crate::colors::text());
        fill_rect(buf, Rect {
            x: area.x,
            y: area.y,
            width: area.width,
            height: visible_count as u16,
        }, Some(' '), bg_style);

        for (idx, line) in layout
            .lines
            .iter()
            .skip(visible_start)
            .take(visible_count)
            .enumerate()
        {
            let y = area.y.saturating_add(idx as u16);
            if y >= area.y.saturating_add(area.height) {
                break;
            }
            write_line(buf, area.x, y, area.width, line, bg_style);
        }
    }
}

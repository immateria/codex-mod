use crate::history::compat::{ExecAction, ExecStatus, ToolStatus as HistoryToolStatus};
use crate::util::buffer::fill_bg;
use ratatui::prelude::*;
use ratatui::style::Style;
use ratatui::widgets::{Block, Paragraph, Widget, Wrap};

use super::formatting::trim_empty_lines;

#[derive(Clone)]
pub(crate) struct CommandOutput {
    pub(crate) exit_code: i32,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
}

impl CommandOutput {
    /// Build a placeholder output used while an exec is still streaming.
    pub(crate) fn streaming_preview(stdout: String, stderr: String) -> Self {
        Self {
            exit_code: super::formatting::STREAMING_EXIT_CODE,
            stdout,
            stderr,
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) enum PatchEventType {
    ApprovalRequest,
    ApplyBegin { auto_approved: bool },
    ApplySuccess,
    ApplyFailure,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HistoryCellType {
    Plain,
    User,
    Assistant,
    ProposedPlan,
    Reasoning,
    Error,
    Exec { kind: ExecKind, status: ExecStatus },
    Tool { status: ToolCellStatus },
    Patch { kind: PatchKind },
    PlanUpdate,
    BackgroundEvent,
    Notice,
    CompactionSummary,
    Diff,
    Image,
    Context,
    AnimatedWelcome,
    Loading,
    Repl { status: ExecStatus },
}

pub(crate) fn gutter_symbol_for_kind(kind: HistoryCellType) -> Option<&'static str> {
    use crate::icons;
    match kind {
        HistoryCellType::Plain
        | HistoryCellType::Reasoning
        | HistoryCellType::PlanUpdate
        | HistoryCellType::Image
        | HistoryCellType::AnimatedWelcome
        | HistoryCellType::Loading => None,
        HistoryCellType::User => Some(icons::gutter_user()),
        HistoryCellType::Assistant => Some(icons::gutter_assistant()),
        HistoryCellType::ProposedPlan => Some(icons::gutter_plan()),
        HistoryCellType::Error => Some(icons::gutter_error()),
        HistoryCellType::Tool { status } => Some(match status {
            ToolCellStatus::Running => icons::gutter_running(),
            ToolCellStatus::Success => icons::gutter_success(),
            ToolCellStatus::Failed => icons::gutter_failure(),
        }),
        HistoryCellType::Exec { kind, status } => {
            match (kind, status) {
                (ExecKind::Run, ExecStatus::Error) => Some(icons::gutter_error()),
                (ExecKind::Run, _) => Some(icons::gutter_exec()),
                _ => None,
            }
        }
        HistoryCellType::Patch { .. } | HistoryCellType::Diff => Some(icons::gutter_patch()),
        HistoryCellType::BackgroundEvent => Some(icons::gutter_background()),
        HistoryCellType::Notice => Some(icons::gutter_notice()),
        HistoryCellType::CompactionSummary => Some(icons::gutter_compaction()),
        HistoryCellType::Context => Some(icons::gutter_context()),
        HistoryCellType::Repl { status } => match status {
            ExecStatus::Error => Some(icons::gutter_error()),
            _ => Some(icons::gutter_exec()),
        },
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ExecKind {
    Read,
    Search,
    List,
    Run,
}

impl From<ExecAction> for ExecKind {
    fn from(action: ExecAction) -> Self {
        match action {
            ExecAction::Read => ExecKind::Read,
            ExecAction::Search => ExecKind::Search,
            ExecAction::List => ExecKind::List,
            ExecAction::Run => ExecKind::Run,
        }
    }
}

impl From<ExecKind> for ExecAction {
    fn from(kind: ExecKind) -> Self {
        match kind {
            ExecKind::Read => ExecAction::Read,
            ExecKind::Search => ExecAction::Search,
            ExecKind::List => ExecAction::List,
            ExecKind::Run => ExecAction::Run,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ToolCellStatus {
    Running,
    Success,
    Failed,
}

impl From<HistoryToolStatus> for ToolCellStatus {
    fn from(status: HistoryToolStatus) -> Self {
        match status {
            HistoryToolStatus::Running => ToolCellStatus::Running,
            HistoryToolStatus::Success => ToolCellStatus::Success,
            HistoryToolStatus::Failed => ToolCellStatus::Failed,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PatchKind {
    Proposed,
    ApplyBegin,
    ApplySuccess,
    ApplyFailure,
}

/// Context passed to `collapsed_display_lines()` with metadata
/// that the cell itself doesn't track (e.g. its ordinal position).
pub(crate) struct CollapsedContext {
    /// 1-indexed position among cells of the same kind (e.g. assistant reply #3).
    pub reply_number: usize,
}

/// Represents an event to display in the conversation history.
/// Returns its `Vec<Line<'static>>` representation to make it easier
/// to display in a scrollable list.
pub(crate) trait HistoryCell {
    fn display_lines(&self) -> Vec<Line<'static>>;
    /// A required, explicit type descriptor for the history cell.
    fn kind(&self) -> HistoryCellType;

    /// Allow downcasting to concrete types
    fn as_any(&self) -> &dyn std::any::Any {
        // Default implementation that doesn't support downcasting
        // Concrete types that need downcasting should override this
        &() as &dyn std::any::Any
    }
    /// Allow mutable downcasting to concrete types
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;

    /// When present, a stable identifier for this history cell. Used for
    /// navigation between nested tool calls (e.g. JS REPL spawning tools).
    fn call_id(&self) -> Option<&str> {
        None
    }

    /// When present, indicates this cell was spawned by another tool. The value
    /// is the parent tool's `call_id`.
    fn parent_call_id(&self) -> Option<&str> {
        None
    }

    /// Get display lines with empty lines trimmed from beginning and end.
    /// This ensures consistent spacing when cells are rendered together.
    fn display_lines_trimmed(&self) -> Vec<Line<'static>> {
        trim_empty_lines(self.display_lines())
    }

    fn desired_height(&self, width: u16) -> u16 {
        Paragraph::new(Text::from(self.display_lines_trimmed()))
            .wrap(Wrap { trim: false })
            .line_count(width)
            .try_into()
            .unwrap_or(0)
    }

    fn render_with_skip(&self, area: Rect, buf: &mut Buffer, skip_rows: u16) {
        // Check if this cell has custom rendering
        if self.has_custom_render() {
            // Allow custom renders to handle top skipping explicitly
            self.custom_render_with_skip(area, buf, skip_rows);
            return;
        }

        // Default path: render the full text and use Paragraph.scroll to skip
        // vertical rows AFTER wrapping. Slicing lines before wrapping causes
        // incorrect blank space when lines wrap across multiple rows.
        // IMPORTANT: Explicitly clear the entire area first. While some containers
        // clear broader regions, custom widgets that shrink or scroll can otherwise
        // leave residual glyphs to the right of shorter lines or from prior frames.
        // We paint spaces with the current theme background to guarantee a clean slate.
        // Assistant messages use a subtly tinted background: theme background
        // moved 5% toward the theme info color for a gentle distinction.
        let assistant_like = matches!(self.kind(), HistoryCellType::Assistant | HistoryCellType::ProposedPlan);
        let cell_bg = if assistant_like {
            crate::colors::assistant_bg()
        } else {
            crate::colors::background()
        };
        let bg_style = Style::default().bg(cell_bg).fg(crate::colors::text());
        if assistant_like {
            fill_bg(buf, area, bg_style);
        }

        // Ensure the entire allocated area is painted with the theme background
        // by attaching a background-styled Block to the Paragraph as well.
        let lines = self.display_lines_trimmed();
        let text = Text::from(lines);

        let bg_block = Block::default().style(Style::default().bg(cell_bg));
        Paragraph::new(text)
            .block(bg_block)
            .wrap(Wrap { trim: false })
            .scroll((skip_rows, 0))
            .style(Style::default().bg(cell_bg))
            .render(area, buf);
    }

    /// Returns true if this cell has custom rendering (e.g., animations)
    fn has_custom_render(&self) -> bool {
        false // Default: most cells use display_lines
    }

    /// Custom render implementation for cells that need it
    fn custom_render(&self, _area: Rect, _buf: &mut Buffer) {
        // Default: do nothing (cells with custom rendering will override)
    }
    /// Custom render with support for skipping top rows
    fn custom_render_with_skip(&self, area: Rect, buf: &mut Buffer, _skip_rows: u16) {
        // Default: fall back to non-skipping custom render
        self.custom_render(area, buf);
    }

    /// Returns true if this cell is currently animating and needs redraws
    fn is_animating(&self) -> bool {
        false // Default: most cells don't animate
    }

    /// Trigger fade-out animation (for `AnimatedWelcomeCell`)
    fn trigger_fade(&self) {
        // Default: do nothing (only AnimatedWelcomeCell implements this)
    }

    /// Check if this cell should be removed (e.g., fully faded out)
    fn should_remove(&self) -> bool {
        false // Default: most cells should not be removed
    }

    /// Returns the gutter symbol for this cell type
    /// Returns None if no symbol should be displayed
    fn gutter_symbol(&self) -> Option<&'static str> {
        gutter_symbol_for_kind(self.kind())
    }

    /// Returns true if this cell supports fold/collapse toggling via click.
    fn is_fold_toggleable(&self) -> bool {
        false
    }

    /// Returns true if this cell is currently in collapsed/folded state.
    fn is_collapsed(&self) -> bool {
        false
    }

    /// Returns the display lines to show when this cell is collapsed.
    /// `ctx` provides metadata that the cell doesn't track internally
    /// (e.g. its position among siblings of the same kind).
    fn collapsed_display_lines(&self, _ctx: &CollapsedContext) -> Vec<Line<'static>> {
        vec![]
    }

    /// Height of this cell when collapsed. Override if collapsed state
    /// uses more than 1 line.
    fn collapsed_height(&self) -> u16 {
        1
    }

    /// Returns the content of this cell as markdown text for clipboard copy.
    /// Default extracts plain text from `display_lines()`; cells with richer
    /// content (e.g. `AssistantMarkdownCell`) override to return the raw markdown.
    fn copyable_markdown(&self) -> Option<String> {
        let lines = self.display_lines();
        if lines.is_empty() {
            return None;
        }
        let mut text = String::new();
        for (i, line) in lines.iter().enumerate() {
            if i > 0 {
                text.push('\n');
            }
            for span in &line.spans {
                text.push_str(span.content.as_ref());
            }
        }
        Some(text)
    }

    /// Cheap check for whether this cell has content worth copying.
    /// Override to avoid the allocation that `copyable_markdown().is_some()` incurs.
    fn has_copyable_content(&self) -> bool {
        true
    }
}

// Allow Box<dyn HistoryCell> to implement HistoryCell
impl HistoryCell for Box<dyn HistoryCell> {
    fn as_any(&self) -> &dyn std::any::Any {
        self.as_ref().as_any()
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self.as_mut().as_any_mut()
    }
    fn kind(&self) -> HistoryCellType {
        self.as_ref().kind()
    }

    fn call_id(&self) -> Option<&str> {
        self.as_ref().call_id()
    }

    fn parent_call_id(&self) -> Option<&str> {
        self.as_ref().parent_call_id()
    }

    fn display_lines(&self) -> Vec<Line<'static>> {
        self.as_ref().display_lines()
    }

    fn display_lines_trimmed(&self) -> Vec<Line<'static>> {
        self.as_ref().display_lines_trimmed()
    }

    fn desired_height(&self, width: u16) -> u16 {
        self.as_ref().desired_height(width)
    }

    fn render_with_skip(&self, area: Rect, buf: &mut Buffer, skip_rows: u16) {
        self.as_ref().render_with_skip(area, buf, skip_rows);
    }

    fn has_custom_render(&self) -> bool {
        self.as_ref().has_custom_render()
    }

    fn custom_render(&self, area: Rect, buf: &mut Buffer) {
        self.as_ref().custom_render(area, buf);
    }

    fn is_animating(&self) -> bool {
        self.as_ref().is_animating()
    }

    fn trigger_fade(&self) {
        self.as_ref().trigger_fade();
    }

    fn should_remove(&self) -> bool {
        self.as_ref().should_remove()
    }

    fn gutter_symbol(&self) -> Option<&'static str> {
        self.as_ref().gutter_symbol()
    }

    fn is_fold_toggleable(&self) -> bool {
        self.as_ref().is_fold_toggleable()
    }

    fn is_collapsed(&self) -> bool {
        self.as_ref().is_collapsed()
    }

    fn collapsed_display_lines(&self, ctx: &CollapsedContext) -> Vec<Line<'static>> {
        self.as_ref().collapsed_display_lines(ctx)
    }

    fn collapsed_height(&self) -> u16 {
        self.as_ref().collapsed_height()
    }

    fn copyable_markdown(&self) -> Option<String> {
        self.as_ref().copyable_markdown()
    }

    fn has_copyable_content(&self) -> bool {
        self.as_ref().has_copyable_content()
    }
}

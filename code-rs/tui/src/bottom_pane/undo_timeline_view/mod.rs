use crate::app_event_sender::AppEventSender;
use ratatui::text::Line;

mod input;
mod model;
mod pane_impl;
mod render;

const MAX_VISIBLE_LIST_ROWS: usize = 12;

#[derive(Clone, Debug)]
pub(crate) enum UndoTimelineEntryKind {
    Snapshot { commit: String },
    Current,
}

#[derive(Clone, Debug)]
pub(crate) struct UndoTimelineEntry {
    pub label: String,
    pub summary: Option<String>,
    pub timestamp_line: Option<String>,
    pub relative_time: Option<String>,
    pub stats_line: Option<String>,
    pub commit_line: Option<String>,
    pub conversation_lines: Vec<Line<'static>>,
    pub file_lines: Vec<Line<'static>>,
    pub conversation_available: bool,
    pub files_available: bool,
    pub kind: UndoTimelineEntryKind,
}

pub(crate) struct UndoTimelineView {
    entries: Vec<UndoTimelineEntry>,
    selected: usize,
    top_row: usize,
    restore_files: bool,
    restore_conversation: bool,
    restore_files_forced_off: bool,
    restore_conversation_forced_off: bool,
    app_event_tx: AppEventSender,
    is_complete: bool,
}

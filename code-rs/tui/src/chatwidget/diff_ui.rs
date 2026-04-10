//! Diff overlay types used by the chat widget.
//!
//! Separated to keep `chatwidget.rs` smaller and focused on behavior.

use ratatui::text::Line;

pub(crate) struct DiffOverlay {
    pub(crate) tabs: Vec<(String, Vec<DiffBlock>)>,
    pub(crate) selected: usize,
    pub(crate) scroll_offsets: Vec<u16>,
}

impl DiffOverlay {
    pub(crate) fn new(tabs: Vec<(String, Vec<DiffBlock>)>) -> Self {
        let n = tabs.len();
        Self { tabs, selected: 0, scroll_offsets: vec![0; n] }
    }
}

#[derive(Clone)]
pub(crate) struct DiffBlock {
    pub(crate) lines: Vec<Line<'static>>,
}

pub(crate) struct DiffConfirm {
    pub(crate) text_to_submit: String,
}


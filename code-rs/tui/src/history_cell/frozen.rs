use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Line;

use crate::history::state::HistoryId;
use crate::history_cell::{HistoryCell, HistoryCellType};
use crate::util::buffer::fill_rect;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FrozenHistoryCell {
    history_id: HistoryId,
    kind: HistoryCellType,
    cached_width: u16,
    cached_height: u16,
    call_id: Option<String>,
    parent_call_id: Option<String>,
}

impl FrozenHistoryCell {
    pub(crate) fn new(
        history_id: HistoryId,
        kind: HistoryCellType,
        cached_width: u16,
        cached_height: u16,
        call_id: Option<String>,
        parent_call_id: Option<String>,
    ) -> Self {
        Self {
            history_id,
            kind,
            cached_width,
            cached_height,
            call_id,
            parent_call_id,
        }
    }

    pub(crate) fn history_id(&self) -> HistoryId {
        self.history_id
    }

    pub(crate) fn cached_width(&self) -> u16 {
        self.cached_width
    }

    pub(crate) fn cached_height(&self) -> u16 {
        self.cached_height
    }

    pub(crate) fn update_cached_height(&mut self, width: u16, height: u16) {
        self.cached_width = width;
        self.cached_height = height;
    }
}

impl HistoryCell for FrozenHistoryCell {
    impl_as_any!();

    fn display_lines(&self) -> Vec<Line<'static>> {
        Vec::new()
    }

    fn kind(&self) -> HistoryCellType {
        self.kind
    }

    fn call_id(&self) -> Option<&str> {
        self.call_id.as_deref()
    }

    fn parent_call_id(&self) -> Option<&str> {
        self.parent_call_id.as_deref()
    }

    fn desired_height(&self, _width: u16) -> u16 {
        self.cached_height
    }

    fn has_custom_render(&self) -> bool {
        true
    }

    fn custom_render_with_skip(&self, area: Rect, buf: &mut Buffer, _skip_rows: u16) {
        let bg_style = Style::default().bg(crate::colors::background());
        fill_rect(buf, area, Some(' '), bg_style);
    }
}

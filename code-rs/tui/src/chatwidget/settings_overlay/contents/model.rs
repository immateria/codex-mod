use crossterm::event::{KeyEvent, MouseEvent};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::bottom_pane::{BottomPaneView, ConditionalUpdate, ModelSelectionView};

use super::super::SettingsContent;

pub(crate) struct ModelSettingsContent {
    view: ModelSelectionView,
}

impl ModelSettingsContent {
    pub(crate) fn new(view: ModelSelectionView) -> Self {
        Self { view }
    }
}

impl SettingsContent for ModelSettingsContent {
    fn render(&self, area: Rect, buf: &mut Buffer) {
        self.view.render_without_frame(area, buf);
    }

    fn handle_key(&mut self, key: KeyEvent) -> bool {
        self.view.handle_key_event_direct(key)
    }

    fn is_complete(&self) -> bool {
        self.view.is_complete()
    }

    fn handle_mouse(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        matches!(
            self.view.handle_mouse_event_direct(mouse_event, area),
            ConditionalUpdate::NeedsRedraw
        )
    }
}

use std::cell::Cell;

use ratatui::layout::Rect;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ChromeMode {
    Framed,
    ContentOnly,
}

#[derive(Debug)]
pub(crate) struct LastRenderContext {
    area: Cell<Option<Rect>>,
    chrome: Cell<ChromeMode>,
}

impl LastRenderContext {
    pub(crate) fn new(default_chrome: ChromeMode) -> Self {
        Self {
            area: Cell::new(None),
            chrome: Cell::new(default_chrome),
        }
    }

    pub(crate) fn set(&self, area: Rect, chrome: ChromeMode) {
        self.area.set(Some(area));
        self.chrome.set(chrome);
    }

    pub(crate) fn get(&self) -> Option<(Rect, ChromeMode)> {
        let area = self.area.get()?;
        Some((area, self.chrome.get()))
    }
}

use super::*;

impl<'a> BottomPaneView<'a> for ThemeSelectionView {
    fn handle_paste(
        &mut self,
        text: String,
    ) -> crate::bottom_pane::bottom_pane_view::ConditionalUpdate {
        use crate::bottom_pane::bottom_pane_view::ConditionalUpdate;

        if let Mode::CreateSpinner(ref mut s) = self.mode {
            if s.is_loading.get() {
                return ConditionalUpdate::NoRedraw;
            }
            if matches!(s.step.get(), CreateStep::Prompt) {
                let paste = text.replace('\r', "\n");
                // The description is a single-line prompt; replace newlines with spaces.
                let paste = paste.replace('\n', " ");
                s.prompt.push_str(&paste);
                return ConditionalUpdate::NeedsRedraw;
            }
        } else if let Mode::CreateTheme(ref mut s) = self.mode {
            if s.is_loading.get() {
                return ConditionalUpdate::NoRedraw;
            }
            if matches!(s.step.get(), CreateStep::Prompt) {
                let paste = text.replace('\r', "\n");
                let paste = paste.replace('\n', " ");
                s.prompt.push_str(&paste);
                return ConditionalUpdate::NeedsRedraw;
            }
        }

        ConditionalUpdate::NoRedraw
    }

    fn desired_height(&self, _width: u16) -> u16 {
        match &self.mode {
            // Border (2) + inner padding (2) + 4 content rows = 8
            Mode::Overview => 8,
            // Detail lists: fixed 9 visible rows (max), shrink if fewer
            Mode::Themes => {
                let extra = if Self::allow_custom_theme_generation() {
                    1
                } else {
                    0
                };
                let n = (Self::theme_option_count() as u16) + extra;
                // Border(2) + padding(2) + title(1)+space(1) + list
                6 + n.min(9)
            }
            Mode::Spinner => {
                // +1 for the "Generate your own…" pseudo-row
                let n = (crate::spinner::spinner_names().len() as u16) + 1;
                // Border(2) + padding(2) + title(1)+space(1) + list
                6 + n.min(9)
            }
            // Title + spacer + 2 fields + buttons + help = 6 content rows
            // plus border(2) + padding(2) = 10; add 2 rows headroom for small terminals
            Mode::CreateSpinner(_) => 12,
            Mode::CreateTheme(_) => 12,
        }
    }

    fn handle_key_event(&mut self, _pane: &mut BottomPane<'a>, key_event: KeyEvent) {
        self.process_key_event(key_event);
    }

    fn is_complete(&self) -> bool {
        self.is_complete
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        self.render_content(area, buf);
    }
}

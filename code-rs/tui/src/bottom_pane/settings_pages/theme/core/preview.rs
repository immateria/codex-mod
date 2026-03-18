use super::super::*;

impl ThemeSelectionView {
    pub(in crate::bottom_pane::settings_pages::theme) fn clear_hovered_theme_preview(&mut self) -> bool {
        if self.hovered_theme_index.take().is_some() {
            self.send_theme_split_preview();
            true
        } else {
            false
        }
    }

    pub(in crate::bottom_pane::settings_pages::theme) fn active_preview_theme(&self) -> ThemeName {
        self.hovered_theme_index
            .and_then(Self::theme_name_for_option_index)
            .or_else(|| Self::theme_name_for_option_index(self.selected_theme_index))
            .unwrap_or(self.current_theme)
    }

    pub(in crate::bottom_pane::settings_pages::theme) fn send_theme_split_preview(&self) {
        if !matches!(self.mode, Mode::Themes) {
            return;
        }
        let preview = self.active_preview_theme();
        self.app_event_tx.send(AppEvent::SetThemeSplitPreview {
            current: self.revert_theme_on_back,
            preview,
        });
    }

    pub(in crate::bottom_pane::settings_pages::theme) fn clear_theme_split_preview(&self) {
        self.app_event_tx.send(AppEvent::ClearThemeSplitPreview);
    }
}

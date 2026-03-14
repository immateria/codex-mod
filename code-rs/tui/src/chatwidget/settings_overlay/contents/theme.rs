use crate::bottom_pane::settings_pages::theme::ThemeSelectionView;

pub(crate) struct ThemeSettingsContent {
    view: ThemeSelectionView,
}

impl ThemeSettingsContent {
    pub(crate) fn new(view: ThemeSelectionView) -> Self {
        Self { view }
    }
}

impl_settings_content!(ThemeSettingsContent);

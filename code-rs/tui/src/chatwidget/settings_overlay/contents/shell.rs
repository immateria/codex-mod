use crate::bottom_pane::settings_pages::shell::ShellSelectionView;

pub(crate) struct ShellSettingsContent {
    view: ShellSelectionView,
}

impl ShellSettingsContent {
    pub(crate) fn new(view: ShellSelectionView) -> Self {
        Self { view }
    }
}

impl_settings_content_with_paste!(ShellSettingsContent);

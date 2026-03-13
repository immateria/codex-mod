use crate::bottom_pane::ShellSelectionView;

pub(crate) struct ShellSettingsContent {
    view: ShellSelectionView,
}

impl ShellSettingsContent {
    pub(crate) fn new(view: ShellSelectionView) -> Self {
        Self { view }
    }
}

impl_settings_content_with_paste!(ShellSettingsContent);

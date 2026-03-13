use crate::bottom_pane::ModelSelectionView;

pub(crate) struct ModelSettingsContent {
    view: ModelSelectionView,
}

impl ModelSettingsContent {
    pub(crate) fn new(view: ModelSelectionView) -> Self {
        Self { view }
    }
}

impl_settings_content_conditional_mouse!(ModelSettingsContent);

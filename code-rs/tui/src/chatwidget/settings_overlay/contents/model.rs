use crate::bottom_pane::settings_pages::model::ModelSelectionView;

pub(crate) struct ModelSettingsContent {
    view: ModelSelectionView,
}

impl ModelSettingsContent {
    pub(crate) fn new(view: ModelSelectionView) -> Self {
        Self { view }
    }
}

impl_settings_content!(ModelSettingsContent);

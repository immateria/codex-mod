mod input;
mod model;
mod mouse;
mod pane_impl;
mod render;

#[cfg(test)]
mod tests;

use std::cell::Cell;

use code_common::model_presets::ModelPreset;

use crate::app_event_sender::AppEventSender;
use crate::components::form_text_field::FormTextField;

use super::model_selection_state::{ModelSelectionData, ModelSelectionViewParams};

pub(super) const SUMMARY_LINE_COUNT: usize = 3;
pub(super) const FOOTER_LINE_COUNT: usize = 2;
// 2 summary rows + 1 summary spacer + 1 footer spacer + 1 footer hint row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum EditTarget {
    ContextWindow,
    AutoCompact,
}

#[derive(Debug)]
pub(super) enum ViewMode {
    Main,
    Edit {
        target: EditTarget,
        field: FormTextField,
        error: Option<String>,
    },
    Transition,
}

pub(crate) struct ModelSelectionView {
    data: ModelSelectionData,
    selected_index: usize,
    app_event_tx: AppEventSender,
    is_complete: bool,
    scroll_offset: usize,
    visible_body_rows: Cell<usize>,
    mode: ViewMode,
}

pub(crate) type ModelSelectionViewFramed<'v> =
    crate::bottom_pane::chrome_view::Framed<'v, ModelSelectionView>;
pub(crate) type ModelSelectionViewContentOnly<'v> =
    crate::bottom_pane::chrome_view::ContentOnly<'v, ModelSelectionView>;
pub(crate) type ModelSelectionViewContentOnlyMut<'v> =
    crate::bottom_pane::chrome_view::ContentOnlyMut<'v, ModelSelectionView>;

impl ModelSelectionView {
    pub fn new(params: ModelSelectionViewParams, app_event_tx: AppEventSender) -> Self {
        let data = ModelSelectionData::new(params);
        let selected_index = data.initial_selection();
        Self {
            data,
            selected_index,
            app_event_tx,
            is_complete: false,
            scroll_offset: 0,
            visible_body_rows: Cell::new(0),
            mode: ViewMode::Main,
        }
    }

    pub(crate) fn framed(&self) -> ModelSelectionViewFramed<'_> {
        crate::bottom_pane::chrome_view::Framed::new(self)
    }

    pub(crate) fn content_only(&self) -> ModelSelectionViewContentOnly<'_> {
        crate::bottom_pane::chrome_view::ContentOnly::new(self)
    }

    pub(crate) fn content_only_mut(&mut self) -> ModelSelectionViewContentOnlyMut<'_> {
        crate::bottom_pane::chrome_view::ContentOnlyMut::new(self)
    }

    pub(crate) fn is_complete(&self) -> bool {
        self.is_complete
    }

    pub(crate) fn update_presets(&mut self, presets: Vec<ModelPreset>) {
        self.selected_index = self.data.update_presets(presets, self.selected_index);
    }
}

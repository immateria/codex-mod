use std::cell::Cell;
use std::path::PathBuf;

use code_core::config::{JsReplRuntimeKindToml, JsReplSettingsToml};

use crate::app_event_sender::AppEventSender;
use crate::chatwidget::BackgroundOrderTicket;
use crate::components::form_text_field::FormTextField;
use crate::components::scroll_state::ScrollState;

mod actions;
mod input;
mod mouse;
mod pages;
mod pane_impl;
mod render;
mod rows;

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TextTarget {
    RuntimePath,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ListTarget {
    RuntimeArgs,
    NodeModuleDirs,
}

#[derive(Debug)]
enum ViewMode {
    Transition,
    Main,
    EditText { target: TextTarget, field: FormTextField },
    EditList { target: ListTarget, field: FormTextField },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RowKind {
    Enabled,
    RuntimeKind,
    RuntimePath,
    PickRuntimePath,
    ClearRuntimePath,
    RuntimeArgs,
    NodeModuleDirs,
    AddNodeModuleDir,
    Apply,
    Close,
}

pub(crate) struct JsReplSettingsView {
    settings: JsReplSettingsToml,
    network_enabled: bool,
    app_event_tx: AppEventSender,
    ticket: BackgroundOrderTicket,
    is_complete: bool,
    dirty: bool,
    mode: ViewMode,
    state: ScrollState,
    viewport_rows: Cell<usize>,
}

crate::bottom_pane::chrome_view::impl_chrome_view!(JsReplSettingsView);

impl JsReplSettingsView {
    const DEFAULT_VISIBLE_ROWS: usize = crate::timing::DEFAULT_VISIBLE_ROWS;
    const HEADER_ROWS: u16 = 2;

    pub(super) fn desired_height_impl(&self, _width: u16) -> u16 {
        match &self.mode {
            ViewMode::Main => {
                let total_rows = self.row_count();
                let visible = (total_rows.clamp(1, 12)) as u16;
                2u16
                    .saturating_add(Self::HEADER_ROWS)
                    .saturating_add(visible)
            }
            ViewMode::EditText { .. } | ViewMode::EditList { .. } => 18,
            ViewMode::Transition => {
                2u16.saturating_add(Self::HEADER_ROWS).saturating_add(8)
            }
        }
    }

    pub(crate) fn new(
        settings: JsReplSettingsToml,
        network_enabled: bool,
        app_event_tx: AppEventSender,
        ticket: BackgroundOrderTicket,
    ) -> Self {
        let state = ScrollState::with_first_selected();
        Self {
            settings,
            network_enabled,
            app_event_tx,
            ticket,
            is_complete: false,
            dirty: false,
            mode: ViewMode::Main,
            state,
            viewport_rows: Cell::new(1),
        }
    }

    pub(crate) fn is_complete(&self) -> bool {
        self.is_complete
    }

    pub(crate) fn has_back_navigation(&self) -> bool {
        !matches!(self.mode, ViewMode::Main)
    }
}

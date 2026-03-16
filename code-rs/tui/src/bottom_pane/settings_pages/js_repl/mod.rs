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

pub(crate) type JsReplSettingsViewFramed<'v> =
    crate::bottom_pane::chrome_view::Framed<'v, JsReplSettingsView>;
pub(crate) type JsReplSettingsViewContentOnly<'v> =
    crate::bottom_pane::chrome_view::ContentOnly<'v, JsReplSettingsView>;
pub(crate) type JsReplSettingsViewFramedMut<'v> =
    crate::bottom_pane::chrome_view::FramedMut<'v, JsReplSettingsView>;
pub(crate) type JsReplSettingsViewContentOnlyMut<'v> =
    crate::bottom_pane::chrome_view::ContentOnlyMut<'v, JsReplSettingsView>;

impl JsReplSettingsView {
    const DEFAULT_VISIBLE_ROWS: usize = 8;
    const HEADER_ROWS: u16 = 3;

    pub(crate) fn new(
        settings: JsReplSettingsToml,
        network_enabled: bool,
        app_event_tx: AppEventSender,
        ticket: BackgroundOrderTicket,
    ) -> Self {
        let mut state = ScrollState::new();
        state.selected_idx = Some(0);
        Self {
            settings,
            network_enabled,
            app_event_tx,
            ticket,
            is_complete: false,
            dirty: false,
            mode: ViewMode::Main,
            state,
            viewport_rows: Cell::new(0),
        }
    }

    pub(crate) fn framed(&self) -> JsReplSettingsViewFramed<'_> {
        crate::bottom_pane::chrome_view::Framed::new(self)
    }

    pub(crate) fn content_only(&self) -> JsReplSettingsViewContentOnly<'_> {
        crate::bottom_pane::chrome_view::ContentOnly::new(self)
    }

    pub(crate) fn framed_mut(&mut self) -> JsReplSettingsViewFramedMut<'_> {
        crate::bottom_pane::chrome_view::FramedMut::new(self)
    }

    pub(crate) fn content_only_mut(&mut self) -> JsReplSettingsViewContentOnlyMut<'_> {
        crate::bottom_pane::chrome_view::ContentOnlyMut::new(self)
    }

    pub(crate) fn is_complete(&self) -> bool {
        self.is_complete
    }
}

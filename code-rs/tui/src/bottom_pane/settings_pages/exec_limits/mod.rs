use std::cell::Cell;
use std::time::Instant;

use crate::app_event_sender::AppEventSender;
use crate::components::form_text_field::FormTextField;
use crate::components::scroll_state::ScrollState;

const DEFAULT_VISIBLE_ROWS: usize = 8;

mod input;
mod model;
mod mouse;
mod pages;
mod pane_impl;
mod render;
#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RowKind {
    PidsMax,
    MemoryMax,
    ResetBothAuto,
    DisableBoth,
    Apply,
    Close,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EditTarget {
    PidsMax,
    MemoryMax,
}

#[derive(Debug)]
enum ViewMode {
    Main,
    Edit {
        target: EditTarget,
        field: FormTextField,
        error: Option<String>,
    },
    Transition,
}

pub(crate) struct ExecLimitsSettingsView {
    settings: code_core::config::ExecLimitsToml,
    last_applied: code_core::config::ExecLimitsToml,
    last_apply_at: Option<Instant>,
    mode: ViewMode,
    // Interior mutability so `render_main(&self, ...)` can clamp/scroll the
    // selection as the viewport changes without needing an outer `&mut self`.
    state: Cell<ScrollState>,
    viewport_rows: Cell<usize>,
    is_complete: bool,
    app_event_tx: AppEventSender,
}

pub(crate) type ExecLimitsSettingsViewFramed<'v> =
    crate::bottom_pane::chrome_view::Framed<'v, ExecLimitsSettingsView>;
pub(crate) type ExecLimitsSettingsViewContentOnly<'v> =
    crate::bottom_pane::chrome_view::ContentOnly<'v, ExecLimitsSettingsView>;
pub(crate) type ExecLimitsSettingsViewFramedMut<'v> =
    crate::bottom_pane::chrome_view::FramedMut<'v, ExecLimitsSettingsView>;
pub(crate) type ExecLimitsSettingsViewContentOnlyMut<'v> =
    crate::bottom_pane::chrome_view::ContentOnlyMut<'v, ExecLimitsSettingsView>;

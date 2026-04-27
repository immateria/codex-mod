use std::cell::{Cell, RefCell};
use std::time::Instant;

use crate::app_event_sender::AppEventSender;
use crate::components::form_text_field::FormTextField;
use crate::components::scroll_state::ScrollState;

use crate::timing::DEFAULT_VISIBLE_ROWS;

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
    // Cached computed values for the current settings (recomputed when settings change).
    // Only used on Linux where cgroup-backed effective values are computed.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    cached_pids_max: RefCell<(code_core::config::ExecLimitsToml, Option<u64>)>,
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    cached_memory_max_bytes: RefCell<(code_core::config::ExecLimitsToml, Option<u64>)>,
}

crate::bottom_pane::chrome_view::impl_chrome_view!(ExecLimitsSettingsView);

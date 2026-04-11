use std::cell::Cell;
use std::path::PathBuf;

use code_core::config_types::MemoriesToml;
use code_core::RolloutSummaryEntry;

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
enum MemoriesScopeChoice {
    Global,
    Profile,
    Project,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RowKind {
    Scope,
    GenerateMemories,
    UseMemories,
    SkipMcpOrWebSearch,
    MaxRawMemories,
    MaxRolloutAgeDays,
    MaxRolloutsPerStartup,
    MinRolloutIdleHours,
    ViewSummary,
    ViewRawMemories,
    BrowseRollouts,
    RefreshArtifacts,
    ClearArtifacts,
    OpenDirectory,
    Apply,
    Close,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EditTarget {
    MaxRawMemories,
    MaxRolloutAgeDays,
    MaxRolloutsPerStartup,
    MinRolloutIdleHours,
}

/// Tracks which view the text viewer should return to on Esc.
#[derive(Debug)]
enum TextViewerParent {
    Main,
    RolloutList(Box<RolloutListState>),
}

#[derive(Debug)]
struct TextViewerState {
    title: &'static str,
    lines: Vec<String>,
    scroll_top: Cell<usize>,
    viewport_rows: Cell<usize>,
    parent: TextViewerParent,
}

#[derive(Debug)]
struct RolloutListState {
    entries: Vec<RolloutSummaryEntry>,
    list_state: Cell<ScrollState>,
    viewport_rows: Cell<usize>,
}

#[derive(Debug)]
enum ViewMode {
    Main,
    Edit {
        target: EditTarget,
        field: FormTextField,
        error: Option<String>,
    },
    TextViewer(Box<TextViewerState>),
    RolloutList(Box<RolloutListState>),
    Transition,
}

pub(crate) struct MemoriesSettingsView {
    code_home: PathBuf,
    current_project: PathBuf,
    active_profile: Option<String>,
    global_settings: MemoriesToml,
    saved_global_settings: MemoriesToml,
    profile_settings: Option<MemoriesToml>,
    saved_profile_settings: Option<MemoriesToml>,
    project_settings: Option<MemoriesToml>,
    saved_project_settings: Option<MemoriesToml>,
    scope: MemoriesScopeChoice,
    mode: ViewMode,
    status: Option<(String, bool)>,
    state: Cell<ScrollState>,
    viewport_rows: Cell<usize>,
    is_complete: bool,
    app_event_tx: AppEventSender,
}

crate::bottom_pane::chrome_view::impl_chrome_view!(MemoriesSettingsView);

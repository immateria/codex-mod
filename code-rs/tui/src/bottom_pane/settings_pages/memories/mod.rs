use std::cell::Cell;
use std::path::PathBuf;

use code_core::config_types::MemoriesToml;

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

pub(crate) type MemoriesSettingsViewFramed<'v> =
    crate::bottom_pane::chrome_view::Framed<'v, MemoriesSettingsView>;
pub(crate) type MemoriesSettingsViewContentOnly<'v> =
    crate::bottom_pane::chrome_view::ContentOnly<'v, MemoriesSettingsView>;
pub(crate) type MemoriesSettingsViewFramedMut<'v> =
    crate::bottom_pane::chrome_view::FramedMut<'v, MemoriesSettingsView>;
pub(crate) type MemoriesSettingsViewContentOnlyMut<'v> =
    crate::bottom_pane::chrome_view::ContentOnlyMut<'v, MemoriesSettingsView>;

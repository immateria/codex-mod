use std::cell::Cell;
use std::path::PathBuf;

use code_core::config_types::MemoriesToml;
use code_core::{EpochSummary, RolloutSummaryEntry, TagCount, UserMemory};

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
    ManageUserMemories,
    BrowseTags,
    BrowseEpochs,
    ViewSummary,
    ViewRawMemories,
    ViewModelPrompt,
    BrowseRollouts,
    ViewStatus,
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
    EpochBrowser(Box<EpochBrowserState>),
}

/// Persistent search state for the text viewer.
#[derive(Debug)]
struct TextSearchState {
    query: String,
    /// Line indices that match the query (case-insensitive).
    matches: Vec<usize>,
    /// Index into `matches` for the current highlighted match.
    current: usize,
}

#[derive(Debug)]
struct TextViewerState {
    title: &'static str,
    lines: Vec<String>,
    scroll_top: Cell<usize>,
    viewport_rows: Cell<usize>,
    parent: TextViewerParent,
    search: Option<TextSearchState>,
}

#[derive(Debug)]
struct RolloutListState {
    entries: Vec<RolloutSummaryEntry>,
    list_state: Cell<ScrollState>,
    viewport_rows: Cell<usize>,
    /// Slug of rollout pending deletion (confirmation required).
    pending_delete: Option<String>,
}

#[derive(Debug)]
struct UserMemoryListState {
    entries: Vec<UserMemory>,
    list_state: Cell<ScrollState>,
    viewport_rows: Cell<usize>,
    /// ID of memory pending deletion (confirmation required).
    pending_delete: Option<String>,
}

/// Editing state for a user memory (create or update).
#[derive(Debug)]
struct UserMemoryEditorState {
    /// `None` = creating new, `Some(id)` = editing existing.
    editing_id: Option<String>,
    /// Original `created_at` for edits (preserved on update).
    original_created_at: Option<i64>,
    content_field: FormTextField,
    tags_field: FormTextField,
    /// Which field is focused.
    focus: UserMemoryEditorFocus,
    error: Option<String>,
    /// Preserved list state to return to on save/cancel.
    parent_list: Box<UserMemoryListState>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UserMemoryEditorFocus {
    Content,
    Tags,
}

/// State for the tag browser view.
#[derive(Debug)]
struct TagBrowserState {
    tags: Vec<TagCount>,
    list_state: Cell<ScrollState>,
    viewport_rows: Cell<usize>,
}

/// State for the epoch browser view.
#[derive(Debug)]
struct EpochBrowserState {
    epochs: Vec<EpochSummary>,
    list_state: Cell<ScrollState>,
    viewport_rows: Cell<usize>,
    /// When Some, we're showing epochs filtered by this tag.
    filter_tag: Option<String>,
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
    UserMemoryList(Box<UserMemoryListState>),
    UserMemoryEditor(Box<UserMemoryEditorState>),
    TagBrowser(Box<TagBrowserState>),
    EpochBrowser(Box<EpochBrowserState>),
    /// Transient search input inside a text viewer.
    SearchInput {
        viewer: Box<TextViewerState>,
        field: FormTextField,
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

crate::bottom_pane::chrome_view::impl_chrome_view!(MemoriesSettingsView);

impl MemoriesSettingsView {
    pub(crate) fn handle_paste_direct(&mut self, text: String) -> bool {
        match &mut self.mode {
            ViewMode::Edit { field, .. } | ViewMode::SearchInput { field, .. } => {
                field.handle_paste(text);
                true
            }
            ViewMode::UserMemoryEditor(editor) => {
                match editor.focus {
                    UserMemoryEditorFocus::Content => editor.content_field.handle_paste(text),
                    UserMemoryEditorFocus::Tags => editor.tags_field.handle_paste(text),
                };
                true
            }
            _ => false,
        }
    }
}

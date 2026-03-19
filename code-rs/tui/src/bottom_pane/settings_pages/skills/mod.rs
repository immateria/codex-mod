use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use code_core::config::{
    find_code_home,
    set_shell_style_profile_mcp_servers,
    set_shell_style_profile_paths,
    set_shell_style_profile_skill_mode,
    ShellStyleSkillMode,
};
use code_core::config_types::{CommandSafetyProfileConfig, ShellScriptStyle, ShellStyleProfileConfig};
use code_core::protocol::Op;
use code_protocol::skills::{Skill, SkillScope};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Position, Rect};
use ratatui::prelude::Widget;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::colors;
use crate::components::form_text_field::{FormTextField, InputFilter};
use crate::components::scroll_state::ScrollState;
use crate::ui_interaction::{
    clipped_vertical_rect_with_scroll,
    next_scroll_top_with_delta,
    redraw_if,
    render_vertical_scrollbar,
    scroll_top_to_keep_visible,
    split_pinned_footer_layout,
    ScrollSelectionBehavior,
    SelectableListMouseConfig,
    SelectableListMouseResult,
};

use crate::bottom_pane::{BottomPaneView, ConditionalUpdate};
use crate::bottom_pane::{ChromeMode, LastRenderContext};
use crate::bottom_pane::BottomPane;

mod document;
mod editor;
mod list;
mod model;
mod pane_impl;
mod persistence;
mod render;

use document::*;
use model::*;

pub(crate) struct SkillsSettingsView {
    skills: Vec<Skill>,
    shell_style_profiles: HashMap<ShellScriptStyle, ShellStyleProfileConfig>,
    list_state: ScrollState,
    list_viewport_rows: Cell<usize>,
    mode: Mode,
    status: Option<(String, Style)>,
    app_event_tx: AppEventSender,
    complete: bool,
    // SIDE EFFECT: `render` caches the most recent area so edit-mode focus
    // scrolling can stay aligned with the last rendered layout.
    last_render: LastRenderContext,
    editor: SkillEditorState,
}

impl SkillsSettingsView {
    pub fn new(
        skills: Vec<Skill>,
        shell_style_profiles: HashMap<ShellScriptStyle, ShellStyleProfileConfig>,
        app_event_tx: AppEventSender,
    ) -> Self {
        Self {
            skills,
            shell_style_profiles,
            list_state: ScrollState {
                selected_idx: Some(0),
                scroll_top: 0,
            },
            list_viewport_rows: Cell::new(
                usize::from(SKILLS_SETTINGS_VIEW_HEIGHT)
                    .saturating_sub(5)
                    .max(1),
            ),
            mode: Mode::List,
            status: None,
            app_event_tx,
            complete: false,
            last_render: LastRenderContext::new(ChromeMode::Framed),
            editor: SkillEditorState::new(),
        }
    }

    pub fn is_complete(&self) -> bool {
        self.complete
    }

    pub(crate) fn framed(&self) -> SkillsSettingsViewFramed<'_> {
        crate::bottom_pane::chrome_view::Framed::new(self)
    }

    pub(crate) fn content_only(&self) -> SkillsSettingsViewContentOnly<'_> {
        crate::bottom_pane::chrome_view::ContentOnly::new(self)
    }

    pub(crate) fn framed_mut(&mut self) -> SkillsSettingsViewFramedMut<'_> {
        crate::bottom_pane::chrome_view::FramedMut::new(self)
    }

    pub(crate) fn content_only_mut(&mut self) -> SkillsSettingsViewContentOnlyMut<'_> {
        crate::bottom_pane::chrome_view::ContentOnlyMut::new(self)
    }

    fn list_item_count(&self) -> usize {
        self.skills.len().saturating_add(1)
    }

    fn selected_list_index(&self) -> usize {
        let item_count = self.list_item_count();
        if item_count == 0 {
            return 0;
        }

        self.list_state
            .selected_idx
            .unwrap_or(0)
            .min(item_count.saturating_sub(1))
    }

    fn clamp_list_state(&mut self) {
        let item_count = self.list_item_count();
        self.list_state.clamp_selection(item_count);
    }

    fn ensure_list_selection_visible(&mut self) {
        let item_count = self.list_item_count();
        self.list_state.clamp_selection(item_count);
        self.list_state
            .ensure_visible(item_count, self.list_viewport_rows.get().max(1));
    }
}

pub(crate) type SkillsSettingsViewFramed<'v> = crate::bottom_pane::chrome_view::Framed<'v, SkillsSettingsView>;
pub(crate) type SkillsSettingsViewContentOnly<'v> =
    crate::bottom_pane::chrome_view::ContentOnly<'v, SkillsSettingsView>;
pub(crate) type SkillsSettingsViewFramedMut<'v> =
    crate::bottom_pane::chrome_view::FramedMut<'v, SkillsSettingsView>;
pub(crate) type SkillsSettingsViewContentOnlyMut<'v> =
    crate::bottom_pane::chrome_view::ContentOnlyMut<'v, SkillsSettingsView>;

#[cfg(test)]
mod tests;

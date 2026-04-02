mod input;
mod mouse;
mod pages;
mod pane_impl;
mod render;
#[cfg(test)]
mod tests;

use std::cell::Cell;

use code_core::config_types::ShellConfig;

use crate::app_event_sender::AppEventSender;
use crate::components::form_text_field::FormTextField;
use crate::components::scroll_state::ScrollState;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EditTarget {
    ZshPath,
    WrapperOverride,
}

#[derive(Debug)]
enum ViewMode {
    Transition,
    Main,
    EditText {
        target: EditTarget,
        field: FormTextField,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RowKind {
    Enabled,
    ZshPath,
    WrapperOverride,
    Apply,
    Close,
}

pub(crate) struct ShellEscalationSettingsView {
    active_profile: Option<String>,
    shell: Option<ShellConfig>,

    baseline_enabled: bool,
    enabled: bool,
    baseline_zsh_path: Option<String>,
    zsh_path: Option<String>,
    baseline_wrapper_override: Option<String>,
    wrapper_override: Option<String>,

    app_event_tx: AppEventSender,
    is_complete: bool,
    dirty: bool,
    mode: ViewMode,
    state: ScrollState,
    viewport_rows: Cell<usize>,
    editor_notice: Option<String>,
}

pub(crate) type ShellEscalationSettingsViewFramed<'v> =
    crate::bottom_pane::chrome_view::Framed<'v, ShellEscalationSettingsView>;
pub(crate) type ShellEscalationSettingsViewContentOnly<'v> =
    crate::bottom_pane::chrome_view::ContentOnly<'v, ShellEscalationSettingsView>;
pub(crate) type ShellEscalationSettingsViewFramedMut<'v> =
    crate::bottom_pane::chrome_view::FramedMut<'v, ShellEscalationSettingsView>;
pub(crate) type ShellEscalationSettingsViewContentOnlyMut<'v> =
    crate::bottom_pane::chrome_view::ContentOnlyMut<'v, ShellEscalationSettingsView>;

impl ShellEscalationSettingsView {
    const DEFAULT_VISIBLE_ROWS: usize = 8;

    pub(crate) fn new(
        active_profile: Option<String>,
        enabled: bool,
        shell: Option<ShellConfig>,
        zsh_path: Option<std::path::PathBuf>,
        wrapper_override: Option<std::path::PathBuf>,
        app_event_tx: AppEventSender,
    ) -> Self {
        let mut state = ScrollState::new();
        state.selected_idx = Some(0);

        let zsh_path = zsh_path.map(|path| path.to_string_lossy().to_string());
        let wrapper_override = wrapper_override.map(|path| path.to_string_lossy().to_string());

        Self {
            active_profile,
            shell,
            baseline_enabled: enabled,
            enabled,
            baseline_zsh_path: zsh_path.clone(),
            zsh_path,
            baseline_wrapper_override: wrapper_override.clone(),
            wrapper_override,
            app_event_tx,
            is_complete: false,
            dirty: false,
            mode: ViewMode::Main,
            state,
            viewport_rows: Cell::new(0),
            editor_notice: None,
        }
    }

    pub(crate) fn framed(&self) -> ShellEscalationSettingsViewFramed<'_> {
        crate::bottom_pane::chrome_view::Framed::new(self)
    }

    pub(crate) fn content_only(&self) -> ShellEscalationSettingsViewContentOnly<'_> {
        crate::bottom_pane::chrome_view::ContentOnly::new(self)
    }

    pub(crate) fn framed_mut(&mut self) -> ShellEscalationSettingsViewFramedMut<'_> {
        crate::bottom_pane::chrome_view::FramedMut::new(self)
    }

    pub(crate) fn content_only_mut(&mut self) -> ShellEscalationSettingsViewContentOnlyMut<'_> {
        crate::bottom_pane::chrome_view::ContentOnlyMut::new(self)
    }

    pub(crate) fn is_complete(&self) -> bool {
        self.is_complete
    }

    fn row_count() -> usize {
        5
    }

    fn build_rows(&self) -> Vec<RowKind> {
        vec![
            RowKind::Enabled,
            RowKind::ZshPath,
            RowKind::WrapperOverride,
            RowKind::Apply,
            RowKind::Close,
        ]
    }

    fn visible_budget(&self, total: usize) -> usize {
        if total == 0 {
            return 1;
        }
        let hint = self.viewport_rows.get();
        let target = if hint == 0 { Self::DEFAULT_VISIBLE_ROWS } else { hint };
        target.clamp(1, total)
    }

    fn reconcile_selection_state(&mut self) {
        let total = Self::row_count();
        if total == 0 {
            self.state.reset();
            return;
        }
        self.state.clamp_selection(total);
        let visible_budget = self.visible_budget(total);
        self.state.ensure_visible(total, visible_budget);
    }

    fn recompute_dirty(&mut self) {
        self.dirty = self.enabled != self.baseline_enabled
            || self.zsh_path != self.baseline_zsh_path
            || self.wrapper_override != self.baseline_wrapper_override;
    }
}

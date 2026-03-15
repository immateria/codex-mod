use super::*;

impl ExecLimitsSettingsView {
    pub(crate) fn new(settings: code_core::config::ExecLimitsToml, app_event_tx: AppEventSender) -> Self {
        let mut state = ScrollState::new();
        state.selected_idx = Some(0);
        let last_applied = settings.clone();
        Self {
            settings,
            last_applied,
            last_apply_at: None,
            mode: ViewMode::Main,
            state: Cell::new(state),
            viewport_rows: Cell::new(DEFAULT_VISIBLE_ROWS),
            is_complete: false,
            app_event_tx,
        }
    }

    pub(super) fn build_rows() -> [RowKind; 6] {
        [
            RowKind::PidsMax,
            RowKind::MemoryMax,
            RowKind::ResetBothAuto,
            RowKind::DisableBoth,
            RowKind::Apply,
            RowKind::Close,
        ]
    }

    pub(super) fn format_limit_pids(limit: code_core::config::ExecLimitToml) -> String {
        match limit {
            code_core::config::ExecLimitToml::Mode(code_core::config::ExecLimitModeToml::Auto) => {
                "Auto".to_string()
            }
            code_core::config::ExecLimitToml::Mode(code_core::config::ExecLimitModeToml::Disabled) => {
                "Disabled".to_string()
            }
            code_core::config::ExecLimitToml::Value(v) => v.to_string(),
        }
    }

    pub(super) fn format_limit_memory(limit: code_core::config::ExecLimitToml) -> String {
        match limit {
            code_core::config::ExecLimitToml::Mode(code_core::config::ExecLimitModeToml::Auto) => {
                "Auto".to_string()
            }
            code_core::config::ExecLimitToml::Mode(code_core::config::ExecLimitModeToml::Disabled) => {
                "Disabled".to_string()
            }
            code_core::config::ExecLimitToml::Value(v) => format!("{v} MiB"),
        }
    }

    pub(crate) fn framed(&self) -> ExecLimitsSettingsViewFramed<'_> {
        crate::bottom_pane::chrome_view::Framed::new(self)
    }

    pub(crate) fn content_only(&self) -> ExecLimitsSettingsViewContentOnly<'_> {
        crate::bottom_pane::chrome_view::ContentOnly::new(self)
    }

    pub(crate) fn framed_mut(&mut self) -> ExecLimitsSettingsViewFramedMut<'_> {
        crate::bottom_pane::chrome_view::FramedMut::new(self)
    }

    pub(crate) fn content_only_mut(&mut self) -> ExecLimitsSettingsViewContentOnlyMut<'_> {
        crate::bottom_pane::chrome_view::ContentOnlyMut::new(self)
    }

    pub(crate) fn is_complete(&self) -> bool {
        self.is_complete
    }

    pub(crate) fn handle_paste_direct(&mut self, text: String) -> bool {
        match &mut self.mode {
            ViewMode::Edit { field, .. } => {
                field.handle_paste(text);
                true
            }
            ViewMode::Main | ViewMode::Transition => false,
        }
    }
}


use super::*;

impl ExecLimitsSettingsView {
    pub(super) fn desired_height_impl(&self, _width: u16) -> u16 {
        match &self.mode {
            ViewMode::Main => {
                let header = u16::try_from(self.render_header_lines().len()).unwrap_or(u16::MAX);
                let total_rows = Self::build_rows().len();
                let visible = u16::try_from(total_rows.clamp(1, 10)).unwrap_or(u16::MAX);
                2u16.saturating_add(header).saturating_add(visible)
            }
            ViewMode::Edit { .. } => 10,
            ViewMode::Transition => {
                let header = u16::try_from(self.render_header_lines().len()).unwrap_or(u16::MAX);
                2u16.saturating_add(header).saturating_add(6)
            }
        }
    }

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

    pub(crate) fn content_only(&self) -> ExecLimitsSettingsViewContentOnly<'_> {
        crate::bottom_pane::chrome_view::ContentOnly::new(self)
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


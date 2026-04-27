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
        let state = ScrollState::with_first_selected();
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
            cached_pids_max: RefCell::new((code_core::config::ExecLimitsToml::default(), None)),
            cached_memory_max_bytes: RefCell::new((code_core::config::ExecLimitsToml::default(), None)),
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
                "Auto".to_owned()
            }
            code_core::config::ExecLimitToml::Mode(code_core::config::ExecLimitModeToml::Disabled) => {
                "Disabled".to_owned()
            }
            code_core::config::ExecLimitToml::Value(v) => v.to_string(),
        }
    }

    pub(super) fn format_limit_memory(limit: code_core::config::ExecLimitToml) -> String {
        match limit {
            code_core::config::ExecLimitToml::Mode(code_core::config::ExecLimitModeToml::Auto) => {
                "Auto".to_owned()
            }
            code_core::config::ExecLimitToml::Mode(code_core::config::ExecLimitModeToml::Disabled) => {
                "Disabled".to_owned()
            }
            code_core::config::ExecLimitToml::Value(v) => format!("{v} MiB"),
        }
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

    pub(crate) fn has_back_navigation(&self) -> bool {
        !matches!(self.mode, ViewMode::Main)
    }

    #[cfg(target_os = "linux")]
    pub(super) fn get_effective_pids_max(&self) -> Option<u64> {
        let cached = self.cached_pids_max.borrow();
        if cached.0 == self.settings {
            return cached.1;
        }
        drop(cached);
        let value = code_core::config::exec_limits_pids_max_with_setting(self.settings.pids_max);
        *self.cached_pids_max.borrow_mut() = (self.settings.clone(), value);
        value
    }

    #[cfg(target_os = "linux")]
    pub(super) fn get_effective_memory_max_mib(&self) -> Option<u64> {
        let cached = self.cached_memory_max_bytes.borrow();
        if cached.0 == self.settings {
            return cached.1;
        }
        drop(cached);
        let bytes = code_core::config::exec_limits_memory_max_bytes_with_setting(self.settings.memory_max_mb);
        let mib = bytes.map(|b| (b.saturating_add(1024 * 1024 - 1)) / (1024 * 1024));
        *self.cached_memory_max_bytes.borrow_mut() = (self.settings.clone(), mib);
        mib
    }
}


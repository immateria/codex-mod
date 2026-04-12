use std::cell::Cell;
use std::path::PathBuf;

use code_core::config_types::{
    SettingsMenuConfig,
    TuiHotkeysConfig,
    TuiHotkeysEnv,
    TuiHotkeysOverrides,
    TuiHotkeysPlatform,
};

use crate::app_event_sender::AppEventSender;
use crate::components::form_text_field::FormTextField;
use crate::components::scroll_state::ScrollState;

mod hotkeys;
mod input;
mod mouse;
mod pages;
mod pane_impl;
mod render;
mod rows;

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RowKind {
    OpenMode,
    OverlayMinWidth,
    NerdFonts,
    HotkeyScope,
    ModelSelectorHotkey,
    ReasoningEffortHotkey,
    ShellSelectorHotkey,
    NetworkSettingsHotkey,
    ExecOutputFoldHotkey,
    JsReplCodeFoldHotkey,
    JumpToParentCallHotkey,
    JumpToLatestChildCallHotkey,
    ShowConfigToml,
    ShowCodeHome,
    Apply,
    Close,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HotkeyScope {
    Global,
    Macos,
    Windows,
    Linux,
    Android,
    Termux,
    FreeBsd,
    OpenBsd,
    NetBsd,
    Dragonfly,
}

impl HotkeyScope {
    fn label(self) -> &'static str {
        match self {
            Self::Global => "global",
            Self::Macos => "macos",
            Self::Windows => "windows",
            Self::Linux => "linux",
            Self::Android => "android",
            Self::Termux => "termux",
            Self::FreeBsd => "freebsd",
            Self::OpenBsd => "openbsd",
            Self::NetBsd => "netbsd",
            Self::Dragonfly => "dragonfly",
        }
    }

    fn env(self) -> TuiHotkeysEnv {
        match self {
            Self::Global => TuiHotkeysEnv {
                platform: TuiHotkeysPlatform::Other,
                termux: false,
            },
            Self::Macos => TuiHotkeysEnv {
                platform: TuiHotkeysPlatform::Macos,
                termux: false,
            },
            Self::Windows => TuiHotkeysEnv {
                platform: TuiHotkeysPlatform::Windows,
                termux: false,
            },
            Self::Linux => TuiHotkeysEnv {
                platform: TuiHotkeysPlatform::Linux,
                termux: false,
            },
            Self::Android => TuiHotkeysEnv {
                platform: TuiHotkeysPlatform::Android,
                termux: false,
            },
            Self::Termux => TuiHotkeysEnv {
                platform: TuiHotkeysPlatform::Android,
                termux: true,
            },
            Self::FreeBsd => TuiHotkeysEnv {
                platform: TuiHotkeysPlatform::FreeBsd,
                termux: false,
            },
            Self::OpenBsd => TuiHotkeysEnv {
                platform: TuiHotkeysPlatform::OpenBsd,
                termux: false,
            },
            Self::NetBsd => TuiHotkeysEnv {
                platform: TuiHotkeysPlatform::NetBsd,
                termux: false,
            },
            Self::Dragonfly => TuiHotkeysEnv {
                platform: TuiHotkeysPlatform::Dragonfly,
                termux: false,
            },
        }
    }

    fn max_function_key(self) -> u8 {
        match self {
            Self::Macos => 20,
            _ => 24,
        }
    }

    fn platform_override(self) -> Option<TuiHotkeysPlatform> {
        match self {
            Self::Global | Self::Termux => None,
            Self::Macos => Some(TuiHotkeysPlatform::Macos),
            Self::Windows => Some(TuiHotkeysPlatform::Windows),
            Self::Linux => Some(TuiHotkeysPlatform::Linux),
            Self::Android => Some(TuiHotkeysPlatform::Android),
            Self::FreeBsd => Some(TuiHotkeysPlatform::FreeBsd),
            Self::OpenBsd => Some(TuiHotkeysPlatform::OpenBsd),
            Self::NetBsd => Some(TuiHotkeysPlatform::NetBsd),
            Self::Dragonfly => Some(TuiHotkeysPlatform::Dragonfly),
        }
    }

    fn next(self) -> Self {
        match self {
            Self::Global => Self::Macos,
            Self::Macos => Self::Windows,
            Self::Windows => Self::Linux,
            Self::Linux => Self::Android,
            Self::Android => Self::Termux,
            Self::Termux => Self::FreeBsd,
            Self::FreeBsd => Self::OpenBsd,
            Self::OpenBsd => Self::NetBsd,
            Self::NetBsd => Self::Dragonfly,
            Self::Dragonfly => Self::Global,
        }
    }

    fn prev(self) -> Self {
        match self {
            Self::Global => Self::Dragonfly,
            Self::Macos => Self::Global,
            Self::Windows => Self::Macos,
            Self::Linux => Self::Windows,
            Self::Android => Self::Linux,
            Self::Termux => Self::Android,
            Self::FreeBsd => Self::Termux,
            Self::OpenBsd => Self::FreeBsd,
            Self::NetBsd => Self::OpenBsd,
            Self::Dragonfly => Self::NetBsd,
        }
    }
}

#[derive(Debug)]
enum ViewMode {
    Transition,
    Main,
    EditWidth { field: FormTextField, error: Option<String> },
    CaptureHotkey { row: RowKind, error: Option<String> },
}

pub(crate) struct InterfaceSettingsView {
    settings: SettingsMenuConfig,
    hotkeys: TuiHotkeysConfig,
    hotkey_scope: HotkeyScope,
    icon_mode: code_core::config_types::IconMode,
    icon_mode_baseline: code_core::config_types::IconMode,
    code_home: PathBuf,
    app_event_tx: AppEventSender,
    is_complete: bool,
    dirty_settings: bool,
    dirty_hotkeys: bool,
    status: Option<(String, bool)>,
    mode: ViewMode,
    state: ScrollState,
    main_viewport_rows: Cell<usize>,
}

crate::bottom_pane::chrome_view::impl_chrome_view!(InterfaceSettingsView);

impl InterfaceSettingsView {
    pub(super) fn desired_height_impl(&self, _width: u16) -> u16 {
        match &self.mode {
            ViewMode::Main => {
                let base = self.build_rows().len().saturating_add(4);
                (base.clamp(12, 20)) as u16
            }
            ViewMode::EditWidth { error, .. } | ViewMode::CaptureHotkey { error, .. } => {
                if error.is_some() { 9 } else { 8 }
            }
            ViewMode::Transition => 8,
        }
    }

    pub fn new(
        code_home: PathBuf,
        settings: SettingsMenuConfig,
        hotkeys: TuiHotkeysConfig,
        icon_mode: code_core::config_types::IconMode,
        app_event_tx: AppEventSender,
    ) -> Self {
        let state = ScrollState::with_first_selected();
        Self {
            settings,
            hotkeys,
            hotkey_scope: HotkeyScope::Global,
            icon_mode,
            icon_mode_baseline: icon_mode,
            code_home,
            app_event_tx,
            is_complete: false,
            dirty_settings: false,
            dirty_hotkeys: false,
            status: None,
            mode: ViewMode::Main,
            state,
            main_viewport_rows: Cell::new(1),
        }
    }

    pub(crate) fn handle_paste_direct(&mut self, text: String) -> bool {
        match &mut self.mode {
            ViewMode::EditWidth { field, .. } => {
                field.handle_paste(text);
                true
            }
            ViewMode::Main | ViewMode::Transition | ViewMode::CaptureHotkey { .. } => false,
        }
    }

    pub(crate) fn is_complete(&self) -> bool {
        self.is_complete
    }

    pub(crate) fn has_back_navigation(&self) -> bool {
        !matches!(self.mode, ViewMode::Main)
    }
}

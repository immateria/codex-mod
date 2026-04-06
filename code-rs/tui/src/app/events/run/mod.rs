use std::sync::atomic::Ordering;
use std::thread;
use std::time::{Duration, Instant};
#[cfg(debug_assertions)]
use std::path::PathBuf;

use color_eyre::eyre::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use crossterm::execute;
use crossterm::SynchronizedUpdate;

use code_cloud_tasks_client::{CloudTaskError, TaskId};
use code_core::config::add_project_allowed_command;
use code_core::config_types::AuthCredentialsStoreMode;
use code_core::config_types::Notifications;
#[cfg(debug_assertions)]
use code_core::protocol::Event;
use code_core::protocol::{Op, SandboxPolicy};
use code_core::SessionCatalog;
use code_login::{AuthManager, AuthMode, ServerOptions};
use portable_pty::PtySize;

use crate::app_event::{AppEvent, SessionPickerAction};
use crate::bottom_pane::SettingsSection;
use crate::chatwidget::ChatWidget;
use crate::cloud_tasks_service;
use crate::exec_command::strip_bash_lc_and_escape;
use crate::external_editor;
use crate::get_git_diff::get_git_diff;
use crate::history_cell;
use crate::slash_command::SlashCommand;
use crate::thread_spawner;
use crate::tui;

use super::priority::is_image_clipboard_paste_shortcut;
use super::shell_style_profile_summary::generate_shell_style_profile_summary;
use super::super::render::flatten_draw_result;
use super::super::state::{
    App,
    AppState,
    ChatWidgetArgs,
    LoginFlowState,
    ThemeSplitPreview,
    BACKPRESSURE_FORCED_DRAW_SKIPS,
};

fn auth_credentials_store_mode_label(mode: AuthCredentialsStoreMode) -> &'static str {
    match mode {
        AuthCredentialsStoreMode::File => "file",
        AuthCredentialsStoreMode::Keyring => "keyring",
        AuthCredentialsStoreMode::Auto => "auto",
        AuthCredentialsStoreMode::Ephemeral => "ephemeral",
    }
}

impl App<'_> {
    pub(crate) fn run(&mut self, terminal: &mut tui::Tui) -> Result<()> {
        // Insert an event to trigger the first render.
        let app_event_tx = self.app_event_tx.clone();
        app_event_tx.send(AppEvent::RequestRedraw);
        // Some Windows/macOS terminals report an initial size that stabilizes
        // shortly after entering the alt screen. Schedule one follow‑up frame
        // to catch any late size change without polling.
        app_event_tx.send(AppEvent::ScheduleFrameIn(Duration::from_millis(120)));

        'main: loop {
            let Some(event) = self.next_event_priority() else { break 'main };
            include!("history_insert.rs");
        }
        if self.alt_screen_active {
            terminal.clear()?;
        }

        Ok(())
    }
}

mod notifications;
mod runs;
mod screen;

use color_eyre::eyre::Result;

use crate::app_event::TerminalRunController;
use crate::tui;

use super::state::App;

impl App<'_> {
    pub(super) fn apply_terminal_title(&self) {
        screen::apply_terminal_title_inner(self);
    }

    pub(super) fn format_notification_message(title: &str, body: Option<&str>) -> Option<String> {
        notifications::format_notification_message_inner(title, body)
    }

    pub(super) fn emit_osc9_notification(message: &str) {
        notifications::emit_osc9_notification_inner(message);
    }

    pub(super) fn start_terminal_run(
        &mut self,
        id: u64,
        command: Vec<String>,
        display: Option<String>,
        controller: Option<TerminalRunController>,
    ) {
        runs::start_terminal_run_inner(self, id, command, display, controller);
    }

    #[cfg(unix)]
    pub(super) fn suspend(&mut self, terminal: &mut tui::Tui) -> Result<()> {
        screen::suspend_inner(self, terminal)
    }

    /// Toggle between alternate-screen TUI and standard terminal buffer (Ctrl+T).
    pub(super) fn toggle_screen_mode(&mut self, _terminal: &mut tui::Tui) -> Result<()> {
        screen::toggle_screen_mode_inner(self, _terminal)
    }
}

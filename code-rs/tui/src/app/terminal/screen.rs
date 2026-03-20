use color_eyre::eyre::Result;

use crate::app::state::{App, AppState};
use crate::app_event::AppEvent;
use crate::tui;

pub(super) fn apply_terminal_title_inner(app: &App<'_>) {
    let title = app
        .terminal_title_override
        .as_deref()
        .unwrap_or(App::DEFAULT_TERMINAL_TITLE);
    let _ = crossterm::execute!(
        std::io::stdout(),
        crossterm::terminal::SetTitle(title.to_string())
    );
}

#[cfg(unix)]
pub(super) fn suspend_inner(app: &mut App<'_>, terminal: &mut tui::Tui) -> Result<()> {
    tui::restore()?;
    // SAFETY: Unix-only code path. We intentionally send SIGTSTP to the
    // current process group (pid 0) to trigger standard job-control
    // suspension semantics. This FFI does not involve any raw pointers,
    // is not called from a signal handler, and uses a constant signal.
    // Errors from kill are acceptable (e.g., if already stopped) — the
    // subsequent re-init path will still leave the terminal in a good state.
    // We considered `nix`, but didn't think it was worth pulling in for this one call.
    unsafe { libc::kill(0, libc::SIGTSTP) };
    let (new_terminal, new_terminal_info) = tui::init(&app.config)?;
    *terminal = new_terminal;
    app.terminal_info = new_terminal_info;
    terminal.clear()?;
    app.app_event_tx.send(AppEvent::RequestRedraw);
    Ok(())
}

pub(super) fn toggle_screen_mode_inner(app: &mut App<'_>, _terminal: &mut tui::Tui) -> Result<()> {
    if app.alt_screen_active {
        // Leave alt screen only; keep raw mode enabled for key handling.
        let _ = crate::tui::leave_alt_screen_only();
        // Clear the normal buffer so our buffered transcript starts at a clean screen
        let _ = crossterm::execute!(
            std::io::stdout(),
            crossterm::style::ResetColor,
            crossterm::terminal::Clear(crossterm::terminal::ClearType::All),
            crossterm::cursor::MoveTo(0, 0),
            crossterm::terminal::EnableLineWrap
        );
        app.alt_screen_active = false;
        // Persist preference
        let _ = code_core::config::set_tui_alternate_screen(&app.config.code_home, false);
        // Immediately mirror the entire transcript into the terminal scrollback so
        // the user sees full history when entering standard mode.
        if let AppState::Chat { widget } = &app.app_state {
            let transcript = widget.export_transcript_lines_for_buffer();
            if !transcript.is_empty() {
                // Best-effort: compute current width and bottom reservation.
                // We don't have `terminal` here; schedule a one-shot redraw event
                // that carries the transcript via InsertHistory to reuse the normal path.
                app.app_event_tx.send(AppEvent::InsertHistory(transcript));
            }
        }
        // Ensure the input is painted in its reserved region immediately.
        app.schedule_redraw();
    } else {
        // Re-enter alt screen and force a clean repaint.
        let fg = crate::colors::text();
        let bg = crate::colors::background();
        let _ = crate::tui::enter_alt_screen_only(fg, bg);
        app.clear_on_first_frame = true;
        app.alt_screen_active = true;
        // Persist preference
        let _ = code_core::config::set_tui_alternate_screen(&app.config.code_home, true);
        // Request immediate redraw
        app.schedule_redraw();
    }
    Ok(())
}


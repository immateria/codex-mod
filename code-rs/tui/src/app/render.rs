use std::sync::atomic::Ordering;
use std::thread;
use std::time::Duration;

use color_eyre::eyre::Result;
use crossterm::SynchronizedUpdate;
use code_core::config_types::ThemeName;
use ratatui::buffer::Buffer;
use ratatui::buffer::Cell;
use ratatui::layout::Rect;

use crate::app_event::AppEvent;
use crate::thread_spawner;
use crate::tui;

use super::state::{App, AppState, REDRAW_DEBOUNCE};

impl App<'_> {
    /// Schedule a redraw immediately and open a short debounce window to coalesce
    /// subsequent requests. Crucially, even if a timer is already armed (e.g., an
    /// animation scheduled a future frame), we still trigger an immediate redraw
    /// to keep keypress echo latency low.
    #[allow(clippy::unwrap_used)]
    pub(super) fn schedule_redraw(&self) {
        // Only queue a new frame when one is not already in flight; otherwise record
        // that we owe a follow-up immediately after the active frame completes.
        let should_send = self
            .redraw_inflight
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Relaxed)
            .is_ok();
        if should_send {
            self.app_event_tx.send(AppEvent::Redraw);
        } else {
            self.post_frame_redraw.store(true, Ordering::Release);
        }

        // Arm debounce window if not already armed.
        if self
            .pending_redraw
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
        {
            let pending_redraw = self.pending_redraw.clone();
            let pending_redraw_for_thread = pending_redraw.clone();
            if thread_spawner::spawn_lightweight("redraw-debounce", move || {
                thread::sleep(REDRAW_DEBOUNCE);
                pending_redraw_for_thread.store(false, Ordering::Release);
            })
            .is_none()
            {
                pending_redraw.store(false, Ordering::Release);
            }
        }
    }

    /// Schedule a redraw after the specified duration.
    pub(super) fn schedule_redraw_in(&self, duration: Duration) {
        self.frame_timer
            .schedule(duration, self.app_event_tx.clone());
    }

    /// Attempt to draw a frame with stdout temporarily set to non‑blocking.
    /// This lets us flush buffered UI even when POLLOUT stays false (tmux reattach,
    /// XON/XOFF). Original flags are restored before returning.
    pub(super) fn draw_frame_with_nonblocking_stdout(
        &mut self,
        terminal: &mut tui::Tui,
    ) -> std::io::Result<Result<()>> {
        #[cfg(unix)]
        {
            use libc::{fcntl, F_GETFL, F_SETFL, O_NONBLOCK};
            use std::os::fd::AsRawFd;

            struct RestoreFlags {
                fd: i32,
                flags: i32,
            }
            impl Drop for RestoreFlags {
                fn drop(&mut self) {
                    unsafe { libc::fcntl(self.fd, libc::F_SETFL, self.flags) };
                }
            }

            let fd = std::io::stdout().as_raw_fd();
            let orig = unsafe { fcntl(fd, F_GETFL) };
            if orig < 0 {
                return Err(std::io::Error::last_os_error());
            }
            let _restore = RestoreFlags { fd, flags: orig };
            let set = unsafe { fcntl(fd, F_SETFL, orig | O_NONBLOCK) };
            if set < 0 {
                return Err(std::io::Error::last_os_error());
            }

            std::io::stdout().sync_update(|_| self.draw_next_frame(terminal))
        }

        #[cfg(not(unix))]
        {
            // Non‑Unix platforms already treat stdout as writable; fall back to normal draw.
            std::io::stdout().sync_update(|_| self.draw_next_frame(terminal))
        }
    }

    pub(super) fn draw_next_frame(&mut self, terminal: &mut tui::Tui) -> Result<()> {
        // Always render a frame. In standard-terminal mode we still draw the
        // chat UI (without status/HUD) directly into the normal buffer.
        // Hard clear on the very first frame (and while onboarding) to ensure a
        // clean background across terminals that don't respect our color attrs
        // during EnterAlternateScreen.
        if self.alt_screen_active && (self.clear_on_first_frame || matches!(self.app_state, AppState::Onboarding { .. })) {
            terminal.clear()?;
            self.clear_on_first_frame = false;
        }

        // If the terminal area changed (actual resize or tab switch that altered
        // viewport), force a full clear once to prevent ghost artifacts. Some
        // terminals on Windows/macOS do not reliably deliver Resize events on
        // focus switches; querying the size each frame is cheap and lets us
        // detect the change without extra event wiring.
        let screen_size = terminal.size()?;
        if self
            .last_frame_size
            .map(|prev| prev != screen_size)
            .unwrap_or(false)
        {
            terminal.clear()?;
        }
        self.last_frame_size = Some(screen_size);

        let split_preview = self
            .theme_split_preview
            .filter(|split| split.current != split.preview);
        let theme_before_split = split_preview.map(|_| crate::theme::current_theme_name());

        let completed_frame = terminal.draw(|frame| {
            match &mut self.app_state {
                AppState::Chat { widget } => {
                    let area = frame.area();
                    let cursor_pos = widget.cursor_pos(area);
                    if let Some(split) = split_preview {
                        apply_runtime_theme_for_render(split.current);
                        if let Some((x, y)) = cursor_pos {
                            frame.set_cursor_position((x, y));
                        }
                        frame.render_widget_ref(&**widget, area);
                        let left_snapshot = snapshot_left_half(frame.buffer_mut(), area);

                        apply_runtime_theme_for_render(split.preview);
                        if let Some((x, y)) = cursor_pos {
                            frame.set_cursor_position((x, y));
                        }
                        frame.render_widget_ref(&**widget, area);
                        restore_left_half(frame.buffer_mut(), area, &left_snapshot);
                    } else {
                        if let Some((x, y)) = cursor_pos {
                            frame.set_cursor_position((x, y));
                        }
                        frame.render_widget_ref(&**widget, area);
                    }
                }
                AppState::Onboarding { screen } => frame.render_widget_ref(&*screen, frame.area()),
            }
        });

        if let Some(theme_name) = theme_before_split {
            apply_runtime_theme_for_render(theme_name);
        }

        let completed_frame = completed_frame?;
        self.buffer_diff_profiler.record(&completed_frame);
        Ok(())
    }
}

fn apply_runtime_theme_for_render(theme_name: ThemeName) {
    let mapped = crate::theme::map_theme_for_palette(theme_name, crate::theme::custom_theme_is_dark());
    if crate::theme::current_theme_name() == mapped {
        return;
    }

    if matches!(theme_name, ThemeName::Custom) {
        if let Some(colors) = crate::theme::custom_theme_colors() {
            crate::theme::init_theme(&code_core::config_types::ThemeConfig {
                name: ThemeName::Custom,
                colors,
                label: crate::theme::custom_theme_label(),
                is_dark: crate::theme::custom_theme_is_dark(),
            });
        } else {
            crate::theme::switch_theme(theme_name);
        }
    } else {
        crate::theme::switch_theme(theme_name);
    }
}

fn snapshot_left_half(buf: &Buffer, area: Rect) -> Vec<Cell> {
    let left_width = (area.width / 2) as usize;
    let height = area.height as usize;
    if left_width == 0 || height == 0 {
        return Vec::new();
    }

    let buf_width = buf.area.width as usize;
    let buf_height = buf.area.height as usize;
    let x0 = area.x.saturating_sub(buf.area.x) as usize;
    let y0 = area.y.saturating_sub(buf.area.y) as usize;
    if x0 >= buf_width || y0 >= buf_height || x0.saturating_add(left_width) > buf_width {
        return Vec::new();
    }

    let mut snapshot = Vec::with_capacity(left_width.saturating_mul(height));
    for row in 0..height {
        let y = y0.saturating_add(row);
        if y >= buf_height {
            break;
        }
        let row_start = y.saturating_mul(buf_width).saturating_add(x0);
        let row_end = row_start.saturating_add(left_width);
        if row_end > buf.content.len() {
            break;
        }
        snapshot.extend(buf.content[row_start..row_end].iter().cloned());
    }
    snapshot
}

fn restore_left_half(buf: &mut Buffer, area: Rect, snapshot: &[Cell]) {
    let left_width = (area.width / 2) as usize;
    let height = area.height as usize;
    let expected = left_width.saturating_mul(height);
    if expected == 0 || snapshot.len() < expected {
        return;
    }

    let buf_width = buf.area.width as usize;
    let buf_height = buf.area.height as usize;
    let x0 = area.x.saturating_sub(buf.area.x) as usize;
    let y0 = area.y.saturating_sub(buf.area.y) as usize;
    if x0 >= buf_width || y0 >= buf_height || x0.saturating_add(left_width) > buf_width {
        return;
    }

    let mut offset = 0usize;
    for row in 0..height {
        let y = y0.saturating_add(row);
        if y >= buf_height {
            break;
        }
        let row_start = y.saturating_mul(buf_width).saturating_add(x0);
        let row_end = row_start.saturating_add(left_width);
        if row_end > buf.content.len() || offset.saturating_add(left_width) > snapshot.len() {
            break;
        }
        buf.content[row_start..row_end]
            .clone_from_slice(&snapshot[offset..offset + left_width]);
        offset = offset.saturating_add(left_width);
    }
}

/// Flatten a nested draw result of the form `io::Result<Result<()>>` into a
/// single `io::Result<()>`, preserving error kinds for WouldBlock handling.
pub(super) fn flatten_draw_result(res: std::io::Result<Result<()>>) -> std::io::Result<()> {
    match res {
        Ok(inner) => match inner {
            Ok(()) => Ok(()),
            Err(err) => {
                // Preserve the original `io::ErrorKind` when the underlying
                // draw failure is (or wraps) an `io::Error`. This keeps
                // backpressure handling (WouldBlock/EAGAIN) working even though
                // the draw path uses `color_eyre::Result`.
                let kind = err
                    .downcast_ref::<std::io::Error>()
                    .or_else(|| err.root_cause().downcast_ref::<std::io::Error>())
                    .map(std::io::Error::kind)
                    .unwrap_or(std::io::ErrorKind::Other);
                Err(std::io::Error::new(kind, err))
            }
        },
        Err(e) => Err(e),
    }
}

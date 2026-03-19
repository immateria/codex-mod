use super::*;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Style, Stylize};

use crate::bottom_pane::chrome::ChromeMode;
use crate::bottom_pane::settings_ui::rows::{KeyValueRow, StyledText};
use crate::bottom_pane::settings_ui::toggle;

impl NetworkSettingsView {
    fn main_row_specs(&self, rows: &[RowKind], show_advanced: bool) -> Vec<KeyValueRow<'_>> {
        let mode = Self::mode_label(self.settings.mode);

        let allowed_count = self.settings.allowed_domains.len();
        let allowed_summary = if allowed_count == 0 {
            "(none)".to_string()
        } else {
            format!("{allowed_count} entries")
        };

        let denied_count = self.settings.denied_domains.len();
        let denied_summary = if denied_count == 0 {
            "(none)".to_string()
        } else {
            format!("{denied_count} entries")
        };

        let advanced_label = if show_advanced { "shown" } else { "hidden" };

        let unix_count = self.settings.allow_unix_sockets.len();
        let unix_summary = if unix_count == 0 {
            "(none)".to_string()
        } else {
            format!("{unix_count} entries")
        };

        let apply_suffix = if self.dirty { " *" } else { "" };
        let mut enabled_status = toggle::enabled_word_warning_off(self.settings.enabled);
        enabled_status.style = enabled_status.style.bold();

        rows.iter()
            .copied()
            .map(|kind| match kind {
                RowKind::Enabled => KeyValueRow::new("Enabled").with_value(enabled_status.clone()),
                RowKind::Mode => KeyValueRow::new("Mode").with_value(StyledText::new(
                    mode,
                    Style::default().fg(crate::colors::info()),
                )),
                RowKind::AllowedDomains => {
                    KeyValueRow::new("Allowed domains").with_value(StyledText::new(
                        allowed_summary.clone(),
                        Style::default().fg(crate::colors::text_dim()),
                    ))
                }
                RowKind::DeniedDomains => {
                    KeyValueRow::new("Denied domains").with_value(StyledText::new(
                        denied_summary.clone(),
                        Style::default().fg(crate::colors::text_dim()),
                    ))
                }
                RowKind::AllowLocalBinding => KeyValueRow::new("Allow local binding")
                    .with_value(toggle::on_off_word(self.settings.allow_local_binding)),
                RowKind::AdvancedToggle => KeyValueRow::new("Advanced").with_value(StyledText::new(
                    advanced_label,
                    Style::default().fg(crate::colors::text_dim()),
                )),
                RowKind::Socks5Enabled => {
                    KeyValueRow::new("SOCKS5").with_value(toggle::on_off_word(self.settings.enable_socks5))
                }
                RowKind::Socks5Udp => KeyValueRow::new("SOCKS5 UDP")
                    .with_value(toggle::on_off_word(self.settings.enable_socks5_udp)),
                RowKind::AllowUpstreamProxyEnv => KeyValueRow::new("Allow upstream proxy env")
                    .with_value(toggle::on_off_word(self.settings.allow_upstream_proxy)),
                RowKind::AllowUnixSockets => KeyValueRow::new("Allow unix sockets").with_value(
                    StyledText::new(unix_summary.clone(), Style::default().fg(crate::colors::text_dim())),
                ),
                RowKind::Apply => KeyValueRow::new("Apply changes").with_value(StyledText::new(
                    apply_suffix,
                    Style::default().fg(crate::colors::warning()),
                )),
                RowKind::Close => KeyValueRow::new("Close"),
            })
            .collect()
    }

    fn render_main_in_chrome(
        &self,
        area: Rect,
        buf: &mut Buffer,
        show_advanced: bool,
        chrome: ChromeMode,
    ) {
        let rows = self.build_rows(show_advanced);
        let total = rows.len();
        let state = self.state.clamped(total);
        let selected_idx = state.selected_idx.unwrap_or(0);
        let scroll_top = state.scroll_top;

        let row_specs = self.main_row_specs(&rows, show_advanced);
        let Some(layout) = self.main_page().render_in_chrome(
            chrome,
            area,
            buf,
            scroll_top,
            Some(selected_idx),
            &row_specs,
        ) else {
            return;
        };
        self.viewport_rows.set(layout.visible_rows());
    }

    fn render_main(&self, area: Rect, buf: &mut Buffer, show_advanced: bool) {
        self.render_main_in_chrome(area, buf, show_advanced, ChromeMode::Framed);
    }

    fn render_main_without_frame(&self, area: Rect, buf: &mut Buffer, show_advanced: bool) {
        self.render_main_in_chrome(area, buf, show_advanced, ChromeMode::ContentOnly);
    }

    fn render_edit(&self, area: Rect, buf: &mut Buffer, target: EditTarget, field: &FormTextField) {
        let _layout = Self::edit_page(target).render_in_chrome(ChromeMode::Framed, area, buf, field);
    }

    fn render_edit_without_frame(
        &self,
        area: Rect,
        buf: &mut Buffer,
        target: EditTarget,
        field: &FormTextField,
    ) {
        let _layout =
            Self::edit_page(target).render_in_chrome(ChromeMode::ContentOnly, area, buf, field);
    }

    pub(super) fn render_content_only(&self, area: Rect, buf: &mut Buffer) {
        match &self.mode {
            ViewMode::Main { show_advanced } => {
                self.render_main_without_frame(area, buf, *show_advanced);
            }
            ViewMode::EditList { target, field, .. } => {
                self.render_edit_without_frame(area, buf, *target, field);
            }
            ViewMode::Transition => self.render_main_without_frame(area, buf, false),
        }
    }

    pub(super) fn render_framed(&self, area: Rect, buf: &mut Buffer) {
        match &self.mode {
            ViewMode::Main { show_advanced } => self.render_main(area, buf, *show_advanced),
            ViewMode::EditList { target, field, .. } => self.render_edit(area, buf, *target, field),
            ViewMode::Transition => self.render_main(area, buf, false),
        }
    }
}

use super::{EditTarget, FOOTER_LINE_COUNT, ModelSelectionView, SUMMARY_LINE_COUNT, ViewMode};

use crate::app_event::AppEvent;

use code_core::config_types::ContextMode;
use code_core::model_family::{
    default_auto_compact_limit_for_context_window, derive_default_model_family,
    resolve_context_settings,
};
use code_protocol::num_format::format_with_separators_u64;

use super::super::model_selection_state::{EntryKind, SelectionAction};

impl ModelSelectionView {
    pub(super) fn entry_count(&self) -> usize {
        self.data.entry_count()
    }

    pub(super) fn content_line_count(&self) -> u16 {
        if matches!(self.mode, ViewMode::Edit { .. }) {
            return 8;
        }
        self.data
            .content_line_count()
            .saturating_sub((SUMMARY_LINE_COUNT + FOOTER_LINE_COUNT) as u16)
    }

    pub(super) fn move_selection_up(&mut self) {
        let total = self.entry_count();
        if total == 0 {
            return;
        }
        self.selected_index = if self.selected_index == 0 {
            total - 1
        } else {
            self.selected_index.saturating_sub(1)
        };
        self.ensure_selected_visible();
    }

    pub(super) fn move_selection_down(&mut self) {
        let total = self.entry_count();
        if total == 0 {
            return;
        }
        self.selected_index = (self.selected_index + 1) % total;
        self.ensure_selected_visible();
    }

    pub(super) fn ensure_selected_visible(&mut self) {
        let body_height = self.visible_body_rows.get();
        if body_height == 0 {
            return;
        }

        let selected_line = self.selected_body_line(self.selected_index);
        if selected_line < self.scroll_offset {
            self.scroll_offset = selected_line;
        }
        let visible_end = self.scroll_offset + body_height;
        if selected_line >= visible_end {
            self.scroll_offset = selected_line.saturating_sub(body_height) + 1;
        }
    }

    pub(super) fn scroll_up(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_sub(1);
    }

    pub(super) fn scroll_down(&mut self) {
        let total_lines = self.content_line_count() as usize;
        let max_scroll = total_lines.saturating_sub(self.visible_body_rows.get());
        if self.scroll_offset < max_scroll {
            self.scroll_offset += 1;
        }
    }

    pub(super) fn select_item(&mut self, index: usize) {
        let total = self.entry_count();
        if index >= total {
            return;
        }
        self.selected_index = index;
        self.confirm_selection();
    }

    pub(super) fn selected_body_line(&self, entry_index: usize) -> usize {
        self.data.entry_line(entry_index).saturating_sub(SUMMARY_LINE_COUNT)
    }

    pub(super) fn confirm_selection(&mut self) {
        if let Some(entry) = self.data.entry_at(self.selected_index) {
            match entry {
                EntryKind::ContextWindow => self.open_edit_for(EditTarget::ContextWindow, false),
                EntryKind::AutoCompact => self.open_edit_for(EditTarget::AutoCompact, false),
                _ => {
                    if let Some(action) = self.data.apply_selection(entry) {
                        self.dispatch_selection_action(action);
                    }
                }
            }
        }
    }

    pub(super) fn dispatch_selection_action(&mut self, action: SelectionAction) {
        let closes_view = action.closes_view();
        self.data
            .target
            .dispatch_selection_action(&self.app_event_tx, &action);
        if closes_view {
            self.send_closed(true);
        }
    }

    pub(super) fn send_closed(&mut self, accepted: bool) {
        if self.is_complete {
            return;
        }
        self.app_event_tx.send(AppEvent::ModelSelectionClosed {
            target: self.data.target.into(),
            accepted,
        });
        self.is_complete = true;
    }

    pub(super) fn current_context_window_label(&self) -> String {
        self.data
            .current
            .current_context_window
            .map(format_with_separators_u64)
            .map(|value| format!("{value} tokens"))
            .unwrap_or_else(|| "default".to_string())
    }

    pub(super) fn current_auto_compact_label(&self) -> String {
        self.data
            .current
            .current_auto_compact_token_limit
            .and_then(|value| u64::try_from(value).ok())
            .map(format_with_separators_u64)
            .map(|value| format!("{value} tokens"))
            .unwrap_or_else(|| "auto".to_string())
    }

    pub(super) fn open_edit_for(&mut self, target: EditTarget, clear_existing: bool) {
        let mut field = crate::components::form_text_field::FormTextField::new_single_line();
        field.set_placeholder(match target {
            EditTarget::ContextWindow => "auto or 500k",
            EditTarget::AutoCompact => "auto, 450k, or 90%",
        });
        if !clear_existing {
            match target {
                EditTarget::ContextWindow => {
                    if let Some(value) = self.data.current.current_context_window {
                        field.set_text(&value.to_string());
                    }
                }
                EditTarget::AutoCompact => {
                    if let Some(value) = self.data.current.current_auto_compact_token_limit {
                        field.set_text(&value.to_string());
                    }
                }
            }
        }
        self.mode = ViewMode::Edit {
            target,
            field,
            error: None,
        };
    }

    fn parse_token_count_arg(raw: &str) -> Result<u64, String> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err("Enter a token count".to_string());
        }

        let lowered = trimmed.to_ascii_lowercase();
        let (number_part, multiplier) = if let Some(number) = lowered.strip_suffix('k') {
            (number, 1_000_u64)
        } else if let Some(number) = lowered.strip_suffix('m') {
            (number, 1_000_000_u64)
        } else {
            (lowered.as_str(), 1_u64)
        };

        let digits = number_part.replace(['_', ','], "");
        if digits.is_empty() {
            return Err("Enter a token count".to_string());
        }

        let parsed = digits
            .parse::<u64>()
            .map_err(|_| "Token count must be an integer like 500000, 500k, or 1m".to_string())?;
        let value = parsed
            .checked_mul(multiplier)
            .ok_or_else(|| "Token count is too large".to_string())?;
        if value == 0 {
            return Err("Token count must be greater than zero".to_string());
        }
        Ok(value)
    }

    fn parse_ratio_arg(raw: &str) -> Result<(u64, u64), String> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err("Enter a ratio like 0.9 or 9/10".to_string());
        }

        if let Some((lhs, rhs)) = trimmed.split_once('/') {
            let numerator = lhs
                .trim()
                .replace(['_', ','], "")
                .parse::<u64>()
                .map_err(|_| "Ratio must use integers like 9/10".to_string())?;
            let denominator = rhs
                .trim()
                .replace(['_', ','], "")
                .parse::<u64>()
                .map_err(|_| "Ratio must use integers like 9/10".to_string())?;
            if denominator == 0 {
                return Err("Ratio denominator must be greater than zero".to_string());
            }
            if numerator == 0 {
                return Err("Ratio must be greater than zero".to_string());
            }
            if numerator > denominator {
                return Err("Ratio must be between 0 and 1".to_string());
            }
            return Ok((numerator, denominator));
        }

        // Decimal ratio: "0.9", ".9", "1.0"
        if let Some((whole, frac)) = trimmed.split_once('.') {
            let whole = whole.trim();
            let frac = frac.trim();
            if whole.is_empty() && frac.is_empty() {
                return Err("Enter a ratio like 0.9".to_string());
            }
            let whole_digits = if whole.is_empty() { "0" } else { whole };
            if !whole_digits.chars().all(|c| c.is_ascii_digit())
                || !frac.chars().all(|c| c.is_ascii_digit())
            {
                return Err("Ratio must look like 0.9 or 9/10".to_string());
            }
            let whole_value = whole_digits
                .parse::<u64>()
                .map_err(|_| "Ratio must look like 0.9".to_string())?;
            if whole_value > 1 {
                return Err("Ratio must be between 0 and 1".to_string());
            }

            let denom = 10_u64
                .checked_pow(frac.len() as u32)
                .ok_or_else(|| "Ratio has too many decimal places".to_string())?;
            let frac_value = if frac.is_empty() {
                0
            } else {
                frac.parse::<u64>()
                    .map_err(|_| "Ratio must look like 0.9".to_string())?
            };

            let numerator = whole_value
                .checked_mul(denom)
                .and_then(|base| base.checked_add(frac_value))
                .ok_or_else(|| "Ratio is too large".to_string())?;
            if numerator == 0 {
                return Err("Ratio must be greater than zero".to_string());
            }
            if numerator > denom {
                return Err("Ratio must be between 0 and 1".to_string());
            }
            return Ok((numerator, denom));
        }

        Err("Enter a ratio like 0.9 or 9/10".to_string())
    }

    fn parse_auto_compact_token_limit_arg(
        raw: &str,
        context_window: Option<u64>,
    ) -> Result<u64, String> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err("Enter a token count".to_string());
        }

        let lowered = trimmed.to_ascii_lowercase();
        if let Some(percent_text) = lowered.strip_suffix('%') {
            let Some(context_window) = context_window else {
                return Err("Set a context window before using a percentage".to_string());
            };
            let digits = percent_text.trim().replace(['_', ','], "");
            if digits.is_empty() {
                return Err("Enter a percentage like 90%".to_string());
            }
            let percent = digits
                .parse::<u64>()
                .map_err(|_| "Percentage must be an integer like 90%".to_string())?;
            if percent == 0 {
                return Err("Percentage must be greater than zero".to_string());
            }
            if percent > 100 {
                return Err("Percentage must be between 1% and 100%".to_string());
            }
            let computed = ((context_window as u128) * (percent as u128)) / 100_u128;
            let computed = u64::try_from(computed).unwrap_or(u64::MAX);
            if computed == 0 {
                return Err("Percentage results in a token limit of zero".to_string());
            }
            return Ok(computed);
        }

        if lowered.contains('/') || lowered.contains('.') {
            let Some(context_window) = context_window else {
                return Err("Set a context window before using a ratio".to_string());
            };
            let (numerator, denominator) = Self::parse_ratio_arg(&lowered)?;
            let computed = ((context_window as u128) * (numerator as u128)) / (denominator as u128);
            let computed = u64::try_from(computed).unwrap_or(u64::MAX);
            if computed == 0 {
                return Err("Ratio results in a token limit of zero".to_string());
            }
            return Ok(computed);
        }

        Self::parse_token_count_arg(trimmed)
    }

    fn default_requested_auto_compact(&self) -> Option<i64> {
        let Some(context_window) = self.data.current.current_context_window else {
            return None;
        };
        let Some(current_auto_compact_token_limit) = self.data.current.current_auto_compact_token_limit else {
            return None;
        };
        if current_auto_compact_token_limit
            == default_auto_compact_limit_for_context_window(context_window)
        {
            None
        } else {
            Some(current_auto_compact_token_limit)
        }
    }

    fn apply_context_settings(
        &mut self,
        context_mode: Option<ContextMode>,
        requested_context_window: Option<u64>,
        requested_auto_compact_token_limit: Option<i64>,
    ) -> bool {
        let family = derive_default_model_family(&self.data.current.current_model);
        let (next_context_window, next_auto_compact_token_limit) = resolve_context_settings(
            &self.data.current.current_model,
            context_mode,
            requested_context_window,
            requested_auto_compact_token_limit,
            &family,
        );

        if self.data.current.current_context_mode == context_mode
            && self.data.current.current_context_window == next_context_window
            && self.data.current.current_auto_compact_token_limit == next_auto_compact_token_limit
        {
            return false;
        }

        self.data.current.current_context_mode = context_mode;
        self.data.current.current_context_window = next_context_window;
        self.data.current.current_auto_compact_token_limit = next_auto_compact_token_limit;
        self.app_event_tx
            .send(AppEvent::UpdateSessionContextSettingsSelection {
                context_mode,
                context_window: requested_context_window,
                auto_compact_token_limit: requested_auto_compact_token_limit,
            });
        true
    }

    pub(super) fn adjust_selected_numeric_value(&mut self, delta: i64) -> bool {
        let Some(entry) = self.data.entry_at(self.selected_index) else {
            return false;
        };
        const STEP: u64 = 25_000;

        match entry {
            EntryKind::ContextWindow => {
                let current = self.data.current.current_context_window.unwrap_or(STEP);
                let next = if delta.is_negative() {
                    current.saturating_sub(STEP).max(1)
                } else {
                    current.saturating_add(STEP)
                };
                self.apply_context_settings(
                    self.data.current.current_context_mode,
                    Some(next),
                    self.default_requested_auto_compact(),
                )
            }
            EntryKind::AutoCompact => {
                let current = self
                    .data
                    .current
                    .current_auto_compact_token_limit
                    .and_then(|value| u64::try_from(value).ok())
                    .unwrap_or_else(|| {
                        self.data
                            .current
                            .current_context_window
                            .map(|window| {
                                default_auto_compact_limit_for_context_window(window) as u64
                            })
                            .unwrap_or(STEP)
                    });
                let next = if delta.is_negative() {
                    current.saturating_sub(STEP).max(1)
                } else {
                    current.saturating_add(STEP)
                };
                self.apply_context_settings(
                    self.data.current.current_context_mode,
                    self.data.current.current_context_window,
                    i64::try_from(next).ok(),
                )
            }
            _ => false,
        }
    }

    pub(super) fn save_edit_value(&mut self, target: EditTarget, text: &str) -> Result<(), String> {
        let trimmed = text.trim();
        match target {
            EditTarget::ContextWindow => {
                if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("auto") {
                    if self.apply_context_settings(
                        self.data.current.current_context_mode,
                        None,
                        None,
                    ) {
                        return Ok(());
                    }
                    return Err("Context settings unchanged".to_string());
                }

                let parsed = Self::parse_token_count_arg(trimmed)?;
                if self.apply_context_settings(
                    self.data.current.current_context_mode,
                    Some(parsed),
                    self.default_requested_auto_compact(),
                ) {
                    Ok(())
                } else {
                    Err("Context settings unchanged".to_string())
                }
            }
            EditTarget::AutoCompact => {
                if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("auto") {
                    if self.apply_context_settings(
                        self.data.current.current_context_mode,
                        self.data.current.current_context_window,
                        None,
                    ) {
                        return Ok(());
                    }
                    return Err("Context settings unchanged".to_string());
                }

                let parsed = Self::parse_auto_compact_token_limit_arg(
                    trimmed,
                    self.data.current.current_context_window,
                )?;
                if self.apply_context_settings(
                    self.data.current.current_context_mode,
                    self.data.current.current_context_window,
                    i64::try_from(parsed).ok(),
                ) {
                    Ok(())
                } else {
                    Err("Context settings unchanged".to_string())
                }
            }
        }
    }
}

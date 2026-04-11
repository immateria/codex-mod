use super::*;

mod spinner;
mod theme;

impl ThemeSelectionView {
    fn append_delta_with_line_wrap(
        current: &mut String,
        history: &mut Vec<String>,
        delta: &str,
    ) {
        current.push_str(delta);
        if let Some(pos) = current.rfind('\n') {
            let (complete, remainder) = current.split_at(pos);
            if !complete.trim().is_empty() {
                history.push(complete.trim().to_owned());
            }
            *current = remainder.trim_start_matches('\n').to_owned();
            let keep = 10usize;
            let len = history.len();
            if len > keep {
                history.drain(0..len - keep);
            }
        }
    }

    fn latest_progress_line(current: &str, history: &[String]) -> String {
        if current.trim().is_empty() {
            history
                .iter()
                .rev()
                .find(|line| !line.trim().is_empty())
                .cloned()
                .unwrap_or_else(|| "Waiting for model…".to_owned())
        } else {
            current.trim().to_owned()
        }
    }

    pub(super) fn render_create_spinner_mode(
        &self,
        body_area: Rect,
        theme: &crate::theme::Theme,
        buf: &mut Buffer,
    ) {
        spinner::render_create_spinner_mode_inner(self, body_area, theme, buf);
    }

    pub(super) fn render_create_theme_mode(
        &self,
        body_area: Rect,
        theme: &crate::theme::Theme,
        buf: &mut Buffer,
    ) {
        theme::render_create_theme_mode_inner(self, body_area, theme, buf);
    }
}

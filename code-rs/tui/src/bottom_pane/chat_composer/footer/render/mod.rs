use super::*;
use unicode_width::UnicodeWidthStr;

mod fit;
mod override_hint;
mod sections;
mod truncate;

const FOOTER_TRAILING_PAD: usize = 1;

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SectionPriority {
    CtrlCQuit = 2,
    AutoReview = 3,
    Editor = 4,
    FooterHint = 5,
    AccessMode = 6,
    RightOther = 7,
}

#[derive(Clone, Debug)]
struct FooterSection {
    priority: SectionPriority,
    spans: Vec<Span<'static>>,
    enabled: bool,
}

fn span_width(spans: &[Span<'static>]) -> usize {
    spans.iter()
        .map(|span| UnicodeWidthStr::width(span.content.as_ref()))
        .sum()
}

fn spans_start_with_bullet(spans: &[Span<'static>]) -> bool {
    spans.iter()
        .find_map(|span| {
            let trimmed = span.content.trim_start();
            (!trimmed.is_empty()).then(|| trimmed.starts_with('•'))
        })
        .unwrap_or(false)
}

impl ChatComposer {
    pub(crate) fn footer_height(&self) -> u16 {
        if self.render_mode == ComposerRenderMode::FooterOnly {
            // Footer-only mode is used for terminal mode/status-only rendering.
            // Suppress popups defensively even if they're still marked active.
            return if self.standard_terminal_hint.is_some() { 1 } else { 0 };
        }

        match (&self.active_popup, self.embedded_mode) {
            (ActivePopup::Command(popup), _) => popup.calculate_required_height(),
            (ActivePopup::File(popup), _) => popup.calculate_required_height(),
            (ActivePopup::None, true) => 0,
            (ActivePopup::None, false) => 1,
        }
    }

    pub(crate) fn render_footer(&self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        // Footer-only mode intentionally suppresses popups (terminal/status-only view),
        // so treat popups as inactive for rendering purposes.
        if self.render_mode != ComposerRenderMode::FooterOnly {
            match &self.active_popup {
                ActivePopup::Command(popup) => {
                    popup.render_ref(area, buf);
                    return;
                }
                ActivePopup::File(popup) => {
                    popup.render_ref(area, buf);
                    return;
                }
                ActivePopup::None => {}
            }
        }

        if self.render_mode == ComposerRenderMode::FooterOnly && self.standard_terminal_hint.is_none()
        {
            return;
        }

        if self.embedded_mode {
            return;
        }

        let now = Instant::now();

        let key_hint_style = Style::default().fg(crate::colors::function());
        let label_style = Style::default().fg(crate::colors::text_dim());

        if override_hint::render_footer_hint_override(self, area, buf, key_hint_style, label_style) {
            return;
        }

        let built = sections::build_sections(self, now, key_hint_style, label_style);
        let total_width = area.width as usize;
        let (left_spans, right_spans, left_len, right_len) =
            fit::fit_sections_to_width(total_width, label_style, &built);
        let (left_spans, left_len) =
            truncate::truncate_left_if_needed(total_width, right_len, left_spans, left_len);

        let spacer = if total_width > left_len + right_len + FOOTER_TRAILING_PAD {
            " ".repeat(total_width - left_len - right_len - FOOTER_TRAILING_PAD)
        } else {
            String::from(" ")
        };

        let mut line_spans = left_spans;
        line_spans.push(Span::from(spacer));
        line_spans.extend(right_spans);
        line_spans.push(Span::from(" "));

        Line::from(line_spans)
            .style(
                Style::default()
                    .fg(crate::colors::text_dim())
                    .add_modifier(Modifier::DIM),
            )
            .render_ref(area, buf);
    }
}

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

use crate::colors;

use super::super::{AutoActiveViewModel, AutoCoordinatorView};
use super::{ButtonContext, VariantContext};

pub(super) fn build_context(model: &AutoActiveViewModel) -> VariantContext {
    let button = model.button.as_ref().map(|btn| ButtonContext {
        label: btn.label.clone(),
        enabled: btn.enabled,
    });
    VariantContext {
        button,
        ctrl_hint: model.ctrl_switch_hint.clone(),
        manual_hint: model.manual_hint.clone(),
    }
}

pub(super) fn manual_hint_line(ctx: &VariantContext) -> Option<Line<'static>> {
    ctx.manual_hint.as_ref().map(|hint| {
        Line::from(Span::styled(
            hint.clone(),
            Style::default()
                .fg(colors::info())
                .add_modifier(Modifier::ITALIC),
        ))
    })
}

pub(super) fn button_block_lines(
    view: &AutoCoordinatorView,
    ctx: &VariantContext,
) -> Option<Vec<Line<'static>>> {
    let button = ctx.button.as_ref()?;
    let label = button.label.trim();
    if label.is_empty() {
        return None;
    }

    let glyphs = view.style.button.glyphs;
    let inner = format!(" {label} ");
    let inner_width = UnicodeWidthStr::width(inner.as_str());
    let horizontal = glyphs.horizontal.to_string().repeat(inner_width);
    let top = format!("{}{}{}", glyphs.top_left, horizontal, glyphs.top_right);
    let middle = format!("{}{}{}", glyphs.vertical, inner, glyphs.vertical);
    let bottom = format!(
        "{}{}{}",
        glyphs.bottom_left, horizontal, glyphs.bottom_right
    );

    let button_style = if button.enabled {
        view.style.button.enabled_style
    } else {
        view.style.button.disabled_style
    };

    let mut lines = Vec::with_capacity(3);
    lines.push(Line::from(Span::styled(top, button_style)));

    let mut middle_spans: Vec<Span<'static>> = vec![Span::styled(middle, button_style)];
    if let Some(mut hint_spans) = ctrl_hint_spans(ctx.ctrl_hint.as_str())
        && !hint_spans.is_empty()
    {
        middle_spans.push(Span::raw("   "));
        middle_spans.append(&mut hint_spans);
    }
    lines.push(Line::from(middle_spans));

    lines.push(Line::from(Span::styled(bottom, button_style)));
    Some(lines)
}

pub(super) fn ctrl_hint_spans(hint: &str) -> Option<Vec<Span<'static>>> {
    let trimmed = hint.trim();
    if trimmed.is_empty() {
        return None;
    }

    let normal_style = Style::default().fg(colors::text());
    let bold_style = Style::default()
        .fg(colors::text())
        .add_modifier(Modifier::BOLD);

    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("esc") {
        let rest = &trimmed[3..];
        let mut use_prefix = rest.is_empty();
        if let Some(ch) = rest.chars().next()
            && (ch.is_whitespace() || matches!(ch, ':' | '-' | ',' | ';'))
        {
            use_prefix = true;
        }

        if use_prefix {
            let prefix = &trimmed[..3];
            let mut spans = Vec::new();
            spans.push(Span::styled(prefix.to_string(), bold_style));
            if !rest.is_empty() {
                spans.push(Span::styled(rest.to_string(), normal_style));
            }
            return Some(spans);
        }
    }

    Some(vec![Span::styled(trimmed.to_string(), normal_style)])
}

pub(super) fn ctrl_hint_line(ctx: &VariantContext) -> Option<Line<'static>> {
    if ctx.button.is_some() {
        return None;
    }
    ctrl_hint_spans(ctx.ctrl_hint.as_str()).map(Line::from)
}


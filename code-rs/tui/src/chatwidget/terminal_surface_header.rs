use super::*;
use code_core::config_types::HeaderHoverStyle;
use ratatui::style::Modifier;
use ratatui::style::Style;
use ratatui::text::Line;
use std::ops::Range;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum McpHeaderIndicatorKind {
    Connecting,
    Error,
}

pub(super) struct HeaderTemplateContext<'a> {
    pub title: &'a str,
    pub model: &'a str,
    pub shell: &'a str,
    pub reasoning: &'a str,
    pub directory: &'a str,
    pub branch: &'a str,
    pub mcp: &'a str,
    pub mcp_kind: Option<McpHeaderIndicatorKind>,
    pub hovered_action: Option<ClickableAction>,
    pub hover_style: HeaderHoverStyle,
}

pub(super) struct HeaderTemplateRender {
    pub line: Line<'static>,
    pub clickable_ranges: Vec<(Range<usize>, ClickableAction)>,
    pub width: usize,
}

pub(super) struct DynamicHeaderLayoutInput<'a> {
    pub title: &'a str,
    pub model: &'a str,
    pub shell: &'a str,
    pub reasoning: &'a str,
    pub directory_full: &'a str,
    pub directory_short: &'a str,
    pub branch: Option<&'a str>,
    pub mcp_indicator: Option<(McpHeaderIndicatorKind, &'a str)>,
    pub hovered_action: Option<ClickableAction>,
    pub hover_style: HeaderHoverStyle,
    pub minimal_header: bool,
    pub demo_mode: bool,
    pub inner_width: usize,
}

mod click_regions;
mod layout;
mod template;

pub(super) fn centered_clickable_regions_from_char_ranges(
    ranges: &[(Range<usize>, ClickableAction)],
    area: Rect,
    total_width: usize,
) -> Vec<ClickableRegion> {
    click_regions::centered_clickable_regions_from_char_ranges(ranges, area, total_width)
}

pub(super) fn render_dynamic_header_line(
    input: &DynamicHeaderLayoutInput<'_>,
) -> HeaderTemplateRender {
    layout::render_dynamic_header_line(input)
}

pub(super) fn render_plain_header_template(
    template: &str,
    context: &HeaderTemplateContext<'_>,
) -> String {
    template::render_plain_header_template(template, context)
}

pub(super) fn render_styled_header_template(
    template: &str,
    context: &HeaderTemplateContext<'_>,
) -> HeaderTemplateRender {
    template::render_styled_header_template(template, context)
}

pub(super) fn apply_hover_style(
    base: Style,
    hover_style: HeaderHoverStyle,
    is_hovered: bool,
) -> Style {
    if !is_hovered {
        return base;
    }

    match hover_style {
        HeaderHoverStyle::Background => base
            .bg(crate::colors::selection())
            .add_modifier(Modifier::BOLD)
            .add_modifier(Modifier::UNDERLINED),
        HeaderHoverStyle::Underline => base.add_modifier(Modifier::UNDERLINED),
        HeaderHoverStyle::None => base,
    }
}

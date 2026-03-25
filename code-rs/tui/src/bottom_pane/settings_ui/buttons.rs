use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};
use unicode_width::UnicodeWidthStr;

use crate::colors;

use super::layout::DEFAULT_BUTTON_GAP;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextButtonAlign {
    End,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SettingsButtonKind {
    Save,
    Delete,
    Cancel,
    Back,
    Apply,
    Install,
    Uninstall,
    Enable,
    Disable,
    Generate,
    GenerateDraft,
    Pick,
    Show,
    Resolve,
    Style,
}

impl SettingsButtonKind {
    fn label(self) -> &'static str {
        match self {
            Self::Save => "Save",
            Self::Delete => "Delete",
            Self::Cancel => "Cancel",
            Self::Back => "Back",
            Self::Apply => "Apply",
            Self::Install => "Install",
            Self::Uninstall => "Uninstall",
            Self::Enable => "Enable",
            Self::Disable => "Disable",
            Self::Generate => "Generate",
            Self::GenerateDraft => "Generate draft",
            Self::Pick => "Pick",
            Self::Show => "Show",
            Self::Resolve => "Resolve",
            Self::Style => "Style",
        }
    }

    fn style(self) -> Style {
        match self {
            Self::Save => Style::new().fg(colors::success()).bold(),
            Self::Delete => Style::new().fg(colors::error()).bold(),
            Self::Cancel | Self::Back => Style::new().fg(colors::text_dim()).bold(),
            Self::Apply => Style::new().fg(colors::success()).bold(),
            Self::Install => Style::new().fg(colors::success()).bold(),
            Self::Uninstall => Style::new().fg(colors::error()).bold(),
            Self::Enable => Style::new().fg(colors::success()).bold(),
            Self::Disable => Style::new().fg(colors::warning()).bold(),
            Self::Generate | Self::GenerateDraft => Style::new().fg(colors::function()).bold(),
            Self::Pick | Self::Show => Style::new().fg(colors::primary()).bold(),
            Self::Resolve => Style::new().fg(colors::function()).bold(),
            Self::Style => Style::new().fg(colors::primary()).bold(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct StandardButtonSpec<Id> {
    pub(crate) id: Id,
    pub(crate) kind: SettingsButtonKind,
    pub(crate) focused: bool,
    pub(crate) hovered: bool,
}

impl<Id> StandardButtonSpec<Id> {
    pub(crate) fn new(
        id: Id,
        kind: SettingsButtonKind,
        focused: bool,
        hovered: bool,
    ) -> Self {
        Self {
            id,
            kind,
            focused,
            hovered,
        }
    }
}

pub(crate) fn standard_button_specs<Id: Copy + PartialEq>(
    items: &[(Id, SettingsButtonKind)],
    focused: Option<Id>,
    hovered: Option<Id>,
) -> Vec<StandardButtonSpec<Id>> {
    items.iter()
        .map(|(id, kind)| StandardButtonSpec::new(
            *id,
            *kind,
            focused == Some(*id),
            hovered == Some(*id),
        ))
        .collect()
}

fn standard_button_width<Id>(button: &StandardButtonSpec<Id>) -> u16 {
    u16::try_from(button.kind.label().width()).unwrap_or(u16::MAX)
}

fn gap_width() -> u16 {
    u16::try_from(DEFAULT_BUTTON_GAP.width()).unwrap_or(u16::MAX)
}

fn standard_button_layouts<'a, Id>(
    origin_x: u16,
    buttons: &'a [StandardButtonSpec<Id>],
) -> impl Iterator<Item = (u16, u16, &'a StandardButtonSpec<Id>)> + 'a {
    let mut cursor_x = origin_x;
    buttons.iter().enumerate().map(move |(index, button)| {
        let x = cursor_x;
        let width = standard_button_width(button);
        cursor_x = cursor_x.saturating_add(width);
        if index + 1 < buttons.len() {
            cursor_x = cursor_x.saturating_add(gap_width());
        }
        (x, width, button)
    })
}

pub(crate) fn standard_button_strip_width<Id>(buttons: &[StandardButtonSpec<Id>]) -> u16 {
    standard_button_layouts(0, buttons)
        .last()
        .map(|(x, width, _)| x.saturating_add(width))
        .unwrap_or(0)
}

pub(crate) fn aligned_standard_button_strip_rect<Id>(
    row: Rect,
    buttons: &[StandardButtonSpec<Id>],
    align: TextButtonAlign,
) -> Rect {
    let width = standard_button_strip_width(buttons).min(row.width);
    let x = match align {
        TextButtonAlign::End => row.x.saturating_add(row.width.saturating_sub(width)),
    };
    Rect::new(x, row.y, width, row.height)
}

pub(crate) fn render_standard_button_strip_aligned<Id>(
    row: Rect,
    buf: &mut Buffer,
    buttons: &[StandardButtonSpec<Id>],
    align: TextButtonAlign,
) {
    let area = aligned_standard_button_strip_rect(row, buttons, align);
    let mut spans = Vec::new();
    for (index, button) in buttons.iter().enumerate() {
        if index > 0 {
            spans.push(Span::raw(DEFAULT_BUTTON_GAP));
        }
        let base_style = button.kind.style();
        let span_style = if button.focused {
            base_style.bg(colors::primary()).fg(colors::background())
        } else if button.hovered {
            base_style.bg(colors::border()).fg(colors::text()).bold()
        } else {
            base_style
        };
        spans.push(Span::styled(button.kind.label(), span_style));
    }
    Paragraph::new(Line::from(spans)).render(area, buf);
}

pub(crate) fn render_standard_button_strip<Id>(
    area: Rect,
    buf: &mut Buffer,
    buttons: &[StandardButtonSpec<Id>],
) {
    let mut spans = Vec::new();
    for (index, button) in buttons.iter().enumerate() {
        if index > 0 {
            spans.push(Span::raw(DEFAULT_BUTTON_GAP));
        }
        let base_style = button.kind.style();
        let span_style = if button.focused {
            base_style.bg(colors::primary()).fg(colors::background())
        } else if button.hovered {
            base_style.bg(colors::border()).fg(colors::text()).bold()
        } else {
            base_style
        };
        spans.push(Span::styled(button.kind.label(), span_style));
    }
    Paragraph::new(Line::from(spans)).render(area, buf);
}

pub(crate) fn standard_button_at<Id: Copy>(
    x: u16,
    y: u16,
    row: Rect,
    buttons: &[StandardButtonSpec<Id>],
) -> Option<Id> {
    if !row.contains(Position { x, y }) {
        return None;
    }
    for (button_x, button_width, button) in standard_button_layouts(row.x, buttons) {
        if x >= button_x && x < button_x.saturating_add(button_width) {
            return Some(button.id);
        }
    }
    None
}

pub(crate) fn standard_button_at_aligned<Id: Copy>(
    x: u16,
    y: u16,
    row: Rect,
    buttons: &[StandardButtonSpec<Id>],
    align: TextButtonAlign,
) -> Option<Id> {
    let area = aligned_standard_button_strip_rect(row, buttons, align);
    if !area.contains(Position { x, y }) {
        return None;
    }
    for (button_x, button_width, button) in standard_button_layouts(area.x, buttons) {
        if x >= button_x && x < button_x.saturating_add(button_width) {
            return Some(button.id);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_button_hit_testing_uses_shared_gap_width() {
        let row = Rect::new(10, 4, 40, 1);
        let items = [
            (10usize, SettingsButtonKind::Save),
            (20usize, SettingsButtonKind::Cancel),
        ];
        let buttons = standard_button_specs(&items, None, None);
        let save_width = u16::try_from("Save".width()).unwrap_or(u16::MAX);
        let gap_width = gap_width();
        assert_eq!(standard_button_at(10, 4, row, &buttons), Some(10));
        assert_eq!(
            standard_button_at(10 + save_width + gap_width, 4, row, &buttons),
            Some(20)
        );
        assert_eq!(standard_button_at(10 + save_width, 4, row, &buttons), None);
        assert_eq!(
            standard_button_strip_width(&buttons),
            save_width + gap_width + 6
        );
    }
}

use std::borrow::Cow;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};

use crate::bottom_pane::chrome::ChromeMode;
use crate::colors;
use crate::live_wrap::RowBuilder;
use crate::ui_interaction::split_header_body_footer;
use crate::util::buffer::{fill_rect, write_line};

use super::line_runs::{
    render_selectable_runs,
    SelectableLineRun,
};
use super::menu_rows::{
    render_menu_rows,
    render_menu_rows_compact,
    selection_id_at as selection_menu_id_at,
    SettingsMenuRow,
};
use super::panel::SettingsPanelStyle;
use super::sectioned_panel::{SettingsSectionedPanel, SettingsSectionedPanelLayout};

#[derive(Clone, Debug)]
pub(crate) struct SettingsMenuPage<'a> {
    panel: SettingsSectionedPanel<'a>,
    header_lines: Vec<Line<'static>>,
    footer_lines: Vec<Line<'static>>,
    render_detail_pane: bool,
}

pub(crate) struct SettingsMenuPageFramed<'p, 'a> {
    page: &'p SettingsMenuPage<'a>,
}

pub(crate) struct SettingsMenuPageContentOnly<'p, 'a> {
    page: &'p SettingsMenuPage<'a>,
}

impl<'a> SettingsMenuPage<'a> {
    pub(crate) fn new(
        title: impl Into<Cow<'a, str>>,
        style: SettingsPanelStyle,
        header_lines: Vec<Line<'static>>,
        footer_lines: Vec<Line<'static>>,
    ) -> Self {
        let panel = SettingsSectionedPanel::new(
            title,
            style,
            header_lines.len(),
            footer_lines.len(),
        )
        .with_min_body_rows(2);

        Self {
            panel,
            header_lines,
            footer_lines,
            render_detail_pane: false,
        }
    }

    pub(crate) fn with_detail_pane(mut self) -> Self {
        self.render_detail_pane = true;
        self
    }

    pub(crate) fn framed(&self) -> SettingsMenuPageFramed<'_, 'a> {
        SettingsMenuPageFramed { page: self }
    }

    pub(crate) fn content_only(&self) -> SettingsMenuPageContentOnly<'_, 'a> {
        SettingsMenuPageContentOnly { page: self }
    }

    fn layout_framed(&self, area: Rect) -> Option<SettingsSectionedPanelLayout> {
        self.panel.layout(area)
    }

    fn layout_content(&self, area: Rect) -> Option<SettingsSectionedPanelLayout> {
        split_header_body_footer(area, self.header_lines.len(), self.footer_lines.len(), 1)
            .map(Into::into)
    }

    fn render_shell_framed(
        &self,
        area: Rect,
        buf: &mut Buffer,
    ) -> Option<SettingsSectionedPanelLayout> {
        let layout = self.panel.render(area, buf)?;
        let base = Style::new().bg(colors::background()).fg(colors::text());
        render_lines(layout.header, buf, &self.header_lines, base);
        render_lines(layout.footer, buf, &self.footer_lines, base);
        Some(layout)
    }

    fn render_shell_content_only(
        &self,
        area: Rect,
        buf: &mut Buffer,
    ) -> Option<SettingsSectionedPanelLayout> {
        let layout = self.layout_content(area)?;
        let base = Style::new().bg(colors::background()).fg(colors::text());
        fill_rect(buf, area, Some(' '), base);
        render_lines(layout.header, buf, &self.header_lines, base);
        render_lines(layout.footer, buf, &self.footer_lines, base);
        Some(layout)
    }

    pub(crate) fn selection_menu_id_in_body<Id: Copy + PartialEq>(
        body: Rect,
        x: u16,
        y: u16,
        scroll_top: usize,
        rows: &[SettingsMenuRow<'_, Id>],
    ) -> Option<Id> {
        selection_menu_id_at(body, x, y, scroll_top, rows)
    }

    pub(crate) fn layout_in_chrome(
        &self,
        chrome: ChromeMode,
        area: Rect,
    ) -> Option<SettingsSectionedPanelLayout> {
        match chrome {
            ChromeMode::Framed => self.framed().layout(area),
            ChromeMode::ContentOnly => self.content_only().layout(area),
        }
    }

    pub(crate) fn render_shell_in_chrome(
        &self,
        chrome: ChromeMode,
        area: Rect,
        buf: &mut Buffer,
    ) -> Option<SettingsSectionedPanelLayout> {
        match chrome {
            ChromeMode::Framed => self.framed().render_shell(area, buf),
            ChromeMode::ContentOnly => self.content_only().render_shell(area, buf),
        }
    }

    pub(crate) fn render_menu_rows_in_chrome<Id: Copy + PartialEq>(
        &self,
        chrome: ChromeMode,
        area: Rect,
        buf: &mut Buffer,
        scroll_top: usize,
        selected_id: Option<Id>,
        rows: &[SettingsMenuRow<'_, Id>],
    ) -> Option<SettingsSectionedPanelLayout> {
        match chrome {
            ChromeMode::Framed => {
                self.framed()
                    .render_menu_rows(area, buf, scroll_top, selected_id, rows)
            }
            ChromeMode::ContentOnly => self
                .content_only()
                .render_menu_rows(area, buf, scroll_top, selected_id, rows),
        }
    }

    pub(crate) fn render_runs_in_chrome<Id: Copy>(
        &self,
        chrome: ChromeMode,
        area: Rect,
        buf: &mut Buffer,
        scroll_top: usize,
        runs: &[SelectableLineRun<'_, Id>],
    ) -> Option<SettingsSectionedPanelLayout> {
        match chrome {
            ChromeMode::Framed => self.framed().render_runs(area, buf, scroll_top, runs),
            ChromeMode::ContentOnly => self
                .content_only()
                .render_runs(area, buf, scroll_top, runs),
        }
    }

    pub(crate) fn menu_body_layout(&self, body: Rect) -> SettingsMenuBodyLayout {
        if !self.render_detail_pane || body.width < 4 || body.height == 0 {
            return SettingsMenuBodyLayout {
                list: body,
                divider: None,
                detail: None,
            };
        }

        const DIVIDER_COLS: u16 = 1;
        const MIN_LIST_COLS: u16 = 18;
        const MIN_DETAIL_COLS: u16 = 18;

        if body.width <= DIVIDER_COLS + 1 {
            return SettingsMenuBodyLayout {
                list: body,
                divider: None,
                detail: None,
            };
        }

        let total = body.width.saturating_sub(DIVIDER_COLS);
        if total <= MIN_LIST_COLS {
            return SettingsMenuBodyLayout {
                list: body,
                divider: None,
                detail: None,
            };
        }

        let mut list_cols = total.saturating_mul(2).saturating_div(5);
        let max_list_cols = total
            .saturating_sub(MIN_DETAIL_COLS)
            .max(MIN_LIST_COLS);
        list_cols = list_cols.clamp(MIN_LIST_COLS, max_list_cols);
        let detail_cols = total.saturating_sub(list_cols);
        if detail_cols == 0 {
            return SettingsMenuBodyLayout {
                list: body,
                divider: None,
                detail: None,
            };
        }

        let list = Rect::new(body.x, body.y, list_cols, body.height);
        let divider = Rect::new(body.x.saturating_add(list_cols), body.y, DIVIDER_COLS, body.height);
        let detail = Rect::new(
            divider.x.saturating_add(DIVIDER_COLS),
            body.y,
            detail_cols,
            body.height,
        );

        SettingsMenuBodyLayout {
            list,
            divider: Some(divider),
            detail: Some(detail),
        }
    }
}

impl<'p, 'a> SettingsMenuPageFramed<'p, 'a> {
    pub(crate) fn layout(&self, area: Rect) -> Option<SettingsSectionedPanelLayout> {
        self.page.layout_framed(area)
    }

    pub(crate) fn render_shell(
        &self,
        area: Rect,
        buf: &mut Buffer,
    ) -> Option<SettingsSectionedPanelLayout> {
        self.page.render_shell_framed(area, buf)
    }

    pub(crate) fn render_menu_rows<Id: Copy + PartialEq>(
        &self,
        area: Rect,
        buf: &mut Buffer,
        scroll_top: usize,
        selected_id: Option<Id>,
        rows: &[SettingsMenuRow<'_, Id>],
    ) -> Option<SettingsSectionedPanelLayout> {
        let layout = self.render_shell(area, buf)?;
        let base = Style::new().bg(colors::background()).fg(colors::text());
        let body_layout = self.page.menu_body_layout(layout.body);
        if let Some(detail) = body_layout.detail {
            render_menu_rows_compact(body_layout.list, buf, scroll_top, selected_id, rows, base);
            if let Some(divider) = body_layout.divider {
                fill_rect(buf, divider, Some('|'), base);
            }
            render_menu_detail_pane(detail, buf, selected_id, rows, base);
        } else {
            render_menu_rows(layout.body, buf, scroll_top, selected_id, rows, base);
        }
        Some(layout)
    }

    pub(crate) fn render_runs<Id: Copy>(
        &self,
        area: Rect,
        buf: &mut Buffer,
        scroll_top: usize,
        runs: &[SelectableLineRun<'_, Id>],
    ) -> Option<SettingsSectionedPanelLayout> {
        let layout = self.render_shell(area, buf)?;
        let base = Style::new().bg(colors::background()).fg(colors::text());
        render_selectable_runs(layout.body, buf, scroll_top, runs, base);
        Some(layout)
    }
}

impl<'p, 'a> SettingsMenuPageContentOnly<'p, 'a> {
    pub(crate) fn layout(&self, area: Rect) -> Option<SettingsSectionedPanelLayout> {
        self.page.layout_content(area)
    }

    pub(crate) fn render_shell(
        &self,
        area: Rect,
        buf: &mut Buffer,
    ) -> Option<SettingsSectionedPanelLayout> {
        self.page.render_shell_content_only(area, buf)
    }

    pub(crate) fn render_menu_rows<Id: Copy + PartialEq>(
        &self,
        area: Rect,
        buf: &mut Buffer,
        scroll_top: usize,
        selected_id: Option<Id>,
        rows: &[SettingsMenuRow<'_, Id>],
    ) -> Option<SettingsSectionedPanelLayout> {
        let layout = self.render_shell(area, buf)?;
        let base = Style::new().bg(colors::background()).fg(colors::text());
        let body_layout = self.page.menu_body_layout(layout.body);
        if let Some(detail) = body_layout.detail {
            render_menu_rows_compact(body_layout.list, buf, scroll_top, selected_id, rows, base);
            if let Some(divider) = body_layout.divider {
                fill_rect(buf, divider, Some('|'), base);
            }
            render_menu_detail_pane(detail, buf, selected_id, rows, base);
        } else {
            render_menu_rows(layout.body, buf, scroll_top, selected_id, rows, base);
        }
        Some(layout)
    }

    pub(crate) fn render_runs<Id: Copy>(
        &self,
        area: Rect,
        buf: &mut Buffer,
        scroll_top: usize,
        runs: &[SelectableLineRun<'_, Id>],
    ) -> Option<SettingsSectionedPanelLayout> {
        let layout = self.render_shell(area, buf)?;
        let base = Style::new().bg(colors::background()).fg(colors::text());
        render_selectable_runs(layout.body, buf, scroll_top, runs, base);
        Some(layout)
    }
}

fn render_lines(area: Rect, buf: &mut Buffer, lines: &[Line<'_>], base_style: Style) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    fill_rect(buf, area, Some(' '), base_style);
    for (idx, line) in lines.iter().enumerate().take(area.height as usize) {
        let y = area.y.saturating_add(idx as u16);
        write_line(buf, area.x, y, area.width, line, base_style);
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct SettingsMenuBodyLayout {
    pub(crate) list: Rect,
    pub(crate) divider: Option<Rect>,
    pub(crate) detail: Option<Rect>,
}

fn render_menu_detail_pane<Id: Copy + PartialEq>(
    area: Rect,
    buf: &mut Buffer,
    selected_id: Option<Id>,
    rows: &[SettingsMenuRow<'_, Id>],
    base_style: Style,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    fill_rect(buf, area, Some(' '), base_style);

    let Some(selected_id) = selected_id else {
        return;
    };
    let Some(row) = rows.iter().find(|row| row.id == selected_id) else {
        return;
    };

    let padding_x = 1u16;
    let origin_x = area.x.saturating_add(padding_x);
    let max_width = area.width.saturating_sub(padding_x);
    if max_width == 0 {
        return;
    }

    let mut y = area.y;
    let mut header_spans = vec![Span::styled(
        row.label.as_ref().to_string(),
        Style::new().fg(colors::text()).bold(),
    )];
    if let Some(value) = &row.value {
        header_spans.push(Span::raw("  "));
        header_spans.push(Span::styled(value.text.as_ref().to_string(), value.style));
    }
    write_line(
        buf,
        origin_x,
        y,
        max_width,
        &Line::from(header_spans),
        base_style,
    );
    y = y.saturating_add(2);
    if y >= area.y.saturating_add(area.height) {
        return;
    }

    if let Some(detail) = &row.detail {
        let wrap_width = usize::from(max_width.max(1));
        let mut builder = RowBuilder::new(wrap_width);
        builder.push_fragment(detail.text.as_ref());
        for wrapped in builder.display_rows() {
            if y >= area.y.saturating_add(area.height) {
                break;
            }
            write_line(
                buf,
                origin_x,
                y,
                max_width,
                &Line::from(Span::styled(wrapped.text, detail.style)),
                base_style,
            );
            y = y.saturating_add(1);
        }
    }

    if y < area.y.saturating_add(area.height)
        && let Some(hint) = row.selected_hint.as_deref()
    {
        write_line(
            buf,
            origin_x,
            y,
            max_width,
            &Line::from(Span::styled(
                hint.to_string(),
                Style::new().fg(colors::text_dim()),
            )),
            base_style,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::layout::Margin;

    fn buffer_lines(buf: &Buffer, area: Rect) -> Vec<String> {
        let mut out = Vec::new();
        for y in area.y..area.y.saturating_add(area.height) {
            let mut line = String::with_capacity(area.width as usize);
            for x in area.x..area.x.saturating_add(area.width) {
                let symbol = buf[(x, y)].symbol();
                line.push(symbol.chars().next().unwrap_or(' '));
            }
            out.push(line);
        }
        out
    }

    #[test]
    fn render_shell_and_layout_agree() {
        let page = SettingsMenuPage::new(
            "Test",
            SettingsPanelStyle::bottom_pane().with_margin(Margin::new(1, 0)),
            vec![Line::from("header")],
            vec![Line::from("footer")],
        );
        let area = Rect::new(0, 0, 30, 10);
        let framed = page.framed();
        let layout = framed.layout(area).expect("layout");
        let mut buf = Buffer::empty(area);
        let rendered = framed.render_shell(area, &mut buf).expect("render");
        assert_eq!(layout, rendered);
    }

    #[test]
    fn selection_menu_id_uses_body_rect() {
        let page = SettingsMenuPage::new(
            "Test",
            SettingsPanelStyle::bottom_pane().with_margin(Margin::new(1, 0)),
            vec![Line::from("header")],
            vec![Line::from("footer")],
        );
        let area = Rect::new(0, 0, 30, 10);
        let layout = page.framed().layout(area).expect("layout");
        let rows = vec![SettingsMenuRow::new(7usize, "row")];

        assert_eq!(
            SettingsMenuPage::selection_menu_id_in_body(
                layout.body,
                layout.body.x.saturating_add(2),
                layout.body.y,
                0,
                &rows,
            ),
            Some(7)
        );
        assert_eq!(
            SettingsMenuPage::selection_menu_id_in_body(
                layout.body,
                layout.header.x,
                layout.header.y,
                0,
                &rows,
            ),
            None
        );
    }

    #[test]
    fn shell_less_layout_matches_header_body_footer_math() {
        let page = SettingsMenuPage::new(
            "Test",
            SettingsPanelStyle::bottom_pane().with_margin(Margin::new(1, 0)),
            vec![Line::from("header")],
            vec![Line::from("footer")],
        );
        let area = Rect::new(0, 0, 30, 8);
        let layout = page.content_only().layout(area).expect("layout");

        assert_eq!(layout.header, Rect::new(0, 0, 30, 1));
        assert_eq!(layout.body, Rect::new(0, 1, 30, 6));
        assert_eq!(layout.footer, Rect::new(0, 7, 30, 1));
    }

    #[test]
    fn render_content_menu_rows_renders_without_outer_frame() {
        let page = SettingsMenuPage::new(
            "Test",
            SettingsPanelStyle::bottom_pane(),
            vec![Line::from("header")],
            vec![Line::from("footer")],
        );
        let rows = vec![SettingsMenuRow::new(1usize, "row")];
        let area = Rect::new(0, 0, 20, 4);
        let mut buf = Buffer::empty(area);

        let layout = page
            .content_only()
            .render_menu_rows(area, &mut buf, 0, Some(1usize), &rows)
            .expect("render");

        assert_eq!(layout.header, Rect::new(0, 0, 20, 1));
        assert_eq!(buf[(0, 0)].symbol(), "h");
        assert_eq!(buf[(0, 1)].symbol(), "›");
    }

    #[test]
    fn render_shell_in_chrome_matches_concrete_impls() {
        let page = SettingsMenuPage::new(
            "Test",
            SettingsPanelStyle::bottom_pane().with_margin(Margin::new(1, 0)),
            vec![Line::from("header")],
            vec![Line::from("footer")],
        );
        let area = Rect::new(0, 0, 30, 10);

        let mut framed_buf = Buffer::empty(area);
        let mut framed_expected_buf = Buffer::empty(area);
        assert_eq!(
            page.render_shell_in_chrome(ChromeMode::Framed, area, &mut framed_buf),
            page.framed().render_shell(area, &mut framed_expected_buf)
        );

        let mut content_buf = Buffer::empty(area);
        let mut content_expected_buf = Buffer::empty(area);
        assert_eq!(
            page.render_shell_in_chrome(ChromeMode::ContentOnly, area, &mut content_buf),
            page.content_only()
                .render_shell(area, &mut content_expected_buf)
        );
    }

    #[test]
    fn detail_pane_renders_selected_row_detail_wrapped() {
        let page = SettingsMenuPage::new(
            "Test",
            SettingsPanelStyle::bottom_pane(),
            vec![Line::from("header")],
            vec![Line::from("footer")],
        )
        .with_detail_pane();

        let rows = vec![SettingsMenuRow::new(1usize, "Feature")
            .with_detail(crate::bottom_pane::settings_ui::rows::StyledText::new(
                "This is a very long description that should be visible in the detail pane",
                Style::new().fg(colors::text_dim()),
            ))];

        let area = Rect::new(0, 0, 80, 8);
        let mut buf = Buffer::empty(area);
        let layout = page
            .content_only()
            .render_menu_rows(area, &mut buf, 0, Some(1usize), &rows)
            .expect("render");

        let body_layout = page.menu_body_layout(layout.body);
        let detail_rect = body_layout.detail.expect("detail pane");
        let list_rect = body_layout.list;

        let detail_text = buffer_lines(&buf, detail_rect).join("\n");
        assert!(detail_text.contains("very long description"));

        let list_text = buffer_lines(&buf, list_rect).join("\n");
        assert!(!list_text.contains("very long description"));
    }
}

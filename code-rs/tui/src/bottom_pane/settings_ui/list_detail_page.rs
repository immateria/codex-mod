use std::borrow::Cow;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::{Block, Widget};

use crate::colors;
use crate::ui_interaction::split_two_pane_when_room;

use super::panel::SettingsPanelStyle;
use super::sectioned_panel::{SettingsSectionedPanel, SettingsSectionedPanelLayout};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SettingsListDetailMode {
    Compact { content: Rect },
    Split {
        list_outer: Rect,
        list_inner: Rect,
        detail_outer: Rect,
        detail_inner: Rect,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SettingsListDetailLayout {
    pub(crate) header: Rect,
    pub(crate) footer: Rect,
    pub(crate) body: Rect,
    pub(crate) mode: SettingsListDetailMode,
}

#[derive(Clone, Debug)]
pub(crate) struct SettingsListDetailPage<'a> {
    panel: SettingsSectionedPanel<'a>,
    min_width: u16,
    min_height: u16,
    list_percent: u16,
    list_title: Cow<'a, str>,
    detail_title: Cow<'a, str>,
}

impl<'a> SettingsListDetailPage<'a> {
    pub(crate) fn new(
        title: impl Into<Cow<'a, str>>,
        style: SettingsPanelStyle,
        header_rows: usize,
        footer_rows: usize,
        min_width: u16,
        min_height: u16,
        list_percent: u16,
        list_title: impl Into<Cow<'a, str>>,
        detail_title: impl Into<Cow<'a, str>>,
    ) -> Self {
        Self {
            panel: SettingsSectionedPanel::new(title, style, header_rows, footer_rows)
                .with_min_body_rows(2),
            min_width,
            min_height,
            list_percent,
            list_title: list_title.into(),
            detail_title: detail_title.into(),
        }
    }

    fn list_block(&self) -> Block<'_> {
        Block::bordered()
            .border_style(Style::new().fg(colors::border()))
            .title(format!(" {} ", self.list_title))
    }

    fn detail_block(&self) -> Block<'_> {
        Block::bordered()
            .border_style(Style::new().fg(colors::border()))
            .title(format!(" {} ", self.detail_title))
    }

    fn mode_from_body(&self, body: Rect) -> SettingsListDetailMode {
        if let Some((list_outer, detail_outer)) = split_two_pane_when_room(
            body,
            self.min_width,
            self.min_height,
            self.list_percent,
        ) {
            let list_block = self.list_block();
            let detail_block = self.detail_block();
            SettingsListDetailMode::Split {
                list_inner: list_block.inner(list_outer),
                detail_inner: detail_block.inner(detail_outer),
                list_outer,
                detail_outer,
            }
        } else {
            SettingsListDetailMode::Compact { content: body }
        }
    }

    fn layout_from_panel(&self, layout: SettingsSectionedPanelLayout) -> SettingsListDetailLayout {
        SettingsListDetailLayout {
            header: layout.header,
            footer: layout.footer,
            body: layout.body,
            mode: self.mode_from_body(layout.body),
        }
    }

    pub(crate) fn layout(&self, area: Rect) -> Option<SettingsListDetailLayout> {
        let layout = self.panel.layout(area)?;
        Some(self.layout_from_panel(layout))
    }

    pub(crate) fn render(&self, area: Rect, buf: &mut Buffer) -> Option<SettingsListDetailLayout> {
        let layout = self.panel.render(area, buf)?;
        let layout = self.layout_from_panel(layout);
        if let SettingsListDetailMode::Split {
            list_outer,
            detail_outer,
            ..
        } = layout.mode
        {
            self.list_block().render(list_outer, buf);
            self.detail_block().render(detail_outer, buf);
        }
        Some(layout)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::layout::Margin;

    #[test]
    fn compact_and_split_switch_at_thresholds() {
        let page = SettingsListDetailPage::new(
            "Accounts",
            SettingsPanelStyle::bottom_pane().with_margin(Margin::new(1, 0)),
            1,
            1,
            30,
            8,
            40,
            "List",
            "Detail",
        );
        let compact = page.layout(Rect::new(0, 0, 20, 8)).expect("compact");
        assert!(matches!(compact.mode, SettingsListDetailMode::Compact { .. }));
        let split = page.layout(Rect::new(0, 0, 50, 12)).expect("split");
        assert!(matches!(split.mode, SettingsListDetailMode::Split { .. }));
    }

    #[test]
    fn render_and_layout_agree_in_split_mode() {
        let page = SettingsListDetailPage::new(
            "Accounts",
            SettingsPanelStyle::bottom_pane(),
            1,
            1,
            30,
            8,
            40,
            "List",
            "Detail",
        );
        let area = Rect::new(0, 0, 60, 12);
        let layout = page.layout(area).expect("layout");
        let mut buf = Buffer::empty(area);
        let rendered = page.render(area, &mut buf).expect("render");
        assert_eq!(layout, rendered);
    }
}

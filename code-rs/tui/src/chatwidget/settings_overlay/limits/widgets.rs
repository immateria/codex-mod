/// Build a [`ShortcutBar`] for the limits overlay hint row.
fn limits_shortcut_bar(
    has_tabs: bool,
    layout_mode: LimitsLayoutMode,
    pane_focus: LimitsPaneFocus,
) -> ShortcutBar {
    use crate::bottom_pane::settings_ui::hints::{hint_nav, hint_nav_horizontal};

    let mut hints: Vec<KeyHint<'static>> = vec![
        hint_nav("scroll"),
        KeyHint::new("V", format!("layout:{}", layout_mode.label())),
        KeyHint::new("F", format!("focus:{}", pane_focus.label())),
    ];
    if has_tabs {
        hints.extend([
            hint_nav_horizontal("tab"),
            KeyHint::new("S", "switch"),
            KeyHint::new("W", "warm all"),
            KeyHint::new("R", "refresh"),
        ]);
    }
    ShortcutBar::at(ShortcutPlacement::Bottom, hints)
        .with_overflow(OverflowMode::Wrap)
}

/// Visible window of tabs after overflow windowing.
struct TabWindow {
    /// Index of the first visible tab.
    start: usize,
    /// One past the last visible tab.
    end: usize,
    /// Whether tabs are hidden to the left.
    has_left_overflow: bool,
    /// Whether tabs are hidden to the right.
    has_right_overflow: bool,
}

/// Per-tab display width: " title " (padding + unicode width) + " " separator.
fn tab_display_width(tab: &LimitsTab) -> usize {
    UnicodeWidthStr::width(tab.title.as_str()) + 3
}

/// Compute which tabs are visible given the available width and selected tab.
fn compute_tab_window(tabs: &[LimitsTab], selected: usize, width: usize) -> TabWindow {
    let n = tabs.len();
    let selected = selected.min(n.saturating_sub(1));
    let tab_widths: Vec<usize> = tabs.iter().map(tab_display_width).collect();
    let total: usize = tab_widths.iter().sum();

    if total <= width || n == 0 {
        return TabWindow { start: 0, end: n, has_left_overflow: false, has_right_overflow: false };
    }

    const LEFT_IND: usize = 2; // tab_prev() + space
    const RIGHT_IND: usize = 2; // space + tab_next()

    let mut start = selected;
    let mut end = selected + 1;
    let mut used = tab_widths[selected];

    loop {
        let mut expanded = false;

        if end < n {
            let left_cost = if start > 0 { LEFT_IND } else { 0 };
            let right_cost = if end + 1 < n { RIGHT_IND } else { 0 };
            if used + tab_widths[end] + left_cost + right_cost <= width {
                used += tab_widths[end];
                end += 1;
                expanded = true;
            }
        }

        if start > 0 {
            let left_cost = if start - 1 > 0 { LEFT_IND } else { 0 };
            let right_cost = if end < n { RIGHT_IND } else { 0 };
            if used + tab_widths[start - 1] + left_cost + right_cost <= width {
                start -= 1;
                used += tab_widths[start];
                expanded = true;
            }
        }

        if !expanded {
            break;
        }
    }

    TabWindow {
        start,
        end,
        has_left_overflow: start > 0,
        has_right_overflow: end < n,
    }
}

struct LimitsTabsRowWidget<'a> {
    tabs: &'a [LimitsTab],
    selected_tab: usize,
}

/// Result of a click in the tab row area.
enum TabHit {
    /// Clicked on a specific tab (by global index).
    Tab(usize),
    /// Clicked the left overflow indicator — navigate to previous tab.
    PrevTab,
    /// Clicked the right overflow indicator — navigate to next tab.
    NextTab,
}

impl LimitsTabsRowWidget<'_> {
    /// Returns what was clicked at the given column within `tabs_area`,
    /// accounting for the same overflow windowing the renderer uses.
    fn hit_at(tabs: &[LimitsTab], selected: usize, tabs_area: Rect, col: u16) -> Option<TabHit> {
        if tabs.len() <= 1 || tabs_area.width == 0 {
            return None;
        }
        let window = compute_tab_window(tabs, selected, tabs_area.width as usize);
        let mut x = tabs_area.x;

        // Left overflow indicator region
        if window.has_left_overflow {
            if col >= x && col < x.saturating_add(2) {
                return Some(TabHit::PrevTab);
            }
            x = x.saturating_add(2);
        }

        // Individual tab regions
        for (i, tab) in tabs[window.start..window.end].iter().enumerate() {
            let w = UnicodeWidthStr::width(tab.title.as_str()) as u16 + 2;
            if col >= x && col < x.saturating_add(w) {
                return Some(TabHit::Tab(window.start + i));
            }
            x = x.saturating_add(w).saturating_add(1);
        }

        // Right overflow indicator region
        if window.has_right_overflow && col >= x {
            return Some(TabHit::NextTab);
        }

        None
    }
}

impl Widget for LimitsTabsRowWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.is_empty() {
            return;
        }

        let window = compute_tab_window(self.tabs, self.selected_tab, area.width as usize);
        let s_text_dim = crate::colors::style_text_dim();

        let mut spans = Vec::new();

        if window.has_left_overflow {
            spans.push(Span::styled(
                format!("{} ", crate::icons::arrow_left()),
                s_text_dim,
            ));
        }

        for (i, tab) in self.tabs[window.start..window.end].iter().enumerate() {
            let idx = window.start + i;
            let is_selected = idx == self.selected_tab;
            let style = if is_selected {
                crate::colors::style_text_bold()
            } else {
                s_text_dim
            };
            spans.push(Span::styled(format!(" {} ", tab.title), style));
            spans.push(Span::raw(" "));
        }

        if window.has_right_overflow {
            spans.push(Span::styled(
                format!(" {}", crate::icons::arrow_right()),
                s_text_dim,
            ));
        }

        Paragraph::new(Line::from(spans))
            .style(crate::colors::style_on_background())
            .render(area, buf);
    }
}

struct LimitsSingleBodyWidget {
    lines: Vec<Line<'static>>,
}

impl Widget for LimitsSingleBodyWidget {
    fn render(self, area: Rect, buf: &mut Buffer) {
        Paragraph::new(self.lines)
            .wrap(Wrap { trim: false })
            .style(crate::colors::style_text_on_bg())
            .render(area, buf);
    }
}

struct LimitsPaneWidget {
    title: &'static str,
    lines: Vec<Line<'static>>,
    focused: bool,
}

impl Widget for LimitsPaneWidget {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.is_empty() {
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Fill(1)])
            .split(area);
        let title_area = chunks[0];
        let body_area = chunks[1];

        let title_style = if self.focused {
            Style::default()
                .fg(crate::colors::function())
                .add_modifier(Modifier::BOLD)
        } else {
            crate::colors::style_text_bold()
        };

        Paragraph::new(Line::from(vec![
            Span::styled(self.title.to_string(), title_style),
            Span::styled(" ─", crate::colors::style_text_dim()),
        ]))
        .style(crate::colors::style_on_background())
        .render(title_area, buf);

        Paragraph::new(self.lines)
            .wrap(Wrap { trim: false })
            .style(crate::colors::style_text_on_bg())
            .render(body_area, buf);
    }
}

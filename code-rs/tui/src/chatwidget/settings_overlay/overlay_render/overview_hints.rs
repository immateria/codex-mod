impl SettingsOverlayView {
    fn trim_with_ellipsis(&self, text: &str, max_width: usize) -> String {
        if max_width == 0 || text.is_empty() {
            return String::new();
        }
        if UnicodeWidthStr::width(text) <= max_width {
            return text.to_owned();
        }
        if max_width <= 3 {
            return "...".chars().take(max_width).collect();
        }
        let keep = max_width.saturating_sub(3);
        let (prefix, _, _) = take_prefix_by_width(text, keep);
        let mut result = prefix;
        result.push_str("...");
        result
    }

    fn push_summary_spans(&self, line: &mut Line<'static>, summary: &str) {
        let label_style = crate::colors::style_text_mid();
        let dim_style = crate::colors::style_text_dim();
        let mut first = true;
        for raw_segment in summary.split(SEP_DOT) {
            let segment = raw_segment.trim();
            if segment.is_empty() {
                continue;
            }
            if !first {
                line.spans.push(Span::styled(SEP_DOT, dim_style));
            }
            first = false;

            if let Some((label, value)) = segment.split_once(':') {
                let label_trim = label.trim_end();
                let value_trim = value.trim_start();
                line.spans
                    .push(Span::styled(format!("{label_trim}:"), label_style));
                if !value_trim.is_empty() {
                    line.spans.push(Span::styled(" ", dim_style));
                    let value_style = self.summary_value_style(value_trim);
                    line.spans
                        .push(Span::styled(value_trim.to_owned(), value_style));
                }
            } else {
                let value_style = self.summary_value_style(segment);
                line.spans
                    .push(Span::styled(segment.to_owned(), value_style));
            }
        }
    }

    fn summary_value_style(&self, value: &str) -> Style {
        let trimmed = value.trim();
        let normalized = trimmed
            .trim_end_matches(['.', '!', ','])
            .to_ascii_lowercase();
        if matches!(normalized.as_str(), "on" | "enabled" | "yes") {
            crate::colors::style_success()
        } else if matches!(normalized.as_str(), "off" | "disabled" | "no") {
            crate::colors::style_error()
        } else {
            crate::colors::style_info()
        }
    }

    fn render_footer_hints_overview(&self, area: Rect, buf: &mut Buffer) {
        if area.is_empty() {
            self.last_hint_hit_areas.borrow_mut().clear();
            return;
        }

        use crate::bottom_pane::settings_ui::hints::{
            ShortcutAction, hit_areas_for_hints,
        };

        let hints = [
            crate::bottom_pane::settings_ui::hints::hint_nav(" navigate"),
            crate::bottom_pane::settings_ui::hints::hint_enter(" open"),
            crate::bottom_pane::settings_ui::hints::hint_esc(" close"),
            crate::bottom_pane::settings_ui::hints::KeyHint::new("?", " help")
                .with_action(ShortcutAction::Help),
        ];

        let line = crate::bottom_pane::settings_ui::hints::shortcut_line(&hints);
        *self.last_hint_hit_areas.borrow_mut() = hit_areas_for_hints(&hints, area.x, area.y);

        Paragraph::new(line)
            .style(crate::colors::style_on_background())
            .alignment(Alignment::Left)
            .render(area, buf);
    }

    fn render_footer_hints_section(&self, area: Rect, buf: &mut Buffer) {
        if area.is_empty() {
            self.last_hint_hit_areas.borrow_mut().clear();
            return;
        }

        use crate::bottom_pane::settings_ui::hints::{HintHitArea, ShortcutAction};
        use unicode_width::UnicodeWidthStr;

        let key = crate::colors::style_text();
        let hint = crate::colors::style_text_dim();
        let separator = crate::colors::style_text_mid();
        let focus = crate::colors::style_primary_bold();
        let focus_label = if self.is_sidebar_focused() {
            "Sidebar"
        } else {
            "Content"
        };
        let sidebar_action = if self.sidebar_collapsed.get() {
            " show"
        } else {
            " hide"
        };

        // Each group: (spans, optional action)
        type SpanGroup = (Vec<Span<'static>>, Option<ShortcutAction>);
        let hint_group = |key_label: String, description: &'static str, action: Option<ShortcutAction>| -> SpanGroup {
            (
                vec![
                    Span::styled(key_label, key),
                    Span::styled(description, hint),
                ],
                action,
            )
        };

        let join_groups_with_hits = |groups: Vec<SpanGroup>, origin_x: u16, y: u16| -> (Vec<Span<'static>>, Vec<HintHitArea>) {
            let mut spans = Vec::new();
            let mut hit_areas = Vec::new();
            let mut cursor = origin_x;
            for (idx, (group_spans, action)) in groups.into_iter().enumerate() {
                if idx > 0 {
                    let sep = Span::styled("  •  ", separator);
                    cursor = cursor.saturating_add(UnicodeWidthStr::width(sep.content.as_ref()) as u16);
                    spans.push(sep);
                }
                let group_start = cursor;
                let group_width: u16 = group_spans
                    .iter()
                    .map(|s| UnicodeWidthStr::width(s.content.as_ref()) as u16)
                    .sum();
                spans.extend(group_spans);
                if let Some(action) = action {
                    hit_areas.push(HintHitArea {
                        action,
                        x_start: group_start,
                        x_end: group_start.saturating_add(group_width),
                        y,
                    });
                }
                cursor = group_start.saturating_add(group_width);
            }
            (spans, hit_areas)
        };

        let groups: Vec<SpanGroup> = vec![
            hint_group(crate::icons::tab().to_owned(), " content", Some(ShortcutAction::FocusContent)),
            hint_group(crate::icons::reverse_tab().to_owned(), " sidebar", Some(ShortcutAction::FocusSidebar)),
            hint_group(crate::icons::ctrl_combo("B"), sidebar_action, Some(ShortcutAction::ToggleSidebar)),
            hint_group(crate::icons::escape().to_owned(), " overview", Some(ShortcutAction::Back)),
            hint_group("?".to_owned(), " help", Some(ShortcutAction::Help)),
            (
                vec![
                    Span::styled("focus", hint),
                    Span::styled(": ", separator),
                    Span::styled(focus_label, focus),
                ],
                None,
            ),
        ];

        let (mut spans, mut hit_areas) = join_groups_with_hits(groups, area.x, area.y);

        // On narrow screens, drop the less-critical hints.
        let full_width: usize = spans.iter().map(Span::width).sum();
        if full_width > area.width as usize {
            let narrow_groups: Vec<SpanGroup> = vec![
                hint_group(crate::icons::ctrl_combo("B"), sidebar_action, Some(ShortcutAction::ToggleSidebar)),
                hint_group(crate::icons::escape().to_owned(), " back", Some(ShortcutAction::Back)),
                (
                    vec![
                        Span::styled("focus", hint),
                        Span::styled(": ", separator),
                        Span::styled(focus_label, focus),
                    ],
                    None,
                ),
            ];
            let (narrow_spans, narrow_hits) = join_groups_with_hits(narrow_groups, area.x, area.y);
            spans = narrow_spans;
            hit_areas = narrow_hits;
        }

        *self.last_hint_hit_areas.borrow_mut() = hit_areas;

        Paragraph::new(Line::from(spans))
            .style(crate::colors::style_on_background())
            .alignment(Alignment::Left)
            .render(area, buf);
    }
}

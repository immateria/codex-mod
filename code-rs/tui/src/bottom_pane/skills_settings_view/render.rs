use super::*;
use crate::bottom_pane::settings_ui::buttons::{
    render_standard_button_strip,
    standard_button_strip_width,
};
use crate::bottom_pane::settings_ui::fields::BorderedField;

const LABEL_COLUMN_WIDTH: u16 = 24;
const LOW_HEIGHT_THRESHOLD: usize = 24;

impl SkillsSettingsView {
    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        self.last_render_area.set(Some(area));
        self.last_render_chrome.set(SkillsRenderChrome::Framed);
        self.render_body(area, buf);
    }

    pub(super) fn render_content_only(&self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        self.last_render_area.set(Some(area));
        self.last_render_chrome.set(SkillsRenderChrome::ContentOnly);
        crate::util::buffer::fill_rect(
            buf,
            area,
            Some(' '),
            Style::default()
                .bg(colors::background())
                .fg(colors::text()),
        );
        self.render_body_content_only(area, buf);
    }

    fn render_body(&self, area: Rect, buf: &mut Buffer) {
        match self.mode {
            Mode::List => self.render_list(area, buf),
            Mode::Edit => self.render_form(area, buf),
        }
    }

    fn render_body_content_only(&self, area: Rect, buf: &mut Buffer) {
        match self.mode {
            Mode::List => self.render_list_content_only(area, buf),
            Mode::Edit => self.render_form_content_only(area, buf),
        }
    }

    fn render_list(&self, area: Rect, buf: &mut Buffer) {
        let mut lines: Vec<Line> = Vec::new();
        for (idx, skill) in self.skills.iter().enumerate() {
            let arrow = if idx == self.selected { ">" } else { " " };
            let name_style = if idx == self.selected {
                Style::default().fg(colors::primary()).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(colors::text())
            };
            let scope_text = match skill.scope {
                SkillScope::Repo => " [repo]",
                SkillScope::User => " [user]",
                SkillScope::System => " [system]",
            };
            let name_span = Span::styled(format!("{arrow} {name}", name = skill.name), name_style);
            let scope_span = Span::styled(scope_text, Style::default().fg(colors::text_dim()));
            let desc_span = Span::styled(
                format!("  {desc}", desc = skill.description),
                Style::default().fg(colors::text_dim()),
            );
            lines.push(Line::from(vec![name_span, scope_span, desc_span]));
        }
        if lines.is_empty() {
            lines.push(Line::from("No skills yet. Press Enter or Ctrl+N to create one."));
        }

        let add_arrow = if self.selected == self.skills.len() { ">" } else { " " };
        let add_style = if self.selected == self.skills.len() {
            Style::default().fg(colors::primary()).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(colors::success()).add_modifier(Modifier::BOLD)
        };
        lines.push(Line::from(vec![Span::styled(
            format!("{add_arrow} Add new..."),
            add_style,
        )]));

        let title = Paragraph::new(vec![Line::from(Span::styled(
            "Skills are reusable instruction bundles stored as SKILL.md files. Use Enter to edit, Ctrl+N for guided create, and Ctrl+G in editor to generate a draft with per-style skill and resource overrides.",
            Style::default().fg(colors::text_dim()),
        ))])
        .alignment(Alignment::Left)
        .wrap(Wrap { trim: true })
        .style(Style::default().bg(colors::background()));

        let list = Paragraph::new(lines)
            .alignment(Alignment::Left)
            .style(Style::default().bg(colors::background()));

        let outer = Block::default()
            .borders(Borders::ALL)
            .style(Style::default().bg(colors::background()));
        let inner = outer.inner(area);
        outer.render(area, buf);
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(1)])
            .split(inner);

        title.render(chunks[0], buf);
        list.render(chunks[1], buf);
    }

    fn render_list_content_only(&self, area: Rect, buf: &mut Buffer) {
        let mut lines: Vec<Line> = Vec::new();
        for (idx, skill) in self.skills.iter().enumerate() {
            let arrow = if idx == self.selected { ">" } else { " " };
            let name_style = if idx == self.selected {
                Style::default()
                    .fg(colors::primary())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(colors::text())
            };
            let scope_text = match skill.scope {
                SkillScope::Repo => " [repo]",
                SkillScope::User => " [user]",
                SkillScope::System => " [system]",
            };
            let name_span = Span::styled(format!("{arrow} {name}", name = skill.name), name_style);
            let scope_span = Span::styled(scope_text, Style::default().fg(colors::text_dim()));
            let desc_span = Span::styled(
                format!("  {desc}", desc = skill.description),
                Style::default().fg(colors::text_dim()),
            );
            lines.push(Line::from(vec![name_span, scope_span, desc_span]));
        }
        if lines.is_empty() {
            lines.push(Line::from("No skills yet. Press Enter or Ctrl+N to create one."));
        }

        let add_arrow = if self.selected == self.skills.len() { ">" } else { " " };
        let add_style = if self.selected == self.skills.len() {
            Style::default()
                .fg(colors::primary())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .fg(colors::success())
                .add_modifier(Modifier::BOLD)
        };
        lines.push(Line::from(vec![Span::styled(
            format!("{add_arrow} Add new..."),
            add_style,
        )]));

        let title = Paragraph::new(vec![Line::from(Span::styled(
            "Skills are reusable instruction bundles stored as SKILL.md files. Use Enter to edit, Ctrl+N for guided create, and Ctrl+G in editor to generate a draft with per-style skill and resource overrides.",
            Style::default().fg(colors::text_dim()),
        ))])
        .alignment(Alignment::Left)
        .wrap(Wrap { trim: true })
        .style(Style::default().bg(colors::background()));

        let list = Paragraph::new(lines)
            .alignment(Alignment::Left)
            .style(Style::default().bg(colors::background()));

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(1)])
            .split(area);

        title.render(chunks[0], buf);
        list.render(chunks[1], buf);
    }

    fn render_form(&self, area: Rect, buf: &mut Buffer) {
        let outer = Block::default()
            .borders(Borders::ALL)
            .title("Skill Creator / Editor")
            .style(Style::default().bg(colors::background()));
        outer.render(area, buf);
        let Some(layout) = self.compute_form_layout_framed(area) else {
            return;
        };

        let label_style = Style::default().fg(colors::text_dim());

        render_labeled_row(
            buf,
            layout.viewport_inner,
            label_style,
            "Name (slug)",
            layout.name_field,
        );
        self.editor.name_field
            .render(layout.name_field, buf, matches!(self.editor.focus, Focus::Name));

        render_labeled_row(
            buf,
            layout.viewport_inner,
            label_style,
            "Description",
            layout.description_field,
        );
        self.editor.description_field.render(
            layout.description_field,
            buf,
            matches!(self.editor.focus, Focus::Description),
        );

        render_labeled_row(
            buf,
            layout.viewport_inner,
            label_style,
            "Shell style (optional)",
            layout.style_field,
        );
        self.editor.style_field
            .render(layout.style_field, buf, matches!(self.editor.focus, Focus::Style));

        render_labeled_row(
            buf,
            layout.viewport_inner,
            label_style,
            "Style profile behavior",
            layout.style_profile_row,
        );
        if layout.style_profile_row.height > 0 {
            let focused = matches!(self.editor.focus, Focus::StyleProfile);
            let mode_style = if focused {
                Style::default()
                    .fg(colors::background())
                    .bg(colors::primary())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
                    .fg(colors::text())
                    .add_modifier(Modifier::BOLD)
            };
            let hint_style = Style::default().fg(colors::text_dim());
            let mode_text = self.editor.style_profile_mode.label().to_string();
            let hint_text = if self.editor.style_field.text().trim().is_empty() {
                "Set shell style first".to_string()
            } else {
                self.editor.style_profile_mode.hint().to_string()
            };
            Paragraph::new(Line::from(vec![
                Span::styled(mode_text, mode_style),
                Span::raw("  "),
                Span::styled(hint_text, hint_style),
            ]))
            .render(layout.style_profile_row, buf);
        }

        let _ = style_references_block(
            self.editor.style_references_dirty,
            matches!(self.editor.focus, Focus::StyleReferences),
        )
        .render(layout.style_references_outer, buf, &self.editor.style_references_field);
        let _ = style_skill_roots_block(
            self.editor.style_skill_roots_dirty,
            matches!(self.editor.focus, Focus::StyleSkillRoots),
        )
        .render(layout.style_skill_roots_outer, buf, &self.editor.style_skill_roots_field);
        let _ = style_mcp_include_block(
            self.editor.style_mcp_include_dirty,
            matches!(self.editor.focus, Focus::StyleMcpInclude),
        )
        .render(layout.style_mcp_include_outer, buf, &self.editor.style_mcp_include_field);
        let _ = style_mcp_exclude_block(
            self.editor.style_mcp_exclude_dirty,
            matches!(self.editor.focus, Focus::StyleMcpExclude),
        )
        .render(layout.style_mcp_exclude_outer, buf, &self.editor.style_mcp_exclude_field);

        let _ = examples_block(matches!(self.editor.focus, Focus::Examples))
            .render(layout.examples_outer, buf, &self.editor.examples_field);

        let _ = body_block(matches!(self.editor.focus, Focus::Body))
            .render(layout.body_outer, buf, &self.editor.body_field);

        let buttons = self.action_button_specs();
        render_standard_button_strip(layout.buttons_row, buf, &buttons);
        let hint_x = layout
            .buttons_row
            .x
            .saturating_add(standard_button_strip_width(&buttons))
            .saturating_add(4);
        Paragraph::new(Line::from(Span::raw(
            "Tab cycle - Enter activates - <-/-> mode - Ctrl+G generate",
        )))
        .render(
            Rect::new(
                hint_x,
                layout.buttons_row.y,
                layout
                    .buttons_row
                    .width
                    .saturating_sub(hint_x.saturating_sub(layout.buttons_row.x)),
                layout.buttons_row.height,
            ),
            buf,
        );

        if layout.status_row.height > 0
            && let Some((msg, style)) = &self.status
        {
            Paragraph::new(Line::from(Span::styled(msg.clone(), *style)))
                .alignment(Alignment::Left)
                .render(layout.status_row, buf);
        }

        if layout.max_scroll > 0 {
            let viewport_len = layout.viewport_inner.height as usize;
            render_vertical_scrollbar(
                buf,
                layout.viewport_inner,
                layout.scroll_top,
                layout.max_scroll,
                viewport_len,
            );
        }
    }

    fn render_form_content_only(&self, area: Rect, buf: &mut Buffer) {
        let Some(layout) = self.compute_form_layout_content_only(area) else {
            return;
        };

        let label_style = Style::default().fg(colors::text_dim());

        render_labeled_row(
            buf,
            layout.viewport_inner,
            label_style,
            "Name (slug)",
            layout.name_field,
        );
        self.editor.name_field
            .render(layout.name_field, buf, matches!(self.editor.focus, Focus::Name));

        render_labeled_row(
            buf,
            layout.viewport_inner,
            label_style,
            "Description",
            layout.description_field,
        );
        self.editor.description_field.render(
            layout.description_field,
            buf,
            matches!(self.editor.focus, Focus::Description),
        );

        render_labeled_row(
            buf,
            layout.viewport_inner,
            label_style,
            "Shell style (optional)",
            layout.style_field,
        );
        self.editor.style_field
            .render(layout.style_field, buf, matches!(self.editor.focus, Focus::Style));

        render_labeled_row(
            buf,
            layout.viewport_inner,
            label_style,
            "Style profile behavior",
            layout.style_profile_row,
        );
        if layout.style_profile_row.height > 0 {
            let focused = matches!(self.editor.focus, Focus::StyleProfile);
            let mode_style = if focused {
                Style::default()
                    .fg(colors::background())
                    .bg(colors::primary())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
                    .fg(colors::text())
                    .add_modifier(Modifier::BOLD)
            };
            let hint_style = Style::default().fg(colors::text_dim());
            let mode_text = self.editor.style_profile_mode.label().to_string();
            let hint_text = if self.editor.style_field.text().trim().is_empty() {
                "Set shell style first".to_string()
            } else {
                self.editor.style_profile_mode.hint().to_string()
            };
            Paragraph::new(Line::from(vec![
                Span::styled(mode_text, mode_style),
                Span::raw("  "),
                Span::styled(hint_text, hint_style),
            ]))
            .render(layout.style_profile_row, buf);
        }

        let _ = style_references_block(
            self.editor.style_references_dirty,
            matches!(self.editor.focus, Focus::StyleReferences),
        )
        .render(layout.style_references_outer, buf, &self.editor.style_references_field);
        let _ = style_skill_roots_block(
            self.editor.style_skill_roots_dirty,
            matches!(self.editor.focus, Focus::StyleSkillRoots),
        )
        .render(layout.style_skill_roots_outer, buf, &self.editor.style_skill_roots_field);
        let _ = style_mcp_include_block(
            self.editor.style_mcp_include_dirty,
            matches!(self.editor.focus, Focus::StyleMcpInclude),
        )
        .render(layout.style_mcp_include_outer, buf, &self.editor.style_mcp_include_field);
        let _ = style_mcp_exclude_block(
            self.editor.style_mcp_exclude_dirty,
            matches!(self.editor.focus, Focus::StyleMcpExclude),
        )
        .render(layout.style_mcp_exclude_outer, buf, &self.editor.style_mcp_exclude_field);

        let _ = examples_block(matches!(self.editor.focus, Focus::Examples))
            .render(layout.examples_outer, buf, &self.editor.examples_field);

        let _ = body_block(matches!(self.editor.focus, Focus::Body))
            .render(layout.body_outer, buf, &self.editor.body_field);

        let buttons = self.action_button_specs();
        render_standard_button_strip(layout.buttons_row, buf, &buttons);
        let hint_x = layout
            .buttons_row
            .x
            .saturating_add(standard_button_strip_width(&buttons))
            .saturating_add(4);
        Paragraph::new(Line::from(Span::raw(
            "Tab cycle - Enter activates - <-/-> mode - Ctrl+G generate",
        )))
        .render(
            Rect::new(
                hint_x,
                layout.buttons_row.y,
                layout
                    .buttons_row
                    .width
                    .saturating_sub(hint_x.saturating_sub(layout.buttons_row.x)),
                layout.buttons_row.height,
            ),
            buf,
        );

        if layout.status_row.height > 0
            && let Some((msg, style)) = &self.status
        {
            Paragraph::new(Line::from(Span::styled(msg.clone(), *style)))
                .alignment(Alignment::Left)
                .render(layout.status_row, buf);
        }

        if layout.max_scroll > 0 {
            let viewport_len = layout.viewport_inner.height as usize;
            render_vertical_scrollbar(
                buf,
                layout.viewport_inner,
                layout.scroll_top,
                layout.max_scroll,
                viewport_len,
            );
        }
    }

    pub(super) fn compute_form_layout_framed(&self, area: Rect) -> Option<SkillsFormLayout> {
        let inner = Block::default().borders(Borders::ALL).inner(area);
        self.compute_form_layout_inner(inner)
    }

    pub(super) fn compute_form_layout_content_only(&self, area: Rect) -> Option<SkillsFormLayout> {
        self.compute_form_layout_inner(area)
    }

    fn compute_form_layout_inner(&self, inner: Rect) -> Option<SkillsFormLayout> {
        if inner.width == 0 || inner.height == 0 {
            return None;
        }

        const BASIC_ROW_H: usize = 3;
        const PATH_BLOCK_H: usize = 4;
        const PATH_BLOCK_H_FOCUSED: usize = 7;
        const EXAMPLES_H: usize = 5;
        const EXAMPLES_H_FOCUSED: usize = 8;
        const BODY_MIN_H: usize = 10;
        const BODY_MIN_H_FOCUSED: usize = 14;

        let footer_layout = split_pinned_footer_layout(inner, 1, 1, 4);
        let viewport_inner = footer_layout.viewport;
        let viewport_h = viewport_inner.height as usize;

        let low_height = viewport_h > 0 && viewport_h <= LOW_HEIGHT_THRESHOLD;
        let style_references_section_h = if low_height && matches!(self.editor.focus, Focus::StyleReferences) {
            PATH_BLOCK_H_FOCUSED
        } else {
            PATH_BLOCK_H
        };
        let style_skill_roots_section_h = if low_height && matches!(self.editor.focus, Focus::StyleSkillRoots) {
            PATH_BLOCK_H_FOCUSED
        } else {
            PATH_BLOCK_H
        };
        let style_mcp_include_section_h = if low_height && matches!(self.editor.focus, Focus::StyleMcpInclude) {
            PATH_BLOCK_H_FOCUSED
        } else {
            PATH_BLOCK_H
        };
        let style_mcp_exclude_section_h = if low_height && matches!(self.editor.focus, Focus::StyleMcpExclude) {
            PATH_BLOCK_H_FOCUSED
        } else {
            PATH_BLOCK_H
        };
        let examples_section_h = if low_height && matches!(self.editor.focus, Focus::Examples) {
            EXAMPLES_H_FOCUSED
        } else {
            EXAMPLES_H
        };
        let body_min_h = if low_height && matches!(self.editor.focus, Focus::Body) {
            BODY_MIN_H_FOCUSED
        } else {
            BODY_MIN_H
        };

        let static_without_body = BASIC_ROW_H
            + BASIC_ROW_H
            + BASIC_ROW_H
            + BASIC_ROW_H
            + style_references_section_h
            + style_skill_roots_section_h
            + style_mcp_include_section_h
            + style_mcp_exclude_section_h
            + examples_section_h;
        let base_total = static_without_body + body_min_h;
        let body_h = body_min_h + viewport_h.saturating_sub(base_total);
        let content_h = static_without_body + body_h;
        let max_scroll = content_h.saturating_sub(viewport_h);
        let scroll_top = self.editor.edit_scroll_top.min(max_scroll);

        let mut top = 0usize;
        let (name_top, name_h) = advance_section(&mut top, BASIC_ROW_H);
        let (description_top, description_h) = advance_section(&mut top, BASIC_ROW_H);
        let (style_top, style_h) = advance_section(&mut top, BASIC_ROW_H);
        let (style_profile_top, style_profile_h) = advance_section(&mut top, BASIC_ROW_H);
        let (style_references_top, style_references_h) =
            advance_section(&mut top, style_references_section_h);
        let (style_skill_roots_top, style_skill_roots_h) =
            advance_section(&mut top, style_skill_roots_section_h);
        let (style_mcp_include_top, style_mcp_include_h) =
            advance_section(&mut top, style_mcp_include_section_h);
        let (style_mcp_exclude_top, style_mcp_exclude_h) =
            advance_section(&mut top, style_mcp_exclude_section_h);
        let (examples_top, examples_h) = advance_section(&mut top, examples_section_h);
        let (body_top, body_h) = advance_section(&mut top, body_h);

        let name_row = clipped_vertical_rect_with_scroll(viewport_inner, name_top, name_h, scroll_top);
        let description_row = clipped_vertical_rect_with_scroll(
            viewport_inner,
            description_top,
            description_h,
            scroll_top,
        );
        let style_row = clipped_vertical_rect_with_scroll(viewport_inner, style_top, style_h, scroll_top);
        let style_profile_row_full = clipped_vertical_rect_with_scroll(
            viewport_inner,
            style_profile_top,
            style_profile_h,
            scroll_top,
        );
        let style_references_outer =
            clipped_vertical_rect_with_scroll(viewport_inner, style_references_top, style_references_h, scroll_top);
        let style_skill_roots_outer =
            clipped_vertical_rect_with_scroll(viewport_inner, style_skill_roots_top, style_skill_roots_h, scroll_top);
        let style_mcp_include_outer =
            clipped_vertical_rect_with_scroll(viewport_inner, style_mcp_include_top, style_mcp_include_h, scroll_top);
        let style_mcp_exclude_outer =
            clipped_vertical_rect_with_scroll(viewport_inner, style_mcp_exclude_top, style_mcp_exclude_h, scroll_top);
        let examples_outer =
            clipped_vertical_rect_with_scroll(viewport_inner, examples_top, examples_h, scroll_top);
        let body_outer = clipped_vertical_rect_with_scroll(viewport_inner, body_top, body_h, scroll_top);

        let buttons_row = footer_layout.action_row;
        let status_row = footer_layout.status_row;

        let split_row = |row: Rect| {
            Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Length(LABEL_COLUMN_WIDTH), Constraint::Min(1)])
                .split(row)
        };

        let name_chunks = split_row(name_row);
        let description_chunks = split_row(description_row);
        let style_chunks = split_row(style_row);
        let style_profile_chunks = split_row(style_profile_row_full);

        Some(SkillsFormLayout {
            viewport_inner,
            scroll_top,
            max_scroll,
            status_row,
            name_field: name_chunks[1],
            name_top,
            name_h,
            description_field: description_chunks[1],
            description_top,
            description_h,
            style_field: style_chunks[1],
            style_top,
            style_h,
            style_profile_row: style_profile_chunks[1],
            style_profile_top,
            style_profile_h,
            style_references_outer,
            style_references_inner: style_references_block(self.editor.style_references_dirty, false)
                .inner(style_references_outer),
            style_references_top,
            style_references_h,
            style_skill_roots_outer,
            style_skill_roots_inner: style_skill_roots_block(self.editor.style_skill_roots_dirty, false)
                .inner(style_skill_roots_outer),
            style_skill_roots_top,
            style_skill_roots_h,
            style_mcp_include_outer,
            style_mcp_include_inner: style_mcp_include_block(self.editor.style_mcp_include_dirty, false)
                .inner(style_mcp_include_outer),
            style_mcp_include_top,
            style_mcp_include_h,
            style_mcp_exclude_outer,
            style_mcp_exclude_inner: style_mcp_exclude_block(self.editor.style_mcp_exclude_dirty, false)
                .inner(style_mcp_exclude_outer),
            style_mcp_exclude_top,
            style_mcp_exclude_h,
            examples_outer,
            examples_inner: examples_block(false).inner(examples_outer),
            examples_top,
            examples_h,
            body_outer,
            body_inner: body_block(false).inner(body_outer),
            body_top,
            body_h,
            buttons_row,
        })
    }
}

pub(super) fn render_labeled_row(
    buf: &mut Buffer,
    viewport_inner: Rect,
    label_style: Style,
    label: &str,
    field_rect: Rect,
) {
    if field_rect.height == 0 || viewport_inner.width == 0 {
        return;
    }
    let label_w = LABEL_COLUMN_WIDTH.min(viewport_inner.width);
    let label_rect = Rect {
        x: viewport_inner.x,
        y: field_rect.y,
        width: label_w,
        height: field_rect.height,
    };
    Paragraph::new(Line::from(Span::styled(label, label_style))).render(label_rect, buf);
}

fn advance_section(top: &mut usize, height: usize) -> (usize, usize) {
    let section_top = *top;
    *top = top.saturating_add(height);
    (section_top, height)
}

fn style_references_block(dirty: bool, focused: bool) -> BorderedField<'static> {
    let title = if dirty {
        "Style references [edited] (one path per line)".to_string()
    } else {
        "Style references (one path per line)".to_string()
    };
    BorderedField::new(title, focused)
}

fn style_skill_roots_block(dirty: bool, focused: bool) -> BorderedField<'static> {
    let title = if dirty {
        "Style skill roots [edited] (one path per line)".to_string()
    } else {
        "Style skill roots (one path per line)".to_string()
    };
    BorderedField::new(title, focused)
}

fn style_mcp_include_block(dirty: bool, focused: bool) -> BorderedField<'static> {
    let title = if dirty {
        "Style MCP include [edited] (one server per line)".to_string()
    } else {
        "Style MCP include (one server per line)".to_string()
    };
    BorderedField::new(title, focused)
}

fn style_mcp_exclude_block(dirty: bool, focused: bool) -> BorderedField<'static> {
    let title = if dirty {
        "Style MCP exclude [edited] (one server per line)".to_string()
    } else {
        "Style MCP exclude (one server per line)".to_string()
    };
    BorderedField::new(title, focused)
}

fn examples_block(focused: bool) -> BorderedField<'static> {
    BorderedField::new("Trigger Examples / User Requests".to_string(), focused)
}

fn body_block(focused: bool) -> BorderedField<'static> {
    BorderedField::new("SKILL.md Body".to_string(), focused)
}

use super::*;
use super::terminal_surface_header::HeaderTemplateContext;
use super::terminal_surface_header::DynamicHeaderLayoutInput;
use super::terminal_surface_header::McpHeaderIndicatorKind;
use super::terminal_surface_header::centered_clickable_regions_from_char_ranges;
use super::terminal_surface_header::render_plain_header_template;
use super::terminal_surface_header::render_dynamic_header_line;
use super::terminal_surface_header::render_styled_header_template;

impl ChatWidget<'_> {
    pub(crate) fn export_transcript_lines_for_buffer(&self) -> Vec<ratatui::text::Line<'static>> {
        let mut out: Vec<ratatui::text::Line<'static>> = Vec::new();
        for (idx, cell) in self.history_cells.iter().enumerate() {
            out.extend(self.render_lines_for_terminal(idx, cell.as_ref()));
        }
        // Include streaming preview if present (treat like assistant output)
        let mut streaming_lines = self
            .live_builder
            .display_rows()
            .into_iter()
            .map(|r| ratatui::text::Line::from(r.text))
            .collect::<Vec<_>>();
        if !streaming_lines.is_empty() {
            // Apply gutter to streaming preview (first line gets " • ", continuations get 3 spaces)
            if let Some(first) = streaming_lines.first_mut() {
                first.spans.insert(0, ratatui::text::Span::raw(" • "));
            }
            for line in streaming_lines.iter_mut().skip(1) {
                line.spans.insert(0, ratatui::text::Span::raw("   "));
            }
            out.extend(streaming_lines);
            out.push(ratatui::text::Line::from(""));
        }
        out
    }

    /// Render a single history cell into terminal-friendly lines:
    /// - Prepend a gutter icon (symbol + space) to the first line when defined.
    /// - Add a single blank line after the cell as a separator.
    fn render_lines_for_terminal(
        &self,
        idx: usize,
        cell: &dyn crate::history_cell::HistoryCell,
    ) -> Vec<ratatui::text::Line<'static>> {
        let mut lines = self.cell_lines_for_terminal_index(idx, cell);
        let _has_icon = cell.gutter_symbol().is_some();
        let first_prefix = if let Some(sym) = cell.gutter_symbol() {
            format!(" {sym} ") // one space, icon, one space
        } else {
            "   ".to_string() // three spaces when no icon
        };
        if let Some(first) = lines.first_mut() {
            first
                .spans
                .insert(0, ratatui::text::Span::raw(first_prefix));
        }
        // For wrapped/subsequent lines, use a 3-space gutter to maintain alignment
        if lines.len() > 1 {
            for (_idx, line) in lines.iter_mut().enumerate().skip(1) {
                // Always 3 spaces for continuation lines
                line.spans.insert(0, ratatui::text::Span::raw("   "));
            }
        }
        lines.push(ratatui::text::Line::from(""));
        lines
    }

    // Desired bottom pane height (in rows) for a given terminal width.
    pub(crate) fn desired_bottom_height(&self, width: u16) -> u16 {
        self.bottom_pane.desired_height(width)
    }

    // The last bottom pane height (rows) that the layout actually used.
    // If not yet set, fall back to a conservative estimate from BottomPane.

    // (Removed) Legacy in-place reset method. The /new command now creates a fresh
    // ChatWidget (new core session) to ensure the agent context is fully reset.

    pub fn cursor_pos(&self, area: Rect) -> Option<(u16, u16)> {
        // Hide the terminal cursor whenever a top‑level overlay is active so the
        // caret does not show inside the input while a modal (help/diff) is open.
        if self.diffs.overlay.is_some()
            || self.help.overlay.is_some()
            || self.settings.overlay.is_some()
            || self.terminal.overlay().is_some()
            || self.browser_overlay_visible
            || self.agents_terminal.active
        {
            return None;
        }
        let layout_areas = self.layout_areas(area);
        let bottom_pane_area = if layout_areas.len() == 4 {
            layout_areas[3]
        } else {
            layout_areas[2]
        };
        self.bottom_pane.cursor_pos(bottom_pane_area)
    }

    pub(super) fn measured_font_size(&self) -> (u16, u16) {
        *self.cached_cell_size.get_or_init(|| {
            let size = self.terminal_info.font_size;

            // HACK: On macOS Retina displays, terminals often report physical pixels
            // but ratatui-image expects logical pixels. If we detect suspiciously
            // large cell sizes (likely 2x scaled), divide by 2.
            #[cfg(target_os = "macos")]
            {
                if size.0 >= 14 && size.1 >= 28 {
                    // Likely Retina display reporting physical pixels
                    tracing::info!(
                        "Detected likely Retina display, adjusting cell size from {:?} to {:?}",
                        size,
                        (size.0 / 2, size.1 / 2)
                    );
                    return (size.0 / 2, size.1 / 2);
                }
            }

            size
        })
    }

    pub(super) fn get_git_branch(&self) -> Option<String> {
        use std::fs;
        use std::path::Path;

        let head_path = self.config.cwd.join(".git/HEAD");
        let mut cache = self.git_branch_cache.borrow_mut();
        let now = Instant::now();

        let needs_refresh = match cache.last_refresh {
            Some(last) => now.duration_since(last) >= Duration::from_millis(500),
            None => true,
        };

        if needs_refresh {
            let modified = fs::metadata(&head_path)
                .and_then(|meta| meta.modified())
                .ok();

            let metadata_changed = cache.last_head_mtime != modified || cache.last_refresh.is_none();

            if metadata_changed {
                cache.value = fs::read_to_string(&head_path)
                    .ok()
                    .and_then(|head_contents| {
                        let head = head_contents.trim();

                        if let Some(rest) = head.strip_prefix("ref: ") {
                            return Path::new(rest)
                                .file_name()
                                .and_then(|s| s.to_str())
                                .filter(|s| !s.is_empty())
                                .map(std::string::ToString::to_string);
                        }

                        if head.len() >= 7
                            && head.as_bytes().iter().all(u8::is_ascii_hexdigit)
                        {
                            return Some(format!("detached: {}", &head[..7]));
                        }

                        None
                    });
                cache.last_head_mtime = modified;
            }

            cache.last_refresh = Some(now);
        }

        cache.value.clone()
    }

    pub(super) fn status_bar_height_rows(&self) -> u16 {
        if self.standard_terminal_mode {
            return 0;
        }
        let header = &self.config.tui.header;
        let show_top = header.show_top_line;
        let show_bottom = header.show_bottom_line
            && header
                .bottom_line_text
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .is_some();

        let line_count = u16::from(show_top) + u16::from(show_bottom);
        if line_count == 0 {
            0
        } else {
            // +2 for the bordered header block.
            line_count.saturating_add(2)
        }
    }

    pub(super) fn render_status_bar(&self, area: Rect, buf: &mut Buffer) {
        use crate::exec_command::relativize_to_home;
        use ratatui::layout::Margin;
        use ratatui::style::Style;
        use ratatui::text::Line;
        use ratatui::widgets::Block;
        use ratatui::widgets::Borders;
        use ratatui::widgets::Paragraph;

        if area.width == 0 || area.height == 0 {
            self.clickable_regions.borrow_mut().clear();
            *self.hovered_clickable_action.borrow_mut() = None;
            return;
        }

        let header_cfg = &self.config.tui.header;
        if !header_cfg.show_top_line && !header_cfg.show_bottom_line {
            self.clickable_regions.borrow_mut().clear();
            *self.hovered_clickable_action.borrow_mut() = None;
            return;
        }

        // Add same horizontal padding as the Message input (2 chars on each side)
        let horizontal_padding = 1u16;
        let padded_area = Rect {
            x: area.x + horizontal_padding,
            y: area.y,
            width: area.width.saturating_sub(horizontal_padding * 2),
            height: area.height,
        };

        // Get current working directory string
        let cwd_str = match relativize_to_home(&self.config.cwd) {
            Some(rel) if !rel.as_os_str().is_empty() => format!("~/{}", rel.display()),
            Some(_) => "~".to_string(),
            None => self.config.cwd.display().to_string(),
        };

        let cwd_short_str = cwd_str
            .rsplit(['/', '\\'])
            .find(|segment| !segment.is_empty())
            .unwrap_or(cwd_str.as_str())
            .to_string();

        let branch_opt = self.get_git_branch();

        // Determine current shell display (configured override or $SHELL fallback)
        let shell_display = match &self.config.shell {
            Some(shell) => format!("{} {}", shell.path, shell.args.join(" ")).trim().to_string(),
            None => std::env::var("SHELL").ok().unwrap_or_else(|| "sh".to_string()),
        };
        let header_title = self
            .config
            .tui
            .branding
            .title
            .as_deref()
            .map(str::trim)
            .filter(|title| !title.is_empty())
            .unwrap_or(crate::glitch_animation::DEFAULT_BRAND_TITLE);

        let mcp_indicator: Option<(McpHeaderIndicatorKind, String)> =
            if self.startup_mcp_error_summary.is_some() || !self.mcp_server_failures.is_empty() {
                let count = self.mcp_server_failures.len();
                let value = if count > 0 {
                    format!("error({count})")
                } else {
                    "error".to_string()
                };
                Some((McpHeaderIndicatorKind::Error, value))
            } else if self.session_id.is_none()
                && !self.config.mcp_servers.is_empty()
                && !self.test_mode
            {
                Some((McpHeaderIndicatorKind::Connecting, "init".to_string()))
            } else {
                None
            };

        let model_display = self.format_model_name(&self.config.model);
        let reasoning_display = Self::format_reasoning_effort(self.config.model_reasoning_effort);
        let mcp_display = mcp_indicator
            .as_ref()
            .map(|(_, value)| value.clone())
            .unwrap_or_else(|| "ok".to_string());
        let branch_display = branch_opt.clone().unwrap_or_default();
        let hovered_action = self.hovered_clickable_action.borrow().clone();
        let hover_style = header_cfg.hover_style;

        // Now recompute exact available width inside the border + padding before measuring
        // Render a bordered status block and explicitly fill its background.
        // Without a background fill, some terminals blend with prior frame
        // contents, which is especially noticeable on dark themes as dark
        // "caps" at the edges. Match the app background for consistency.
        let status_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(crate::colors::border()))
            .style(Style::default().bg(crate::colors::background()));
        let inner_area = status_block.inner(padded_area);
        let padded_inner = inner_area.inner(Margin::new(1, 0));
        let inner_width = padded_inner.width as usize;
        let dynamic_header = render_dynamic_header_line(
            &DynamicHeaderLayoutInput {
                title: header_title,
                model: model_display.as_str(),
                shell: shell_display.as_str(),
                reasoning: reasoning_display,
                directory_full: cwd_str.as_str(),
                directory_short: cwd_short_str.as_str(),
                branch: branch_opt.as_deref(),
                mcp_indicator: mcp_indicator
                    .as_ref()
                    .map(|(kind, value)| (*kind, value.as_str())),
                hovered_action: hovered_action.clone(),
                hover_style,
                minimal_header: std::env::var_os("CODEX_TUI_FORCE_MINIMAL_HEADER").is_some(),
                demo_mode: self.config.demo_developer_message.is_some(),
                inner_width,
            },
        );

        let now = Instant::now();
        let mut frame_needed = false;
        if ENABLE_WARP_STRIPES && self.header_wave.schedule_if_needed(now) {
            frame_needed = true;
        }
        if frame_needed {
            self.app_event_tx
                .send(AppEvent::ScheduleFrameIn(HeaderWaveEffect::FRAME_INTERVAL));
        }

        // Render the block first
        status_block.render(padded_area, buf);
        let wave_enabled = self.header_wave.is_enabled();
        if wave_enabled {
            self.header_wave.render(padded_area, buf, now);
        }

        // Then render the text inside with padding, centered
        let effect_enabled = wave_enabled;
        let status_style = if effect_enabled {
            Style::default().fg(crate::colors::text())
        } else {
            Style::default()
                .bg(crate::colors::background())
                .fg(crate::colors::text())
        };

        let mcp_kind = mcp_indicator.as_ref().map(|(kind, _)| *kind);
        let header_template_ctx = HeaderTemplateContext {
            title: header_title,
            model: model_display.as_str(),
            shell: shell_display.as_str(),
            reasoning: reasoning_display,
            directory: cwd_str.as_str(),
            branch: branch_display.as_str(),
            mcp: mcp_display.as_str(),
            mcp_kind,
            hovered_action,
            hover_style,
        };
        let top_text_with_regions = header_cfg
            .top_line_text
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| render_styled_header_template(value, &header_template_ctx));
        let top_status_line_items = self.status_line_top_items();
        let has_selected_status_line =
            top_text_with_regions.is_none() && !top_status_line_items.is_empty();
        let selected_status_line = if has_selected_status_line {
            Some(self.render_selected_status_line(
                &top_status_line_items,
                header_template_ctx.hovered_action.clone(),
                hover_style,
            ))
        } else {
            None
        };
        let bottom_text = header_cfg
            .bottom_line_text
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| render_plain_header_template(value, &header_template_ctx));
        let show_bottom_line = header_cfg.show_bottom_line && bottom_text.is_some();

        let mut status_lines: Vec<Line<'static>> = Vec::new();
        let mut custom_top_line_regions: Option<Vec<(std::ops::Range<usize>, ClickableAction)>> =
            None;
        let mut custom_top_line_width = 0usize;
        if header_cfg.show_top_line {
            if let Some(custom_top) = top_text_with_regions {
                custom_top_line_width = custom_top.width;
                custom_top_line_regions = Some(custom_top.clickable_ranges);
                status_lines.push(custom_top.line);
            } else if let Some(selected_top) = selected_status_line {
                custom_top_line_width = selected_top.width;
                custom_top_line_regions = Some(selected_top.clickable_ranges);
                status_lines.push(selected_top.line);
            } else {
                custom_top_line_width = dynamic_header.width;
                custom_top_line_regions = Some(dynamic_header.clickable_ranges.clone());
                status_lines.push(dynamic_header.line.clone());
            }
        }
        if show_bottom_line {
            // Safe unwrap: gated on bottom_text.is_some().
            status_lines.push(Line::from(bottom_text.unwrap_or_default()));
        }

        if status_lines.is_empty() {
            self.clickable_regions.borrow_mut().clear();
            *self.hovered_clickable_action.borrow_mut() = None;
            return;
        }

        let status_widget = Paragraph::new(status_lines)
            .alignment(ratatui::layout::Alignment::Center)
            .style(status_style);
        ratatui::widgets::Widget::render(status_widget, padded_inner, buf);

        // Track clickable regions for Model, Shell, and Reasoning
        if let Some(custom_ranges) = custom_top_line_regions {
            let top_line_area = Rect {
                x: padded_inner.x,
                y: padded_inner.y,
                width: padded_inner.width,
                height: 1,
            };
            self.track_status_bar_clickable_regions_from_char_ranges(
                &custom_ranges,
                top_line_area,
                custom_top_line_width,
            );
        } else {
            self.clickable_regions.borrow_mut().clear();
            *self.hovered_clickable_action.borrow_mut() = None;
        }
    }

    /// Calculate and store clickable regions using character-index ranges within
    /// a rendered top status line (used for custom header templates).
    fn track_status_bar_clickable_regions_from_char_ranges(
        &self,
        ranges: &[(std::ops::Range<usize>, ClickableAction)],
        area: Rect,
        total_width: usize,
    ) {
        let mut regions = self.clickable_regions.borrow_mut();
        *regions = centered_clickable_regions_from_char_ranges(ranges, area, total_width);
    }

    fn render_selected_status_line(
        &self,
        items: &[crate::bottom_pane::StatusLineItem],
        hovered_action: Option<ClickableAction>,
        hover_style: code_core::config_types::HeaderHoverStyle,
    ) -> super::terminal_surface_header::HeaderTemplateRender {
        use ratatui::style::Style;
        use ratatui::text::{Line, Span};

        let mut spans: Vec<Span<'static>> = Vec::new();
        let mut ranges: Vec<(std::ops::Range<usize>, ClickableAction)> = Vec::new();
        let mut width = 0usize;
        let mut added_any = false;

        for item in items {
            let Some(value) = self.status_line_value_for_item(*item) else {
                continue;
            };

            if added_any {
                spans.push(Span::styled(
                    " • ".to_string(),
                    Style::default().fg(crate::colors::text_dim()),
                ));
                width += 3;
            }
            added_any = true;

            let click_action = match item {
                crate::bottom_pane::StatusLineItem::ModelName => {
                    Some(ClickableAction::ShowModelSelector)
                }
                crate::bottom_pane::StatusLineItem::ModelWithReasoning => {
                    Some(ClickableAction::ShowReasoningSelector)
                }
                _ => None,
            };

            let segment_width = value.chars().count();
            let mut style = Style::default().fg(crate::colors::text());
            if let Some(action) = click_action.clone() {
                style = super::terminal_surface_header::apply_hover_style(
                    style,
                    hover_style,
                    hovered_action.as_ref() == Some(&action),
                );
                ranges.push((width..width + segment_width, action));
            }
            spans.push(Span::styled(value, style));
            width += segment_width;
        }

        if !added_any {
            let fallback = "Status line configured with no available values".to_string();
            width = fallback.chars().count();
            spans.push(Span::styled(
                fallback,
                Style::default().fg(crate::colors::text_dim()),
            ));
        }

        super::terminal_surface_header::HeaderTemplateRender {
            line: Line::from(spans),
            clickable_ranges: ranges,
            width,
        }
    }

    pub(super) fn render_plain_status_line(
        &self,
        items: &[crate::bottom_pane::StatusLineItem],
    ) -> Line<'static> {
        use ratatui::style::Style;
        use ratatui::text::Span;

        let mut spans: Vec<Span<'static>> = Vec::new();
        let mut added_any = false;

        for item in items {
            let Some(value) = self.status_line_value_for_item(*item) else {
                continue;
            };

            if added_any {
                spans.push(Span::styled(
                    " • ".to_string(),
                    Style::default().fg(crate::colors::text_dim()),
                ));
            }
            spans.push(Span::styled(value, Style::default().fg(crate::colors::text())));
            added_any = true;
        }

        if !added_any {
            return Line::from("");
        }

        Line::from(spans)
    }

    pub(super) fn render_bottom_status_line(&self, bottom_pane_area: Rect, buf: &mut Buffer) {
        use ratatui::layout::Alignment;
        use ratatui::style::Style;
        use ratatui::widgets::Paragraph;

        if self.standard_terminal_mode
            || bottom_pane_area.width == 0
            || bottom_pane_area.height == 0
            || self.bottom_pane.has_active_view()
            || !self.bottom_pane.top_spacer_enabled()
        {
            return;
        }

        let bottom_items = self.status_line_bottom_items();
        if bottom_items.is_empty() {
            return;
        }

        let horizontal_padding = 1u16;
        let line_area = Rect {
            x: bottom_pane_area.x.saturating_add(horizontal_padding),
            y: bottom_pane_area.y,
            width: bottom_pane_area.width.saturating_sub(horizontal_padding * 2),
            height: 1,
        };
        if line_area.width == 0 {
            return;
        }

        let line = self.render_plain_status_line(&bottom_items);
        let widget = Paragraph::new(vec![line])
            .alignment(Alignment::Center)
            .style(Style::default().fg(crate::colors::text_dim()));
        ratatui::widgets::Widget::render(widget, line_area, buf);
    }
}

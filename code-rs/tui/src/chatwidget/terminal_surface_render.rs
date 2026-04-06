use super::*;
use super::terminal_surface_header::HeaderTemplateContext;
use super::terminal_surface_header::DynamicHeaderLayoutInput;
use super::terminal_surface_header::McpHeaderIndicatorKind;
use super::terminal_surface_header::centered_clickable_regions_from_char_ranges;
use super::terminal_surface_header::scrollable_clickable_regions_from_char_ranges;
use super::terminal_surface_header::render_plain_header_template;
use super::terminal_surface_header::render_dynamic_header_line;
use super::terminal_surface_header::render_styled_header_template;
use crate::bottom_pane::settings_pages::status_line::StatusLineItem;
use unicode_width::UnicodeWidthStr;

type TrackedClickableLine = (
    usize,
    Vec<(std::ops::Range<usize>, ClickableAction)>,
    usize,
);

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
                                .map(ToString::to_string);
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
            return;
        }

        let header_cfg = &self.config.tui.header;
        if !header_cfg.show_top_line && !header_cfg.show_bottom_line {
            self.clickable_regions.borrow_mut().clear();
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
        let service_tier_display = if code_core::model_family::supports_service_tier(&self.config.model)
        {
            if matches!(
                self.config.service_tier,
                Some(code_core::config_types::ServiceTier::Fast)
            ) {
                "fast"
            } else {
                "standard"
            }
        } else {
            ""
        };
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
        // Build the header at full width — scroll handles overflow instead of
        // cascading segment removal.
        let dynamic_header = render_dynamic_header_line(
            &DynamicHeaderLayoutInput {
                title: header_title,
                model: model_display.as_str(),
                service_tier: service_tier_display,
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
                minimal_header: crate::tui_env::force_minimal_header(),
                demo_mode: self.config.demo_developer_message.is_some(),
                inner_width: usize::MAX,
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
            service_tier: service_tier_display,
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
            Some(self.render_selected_status_line_with_width(
                &top_status_line_items,
                header_template_ctx.hovered_action.clone(),
                hover_style,
                usize::MAX,
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
        let mut tracked_clickable_lines: Vec<TrackedClickableLine> = Vec::new();
        if header_cfg.show_top_line {
            if let Some(custom_top) = top_text_with_regions {
                tracked_clickable_lines.push((0, custom_top.clickable_ranges, custom_top.width));
                status_lines.push(custom_top.line);
            } else if let Some(selected_top) = selected_status_line {
                tracked_clickable_lines.push((
                    0,
                    selected_top.clickable_ranges,
                    selected_top.width,
                ));
                status_lines.push(selected_top.line);
            } else {
                tracked_clickable_lines.push((
                    0,
                    dynamic_header.clickable_ranges.clone(),
                    dynamic_header.width,
                ));
                status_lines.push(dynamic_header.line.clone());
            }
        }
        if show_bottom_line {
            // Safe unwrap: gated on bottom_text.is_some().
            status_lines.push(Line::from(bottom_text.unwrap_or_default()));
        }

        if status_lines.is_empty() {
            self.clickable_regions.borrow_mut().clear();
            return;
        }

        // Determine if horizontal scrolling is needed. The tracked lines
        // record the full content width; if any exceeds the viewport, we
        // switch from centered alignment to left-aligned with a scroll offset.
        let max_content_width = tracked_clickable_lines
            .iter()
            .map(|(_, _, w)| *w)
            .max()
            .unwrap_or(0);
        let overflows = max_content_width > inner_width;

        // Clamp scroll offset to valid range.
        let max_hscroll = if overflows {
            (max_content_width - inner_width) as u16
        } else {
            0
        };
        let hscroll = self.status_bar_hscroll.get().min(max_hscroll);
        self.status_bar_hscroll.set(hscroll);

        let (alignment, scroll_cols) = if overflows {
            (ratatui::layout::Alignment::Left, hscroll)
        } else {
            (ratatui::layout::Alignment::Center, 0u16)
        };

        let status_widget = Paragraph::new(status_lines)
            .alignment(alignment)
            .scroll((0, scroll_cols))
            .style(status_style);
        ratatui::widgets::Widget::render(status_widget, padded_inner, buf);

        // Render scroll indicators at the edges when content is clipped.
        if overflows {
            let indicator_style = ratatui::style::Style::default()
                .fg(crate::colors::text_dim())
                .bg(crate::colors::background());
            if hscroll > 0 {
                if let Some(cell) = buf.cell_mut(ratatui::layout::Position::new(padded_inner.x, padded_inner.y)) {
                    cell.set_char('◂');
                    cell.set_style(indicator_style);
                }
            }
            if hscroll < max_hscroll {
                let right_x = padded_inner.x.saturating_add(padded_inner.width).saturating_sub(1);
                if let Some(cell) = buf.cell_mut(ratatui::layout::Position::new(right_x, padded_inner.y)) {
                    cell.set_char('▸');
                    cell.set_style(indicator_style);
                }
            }
        }

        self.clickable_regions.borrow_mut().clear();
        for (line_offset, ranges, total_width) in tracked_clickable_lines {
            let line_area = Rect {
                x: padded_inner.x,
                y: padded_inner.y.saturating_add(line_offset as u16),
                width: padded_inner.width,
                height: 1,
            };
            if overflows {
                let mut regions = self.clickable_regions.borrow_mut();
                regions.extend(scrollable_clickable_regions_from_char_ranges(
                    &ranges,
                    line_area,
                    total_width,
                    hscroll,
                ));
            } else {
                self.append_status_bar_clickable_regions_from_char_ranges(
                    &ranges,
                    line_area,
                    total_width,
                );
            }
        }
    }

    /// Append clickable regions using character-index ranges within a rendered
    /// status/header line.
    fn append_status_bar_clickable_regions_from_char_ranges(
        &self,
        ranges: &[(std::ops::Range<usize>, ClickableAction)],
        area: Rect,
        total_width: usize,
    ) {
        let mut regions = self.clickable_regions.borrow_mut();
        regions.extend(centered_clickable_regions_from_char_ranges(
            ranges,
            area,
            total_width,
        ));
    }

    fn render_selected_status_line_with_width(
        &self,
        items: &[StatusLineItem],
        hovered_action: Option<ClickableAction>,
        hover_style: code_core::config_types::HeaderHoverStyle,
        max_width: usize,
    ) -> super::terminal_surface_header::HeaderTemplateRender {
        // Resolve values for all items, remembering which have values.
        let resolved: Vec<(StatusLineItem, String)> = items
            .iter()
            .filter_map(|item| {
                self.status_line_value_for_item(*item)
                    .map(|v| (*item, v))
            })
            .collect();

        // Try with all items first; drop from the end if it doesn't fit.
        let mut count = resolved.len();
        loop {
            let render = self.build_status_line_spans(
                &resolved[..count],
                &hovered_action,
                hover_style,
                max_width,
            );
            if render.width <= max_width || count <= 1 {
                return render;
            }
            count -= 1;
        }
    }

    /// Build a status line from already-resolved (item, value) pairs,
    /// truncating individual values that exceed the per-segment budget.
    fn build_status_line_spans(
        &self,
        resolved: &[(StatusLineItem, String)],
        hovered_action: &Option<ClickableAction>,
        hover_style: code_core::config_types::HeaderHoverStyle,
        max_width: usize,
    ) -> super::terminal_surface_header::HeaderTemplateRender {
        use ratatui::style::Style;
        use ratatui::text::{Line, Span};

        let mut spans: Vec<Span<'static>> = Vec::new();
        let mut ranges: Vec<(std::ops::Range<usize>, ClickableAction)> = Vec::new();
        let mut width = 0usize;
        let mut added_any = false;

        // Compute a per-segment width budget: split available width evenly
        // among segments, minus separator overhead.
        let n = resolved.len();
        let separator_overhead = if n > 1 { (n - 1) * 3 } else { 0 };
        let per_segment_budget = if n > 0 {
            max_width.saturating_sub(separator_overhead) / n
        } else {
            max_width
        };

        for (item, value) in resolved {
            if added_any {
                spans.push(Span::styled(
                    " • ".to_string(),
                    Style::default().fg(crate::colors::text_dim()),
                ));
                width += 3;
            }
            added_any = true;

            let click_action = match item {
                StatusLineItem::ModelName => {
                    Some(ClickableAction::ShowModelSelector)
                }
                StatusLineItem::ModelWithReasoning => {
                    Some(ClickableAction::ShowReasoningSelector)
                }
                StatusLineItem::ServiceTier => {
                    Some(ClickableAction::ToggleServiceTier)
                }
                StatusLineItem::Shell | StatusLineItem::ShellStyle => {
                    Some(ClickableAction::ShowShellSelector)
                }
                StatusLineItem::CurrentDir | StatusLineItem::ProjectRoot => {
                    crate::platform_caps::supports_native_picker()
                        .then_some(ClickableAction::ShowDirectoryPicker)
                }
                #[cfg(feature = "managed-network-proxy")]
                StatusLineItem::NetworkMediation => Some(ClickableAction::ShowNetworkSettings),
                _ => None,
            };

            // Truncate the value if it exceeds the per-segment budget.
            let display_value = if value.width() > per_segment_budget && per_segment_budget > 1 {
                crate::text_formatting::truncate_to_display_width_with_suffix(
                    value,
                    per_segment_budget,
                    "…",
                )
            } else {
                value.clone()
            };

            let segment_width = display_value.width();
            let mut style = Style::default().fg(crate::colors::text());
            if let Some(action) = click_action.clone() {
                style = super::terminal_surface_header::apply_hover_style(
                    style,
                    hover_style,
                    hovered_action.as_ref() == Some(&action),
                );
                ranges.push((width..width + segment_width, action));
            }
            spans.push(Span::styled(display_value, style));
            width += segment_width;
        }

        if !added_any {
            let fallback = "Status line configured with no available values".to_string();
            width = fallback.width();
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

    // (startup model migration notice removed)

    pub(super) fn render_bottom_status_line(&self, bottom_pane_area: Rect, buf: &mut Buffer) {
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

        let hovered_action = self.hovered_clickable_action.borrow().clone();
        let hover_style = self.config.tui.header.hover_style;
        // Build with full width — scroll handles overflow.
        let rendered = self.render_selected_status_line_with_width(
            &bottom_items,
            hovered_action,
            hover_style,
            usize::MAX,
        );

        let viewport_width = line_area.width as usize;
        let overflows = rendered.width > viewport_width;
        let max_hscroll = if overflows {
            (rendered.width - viewport_width) as u16
        } else {
            0
        };
        let hscroll = self.bottom_status_hscroll.get().min(max_hscroll);
        self.bottom_status_hscroll.set(hscroll);

        let (alignment, scroll_cols) = if overflows {
            (ratatui::layout::Alignment::Left, hscroll)
        } else {
            (ratatui::layout::Alignment::Center, 0u16)
        };

        // Add clickable regions for the bottom status line.
        {
            if overflows {
                let mut regions = self.clickable_regions.borrow_mut();
                regions.extend(scrollable_clickable_regions_from_char_ranges(
                    &rendered.clickable_ranges,
                    line_area,
                    rendered.width,
                    hscroll,
                ));
            } else {
                let mut regions = self.clickable_regions.borrow_mut();
                regions.extend(centered_clickable_regions_from_char_ranges(
                    &rendered.clickable_ranges,
                    line_area,
                    rendered.width,
                ));
            }
        }

        let base_style = Style::default().fg(crate::colors::text_dim());
        let widget = Paragraph::new(vec![rendered.line])
            .alignment(alignment)
            .scroll((0, scroll_cols))
            .style(base_style);
        ratatui::widgets::Widget::render(widget, line_area, buf);

        // Scroll indicators for bottom status line.
        if overflows {
            let indicator_style = ratatui::style::Style::default()
                .fg(crate::colors::text_dim())
                .bg(crate::colors::background());
            if hscroll > 0 {
                if let Some(cell) = buf.cell_mut(ratatui::layout::Position::new(line_area.x, line_area.y)) {
                    cell.set_char('◂');
                    cell.set_style(indicator_style);
                }
            }
            if hscroll < max_hscroll {
                let right_x = line_area.x.saturating_add(line_area.width).saturating_sub(1);
                if let Some(cell) = buf.cell_mut(ratatui::layout::Position::new(right_x, line_area.y)) {
                    cell.set_char('▸');
                    cell.set_style(indicator_style);
                }
            }
        }
    }
}

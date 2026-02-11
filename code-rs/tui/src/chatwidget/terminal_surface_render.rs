use super::*;

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

    fn get_git_branch(&self) -> Option<String> {
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

    pub(super) fn render_status_bar(&self, area: Rect, buf: &mut Buffer) {
        use crate::exec_command::relativize_to_home;
        use ratatui::layout::Margin;
        use ratatui::style::Modifier;
        use ratatui::style::Style;
        use ratatui::text::Line;
        use ratatui::text::Span;
        use ratatui::widgets::Block;
        use ratatui::widgets::Borders;
        use ratatui::widgets::Paragraph;

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

        // Build status line spans with dynamic elision based on width.
        // Removal priority when space is tight:
        //   1) Reasoning level
        //   2) Model
        //   3) Shell
        //   4) Branch
        //   5) Directory
        let branch_opt = self.get_git_branch();

        // Determine current shell display (configured override or $SHELL fallback)
        let shell_display = match &self.config.shell {
            Some(shell) => format!("{} {}", shell.path, shell.args.join(" ")).trim().to_string(),
            None => std::env::var("SHELL").ok().unwrap_or_else(|| "sh".to_string()),
        };

        // Helper to assemble spans based on include flags
        let build_spans = |include_reasoning: bool,
                           include_model: bool,
                           include_shell: bool,
                           include_branch: bool,
                           include_dir: bool,
                           dir_display: &str| {
            let mut spans: Vec<Span> = Vec::new();
            // Title follows theme text color
            spans.push(Span::styled(
                "Every Code",
                Style::default()
                    .fg(crate::colors::text())
                    .add_modifier(Modifier::BOLD),
            ));

            if include_model {
                spans.push(Span::styled(
                    "  •  ",
                    Style::default().fg(crate::colors::text_dim()),
                ));
                spans.push(Span::styled(
                    "Model: ",
                    Style::default().fg(crate::colors::text_dim()),
                ));
                spans.push(Span::styled(
                    self.format_model_name(&self.config.model),
                    Style::default().fg(crate::colors::info()),
                ));
            }

            if include_shell {
                spans.push(Span::styled(
                    "  •  ",
                    Style::default().fg(crate::colors::text_dim()),
                ));
                spans.push(Span::styled(
                    "Shell: ",
                    Style::default().fg(crate::colors::text_dim()),
                ));
                spans.push(Span::styled(
                    shell_display.clone(),
                    Style::default().fg(crate::colors::info()),
                ));
            }

            if include_reasoning {
                spans.push(Span::styled(
                    "  •  ",
                    Style::default().fg(crate::colors::text_dim()),
                ));
                spans.push(Span::styled(
                    "Reasoning: ",
                    Style::default().fg(crate::colors::text_dim()),
                ));
                spans.push(Span::styled(
                    Self::format_reasoning_effort(self.config.model_reasoning_effort),
                    Style::default().fg(crate::colors::info()),
                ));
            }

            if include_dir {
                spans.push(Span::styled(
                    "  •  ",
                    Style::default().fg(crate::colors::text_dim()),
                ));
                spans.push(Span::styled(
                    "Directory: ",
                    Style::default().fg(crate::colors::text_dim()),
                ));
                spans.push(Span::styled(
                    dir_display.to_string(),
                    Style::default().fg(crate::colors::info()),
                ));
            }

            if include_branch
                && let Some(branch) = &branch_opt {
                    spans.push(Span::styled(
                        "  •  ",
                        Style::default().fg(crate::colors::text_dim()),
                    ));
                    spans.push(Span::styled(
                        "Branch: ",
                        Style::default().fg(crate::colors::text_dim()),
                    ));
                    spans.push(Span::styled(
                        branch.clone(),
                        Style::default().fg(crate::colors::success_green()),
                    ));
                }

            // Footer already shows the Ctrl+R hint; avoid duplicating it here.

            spans
        };

        // Start with all items in production; tests can opt-in to a minimal header via env flag.
        let minimal_header = std::env::var_os("CODEX_TUI_FORCE_MINIMAL_HEADER").is_some();
        let demo_mode = self.config.demo_developer_message.is_some();
        let mut include_reasoning = !minimal_header;
        let mut include_model = !minimal_header;
        let mut include_shell = !minimal_header;
        let mut include_branch = !minimal_header && branch_opt.is_some();
        let mut include_dir = !minimal_header && !demo_mode;
        let mut use_short_dir = false;
        let mut status_spans = build_spans(
            include_reasoning,
            include_model,
            include_shell,
            include_branch,
            include_dir,
            &cwd_str,
        );

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

        // Helper to measure current spans width
        let measure =
            |spans: &Vec<Span>| -> usize { spans.iter().map(|s| s.content.chars().count()).sum() };

        if include_dir && !use_short_dir && measure(&status_spans) > inner_width {
            use_short_dir = true;
            status_spans = build_spans(
                include_reasoning,
                include_model,
                include_shell,
                include_branch,
                include_dir,
                &cwd_short_str,
            );
        }

        // Elide items in priority order until content fits
        while measure(&status_spans) > inner_width {
            if include_reasoning {
                include_reasoning = false;
            } else if include_model {
                include_model = false;
            } else if include_shell {
                include_shell = false;
            } else if include_branch {
                include_branch = false;
            } else if include_dir {
                include_dir = false;
            } else {
                break;
            }
            status_spans = build_spans(
                include_reasoning,
                include_model,
                include_shell,
                include_branch,
                include_dir,
                if use_short_dir { &cwd_short_str } else { &cwd_str },
            );
        }

        // Note: The reasoning visibility hint is appended inside `build_spans`
        // so it participates in width measurement and elision. Do not append
        // it again here to avoid overflow that caused corrupted glyph boxes on
        // some terminals.

        let status_line = Line::from(status_spans);

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

        let status_widget = Paragraph::new(vec![status_line.clone()])
            .alignment(ratatui::layout::Alignment::Center)
            .style(status_style);
        ratatui::widgets::Widget::render(status_widget, padded_inner, buf);

        // Track clickable regions for Model, Shell, and Reasoning
        self.track_status_bar_clickable_regions(
            &status_line.spans,
            padded_inner,
            include_model,
            include_shell,
            include_reasoning,
        );
    }

    /// Calculate and store clickable regions for the status bar (Model, Shell, Reasoning)
    fn track_status_bar_clickable_regions(
        &self,
        spans: &[Span],
        area: Rect,
        include_model: bool,
        include_shell: bool,
        include_reasoning: bool,
    ) {
        // Calculate total width of all spans
        let total_width: usize = spans.iter().map(|s| s.content.chars().count()).sum();
        
        // Calculate starting x position for centered text
        let start_x = if total_width < area.width as usize {
            area.x + ((area.width as usize - total_width) / 2) as u16
        } else {
            area.x
        };
        
        let mut current_x = start_x;
        let mut regions = self.clickable_regions.borrow_mut();
        regions.clear();  // Clear previous frame's regions
        
        // Scan through spans to find Model, Shell, and Reasoning sections
        let mut i = 0;
        while i < spans.len() {
            let span = &spans[i];
            let content = span.content.as_ref();
            
            // Check if this is a clickable label
            if include_model && content.contains("Model:") {
                // Find the extent of the Model section (label + value)
                let mut section_width = content.chars().count();
                if i + 1 < spans.len() {
                    // Include the value span
                    section_width += spans[i + 1].content.chars().count();
                }
                regions.push(ClickableRegion {
                    rect: Rect {
                        x: current_x,
                        y: area.y,
                        width: section_width as u16,
                        height: 1,
                    },
                    action: ClickableAction::ShowModelSelector,
                });
                current_x += content.chars().count() as u16;
                i += 1;
                if i < spans.len() {
                    current_x += spans[i].content.chars().count() as u16;
                    i += 1;
                }
                continue;
            }
            
            if include_shell && content.contains("Shell:") {
                let mut section_width = content.chars().count();
                if i + 1 < spans.len() {
                    section_width += spans[i + 1].content.chars().count();
                }
                regions.push(ClickableRegion {
                    rect: Rect {
                        x: current_x,
                        y: area.y,
                        width: section_width as u16,
                        height: 1,
                    },
                    action: ClickableAction::ShowShellSelector,
                });
                current_x += content.chars().count() as u16;
                i += 1;
                if i < spans.len() {
                    current_x += spans[i].content.chars().count() as u16;
                    i += 1;
                }
                continue;
            }
            
            if include_reasoning && content.contains("Reasoning:") {
                let mut section_width = content.chars().count();
                if i + 1 < spans.len() {
                    section_width += spans[i + 1].content.chars().count();
                }
                regions.push(ClickableRegion {
                    rect: Rect {
                        x: current_x,
                        y: area.y,
                        width: section_width as u16,
                        height: 1,
                    },
                    action: ClickableAction::ShowReasoningSelector,
                });
                current_x += content.chars().count() as u16;
                i += 1;
                if i < spans.len() {
                    current_x += spans[i].content.chars().count() as u16;
                    i += 1;
                }
                continue;
            }
            
            // Not a clickable section, just advance position
            current_x += content.chars().count() as u16;
            i += 1;
        }
    }

    pub(super) fn render_screenshot_highlevel(&self, path: &PathBuf, area: Rect, buf: &mut Buffer) {
        use ratatui::widgets::Widget;
        use ratatui_image::Image;
        use ratatui_image::Resize;
        use ratatui_image::picker::Picker;
        use ratatui_image::picker::ProtocolType;

        // First, cheaply read image dimensions without decoding the full image
        let (img_w, img_h) = match image::image_dimensions(path) {
            Ok(dim) => dim,
            Err(_) => {
                self.render_screenshot_placeholder(path, area, buf);
                return;
            }
        };

        // picker (Retina 2x workaround preserved)
        let mut cached_picker = self.cached_picker.borrow_mut();
        if cached_picker.is_none() {
            // If we didn't get a picker from terminal query at startup, create one from font size
            let (fw, fh) = self.measured_font_size();
            let p = Picker::from_fontsize((fw, fh));

            *cached_picker = Some(p);
        }
        let Some(picker) = cached_picker.as_ref() else {
            self.render_screenshot_placeholder(path, area, buf);
            return;
        };

        // quantize step by protocol to avoid rounding bias
        let (_qx, _qy): (u16, u16) = match picker.protocol_type() {
            ProtocolType::Halfblocks => (1, 2), // half-block cell = 1 col x 2 half-rows
            _ => (1, 1),                        // pixel protocols (Kitty/iTerm2/Sixel)
        };

        // terminal cell aspect
        let (cw, ch) = self.measured_font_size();
        let cols = area.width as u32;
        let rows = area.height as u32;
        let cw = cw as u32;
        let ch = ch as u32;

        // fit (floor), then choose limiting dimension
        let mut rows_by_w = (cols * cw * img_h) / (img_w * ch);
        if rows_by_w == 0 {
            rows_by_w = 1;
        }
        let mut cols_by_h = (rows * ch * img_w) / (img_h * cw);
        if cols_by_h == 0 {
            cols_by_h = 1;
        }

        let (_used_cols, _used_rows) = if rows_by_w <= rows {
            (cols, rows_by_w)
        } else {
            (cols_by_h, rows)
        };

        // Compute a centered target rect based on image aspect and font cell size
        let (cell_w, cell_h) = self.measured_font_size();
        let area_px_w = (area.width as u32) * (cell_w as u32);
        let area_px_h = (area.height as u32) * (cell_h as u32);
        // If either dimension is zero, bail to placeholder
        if area.width == 0 || area.height == 0 || area_px_w == 0 || area_px_h == 0 {
            self.render_screenshot_placeholder(path, area, buf);
            return;
        }
        let (img_w, img_h) = match image::image_dimensions(path) {
            Ok(dim) => dim,
            Err(_) => {
                self.render_screenshot_placeholder(path, area, buf);
                return;
            }
        };
        let scale_num_w = area_px_w;
        let scale_num_h = area_px_h;
        let scale_w = scale_num_w as f64 / img_w as f64;
        let scale_h = scale_num_h as f64 / img_h as f64;
        let scale = scale_w.min(scale_h).max(0.0);
        // Compute target size in cells
        let target_w_cells = ((img_w as f64 * scale) / (cell_w as f64)).floor() as u16;
        let target_h_cells = ((img_h as f64 * scale) / (cell_h as f64)).floor() as u16;
        let target_w = target_w_cells.clamp(1, area.width);
        let target_h = target_h_cells.clamp(1, area.height);
        let target_x = area.x + (area.width.saturating_sub(target_w)) / 2;
        let target_y = area.y + (area.height.saturating_sub(target_h)) / 2;
        let target = Rect {
            x: target_x,
            y: target_y,
            width: target_w,
            height: target_h,
        };

        // cache by (path, target)
        let needs_recreate = {
            let cached = self.cached_image_protocol.borrow();
            match cached.as_ref() {
                Some((cached_path, cached_rect, _)) => {
                    cached_path != path || *cached_rect != target
                }
                None => true,
            }
        };
        if needs_recreate {
            // Only decode when we actually need to (path/target changed)
            let dyn_img = match image::ImageReader::open(path) {
                Ok(r) => match r.decode() {
                    Ok(img) => img,
                    Err(_) => {
                        self.render_screenshot_placeholder(path, area, buf);
                        return;
                    }
                },
                Err(_) => {
                    self.render_screenshot_placeholder(path, area, buf);
                    return;
                }
            };
            match picker.new_protocol(dyn_img, target, Resize::Fit(Some(FilterType::Lanczos3))) {
                Ok(protocol) => {
                    *self.cached_image_protocol.borrow_mut() =
                        Some((path.clone(), target, protocol))
                }
                Err(_) => {
                    self.render_screenshot_placeholder(path, area, buf);
                    return;
                }
            }
        }

        if let Some((_, rect, protocol)) = &*self.cached_image_protocol.borrow() {
            let image = Image::new(protocol);
            Widget::render(image, *rect, buf);
        } else {
            self.render_screenshot_placeholder(path, area, buf);
        }
    }

    fn render_screenshot_placeholder(&self, path: &Path, area: Rect, buf: &mut Buffer) {
        use ratatui::style::Modifier;
        use ratatui::style::Style;
        use ratatui::widgets::Block;
        use ratatui::widgets::Borders;
        use ratatui::widgets::Paragraph;

        // Show a placeholder box with screenshot info
        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("screenshot");

        let placeholder_text = format!("[Screenshot]\n{filename}");
        let placeholder_widget = Paragraph::new(placeholder_text)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(crate::colors::info()))
                    .title("Browser"),
            )
            .style(
                Style::default()
                    .fg(crate::colors::text_dim())
                    .add_modifier(Modifier::ITALIC),
            )
            .wrap(ratatui::widgets::Wrap { trim: true });

        placeholder_widget.render(area, buf);
    }
}

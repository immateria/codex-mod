impl LimitsSettingsContent {
    const WIDE_LAYOUT_MIN_WIDTH: u16 = 110;
    const WIDE_GUTTER_WIDTH: u16 = 1;
    const WIDE_MIN_LEFT_WIDTH: u16 = 42;
    const WIDE_MIN_RIGHT_WIDTH: u16 = 52;
    const WIDE_MAX_LEFT_PERCENT: u16 = 58;

    pub(crate) fn new(content: LimitsOverlayContent, layout_mode: ConfigLimitsLayoutMode) -> Self {
        Self {
            overlay: LimitsOverlay::new(content),
            layout_mode: LimitsLayoutMode::from_config(layout_mode),
            pane_focus: LimitsPaneFocus::Sync,
            left_scroll: Cell::new(0),
            right_scroll: Cell::new(0),
            left_max_scroll: Cell::new(0),
            right_max_scroll: Cell::new(0),
            last_wide_active: Cell::new(false),
        }
    }

    pub(crate) fn layout_mode_config(&self) -> ConfigLimitsLayoutMode {
        self.layout_mode.to_config()
    }

    pub(crate) fn set_content(&mut self, content: LimitsOverlayContent) {
        self.overlay.set_content(content);
    }

    fn set_wide_active(&self, active: bool) {
        self.last_wide_active.set(active);
        if !active {
            self.left_max_scroll.set(0);
            self.right_max_scroll.set(0);
        }
    }

    fn render_tabs(&self, area: Rect, buf: &mut Buffer) {
        if let Some(tabs) = self.overlay.tabs() {
            LimitsTabsRowWidget {
                tabs,
                selected_tab: self.overlay.selected_tab(),
            }
            .render(area, buf);
        }
    }

    fn render_body(&self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            self.overlay.set_visible_rows(0);
            self.overlay.set_max_scroll(0);
            self.set_wide_active(false);
            return;
        }

        self.overlay.set_visible_rows(area.height);

        let lines = self.overlay.lines_for_width(area.width);
        if self.layout_mode == LimitsLayoutMode::SingleColumn {
            self.set_wide_active(false);
            return self.render_single_column(area, buf, &lines);
        }

        let Some((left_lines, right_lines)) = self.wide_lines(&lines, area.width) else {
            self.set_wide_active(false);
            return self.render_single_column(area, buf, &lines);
        };
        let Some((left_area, gutter_area, right_area)) = Self::wide_areas(area, &left_lines) else {
            self.set_wide_active(false);
            return self.render_single_column(area, buf, &lines);
        };

        self.set_wide_active(true);
        self.update_wide_bounds(&left_lines, &right_lines, area.height);

        fill_rect(
            buf,
            gutter_area,
            Some('│'),
            Style::default()
                .fg(crate::colors::text_dim())
                .bg(crate::colors::background()),
        );

        let (left_start, right_start, left_focused, right_focused) =
            match self.effective_focus_mode() {
                LimitsPaneFocus::Sync => {
                    let sync = self.overlay.scroll() as usize;
                    self.left_scroll.set(sync as u16);
                    self.right_scroll.set(sync as u16);
                    (sync, sync, true, true)
                }
                LimitsPaneFocus::Left => {
                    let left = self.left_scroll.get() as usize;
                    let right = self.right_scroll.get() as usize;
                    (left, right, true, false)
                }
                LimitsPaneFocus::Right => {
                    let left = self.left_scroll.get() as usize;
                    let right = self.right_scroll.get() as usize;
                    (left, right, false, true)
                }
            };

        let left_end = left_start.saturating_add(area.height as usize);
        let right_end = right_start.saturating_add(area.height as usize);

        let left_viewport = Self::viewport_lines(&left_lines, left_start, left_end);
        let right_viewport = Self::viewport_lines(&right_lines, right_start, right_end);

        LimitsPaneWidget {
            title: "Summary",
            lines: left_viewport,
            focused: left_focused,
        }
        .render(left_area, buf);

        LimitsPaneWidget {
            title: "Chart & History",
            lines: right_viewport,
            focused: right_focused,
        }
        .render(right_area, buf);
    }

    fn render_single_column(&self, area: Rect, buf: &mut Buffer, lines: &[Line<'static>]) {
        let max_scroll = lines.len().saturating_sub(area.height as usize) as u16;
        self.overlay.set_max_scroll(max_scroll);

        let start = self.overlay.scroll() as usize;
        let end = start.saturating_add(area.height as usize);
        let viewport = Self::viewport_lines(lines, start, end);

        LimitsSingleBodyWidget { lines: viewport }.render(area, buf);
    }

    fn viewport_lines(lines: &[Line<'static>], start: usize, end: usize) -> Vec<Line<'static>> {
        let bounded_end = end.min(lines.len());
        if start < bounded_end {
            lines[start..bounded_end].to_vec()
        } else {
            Vec::new()
        }
    }

    fn slice_range(lines: &[Line<'static>], range: Range<usize>) -> Vec<Line<'static>> {
        if range.start >= range.end || range.end > lines.len() {
            return Vec::new();
        }
        lines[range.start..range.end].to_vec()
    }

    fn structured_wide_lines(&self, detail_width: u16) -> Option<(Vec<Line<'static>>, Vec<Line<'static>>)> {
        let tabs = self.overlay.tabs()?;
        let tab = tabs.get(self.overlay.selected_tab())?;
        match &tab.body {
            LimitsTabBody::View(view) => {
                let mut left = Vec::new();
                if !tab.header.is_empty() {
                    left.extend(tab.header.clone());
                    left.push(Line::from(String::new()));
                }

                let summary = Self::summary_lines_without_chart_header(&view.summary_lines);
                left.extend(summary);
                if !view.footer_lines.is_empty() {
                    let left_last_is_blank = left.last().is_none_or(Self::line_is_blank);
                    if !left_last_is_blank {
                        left.push(Line::from(String::new()));
                    }
                    left.extend(view.footer_lines.clone());
                }

                let mut right = view.gauge_lines(detail_width);
                if !view.legend_lines.is_empty() {
                    right.extend(view.legend_lines.clone());
                }
                if !tab.extra.is_empty() {
                    let right_last_is_blank = right.last().is_none_or(Self::line_is_blank);
                    if !right_last_is_blank {
                        right.push(Line::from(String::new()));
                    }
                    right.extend(tab.extra.clone());
                }
                Some((left, right))
            }
            LimitsTabBody::Lines(_) => None,
        }
    }

    fn summary_lines_without_chart_header(lines: &[Line<'static>]) -> Vec<Line<'static>> {
        if lines.is_empty() {
            return Vec::new();
        }

        let mut end = lines.len();
        while end > 0 && Self::line_is_blank(&lines[end - 1]) {
            end -= 1;
        }
        if end == 0 {
            return Vec::new();
        }

        let last_text = Self::line_text(&lines[end - 1]);
        if last_text.trim() == "Chart" {
            end -= 1;
            while end > 0 && Self::line_is_blank(&lines[end - 1]) {
                end -= 1;
            }
        }
        lines[..end].to_vec()
    }

    fn split_for_wide_layout(lines: &[Line<'static>], width: u16) -> Option<(Range<usize>, Range<usize>)> {
        if width < Self::WIDE_LAYOUT_MIN_WIDTH {
            return None;
        }

        let split_idx = lines.iter().position(Self::is_chart_or_history_header)?;
        if split_idx == 0 || split_idx >= lines.len() {
            return None;
        }

        let left_end = lines[..split_idx]
            .iter()
            .rposition(|line| !Self::line_is_blank(line))
            .map(|idx| idx + 1)
            .unwrap_or(split_idx);
        if left_end == 0 {
            return None;
        }

        let right_start = split_idx;
        if right_start >= lines.len() {
            return None;
        }
        Some((0..left_end, right_start..lines.len()))
    }

    fn is_chart_or_history_header(line: &Line<'static>) -> bool {
        let text = Self::line_text(line);
        let trimmed = text.trim();
        trimmed == "Chart" || trimmed == "12 Hour History"
    }

    fn line_text(line: &Line<'static>) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>()
    }

    fn line_is_blank(line: &Line<'static>) -> bool {
        line.spans
            .iter()
            .all(|span| span.content.chars().all(char::is_whitespace))
    }

    fn line_display_width(line: &Line<'static>) -> usize {
        line.spans
            .iter()
            .map(|span| UnicodeWidthStr::width(span.content.as_ref()))
            .sum()
    }

    fn render_hint_row(&self, area: Rect, buf: &mut Buffer, has_tabs: bool) {
        LimitsHintRowWidget {
            has_tabs,
            layout_mode: self.layout_mode,
            pane_focus: self.effective_focus_mode(),
        }
        .render(area, buf);
    }

    fn wide_lines(
        &self,
        lines: &[Line<'static>],
        width: u16,
    ) -> Option<(Vec<Line<'static>>, Vec<Line<'static>>)> {
        let default = Self::split_for_wide_layout(lines, width).map(|(left_range, right_range)| {
            let left = Self::slice_range(lines, left_range);
            let right = Self::slice_range(lines, right_range);
            (left, right)
        });

        let structured = self
            .structured_wide_lines(width)
            .filter(|(_, right)| !right.is_empty());

        match (structured, default) {
            (Some((left, right)), _) => Some((left, right)),
            (None, Some((left, right))) if !left.is_empty() && !right.is_empty() => Some((left, right)),
            _ => None,
        }
    }

    fn wide_areas(area: Rect, left_lines: &[Line<'static>]) -> Option<(Rect, Rect, Rect)> {
        if area.width < Self::WIDE_LAYOUT_MIN_WIDTH {
            return None;
        }
        let gutter_width = Self::WIDE_GUTTER_WIDTH;
        let available = area.width.saturating_sub(gutter_width);
        if available < Self::WIDE_MIN_LEFT_WIDTH.saturating_add(Self::WIDE_MIN_RIGHT_WIDTH) {
            return None;
        }

        let preferred = Self::preferred_left_width(left_lines);
        let max_left = available.saturating_mul(Self::WIDE_MAX_LEFT_PERCENT) / 100;
        let left_width = preferred
            .max(Self::WIDE_MIN_LEFT_WIDTH)
            .min(max_left)
            .min(available.saturating_sub(Self::WIDE_MIN_RIGHT_WIDTH));
        let right_width = available.saturating_sub(left_width);
        if left_width < Self::WIDE_MIN_LEFT_WIDTH || right_width < Self::WIDE_MIN_RIGHT_WIDTH {
            return None;
        }

        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(left_width),
                Constraint::Length(gutter_width),
                Constraint::Length(right_width),
            ])
            .split(area);
        Some((chunks[0], chunks[1], chunks[2]))
    }

    fn preferred_left_width(lines: &[Line<'static>]) -> u16 {
        let widest = lines.iter().map(Self::line_display_width).max().unwrap_or(0);
        widest.saturating_add(2).min(u16::MAX as usize) as u16
    }

    fn tab_at(&self, tabs_area: Rect, mouse_event: MouseEvent) -> Option<usize> {
        if self.overlay.tab_count() <= 1 || tabs_area.width == 0 || tabs_area.height == 0 {
            return None;
        }

        if mouse_event.column < tabs_area.x
            || mouse_event.column >= tabs_area.x.saturating_add(tabs_area.width)
            || mouse_event.row < tabs_area.y
            || mouse_event.row >= tabs_area.y.saturating_add(tabs_area.height)
        {
            return None;
        }

        let tabs = self.overlay.tabs()?;
        let mut x = tabs_area.x;
        for (idx, tab) in tabs.iter().enumerate() {
            let tab_width = UnicodeWidthStr::width(tab.title.as_str()) as u16 + 2;
            let start = x;
            let end = start.saturating_add(tab_width);
            if mouse_event.column >= start && mouse_event.column < end {
                return Some(idx);
            }
            x = end.saturating_add(1);
            if x >= tabs_area.x.saturating_add(tabs_area.width) {
                break;
            }
        }
        None
    }

    fn content_areas(area: Rect, has_tabs: bool) -> (Rect, Option<Rect>, Rect) {
        let constraints = if has_tabs {
            vec![Constraint::Length(1), Constraint::Length(2), Constraint::Fill(1)]
        } else {
            vec![Constraint::Length(1), Constraint::Fill(1)]
        };

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(constraints)
            .split(area);

        if has_tabs {
            (chunks[0], Some(chunks[1]), chunks[2])
        } else {
            (chunks[0], None, chunks[1])
        }
    }

    fn update_wide_bounds(&self, left_lines: &[Line<'static>], right_lines: &[Line<'static>], height: u16) {
        let left_max = left_lines.len().saturating_sub(height as usize) as u16;
        let right_max = right_lines.len().saturating_sub(height as usize) as u16;

        self.left_max_scroll.set(left_max);
        self.right_max_scroll.set(right_max);

        let left = self.left_scroll.get().min(left_max);
        let right = self.right_scroll.get().min(right_max);
        self.left_scroll.set(left);
        self.right_scroll.set(right);

        let sync_max = left_max.max(right_max);
        self.overlay.set_max_scroll(sync_max);
        self.overlay.set_scroll(self.overlay.scroll().min(sync_max));
    }

    fn effective_focus_mode(&self) -> LimitsPaneFocus {
        if self.last_wide_active.get() {
            self.pane_focus
        } else {
            LimitsPaneFocus::Sync
        }
    }

    fn active_scroll_target_for_keyboard(&self) -> ScrollTarget {
        match self.effective_focus_mode() {
            LimitsPaneFocus::Sync => ScrollTarget::Sync,
            LimitsPaneFocus::Left => ScrollTarget::Left,
            LimitsPaneFocus::Right => ScrollTarget::Right,
        }
    }

    fn scroll_value_for(&self, target: ScrollTarget) -> u16 {
        match target {
            ScrollTarget::Sync => self.overlay.scroll(),
            ScrollTarget::Left => self.left_scroll.get(),
            ScrollTarget::Right => self.right_scroll.get(),
        }
    }

    fn max_scroll_for(&self, target: ScrollTarget) -> u16 {
        match target {
            ScrollTarget::Sync => self.overlay.max_scroll(),
            ScrollTarget::Left => self.left_max_scroll.get(),
            ScrollTarget::Right => self.right_max_scroll.get(),
        }
    }

    fn set_scroll_for(&self, target: ScrollTarget, value: u16) {
        let max = self.max_scroll_for(target);
        let clamped = value.min(max);
        match target {
            ScrollTarget::Sync => self.overlay.set_scroll(clamped),
            ScrollTarget::Left => self.left_scroll.set(clamped),
            ScrollTarget::Right => self.right_scroll.set(clamped),
        }
    }

    fn scroll_by(&self, target: ScrollTarget, delta: i32) -> bool {
        let current = self.scroll_value_for(target) as i32;
        let max = self.max_scroll_for(target) as i32;
        let next = (current + delta).clamp(0, max);
        if next != current {
            self.set_scroll_for(target, next as u16);
            true
        } else {
            false
        }
    }

    fn page_scroll(&self, target: ScrollTarget, forward: bool) -> bool {
        let step = self.overlay.visible_rows().max(1) as i32;
        let delta = if forward { step } else { -step };
        self.scroll_by(target, delta)
    }

    fn jump_scroll(&self, target: ScrollTarget, end: bool) -> bool {
        let next = if end { self.max_scroll_for(target) } else { 0 };
        let current = self.scroll_value_for(target);
        if next != current {
            self.set_scroll_for(target, next);
            true
        } else {
            false
        }
    }

    fn synchronize_shared_scroll_from_focus(&self) {
        let source = match self.pane_focus {
            LimitsPaneFocus::Sync | LimitsPaneFocus::Left => self.left_scroll.get(),
            LimitsPaneFocus::Right => self.right_scroll.get(),
        };
        self.overlay.set_scroll(source.min(self.overlay.max_scroll()));
    }

    fn seed_independent_scrolls_from_shared(&self) {
        let sync = self.overlay.scroll();
        self.left_scroll.set(sync.min(self.left_max_scroll.get()));
        self.right_scroll.set(sync.min(self.right_max_scroll.get()));
    }

    fn toggle_layout_mode(&mut self) -> bool {
        self.layout_mode = self.layout_mode.next();
        if self.layout_mode == LimitsLayoutMode::SingleColumn {
            self.synchronize_shared_scroll_from_focus();
        }
        true
    }

    fn cycle_focus_mode(&mut self) -> bool {
        let next = self.pane_focus.next();
        if self.pane_focus == next {
            return false;
        }
        if self.pane_focus == LimitsPaneFocus::Sync {
            self.seed_independent_scrolls_from_shared();
        }
        if next == LimitsPaneFocus::Sync {
            self.synchronize_shared_scroll_from_focus();
        }
        self.pane_focus = next;
        true
    }

    fn set_focus_from_pane_click(&mut self, pane: PaneHit) -> bool {
        if !self.last_wide_active.get() {
            return false;
        }
        let requested = match pane {
            PaneHit::Left => LimitsPaneFocus::Left,
            PaneHit::Right => LimitsPaneFocus::Right,
        };

        if self.pane_focus == requested {
            self.synchronize_shared_scroll_from_focus();
            self.pane_focus = LimitsPaneFocus::Sync;
            return true;
        }

        if self.pane_focus == LimitsPaneFocus::Sync {
            self.seed_independent_scrolls_from_shared();
        }
        self.pane_focus = requested;
        true
    }

    fn hint_line_text(&self, has_tabs: bool) -> String {
        let mut text = format!(
            "↑↓ scroll  PgUp/PgDn page  Home/End jump  V layout:{}  F focus:{}  ",
            self.layout_mode.label(),
            self.effective_focus_mode().label()
        );
        if has_tabs {
            text.push_str("◂ ▸ change tab");
        }
        text
    }

    fn token_hit(text: &str, token: &str, relative_col: u16) -> bool {
        let Some(start_idx) = text.find(token) else {
            return false;
        };
        let start = UnicodeWidthStr::width(&text[..start_idx]) as u16;
        let width = UnicodeWidthStr::width(token) as u16;
        relative_col >= start && relative_col < start.saturating_add(width)
    }

    fn hint_action_at(&self, hint_area: Rect, has_tabs: bool, mouse_event: MouseEvent) -> Option<HintAction> {
        if hint_area.width == 0 || hint_area.height == 0 {
            return None;
        }
        if mouse_event.column < hint_area.x
            || mouse_event.column >= hint_area.x.saturating_add(hint_area.width)
            || mouse_event.row < hint_area.y
            || mouse_event.row >= hint_area.y.saturating_add(hint_area.height)
        {
            return None;
        }

        let rel = mouse_event.column.saturating_sub(hint_area.x);
        let text = self.hint_line_text(has_tabs);
        let layout_token = format!("V layout:{}", self.layout_mode.label());
        let focus_token = format!("F focus:{}", self.effective_focus_mode().label());

        if Self::token_hit(&text, &layout_token, rel) {
            return Some(HintAction::ToggleLayout);
        }
        if Self::token_hit(&text, &focus_token, rel) {
            return Some(HintAction::CycleFocus);
        }
        None
    }

    fn wide_snapshot_for_body(&self, body_area: Rect) -> Option<WideLayoutSnapshot> {
        if self.layout_mode == LimitsLayoutMode::SingleColumn {
            return None;
        }
        let lines = self.overlay.lines_for_width(body_area.width);
        let (left_lines, right_lines) = self.wide_lines(&lines, body_area.width)?;
        let (left_area, _gutter, right_area) = Self::wide_areas(body_area, &left_lines)?;
        Some(WideLayoutSnapshot {
            left_area,
            right_area,
            left_lines,
            right_lines,
        })
    }

    fn pane_hit(snapshot: &WideLayoutSnapshot, mouse_event: MouseEvent) -> Option<PaneHit> {
        let in_left = mouse_event.column >= snapshot.left_area.x
            && mouse_event.column < snapshot.left_area.x.saturating_add(snapshot.left_area.width)
            && mouse_event.row >= snapshot.left_area.y
            && mouse_event.row < snapshot.left_area.y.saturating_add(snapshot.left_area.height);
        if in_left {
            return Some(PaneHit::Left);
        }

        let in_right = mouse_event.column >= snapshot.right_area.x
            && mouse_event.column < snapshot.right_area.x.saturating_add(snapshot.right_area.width)
            && mouse_event.row >= snapshot.right_area.y
            && mouse_event.row < snapshot.right_area.y.saturating_add(snapshot.right_area.height);
        if in_right {
            return Some(PaneHit::Right);
        }
        None
    }

    fn scroll_target_for_mouse(&self, snapshot: &WideLayoutSnapshot, mouse_event: MouseEvent) -> ScrollTarget {
        match Self::pane_hit(snapshot, mouse_event) {
            Some(PaneHit::Left) => ScrollTarget::Left,
            Some(PaneHit::Right) => ScrollTarget::Right,
            None => self.active_scroll_target_for_keyboard(),
        }
    }
}

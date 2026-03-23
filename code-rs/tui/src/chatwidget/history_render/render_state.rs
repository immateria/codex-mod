/// Memoized layout data for history rendering.
pub(crate) struct HistoryRenderState {
    pub(crate) layout_cache: RefCell<HashMap<CacheKey, Rc<CachedLayout>>>,
    pub(crate) height_cache: RefCell<HashMap<CacheKey, u16>>,
    fallback_cache: RefCell<HashMap<HistoryId, Rc<Vec<Line<'static>>>>>,
    pub(crate) height_cache_last_width: Cell<u16>,
    pub(crate) prefix_sums: RefCell<Vec<u16>>,
    pub(crate) last_prefix_width: Cell<u16>,
    pub(crate) last_prefix_count: Cell<usize>,
    pub(crate) last_total_height: Cell<u16>,
    pub(crate) last_history_count: Cell<usize>,
    pub(crate) prefix_valid: Cell<bool>,
    // Row intervals that correspond to inter-cell spacing so we can avoid
    // landing the viewport on empty gaps when scrolling.
    spacing_ranges: RefCell<Vec<(u16, u16)>>,
    bottom_spacer_range: Cell<Option<(u16, u16)>>,
    bottom_spacer_lines: Cell<u16>,
    pending_bottom_spacer_lines: Cell<Option<u16>>,
}

impl HistoryRenderState {
    pub(crate) fn new() -> Self {
        Self {
            layout_cache: RefCell::new(HashMap::new()),
            height_cache: RefCell::new(HashMap::new()),
            fallback_cache: RefCell::new(HashMap::new()),
            height_cache_last_width: Cell::new(0),
            prefix_sums: RefCell::new(Vec::new()),
            last_prefix_width: Cell::new(0),
            last_prefix_count: Cell::new(0),
            last_total_height: Cell::new(0),
            last_history_count: Cell::new(0),
            prefix_valid: Cell::new(false),
            spacing_ranges: RefCell::new(Vec::new()),
            bottom_spacer_range: Cell::new(None),
            bottom_spacer_lines: Cell::new(0),
            pending_bottom_spacer_lines: Cell::new(None),
        }
    }

    pub(crate) fn invalidate_height_cache(&self) {
        self.layout_cache.borrow_mut().clear();
        self.height_cache.borrow_mut().clear();
        self.fallback_cache.borrow_mut().clear();
        self.prefix_sums.borrow_mut().clear();
        self.last_total_height.set(0);
        self.last_history_count.set(0);
        self.prefix_valid.set(false);
        self.spacing_ranges.borrow_mut().clear();
        self.bottom_spacer_range.set(None);
        self.bottom_spacer_lines.set(0);
        self.pending_bottom_spacer_lines.set(None);
    }

    pub(crate) fn handle_width_change(&self, width: u16) {
        if self.height_cache_last_width.get() != width {
            self.layout_cache
                .borrow_mut()
                .retain(|key, _| key.width == width);
            self.height_cache
                .borrow_mut()
                .retain(|key, _| key.width == width);
            self.fallback_cache.borrow_mut().clear();
            self.prefix_sums.borrow_mut().clear();
            self.last_total_height.set(0);
            self.last_history_count.set(0);
            self.prefix_valid.set(false);
            self.height_cache_last_width.set(width);
            self.spacing_ranges.borrow_mut().clear();
            self.bottom_spacer_range.set(None);
            self.bottom_spacer_lines.set(0);
            self.pending_bottom_spacer_lines.set(None);
        }
    }

    pub(crate) fn invalidate_history_id(&self, id: HistoryId) {
        if id == HistoryId::ZERO {
            return;
        }
        self.layout_cache
            .borrow_mut()
            .retain(|key, _| key.history_id != id);
        self.height_cache
            .borrow_mut()
            .retain(|key, _| key.history_id != id);
        self.fallback_cache.borrow_mut().remove(&id);
        self.prefix_sums.borrow_mut().clear();
        self.last_total_height.set(0);
        self.last_history_count.set(0);
        self.prefix_valid.set(false);
        self.spacing_ranges.borrow_mut().clear();
        self.bottom_spacer_range.set(None);
        self.bottom_spacer_lines.set(0);
        self.pending_bottom_spacer_lines.set(None);
    }

    pub(crate) fn invalidate_all(&self) {
        self.layout_cache.borrow_mut().clear();
        self.height_cache.borrow_mut().clear();
        self.fallback_cache.borrow_mut().clear();
        self.prefix_sums.borrow_mut().clear();
        self.last_total_height.set(0);
        self.last_history_count.set(0);
        self.prefix_valid.set(false);
        self.spacing_ranges.borrow_mut().clear();
        self.bottom_spacer_range.set(None);
        self.bottom_spacer_lines.set(0);
        self.pending_bottom_spacer_lines.set(None);
    }

    pub(crate) fn invalidate_prefix_only(&self) {
        self.prefix_sums.borrow_mut().clear();
        self.last_total_height.set(0);
        self.last_history_count.set(0);
        self.prefix_valid.set(false);
        self.spacing_ranges.borrow_mut().clear();
        self.bottom_spacer_range.set(None);
        self.bottom_spacer_lines.set(0);
        self.pending_bottom_spacer_lines.set(None);
    }

    pub(crate) fn should_rebuild_prefix(&self, width: u16, count: usize) -> bool {
        if !self.prefix_valid.get() {
            return true;
        }
        if self.last_prefix_width.get() != width {
            return true;
        }
        if self.last_prefix_count.get() != count {
            return true;
        }
        false
    }

    pub(crate) fn update_prefix_cache(
        &self,
        width: u16,
        prefix: Vec<u16>,
        total_height: u16,
        count: usize,
        history_count: usize,
    ) {
        {
            let mut ps = self.prefix_sums.borrow_mut();
            *ps = prefix;
        }
        self.last_prefix_width.set(width);
        self.last_prefix_count.set(count);
        self.last_total_height.set(total_height);
        self.last_history_count.set(history_count);
        self.prefix_valid.set(true);
    }

    pub(crate) fn cached_fallback_lines<F>(&self, history_id: HistoryId, build: F) -> Rc<Vec<Line<'static>>>
    where
        F: FnOnce() -> Vec<Line<'static>>,
    {
        if history_id == HistoryId::ZERO {
            return Rc::new(build());
        }
        if let Some(lines) = self.fallback_cache.borrow().get(&history_id) {
            return Rc::clone(lines);
        }
        let lines = Rc::new(build());
        self.fallback_cache
            .borrow_mut()
            .insert(history_id, Rc::clone(&lines));
        lines
    }

    pub(crate) fn cached_height(&self, history_id: HistoryId, settings: RenderSettings) -> Option<u16> {
        if history_id == HistoryId::ZERO {
            return None;
        }
        let key = CacheKey::new(history_id, settings);
        self.height_cache.borrow().get(&key).copied()
    }

    pub(crate) fn update_spacing_ranges(&self, ranges: Vec<(u16, u16)>) {
        *self.spacing_ranges.borrow_mut() = ranges;
    }

    pub(crate) fn set_bottom_spacer_range(&self, range: Option<(u16, u16)>) {
        self.bottom_spacer_range.set(range);
    }

    pub(crate) fn select_bottom_spacer_lines(&self, requested: u16) -> (u16, bool) {
        let current = self.bottom_spacer_lines.get();
        if requested >= current {
            self.bottom_spacer_lines.set(requested);
            self.pending_bottom_spacer_lines.set(None);
            return (requested, false);
        }

        let pending = self.pending_bottom_spacer_lines.get();
        if pending == Some(requested) {
            self.bottom_spacer_lines.set(requested);
            self.pending_bottom_spacer_lines.set(None);
            (requested, false)
        } else {
            self.pending_bottom_spacer_lines.set(Some(requested));
            (current, true)
        }
    }

    #[cfg(any(test, feature = "test-helpers"))]
    pub(crate) fn bottom_spacer_lines_for_test(&self) -> u16 {
        self.bottom_spacer_lines.get()
    }

    #[cfg(any(test, feature = "test-helpers"))]
    pub(crate) fn pending_bottom_spacer_lines_for_test(&self) -> Option<u16> {
        self.pending_bottom_spacer_lines.get()
    }

    pub(crate) fn adjust_scroll_to_content(&self, mut scroll_pos: u16) -> u16 {
        if scroll_pos == 0 {
            return scroll_pos;
        }
        let ranges = self.spacing_ranges.borrow();
        let bottom_spacer = self.bottom_spacer_range.get();
        if ranges.is_empty() && bottom_spacer.is_none() {
            return scroll_pos;
        }
        // Walk backwards until we hit a true cell row or run out of history.
        loop {
            let mut adjusted = false;
            if let Some((start, end)) = bottom_spacer
                && start > 0 && scroll_pos >= start && scroll_pos < end {
                    scroll_pos = start.saturating_sub(1);
                    adjusted = true;
                }
            if !adjusted {
                for &(start, end) in ranges.iter() {
                    if start == 0 {
                        continue;
                    }
                    if scroll_pos >= start && scroll_pos < end {
                        scroll_pos = start.saturating_sub(1);
                        adjusted = true;
                        break;
                    }
                }
            }
            if !adjusted || scroll_pos == 0 {
                break;
            }
        }
        scroll_pos
    }

    #[cfg(test)]
    pub(crate) fn spacing_ranges_for_test(&self) -> Vec<(u16, u16)> {
        self.spacing_ranges.borrow().clone()
    }

    pub(crate) fn last_total_height(&self) -> u16 {
        self.last_total_height.get()
    }

    pub(crate) fn last_prefix_count(&self) -> usize {
        self.last_prefix_count.get()
    }

    pub(crate) fn last_history_count(&self) -> usize {
        self.last_history_count.get()
    }

    pub(crate) fn can_append_prefix(&self, width: u16, count: usize) -> bool {
        self.prefix_valid.get()
            && self.last_prefix_width.get() == width
            && count == self.last_prefix_count.get().saturating_add(1)
    }

    pub(crate) fn extend_prefix_for_append(
        &self,
        width: u16,
        spacing: u16,
        new_height: u16,
        new_history_count: usize,
    ) -> Option<(u16, u16)> {
        if !self.prefix_valid.get() || self.last_prefix_width.get() != width {
            return None;
        }
        let prev_count = self.last_prefix_count.get();
        if prev_count == 0 {
            return None;
        }
        if new_history_count != self.last_history_count.get().saturating_add(1) {
            return None;
        }
        let mut ps = self.prefix_sums.borrow_mut();
        if ps.len() != prev_count.saturating_add(1) {
            return None;
        }
        let old_total = *ps.last().unwrap_or(&0);
        let spacing_start = old_total;
        let spacing_end = spacing_start.saturating_add(spacing);
        if let Some(last) = ps.last_mut() {
            *last = spacing_end;
        } else {
            return None;
        }
        let new_total = spacing_end.saturating_add(new_height);
        ps.push(new_total);
        self.last_total_height.set(new_total);
        self.last_prefix_count.set(prev_count.saturating_add(1));
        self.last_history_count.set(new_history_count);
        self.last_prefix_width.set(width);
        self.prefix_valid.set(true);
        if spacing > 0 { Some((spacing_start, spacing_end)) } else { None }
    }

    pub(crate) fn append_spacing_range(&self, range: (u16, u16)) {
        self.spacing_ranges.borrow_mut().push(range);
    }

    pub(crate) fn visible_cells<'a>(
        &self,
        history_state: &HistoryState,
        requests: &[RenderRequest<'a>],
        settings: RenderSettings,
    ) -> Vec<VisibleCell<'a>> {
        requests
            .iter()
            .map(|req| {
                let assistant_plan = if settings.width == 0 {
                    None
                } else if let Some(assistant_cell) = req.assistant {
                    Some(assistant_cell.ensure_layout(settings.width))
                } else if let RenderRequestKind::Assistant { id } = req.kind {
                    history_state
                        .record(id)
                        .and_then(|record| match record {
                            HistoryRecord::AssistantMessage(state) => Some(Rc::new(
                                compute_assistant_layout(state, req.config, settings.width),
                            )),
                            _ => None,
                        })
                } else {
                    None
                };

                let has_custom_render = req
                    .cell
                    .map(crate::history_cell::HistoryCell::has_custom_render)
                    .unwrap_or(false);

                let prohibit_cache = matches!(req.kind, RenderRequestKind::Streaming { .. });
                let use_cache = req.use_cache && !prohibit_cache;

                let layout = if has_custom_render
                    || settings.width == 0
                    || assistant_plan.is_some()
                {
                    None
                } else if use_cache && req.history_id != HistoryId::ZERO {
                    Some(self.render_cached(req.history_id, settings, || {
                        req.build_lines(history_state)
                    }))
                } else {
                    Some(self.render_adhoc(settings.width, || {
                        req.build_lines(history_state)
                    }))
                };

                let use_height_cache = use_cache && req.history_id != HistoryId::ZERO;
                let cached_height = if use_height_cache {
                    let key = CacheKey::new(req.history_id, settings);
                    self.height_cache
                        .borrow()
                        .get(&key)
                        .copied()
                        .map(|h| (h, HeightSource::Cached, None))
                } else {
                    None
                };

                let (height, height_source, height_measure_ns) = if settings.width == 0 {
                    (0, HeightSource::ZeroWidth, None)
                } else if let Some(plan) = assistant_plan.as_ref() {
                    (plan.total_rows(), HeightSource::AssistantPlan, None)
                } else if let Some(layout_ref) = layout.as_ref() {
                    (
                        layout_ref
                            .line_count()
                            .min(u16::MAX as usize) as u16,
                        HeightSource::Layout,
                        None,
                    )
                } else if let Some((h, src, measure)) = cached_height {
                    (h, src, measure)
                } else if let Some(cell) = req.cell {
                    if cell.has_custom_render() {
                        let start = Instant::now();
                        let computed = cell.desired_height(settings.width);
                        let elapsed = start.elapsed().as_nanos();
                        if use_height_cache {
                            let key = CacheKey::new(req.history_id, settings);
                            self.height_cache.borrow_mut().insert(key, computed);
                        }
                        (
                            computed,
                            HeightSource::DesiredHeight,
                            Some(elapsed),
                        )
                    } else if let Some(lines) = req.fallback_lines.as_ref() {
                        let wrapped = word_wrap_lines(lines, settings.width);
                        let height = wrapped.len().min(u16::MAX as usize) as u16;
                        if use_height_cache {
                            let key = CacheKey::new(req.history_id, settings);
                            self.height_cache.borrow_mut().insert(key, height);
                        }
                        (height, HeightSource::FallbackLines, None)
                    } else {
                        let start = Instant::now();
                        let computed = cell.desired_height(settings.width);
                        let elapsed = start.elapsed().as_nanos();
                        if use_height_cache {
                            let key = CacheKey::new(req.history_id, settings);
                            self.height_cache.borrow_mut().insert(key, computed);
                        }
                        (
                            computed,
                            HeightSource::DesiredHeight,
                            Some(elapsed),
                        )
                    }
                } else if let Some(lines) = req.fallback_lines.as_ref() {
                    let wrapped = word_wrap_lines(lines, settings.width);
                    let height = wrapped.len().min(u16::MAX as usize) as u16;
                    if use_height_cache {
                        let key = CacheKey::new(req.history_id, settings);
                        self.height_cache.borrow_mut().insert(key, height);
                    }
                    (height, HeightSource::FallbackLines, None)
                } else {
                    (0, HeightSource::Unknown, None)
                };

                VisibleCell {
                    cell: req.cell,
                    assistant_plan,
                    layout,
                    height,
                    height_source,
                    height_measure_ns,
                }
            })
            .collect()
    }

    fn render_cached<F>(&self, history_id: HistoryId, settings: RenderSettings, build_lines: F) -> LayoutRef
    where
        F: FnOnce() -> Vec<Line<'static>>,
    {
        if settings.width == 0 {
            return LayoutRef::empty();
        }

        let key = CacheKey::new(history_id, settings);
        if let Some(layout) = self.layout_cache.borrow().get(&key).cloned() {
            #[cfg(feature = "test-helpers")]
            HISTORY_LAYOUT_CACHE_HITS.with(|c| c.set(c.get().saturating_add(1)));
            return LayoutRef { data: layout };
        }

        #[cfg(feature = "test-helpers")]
        HISTORY_LAYOUT_CACHE_MISSES.with(|c| c.set(c.get().saturating_add(1)));

        let layout = Rc::new(build_cached_layout(build_lines(), settings.width));
        self.layout_cache
            .borrow_mut()
            .insert(key, Rc::clone(&layout));
        LayoutRef { data: layout }
    }

    fn render_adhoc<F>(&self, width: u16, build_lines: F) -> LayoutRef
    where
        F: FnOnce() -> Vec<Line<'static>>,
    {
        if width == 0 {
            return LayoutRef::empty();
        }
        LayoutRef {
            data: Rc::new(build_cached_layout(build_lines(), width)),
        }
    }
}

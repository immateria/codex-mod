/// Settings that affect layout caching. Any change to these fields invalidates
/// the cached `CachedLayout` entries keyed by `(HistoryId, width, theme_epoch,
/// reasoning_visible)`.
#[derive(Clone, Copy)]
pub(crate) struct RenderSettings {
    pub width: u16,
    pub theme_epoch: u64,
    pub reasoning_visible: bool,
}

impl RenderSettings {
    pub fn new(width: u16, theme_epoch: u64, reasoning_visible: bool) -> Self {
        Self {
            width,
            theme_epoch,
            reasoning_visible,
        }
    }
}

/// A rendering input assembled by `ChatWidget::draw_history` for a single
/// history record. We keep both the legacy `HistoryCell` (if one exists) and a
/// semantic fallback so the renderer can rebuild layouts directly from
/// `HistoryRecord` data when needed.
pub(crate) struct RenderRequest<'a> {
    pub history_id: HistoryId,
    pub cell: Option<&'a dyn HistoryCell>,
    pub assistant: Option<&'a AssistantMarkdownCell>,
    pub use_cache: bool,
    pub fallback_lines: Option<Rc<Vec<Line<'static>>>>,
    pub kind: RenderRequestKind,
    pub config: &'a Config,
}

impl<'a> RenderRequest<'a> {
    /// Returns the best-effort lines for this record. We prefer the existing
    /// `HistoryCell` cache (which may include per-cell layout bridges) and fall
    /// back to semantic lines derived from the record state.
    fn build_lines(&self, history_state: &HistoryState) -> Vec<Line<'static>> {
        if let RenderRequestKind::Exec { id } = self.kind
            && let Some(HistoryRecord::Exec(record)) = history_state.record(id) {
                return exec_display_lines_from_record(record);
            }

        if let RenderRequestKind::MergedExec { id } = self.kind
            && let Some(HistoryRecord::MergedExec(record)) = history_state.record(id) {
                return merged_exec_lines_from_record(record);
            }

        if let RenderRequestKind::Explore {
            id,
            hold_header,
            full_detail,
        } = self.kind
            && let Some(HistoryRecord::Explore(record)) = history_state.record(id) {
                if full_detail {
                    return explore_lines_without_truncation(record, hold_header);
                }
                return explore_lines_from_record_with_force(record, hold_header);
            }

        if let RenderRequestKind::Diff { id } = self.kind
            && let Some(HistoryRecord::Diff(record)) = history_state.record(id) {
                return diff_lines_from_record(record);
            }

        if let RenderRequestKind::Streaming { id } = self.kind
            && let Some(HistoryRecord::AssistantStream(record)) = history_state.record(id) {
                return stream_lines_from_state(record, self.config, record.in_progress);
            }

        if let RenderRequestKind::Assistant { id } = self.kind
            && let Some(HistoryRecord::AssistantMessage(record)) = history_state.record(id) {
                return assistant_markdown_lines(record, self.config);
            }

        if let Some(cell) = self.cell {
            return cell.display_lines_trimmed();
        }

        if let Some(lines) = &self.fallback_lines {
            return lines.as_ref().clone();
        }
        Vec::new()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Identifies the source for `RenderRequest` line construction.
/// Exec variants always rebuild lines from `HistoryState`, ensuring the
/// shared renderer cache is the single source of truth for layout data.
pub(crate) enum RenderRequestKind {
    Legacy,
    Exec { id: HistoryId },
    MergedExec { id: HistoryId },
    Explore {
        id: HistoryId,
        hold_header: bool,
        full_detail: bool,
    },
    Diff { id: HistoryId },
    Streaming { id: HistoryId },
    Assistant { id: HistoryId },
}

/// Output from `HistoryRenderState::visible_cells()`. Contains the resolved
/// layout (if any), plus the optional `HistoryCell` pointer so the caller can
/// reuse existing caches.
pub(crate) struct VisibleCell<'a> {
    pub cell: Option<&'a dyn HistoryCell>,
    pub assistant_plan: Option<Rc<AssistantLayoutCache>>,
    pub layout: Option<LayoutRef>,
    pub height: u16,
    pub height_source: HeightSource,
    pub height_measure_ns: Option<u128>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HeightSource {
    AssistantPlan,
    Layout,
    Cached,
    DesiredHeight,
    FallbackLines,
    ZeroWidth,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct CacheKey {
    history_id: HistoryId,
    width: u16,
    theme_epoch: u64,
    reasoning_visible: bool,
}

impl CacheKey {
    fn new(history_id: HistoryId, settings: RenderSettings) -> Self {
        Self {
            history_id,
            width: settings.width,
            theme_epoch: settings.theme_epoch,
            reasoning_visible: settings.reasoning_visible,
        }
    }
}

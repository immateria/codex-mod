use ratatui::prelude::{Buffer, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Padding, Widget};

use crate::history::compat::{
    ExecAction,
    ExecRecord,
    ExecStatus,
    HistoryId,
    MergedExecRecord,
};
use crate::util::buffer::{fill_rect, write_line};

use super::core::{ExecKind, HistoryCell, HistoryCellType};
use super::exec::ExecCell;
use super::exec_helpers::coalesce_read_ranges_in_lines_local;
use super::formatting::trim_empty_lines;

#[cfg(feature = "code-fork")]
use crate::foundation::wrapping::word_wrap_lines;
#[cfg(not(feature = "code-fork"))]
use crate::insert_history::word_wrap_lines;

#[cfg(feature = "test-helpers")]
thread_local! {
    static MERGED_EXEC_LAYOUT_BUILDS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

#[cfg(feature = "test-helpers")]
pub(crate) fn reset_merged_exec_layout_builds_for_test() {
    MERGED_EXEC_LAYOUT_BUILDS.with(|c| c.set(0));
}

#[cfg(feature = "test-helpers")]
pub(crate) fn merged_exec_layout_builds_for_test() -> u64 {
    MERGED_EXEC_LAYOUT_BUILDS.with(std::cell::Cell::get)
}

// ==================== MergedExecCell ====================
// Represents multiple completed exec results merged into one cell while preserving
// the bordered, dimmed output styling for each command's stdout/stderr preview.

struct MergedExecSegment {
    record: ExecRecord,
}

impl MergedExecSegment {
    fn new(record: ExecRecord) -> Self {
        Self { record }
    }

    fn exec_parts(&self) -> (Vec<Line<'static>>, Vec<Line<'static>>, Option<Line<'static>>) {
        let exec_cell = ExecCell::from_record(self.record.clone());
        exec_cell.exec_render_parts()
    }

    fn lines(&self) -> (Vec<Line<'static>>, Vec<Line<'static>>) {
        let (pre, mut out, status_line) = self.exec_parts();
        if let Some(status) = status_line {
            out.push(status);
        }
        (pre, out)
    }
}

pub(crate) struct MergedExecCell {
    segments: Vec<MergedExecSegment>,
    kind: ExecKind,
    history_id: HistoryId,
    layout_cache: std::cell::RefCell<MergedExecLayoutCacheEntry>,
}

impl MergedExecCell {
    pub(crate) fn rebuild_with_theme(&self) {
        self.invalidate_layout_cache();
    }

    fn invalidate_layout_cache(&self) {
        *self.layout_cache.borrow_mut() = MergedExecLayoutCacheEntry {
            width: 0,
            layout: MergedExecLayout::default(),
        };
    }

    pub(crate) fn set_history_id(&mut self, id: HistoryId) {
        self.history_id = id;
    }

    pub(crate) fn to_record(&self) -> MergedExecRecord {
        MergedExecRecord {
            id: self.history_id,
            action: self.kind.into(),
            segments: self
                .segments
                .iter()
                .map(|segment| segment.record.clone())
                .collect(),
        }
    }

    pub(crate) fn from_records(
        history_id: HistoryId,
        action: ExecAction,
        segments: Vec<ExecRecord>,
    ) -> Self {
        Self {
            segments: segments.into_iter().map(MergedExecSegment::new).collect(),
            kind: action.into(),
            history_id,
            layout_cache: std::cell::RefCell::new(MergedExecLayoutCacheEntry {
                width: 0,
                layout: MergedExecLayout::default(),
            }),
        }
    }

    pub(crate) fn from_state(record: MergedExecRecord) -> Self {
        let history_id = record.id;
        let kind: ExecKind = record.action.into();
        let segments = record
            .segments
            .into_iter()
            .map(MergedExecSegment::new)
            .collect();
        Self {
            segments,
            kind,
            history_id,
            layout_cache: std::cell::RefCell::new(MergedExecLayoutCacheEntry {
                width: 0,
                layout: MergedExecLayout::default(),
            }),
        }
    }

    fn ensure_layout_for_width(&self, width: u16) {
        if width == 0 {
            *self.layout_cache.borrow_mut() = MergedExecLayoutCacheEntry {
                width,
                layout: MergedExecLayout::default(),
            };
            return;
        }

        let needs_rebuild = self.layout_cache.borrow().width != width;
        if !needs_rebuild {
            return;
        }

        #[cfg(feature = "test-helpers")]
        MERGED_EXEC_LAYOUT_BUILDS.with(|c| c.set(c.get().saturating_add(1)));

        let layout = self.compute_layout_for_width(width);
        *self.layout_cache.borrow_mut() = MergedExecLayoutCacheEntry { width, layout };
    }

    fn layout_for_width(&self, width: u16) -> std::cell::Ref<'_, MergedExecLayout> {
        self.ensure_layout_for_width(width);
        std::cell::Ref::map(self.layout_cache.borrow(), |cache| &cache.layout)
    }

    fn compute_layout_for_width(&self, width: u16) -> MergedExecLayout {
        if width == 0 {
            return MergedExecLayout::default();
        }

        let header_total: u16 = if self.kind == ExecKind::Run { 0 } else { 1 };
        let out_wrap_width = width.saturating_sub(2);

        if let Some(agg_pre) = self.aggregated_read_preamble_lines() {
            let pre_wrapped = word_wrap_lines(&agg_pre, width);
            let pre_total = pre_wrapped.len().min(u16::MAX as usize) as u16;

            let mut segments: Vec<MergedExecSegmentLayout> = Vec::with_capacity(self.segments.len());
            let mut total = header_total.saturating_add(pre_total);
            for segment in &self.segments {
                let (_, out_raw) = segment.lines();
                let out = trim_empty_lines(out_raw);
                let out_wrapped = word_wrap_lines(&out, out_wrap_width);
                let out_total = out_wrapped.len().min(u16::MAX as usize) as u16;
                total = total.saturating_add(out_total);
                segments.push(MergedExecSegmentLayout {
                    pre_lines: Vec::new(),
                    out_lines: out_wrapped,
                    pre_total: 0,
                    out_total,
                });
            }

            return MergedExecLayout {
                aggregated_preamble: Some(pre_wrapped),
                aggregated_preamble_total: pre_total,
                segments,
                total_rows: total,
            };
        }

        let mut segments: Vec<MergedExecSegmentLayout> = Vec::with_capacity(self.segments.len());
        let mut total = header_total;
        let mut added_corner = false;
        for segment in &self.segments {
            let (pre_raw, out_raw) = segment.lines();
            let mut pre = trim_empty_lines(pre_raw);
            if self.kind != ExecKind::Run && !pre.is_empty() {
                pre.remove(0);
            }
            if self.kind != ExecKind::Run
                && let Some(first) = pre.first_mut()
            {
                let flat: String = first.spans.iter().map(|s| s.content.as_ref()).collect();
                let has_corner = flat.trim_start().starts_with("└ ");
                let has_spaced_corner = flat.trim_start().starts_with("  └ ");
                if !added_corner {
                    if !(has_corner || has_spaced_corner) {
                        first.spans.insert(
                            0,
                            Span::styled("└ ", Style::default().fg(crate::colors::text_dim())),
                        );
                    }
                    added_corner = true;
                } else if let Some(sp0) = first.spans.get_mut(0)
                    && sp0.content.as_ref() == "└ "
                {
                    sp0.content = "  ".into();
                    sp0.style = sp0.style.add_modifier(Modifier::DIM);
                }
            }

            let out = trim_empty_lines(out_raw);

            let pre_wrapped = word_wrap_lines(&pre, width);
            let out_wrapped = word_wrap_lines(&out, out_wrap_width);
            let pre_total = pre_wrapped.len().min(u16::MAX as usize) as u16;
            let out_total = out_wrapped.len().min(u16::MAX as usize) as u16;
            total = total.saturating_add(pre_total).saturating_add(out_total);
            segments.push(MergedExecSegmentLayout {
                pre_lines: pre_wrapped,
                out_lines: out_wrapped,
                pre_total,
                out_total,
            });
        }

        MergedExecLayout {
            aggregated_preamble: None,
            aggregated_preamble_total: 0,
            segments,
            total_rows: total,
        }
    }

    fn aggregated_read_preamble_lines(&self) -> Option<Vec<Line<'static>>> {
        if self.kind != ExecKind::Read {
            return None;
        }
        use ratatui::text::Span;

        fn parse_read_line(line: &Line<'_>) -> Option<(String, u32, u32)> {
            if line.spans.is_empty() {
                return None;
            }
            let first = line.spans[0].content.as_ref();
            if !(first == "└ " || first == "  ") {
                return None;
            }
            let rest: String = line
                .spans
                .iter()
                .skip(1)
                .map(|s| s.content.as_ref())
                .collect();
            if let Some(idx) = rest.rfind(" (lines ") {
                let fname = rest[..idx].to_string();
                let tail = &rest[idx + 1..];
                if tail.starts_with("(lines ") && tail.ends_with(")") {
                    let inner = &tail[7..tail.len().saturating_sub(1)];
                    if let Some((a, b)) = inner.split_once(" to ")
                        && let (Ok(s), Ok(e)) = (a.trim().parse::<u32>(), b.trim().parse::<u32>()) {
                            return Some((fname, s, e));
                        }
                }
            }
            None
        }

        fn is_search_like(line: &Line<'_>) -> bool {
            let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
            let t = text.trim();
            t.contains(" (in ")
                || t.rsplit_once(" in ")
                    .map(|(_, rhs)| rhs.trim_end().ends_with('/'))
                    .unwrap_or(false)
        }

        let mut kept: Vec<Line<'static>> = Vec::new();
        for (seg_idx, segment) in self.segments.iter().enumerate() {
            let (pre_raw, _, _) = segment.exec_parts();
            let mut pre = trim_empty_lines(pre_raw);
            if !pre.is_empty() {
                pre.remove(0);
            }
            for line in pre.into_iter() {
                if is_search_like(&line) {
                    continue;
                }
                let keep = parse_read_line(&line).is_some() || seg_idx == 0;
                if keep {
                    kept.push(line);
                }
            }
        }

        if kept.is_empty() {
            return Some(kept);
        }

        if let Some(first) = kept.first_mut() {
            let flat: String = first.spans.iter().map(|s| s.content.as_ref()).collect();
            let has_connector = flat.trim_start().starts_with("└ ");
            if !has_connector {
                first.spans.insert(
                    0,
                    Span::styled("└ ", Style::default().fg(crate::colors::text_dim())),
                );
            }
        }
        for line in kept.iter_mut().skip(1) {
            if let Some(span0) = line.spans.get_mut(0)
                && span0.content.as_ref() == "└ " {
                    span0.content = "  ".into();
                    span0.style = span0.style.add_modifier(Modifier::DIM);
                }
        }

        coalesce_read_ranges_in_lines_local(&mut kept);
        Some(kept)
    }
}

#[derive(Default)]
struct MergedExecLayout {
    aggregated_preamble: Option<Vec<Line<'static>>>,
    aggregated_preamble_total: u16,
    segments: Vec<MergedExecSegmentLayout>,
    total_rows: u16,
}

struct MergedExecSegmentLayout {
    pre_lines: Vec<Line<'static>>,
    out_lines: Vec<Line<'static>>,
    pre_total: u16,
    out_total: u16,
}

struct MergedExecLayoutCacheEntry {
    width: u16,
    layout: MergedExecLayout,
}

impl HistoryCell for MergedExecCell {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
    fn kind(&self) -> HistoryCellType {
        HistoryCellType::Exec {
            kind: self.kind,
            status: ExecStatus::Success,
        }
    }
    fn desired_height(&self, width: u16) -> u16 {
        self.layout_for_width(width).total_rows
    }
    fn display_lines(&self) -> Vec<Line<'static>> {
        let mut out: Vec<Line<'static>> = Vec::new();
        for (i, segment) in self.segments.iter().enumerate() {
            let (pre_raw, out_raw) = segment.lines();
            if i > 0 {
                out.push(Line::from(""));
            }
            out.extend(trim_empty_lines(pre_raw));
            out.extend(trim_empty_lines(out_raw));
        }
        out
    }
    fn has_custom_render(&self) -> bool {
        true
    }
    fn custom_render_with_skip(&self, area: Rect, buf: &mut Buffer, mut skip_rows: u16) {
        let bg = Style::default()
            .bg(crate::colors::background())
            .fg(crate::colors::text());
        fill_rect(buf, area, Some(' '), bg);
        if area.width == 0 || area.height == 0 {
            return;
        }

        let layout = self.layout_for_width(area.width);

        // Build one header line based on exec kind
        let header_line = match self.kind {
            ExecKind::Read => Some(Line::styled(
                "Read",
                Style::default().fg(crate::colors::text()),
            )),
            ExecKind::Search => Some(Line::styled(
                "Search",
                Style::default().fg(crate::colors::text_dim()),
            )),
            ExecKind::List => Some(Line::styled(
                "List",
                Style::default().fg(crate::colors::text()),
            )),
            ExecKind::Run => None,
        };

        let mut cur_y = area.y;
        let end_y = area.y.saturating_add(area.height);

        // Render or skip header line
        if let Some(header_line) = header_line {
            if skip_rows == 0 {
                if cur_y < end_y {
                    write_line(buf, area.x, cur_y, area.width, &header_line, bg);
                    cur_y = cur_y.saturating_add(1);
                }
            } else {
                skip_rows = skip_rows.saturating_sub(1);
            }
        }

        // Special aggregated rendering for Read: collapse file ranges
        if let Some(agg_pre) = layout.aggregated_preamble.as_ref() {
            let pre_total = layout.aggregated_preamble_total;
            let pre_skip = skip_rows.min(pre_total);
            skip_rows = skip_rows.saturating_sub(pre_skip);

            if cur_y < end_y {
                let pre_height = pre_total
                    .saturating_sub(pre_skip)
                    .min(end_y.saturating_sub(cur_y));
                if pre_height > 0 {
                    for (idx, line) in agg_pre
                        .iter()
                        .skip(pre_skip as usize)
                        .take(pre_height as usize)
                        .enumerate()
                    {
                        let y = cur_y.saturating_add(idx as u16);
                        if y >= end_y {
                            break;
                        }
                        write_line(buf, area.x, y, area.width, line, bg);
                    }
                    cur_y = cur_y.saturating_add(pre_height);
                }
            }

            let dim_style = Style::default()
                .bg(crate::colors::background())
                .fg(crate::colors::text_dim());

            for segment in &layout.segments {
                if cur_y >= end_y {
                    break;
                }
                let out_total = segment.out_total;
                let out_skip = skip_rows.min(out_total);
                skip_rows = skip_rows.saturating_sub(out_skip);
                let out_height = out_total
                    .saturating_sub(out_skip)
                    .min(end_y.saturating_sub(cur_y));
                if out_height == 0 {
                    continue;
                }
                let out_area = Rect {
                    x: area.x,
                    y: cur_y,
                    width: area.width,
                    height: out_height,
                };
                fill_rect(buf, out_area, Some(' '), dim_style);
                let block = Block::default()
                    .borders(Borders::LEFT)
                    .border_style(
                        Style::default()
                            .fg(crate::colors::border_dim())
                            .bg(crate::colors::background()),
                    )
                    .style(Style::default().bg(crate::colors::background()))
                    .padding(Padding {
                        left: 1,
                        right: 0,
                        top: 0,
                        bottom: 0,
                    });
                let inner = block.inner(out_area);
                block.render(out_area, buf);
                if inner.width > 0 {
                    for (idx, line) in segment
                        .out_lines
                        .iter()
                        .skip(out_skip as usize)
                        .take(out_height as usize)
                        .enumerate()
                    {
                        let y = inner.y.saturating_add(idx as u16);
                        if y >= inner.y.saturating_add(inner.height) {
                            break;
                        }
                        write_line(buf, inner.x, y, inner.width, line, dim_style);
                    }
                }
                cur_y = cur_y.saturating_add(out_height);
            }
            return;
        }

        // Fallback: each segment retains its own preamble and output

        let dim_style = Style::default()
            .bg(crate::colors::background())
            .fg(crate::colors::text_dim());

        for segment in &layout.segments {
            if cur_y >= end_y {
                break;
            }
            let pre_total = segment.pre_total;
            let out_total = segment.out_total;

            let pre_skip = skip_rows.min(pre_total);
            skip_rows = skip_rows.saturating_sub(pre_skip);
            let pre_height = pre_total
                .saturating_sub(pre_skip)
                .min(end_y.saturating_sub(cur_y));

            if pre_height > 0 {
                for (idx, line) in segment
                    .pre_lines
                    .iter()
                    .skip(pre_skip as usize)
                    .take(pre_height as usize)
                    .enumerate()
                {
                    let y = cur_y.saturating_add(idx as u16);
                    if y >= end_y {
                        break;
                    }
                    write_line(buf, area.x, y, area.width, line, bg);
                }
                cur_y = cur_y.saturating_add(pre_height);
            }

            if cur_y >= end_y {
                break;
            }

            let out_skip = skip_rows.min(out_total);
            skip_rows = skip_rows.saturating_sub(out_skip);
            let out_height = out_total
                .saturating_sub(out_skip)
                .min(end_y.saturating_sub(cur_y));

            if out_height > 0 {
                let out_area = Rect {
                    x: area.x,
                    y: cur_y,
                    width: area.width,
                    height: out_height,
                };
                fill_rect(buf, out_area, Some(' '), dim_style);
                let block = Block::default()
                    .borders(Borders::LEFT)
                    .border_style(
                        Style::default()
                            .fg(crate::colors::border_dim())
                            .bg(crate::colors::background()),
                    )
                    .style(Style::default().bg(crate::colors::background()))
                    .padding(Padding {
                        left: 1,
                        right: 0,
                        top: 0,
                        bottom: 0,
                    });
                let inner = block.inner(out_area);
                block.render(out_area, buf);
                if inner.width > 0 {
                    for (idx, line) in segment
                        .out_lines
                        .iter()
                        .skip(out_skip as usize)
                        .take(out_height as usize)
                        .enumerate()
                    {
                        let y = inner.y.saturating_add(idx as u16);
                        if y >= inner.y.saturating_add(inner.height) {
                            break;
                        }
                        write_line(buf, inner.x, y, inner.width, line, dim_style);
                    }
                }
                cur_y = cur_y.saturating_add(out_height);
            }
        }
    }
}

pub(crate) fn merged_exec_lines_from_record(record: &MergedExecRecord) -> Vec<Line<'static>> {
    MergedExecCell::from_state(record.clone()).display_lines()
}

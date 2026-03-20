use std::collections::BTreeSet;

use ratatui::CompletedFrame;

use super::BufferDiffProfiler;

impl BufferDiffProfiler {
    pub(in crate::app) fn new_from_env() -> Self {
        match std::env::var("CODE_BUFFER_DIFF_METRICS") {
            Ok(raw) => {
                let trimmed = raw.trim();
                if trimmed.is_empty() || trimmed == "0" {
                    Self::disabled()
                } else {
                    let log_every = trimmed.parse::<usize>().unwrap_or(1).max(1);
                    let min_changed = std::env::var("CODE_BUFFER_DIFF_MIN_CHANGED")
                        .ok()
                        .and_then(|v| v.trim().parse::<usize>().ok())
                        .unwrap_or(100);
                    let min_percent = std::env::var("CODE_BUFFER_DIFF_MIN_PERCENT")
                        .ok()
                        .and_then(|v| v.trim().parse::<f64>().ok())
                        .unwrap_or(1.0_f64);
                    Self {
                        enabled: true,
                        prev: None,
                        frame_seq: 0,
                        log_every,
                        min_changed,
                        min_percent,
                    }
                }
            }
            Err(_) => Self::disabled(),
        }
    }

    fn disabled() -> Self {
        Self {
            enabled: false,
            prev: None,
            frame_seq: 0,
            log_every: 1,
            min_changed: usize::MAX,
            min_percent: f64::MAX,
        }
    }

    pub(in crate::app) fn record(&mut self, frame: &CompletedFrame<'_>) {
        if !self.enabled {
            return;
        }

        let current_buffer = frame.buffer.clone();
        self.frame_seq = self.frame_seq.saturating_add(1);

        if let Some(prev_buffer) = &self.prev
            && self.should_log_frame()
        {
            if prev_buffer.area != current_buffer.area {
                tracing::info!(
                    target: "code_tui::buffer_diff",
                    frame = self.frame_seq,
                    prev_width = prev_buffer.area.width,
                    prev_height = prev_buffer.area.height,
                    width = current_buffer.area.width,
                    height = current_buffer.area.height,
                    "Buffer area changed; skipping diff metrics for this frame"
                );
            } else {
                let inspected = prev_buffer.content.len().min(current_buffer.content.len());
                let updates = prev_buffer.diff(&current_buffer);
                let changed = updates.len();
                if changed == 0 {
                    self.prev = Some(current_buffer);
                    return;
                }
                let percent = if inspected > 0 {
                    (changed as f64 / inspected as f64) * 100.0
                } else {
                    0.0
                };
                if changed < self.min_changed && percent < self.min_percent {
                    self.prev = Some(current_buffer);
                    return;
                }
                let mut min_col = u16::MAX;
                let mut max_col = 0u16;
                let mut rows = BTreeSet::new();
                let mut longest_run = 0usize;
                let mut current_run = 0usize;
                let mut last_cell = None;
                for (x, y, _) in &updates {
                    min_col = min_col.min(*x);
                    max_col = max_col.max(*x);
                    rows.insert(*y);
                    match last_cell {
                        Some((last_x, last_y)) if *y == last_y && *x == last_x + 1 => {
                            current_run += 1;
                        }
                        _ => {
                            current_run = 1;
                        }
                    }
                    if current_run > longest_run {
                        longest_run = current_run;
                    }
                    last_cell = Some((*x, *y));
                }
                let row_min = rows.iter().copied().min().unwrap_or(0);
                let row_max = rows.iter().copied().max().unwrap_or(0);
                let mut spans: Vec<(u16, u16)> = Vec::new();
                if !rows.is_empty() {
                    let mut iter = rows.iter();
                    if let Some(&mut_start) = iter.next() {
                        let mut start = mut_start;
                        let mut prev = start;
                        for &row in iter {
                            if row == prev + 1 {
                                prev = row;
                                continue;
                            }
                            spans.push((start, prev));
                            start = row;
                            prev = row;
                        }
                        spans.push((start, prev));
                    }
                }
                spans.sort_by(|(a_start, a_end), (b_start, b_end)| {
                    let a_len = usize::from(*a_end) - usize::from(*a_start) + 1;
                    let b_len = usize::from(*b_end) - usize::from(*b_start) + 1;
                    b_len.cmp(&a_len)
                });
                let top_spans: Vec<(u16, u16)> = spans.into_iter().take(3).collect();
                let (col_min, col_max) = if min_col == u16::MAX {
                    (0u16, 0u16)
                } else {
                    (min_col, max_col)
                };
                let skipped_cells = current_buffer.content.iter().filter(|cell| cell.skip).count();
                tracing::info!(
                    target: "code_tui::buffer_diff",
                    frame = self.frame_seq,
                    inspected,
                    changed,
                    percent = format!("{percent:.2}"),
                    width = current_buffer.area.width,
                    height = current_buffer.area.height,
                    dirty_rows = rows.len(),
                    longest_run,
                    row_min,
                    row_max,
                    col_min,
                    col_max,
                    row_spans = ?top_spans,
                    skipped_cells,
                    "Buffer diff metrics"
                );
            }
        }

        self.prev = Some(current_buffer);
    }

    fn should_log_frame(&self) -> bool {
        let interval = self.log_every.max(1) as u64;
        interval == 1 || self.frame_seq.is_multiple_of(interval)
    }
}

use std::time::Duration;

use ratatui::layout::Rect;

use crate::components::form_text_field::FormTextField;

use code_core::config_types::{
    McpDispatchMode,
    McpServerSchedulingToml,
    McpToolSchedulingOverrideToml,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ServerRow {
    Dispatch,
    MaxConcurrent,
    MinInterval,
    QueueTimeout,
    MaxQueueDepth,
    Save,
    Cancel,
}

pub(super) const SERVER_ROWS: [ServerRow; 7] = [
    ServerRow::Dispatch,
    ServerRow::MaxConcurrent,
    ServerRow::MinInterval,
    ServerRow::QueueTimeout,
    ServerRow::MaxQueueDepth,
    ServerRow::Save,
    ServerRow::Cancel,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ToolRow {
    MinInterval,
    MaxConcurrent,
    ClearOverride,
    Save,
    Cancel,
}

pub(super) const TOOL_ROWS: [ToolRow; 5] = [
    ToolRow::MinInterval,
    ToolRow::MaxConcurrent,
    ToolRow::ClearOverride,
    ToolRow::Save,
    ToolRow::Cancel,
];

pub(super) fn format_secs_compact(duration: Duration) -> String {
    let value = format_secs_for_edit(duration);
    format!("{value}s")
}

pub(super) fn format_opt_secs_compact(duration: Option<Duration>) -> String {
    duration
        .map(format_secs_compact)
        .unwrap_or_else(|| "none".to_string())
}

pub(super) fn format_secs_for_edit(duration: Duration) -> String {
    let secs = duration.as_secs_f64();
    if secs.fract() == 0.0 {
        format!("{}", duration.as_secs())
    } else {
        // Keep enough precision for sub-second limits without being noisy.
        format!("{secs:.3}")
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string()
    }
}

pub(super) fn parse_secs_field(label: &str, text: &str) -> Result<Option<Duration>, String> {
    let trimmed = text.trim();
    let trimmed = trimmed
        .strip_suffix('s')
        .or_else(|| trimmed.strip_suffix('S'))
        .unwrap_or(trimmed);
    if trimmed.is_empty() {
        return Ok(None);
    }
    let value: f64 = trimmed
        .parse()
        .map_err(|_| format!("{label} must be a number (seconds)"))?;
    if value < 0.0 {
        return Err(format!("{label} must be >= 0"));
    }
    Ok(Some(Duration::from_secs_f64(value)))
}

pub(super) fn parse_u32_field(label: &str, text: &str) -> Result<u32, String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err(format!("{label} is required"));
    }
    let value: u32 = trimmed
        .parse()
        .map_err(|_| format!("{label} must be an integer"))?;
    if value == 0 {
        return Err(format!("{label} must be >= 1"));
    }
    Ok(value)
}

pub(super) fn parse_optional_u32_field(label: &str, text: &str) -> Result<Option<u32>, String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let value: u32 = trimmed
        .parse()
        .map_err(|_| format!("{label} must be an integer"))?;
    if value == 0 {
        return Err(format!("{label} must be >= 1"));
    }
    Ok(Some(value))
}

pub(super) fn centered_overlay_rect(area: Rect, max_w: u16, max_h: u16) -> Rect {
    let w = max_w
        .min(area.width.saturating_sub(2))
        .max(24)
        .min(area.width);
    let h = max_h
        .min(area.height.saturating_sub(2))
        .max(8)
        .min(area.height);
    let x = area.x.saturating_add((area.width.saturating_sub(w)) / 2);
    let y = area.y.saturating_add((area.height.saturating_sub(h)) / 2);
    Rect {
        x,
        y,
        width: w,
        height: h,
    }
}

#[derive(Debug)]
pub(in crate::bottom_pane::settings_pages::mcp) struct ServerSchedulingEditor {
    pub(super) server: String,
    pub(super) scheduling: McpServerSchedulingToml,
    pub(super) selected_row: usize,
    pub(super) editing: Option<ServerRow>,
    pub(super) error: Option<String>,
    pub(super) max_concurrent_field: FormTextField,
    pub(super) min_interval_field: FormTextField,
    pub(super) queue_timeout_field: FormTextField,
    pub(super) max_queue_depth_field: FormTextField,
}

impl ServerSchedulingEditor {
    pub(super) fn new(server: &str, scheduling: McpServerSchedulingToml) -> Self {
        let mut max_concurrent_field = FormTextField::new_single_line();
        max_concurrent_field.set_text(&scheduling.max_concurrent.to_string());

        let mut min_interval_field = FormTextField::new_single_line();
        min_interval_field.set_placeholder("none");
        if let Some(value) = scheduling.min_interval_sec {
            min_interval_field.set_text(&format_secs_for_edit(value));
        }

        let mut queue_timeout_field = FormTextField::new_single_line();
        queue_timeout_field.set_placeholder("none");
        if let Some(value) = scheduling.queue_timeout_sec {
            queue_timeout_field.set_text(&format_secs_for_edit(value));
        }

        let mut max_queue_depth_field = FormTextField::new_single_line();
        max_queue_depth_field.set_placeholder("none");
        if let Some(value) = scheduling.max_queue_depth {
            max_queue_depth_field.set_text(&value.to_string());
        }

        Self {
            server: server.to_string(),
            scheduling,
            selected_row: 0,
            editing: None,
            error: None,
            max_concurrent_field,
            min_interval_field,
            queue_timeout_field,
            max_queue_depth_field,
        }
    }

    pub(super) fn selected(&self) -> ServerRow {
        debug_assert!(self.selected_row < SERVER_ROWS.len());
        SERVER_ROWS
            .get(self.selected_row)
            .copied()
            .unwrap_or(ServerRow::Dispatch)
    }

    pub(super) fn set_selected_row(&mut self, idx: usize) {
        self.selected_row = idx.min(SERVER_ROWS.len().saturating_sub(1));
    }

    pub(super) fn move_selected(&mut self, delta: isize) {
        let len = SERVER_ROWS.len();
        if len == 0 {
            return;
        }
        let cur = self.selected_row as isize;
        let next = (cur + delta).rem_euclid(len as isize) as usize;
        self.selected_row = next;
    }

    pub(super) fn field_mut_for_row(&mut self, row: ServerRow) -> Option<&mut FormTextField> {
        match row {
            ServerRow::MaxConcurrent => Some(&mut self.max_concurrent_field),
            ServerRow::MinInterval => Some(&mut self.min_interval_field),
            ServerRow::QueueTimeout => Some(&mut self.queue_timeout_field),
            ServerRow::MaxQueueDepth => Some(&mut self.max_queue_depth_field),
            _ => None,
        }
    }

    pub(super) fn toggle_dispatch(&mut self) {
        self.scheduling.dispatch = match self.scheduling.dispatch {
            McpDispatchMode::Exclusive => McpDispatchMode::Parallel,
            McpDispatchMode::Parallel => McpDispatchMode::Exclusive,
        };
    }

    pub(super) fn clear_optional_field(&mut self, row: ServerRow) {
        match row {
            ServerRow::MinInterval => self.min_interval_field.set_text(""),
            ServerRow::QueueTimeout => self.queue_timeout_field.set_text(""),
            ServerRow::MaxQueueDepth => self.max_queue_depth_field.set_text(""),
            _ => {}
        }
    }

    pub(super) fn commit(&mut self) -> Result<McpServerSchedulingToml, String> {
        let max_concurrent = parse_u32_field("Max concurrent", self.max_concurrent_field.text())?;
        let min_interval_sec = parse_secs_field("Min interval", self.min_interval_field.text())?;
        let queue_timeout_sec = parse_secs_field("Queue timeout", self.queue_timeout_field.text())?;
        let max_queue_depth =
            parse_optional_u32_field("Max queue depth", self.max_queue_depth_field.text())?;
        Ok(McpServerSchedulingToml {
            dispatch: self.scheduling.dispatch,
            max_concurrent,
            min_interval_sec,
            queue_timeout_sec,
            max_queue_depth,
        })
    }
}

#[derive(Debug)]
pub(in crate::bottom_pane::settings_pages::mcp) struct ToolSchedulingEditor {
    pub(super) server: String,
    pub(super) tool: String,
    pub(super) server_scheduling: McpServerSchedulingToml,
    pub(super) selected_row: usize,
    pub(super) editing: Option<ToolRow>,
    pub(super) error: Option<String>,
    pub(super) override_min_interval: bool,
    pub(super) override_max_concurrent: bool,
    pub(super) min_interval_field: FormTextField,
    pub(super) max_concurrent_field: FormTextField,
}

impl ToolSchedulingEditor {
    pub(super) fn new(
        server: &str,
        tool: &str,
        server_scheduling: McpServerSchedulingToml,
        override_cfg: Option<McpToolSchedulingOverrideToml>,
    ) -> Self {
        let mut min_interval_field = FormTextField::new_single_line();
        min_interval_field.set_placeholder("inherit");
        let mut max_concurrent_field = FormTextField::new_single_line();
        max_concurrent_field.set_placeholder("inherit");

        let mut override_min_interval = false;
        let mut override_max_concurrent = false;
        if let Some(cfg) = override_cfg {
            if let Some(value) = cfg.min_interval_sec {
                override_min_interval = true;
                min_interval_field.set_text(&format_secs_for_edit(value));
            }
            if let Some(value) = cfg.max_concurrent {
                override_max_concurrent = true;
                max_concurrent_field.set_text(&value.to_string());
            }
        }

        Self {
            server: server.to_string(),
            tool: tool.to_string(),
            server_scheduling,
            selected_row: 0,
            editing: None,
            error: None,
            override_min_interval,
            override_max_concurrent,
            min_interval_field,
            max_concurrent_field,
        }
    }

    pub(super) fn selected(&self) -> ToolRow {
        debug_assert!(self.selected_row < TOOL_ROWS.len());
        TOOL_ROWS
            .get(self.selected_row)
            .copied()
            .unwrap_or(ToolRow::MinInterval)
    }

    pub(super) fn set_selected_row(&mut self, idx: usize) {
        self.selected_row = idx.min(TOOL_ROWS.len().saturating_sub(1));
    }

    pub(super) fn move_selected(&mut self, delta: isize) {
        let len = TOOL_ROWS.len();
        if len == 0 {
            return;
        }
        let cur = self.selected_row as isize;
        let next = (cur + delta).rem_euclid(len as isize) as usize;
        self.selected_row = next;
    }

    pub(super) fn clear_all_overrides(&mut self) {
        self.override_min_interval = false;
        self.override_max_concurrent = false;
        self.min_interval_field.set_text("");
        self.max_concurrent_field.set_text("");
        self.editing = None;
    }

    pub(super) fn clear_override_for_row(&mut self, row: ToolRow) {
        match row {
            ToolRow::MinInterval => {
                self.override_min_interval = false;
                self.min_interval_field.set_text("");
            }
            ToolRow::MaxConcurrent => {
                self.override_max_concurrent = false;
                self.max_concurrent_field.set_text("");
            }
            _ => {}
        }
    }

    fn ensure_override_defaults(&mut self, row: ToolRow) {
        match row {
            ToolRow::MinInterval => {
                if self.min_interval_field.text().trim().is_empty() {
                    if let Some(v) = self.server_scheduling.min_interval_sec {
                        self.min_interval_field.set_text(&format_secs_for_edit(v));
                    } else {
                        self.min_interval_field.set_text("1");
                    }
                }
            }
            ToolRow::MaxConcurrent => {
                if self.max_concurrent_field.text().trim().is_empty() {
                    self.max_concurrent_field
                        .set_text(&self.server_scheduling.max_concurrent.to_string());
                }
            }
            _ => {}
        }
    }

    pub(super) fn toggle_inherit_override(&mut self, row: ToolRow) {
        match row {
            ToolRow::MinInterval => {
                if self.override_min_interval {
                    self.override_min_interval = false;
                    self.min_interval_field.set_text("");
                    self.editing = None;
                } else {
                    self.override_min_interval = true;
                    self.ensure_override_defaults(ToolRow::MinInterval);
                    self.editing = Some(ToolRow::MinInterval);
                }
            }
            ToolRow::MaxConcurrent => {
                if self.override_max_concurrent {
                    self.override_max_concurrent = false;
                    self.max_concurrent_field.set_text("");
                    self.editing = None;
                } else {
                    self.override_max_concurrent = true;
                    self.ensure_override_defaults(ToolRow::MaxConcurrent);
                    self.editing = Some(ToolRow::MaxConcurrent);
                }
            }
            _ => {}
        }
    }

    pub(super) fn field_mut_for_row(&mut self, row: ToolRow) -> Option<&mut FormTextField> {
        match row {
            ToolRow::MinInterval => Some(&mut self.min_interval_field),
            ToolRow::MaxConcurrent => Some(&mut self.max_concurrent_field),
            _ => None,
        }
    }

    pub(super) fn commit(&mut self) -> Result<Option<McpToolSchedulingOverrideToml>, String> {
        let min_interval_sec = if self.override_min_interval {
            let raw = self.min_interval_field.text().trim();
            if raw.is_empty() {
                return Err("Min interval: enter a value or toggle override off".to_string());
            }
            let parsed = parse_secs_field("Min interval", raw)?;
            let Some(value) = parsed else {
                return Err("Min interval: enter a value or toggle override off".to_string());
            };
            Some(value)
        } else {
            None
        };
        let max_concurrent = if self.override_max_concurrent {
            Some(parse_u32_field("Max concurrent", self.max_concurrent_field.text())?)
        } else {
            None
        };

        let cfg = McpToolSchedulingOverrideToml {
            max_concurrent,
            min_interval_sec,
        };
        if cfg.max_concurrent.is_none() && cfg.min_interval_sec.is_none() {
            Ok(None)
        } else {
            Ok(Some(cfg))
        }
    }
}

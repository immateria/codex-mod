use super::super::*;

pub(crate) fn append_thought_ellipsis(text: &str) -> String {
    let trimmed = text.trim_end();
    if trimmed.ends_with('…') {
        trimmed.to_string()
    } else {
        format!("{trimmed}…")
    }
}

pub(crate) fn extract_latest_bold_title(text: &str) -> Option<String> {
    fn prev_non_ws(text: &str, end: usize) -> Option<char> {
        text[..end].chars().rev().find(|ch| !ch.is_whitespace())
    }

    fn next_non_ws(text: &str, start: usize) -> Option<char> {
        text[start..].chars().find(|ch| !ch.is_whitespace())
    }

    fn normalize_candidate(candidate: &str) -> Option<String> {
        let trimmed = candidate.trim();
        if trimmed.is_empty() {
            return None;
        }
        Some(trimmed.to_string())
    }

    let bytes = text.as_bytes();
    let mut idx = 0usize;
    let mut latest: Option<String> = None;
    let mut open_start: Option<usize> = None;

    while idx + 1 < bytes.len() {
        if bytes[idx] == b'*' && bytes[idx + 1] == b'*' {
            if let Some(start) = open_start {
                let candidate = &text[start..idx];
                let before = prev_non_ws(text, start);
                let after = next_non_ws(text, idx + 2);
                let looks_like_heading = before
                    .map(|ch| matches!(ch, '"' | '\n' | '\r' | ':' | '[' | '{'))
                    .unwrap_or(true)
                    && after
                        .map(|ch| matches!(ch, '"' | '\n' | '\r' | ',' | '}' | ']'))
                        .unwrap_or(true);

                if looks_like_heading
                    && let Some(clean) = normalize_candidate(candidate) {
                        latest = Some(clean);
                    }
                open_start = None;
                idx += 2;
                continue;
            } else {
                open_start = Some(idx + 2);
                idx += 2;
                continue;
            }
        }
        idx += 1;
    }

    if latest.is_none()
        && let Some(start) = open_start
            && let Some(clean) = normalize_candidate(&text[start..]) {
                latest = Some(clean);
            }

    if latest.is_some() {
        return latest;
    }

    for raw_line in text.lines() {
        let trimmed = raw_line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if let Some(title) = heading_from_line(trimmed) {
            latest = Some(title);
        }
    }

    latest
}

pub(crate) fn heading_from_line(line: &str) -> Option<String> {
    let normalized = remove_bullet_prefix(line.trim_start());
    if !normalized.starts_with("**") {
        return None;
    }

    let rest = &normalized[2..];
    let end = rest.find("**");
    let title = match end {
        Some(idx) => &rest[..idx],
        None => rest,
    };

    if title.trim().is_empty() {
        return None;
    }

    Some(title.to_string())
}

pub(crate) fn remove_bullet_prefix(line: &str) -> &str {
    let mut normalized = line;
    for prefix in ["- ", "* ", "\u{2022} "] {
        if normalized.starts_with(prefix) {
            normalized = normalized[prefix.len()..].trim_start();
            break;
        }
    }
    normalized
}

pub(crate) fn strip_role_prefix_if_present(input: &str) -> (&str, bool) {
    const PREFIXES: [&str; 2] = ["Coordinator:", "CLI:"];
    for prefix in PREFIXES {
        if input.len() >= prefix.len() {
            let (head, tail) = input.split_at(prefix.len());
            if head.eq_ignore_ascii_case(prefix) {
                return (tail, true);
            }
        }
    }
    (input, false)
}



#[derive(Default)]
pub(crate) struct ExecState {
    pub(crate) running_commands: HashMap<ExecCallId, RunningCommand>,
    pub(crate) running_explore_agg_index: Option<usize>,
    // Pairing map for out-of-order exec events. If an ExecEnd arrives before
    // ExecBegin, we stash it briefly and either pair it when Begin arrives or
    // flush it after a short timeout to show a fallback cell.
    pub(crate) pending_exec_ends: HashMap<
        ExecCallId,
        (
            ExecCommandEndEvent,
            code_core::protocol::OrderMeta,
            std::time::Instant,
        ),
    >,
    pub(crate) suppressed_exec_end_call_ids: HashSet<ExecCallId>,
    pub(crate) suppressed_exec_end_order: VecDeque<ExecCallId>,
}

impl ExecState {
    pub(crate) fn suppress_exec_end(&mut self, call_id: ExecCallId) {
        if self.suppressed_exec_end_call_ids.insert(call_id.clone()) {
            self.suppressed_exec_end_order.push_back(call_id);
            const MAX_TRACKED_SUPPRESSED_IDS: usize = 64;
            if self.suppressed_exec_end_order.len() > MAX_TRACKED_SUPPRESSED_IDS
                && let Some(old) = self.suppressed_exec_end_order.pop_front() {
                    self.suppressed_exec_end_call_ids.remove(&old);
                }
        }
    }

    pub(crate) fn unsuppress_exec_end(&mut self, call_id: &ExecCallId) {
        if self.suppressed_exec_end_call_ids.remove(call_id) {
            self.suppressed_exec_end_order.retain(|cid| cid != call_id);
        }
    }

    pub(crate) fn should_suppress_exec_end(&self, call_id: &ExecCallId) -> bool {
        self.suppressed_exec_end_call_ids.contains(call_id)
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct RunningToolEntry {
    pub(crate) order_key: OrderKey,
    pub(crate) fallback_index: usize,
    pub(crate) history_id: Option<HistoryId>,
}

impl RunningToolEntry {
    pub(crate) fn new(order_key: OrderKey, fallback_index: usize) -> Self {
        Self {
            order_key,
            fallback_index,
            history_id: None,
        }
    }

    pub(crate) fn with_history_id(mut self, id: Option<HistoryId>) -> Self {
        self.history_id = id;
        self
    }
}

#[derive(Default)]
pub(crate) struct ToolState {
    pub(crate) running_custom_tools: HashMap<ToolCallId, RunningToolEntry>,
    pub(crate) web_search_sessions: HashMap<String, web_search_sessions::WebSearchTracker>,
    pub(crate) web_search_by_call: HashMap<String, String>,
    pub(crate) web_search_by_order: HashMap<u64, String>,
    pub(crate) running_wait_tools: HashMap<ToolCallId, ExecCallId>,
    pub(crate) running_kill_tools: HashMap<ToolCallId, ExecCallId>,
    pub(crate) image_viewed_calls: HashSet<ToolCallId>,
    pub(crate) browser_sessions: HashMap<String, browser_sessions::BrowserSessionTracker>,
    pub(crate) browser_session_by_call: HashMap<String, String>,
    pub(crate) browser_session_by_order: HashMap<BrowserSessionOrderKey, String>,
    pub(crate) browser_last_key: Option<String>,
    pub(crate) agent_runs: HashMap<String, agent_runs::AgentRunTracker>,
    pub(crate) agent_run_by_call: HashMap<String, String>,
    pub(crate) agent_run_by_order: HashMap<u64, String>,
    pub(crate) agent_run_by_batch: HashMap<String, String>,
    pub(crate) agent_run_by_agent: HashMap<String, String>,
    pub(crate) agent_last_key: Option<String>,
    pub(crate) auto_drive_tracker: Option<auto_drive_cards::AutoDriveTracker>,
}
#[derive(Default)]
pub(crate) struct StreamState {
    pub(crate) current_kind: Option<StreamKind>,
    pub(crate) closed_answer_ids: HashSet<StreamId>,
    pub(crate) closed_reasoning_ids: HashSet<StreamId>,
    pub(crate) seq_answer_final: Option<u64>,
    pub(crate) drop_streaming: bool,
    pub(crate) answer_markup: HashMap<String, AnswerMarkupState>,
}

#[derive(Default)]
pub(crate) struct AnswerMarkupState {
    pub(crate) parser: code_utils_stream_parser::AssistantTextStreamParser,
    pub(crate) citations: Vec<String>,
    pub(crate) plan_markdown: String,
}

#[derive(Default)]
pub(crate) struct LayoutState {
    // Scroll offset from bottom (0 = bottom)
    pub(crate) scroll_offset: Cell<u16>,
    // Cached max scroll from last render
    pub(crate) last_max_scroll: std::cell::Cell<u16>,
    // Track last viewport height of the history content area
    pub(crate) last_history_viewport_height: std::cell::Cell<u16>,
    // Stateful vertical scrollbar for history view
    pub(crate) vertical_scrollbar_state: std::cell::RefCell<ScrollbarState>,
    // Auto-hide scrollbar timer
    pub(crate) scrollbar_visible_until: std::cell::Cell<Option<std::time::Instant>>,
    // Last effective bottom pane height used by layout (rows)
    pub(crate) last_bottom_reserved_rows: std::cell::Cell<u16>,
    pub(crate) last_frame_height: std::cell::Cell<u16>,
    pub(crate) last_frame_width: std::cell::Cell<u16>,
    // Last bottom pane area for mouse hit testing
    pub(crate) last_bottom_pane_area: std::cell::Cell<Rect>,
}

#[derive(Default)]
pub(crate) struct DiffsState {
    pub(crate) session_patch_sets: Vec<HashMap<PathBuf, code_core::protocol::FileChange>>,
    pub(crate) baseline_file_contents: HashMap<PathBuf, String>,
    pub(crate) overlay: Option<DiffOverlay>,
    pub(crate) confirm: Option<DiffConfirm>,
    pub(crate) body_visible_rows: std::cell::Cell<u16>,
}

#[derive(Default)]
pub(crate) struct HelpState {
    pub(crate) overlay: Option<HelpOverlay>,
    pub(crate) body_visible_rows: std::cell::Cell<u16>,
}

#[derive(Default)]
pub(crate) struct SettingsState {
    pub(crate) overlay: Option<SettingsOverlayView>,
}

pub(crate) struct BrowserOverlayState {
    pub(crate) session_key: RefCell<Option<String>>,
    pub(crate) screenshot_index: Cell<usize>,
    pub(crate) action_scroll: Cell<u16>,
    pub(crate) last_action_view_height: Cell<u16>,
    pub(crate) max_action_scroll: Cell<u16>,
}

impl Default for BrowserOverlayState {
    fn default() -> Self {
        Self {
            session_key: RefCell::new(None),
            screenshot_index: Cell::new(0),
            action_scroll: Cell::new(0),
            last_action_view_height: Cell::new(0),
            max_action_scroll: Cell::new(0),
        }
    }
}

impl BrowserOverlayState {
    pub(crate) fn reset(&self) {
        self.screenshot_index.set(0);
        self.action_scroll.set(0);
        self.last_action_view_height.set(0);
        self.max_action_scroll.set(0);
    }

    pub(crate) fn session_key(&self) -> Option<String> {
        self.session_key.borrow().clone()
    }

    pub(crate) fn set_session_key(&self, key: Option<String>) {
        *self.session_key.borrow_mut() = key;
    }

    pub(crate) fn screenshot_index(&self) -> usize {
        self.screenshot_index.get()
    }

    pub(crate) fn set_screenshot_index(&self, index: usize) {
        self.screenshot_index.set(index);
    }

    pub(crate) fn action_scroll(&self) -> u16 {
        self.action_scroll.get()
    }

    pub(crate) fn set_action_scroll(&self, value: u16) {
        self.action_scroll.set(value);
    }

    pub(crate) fn update_action_metrics(&self, height: u16, max_scroll: u16) {
        self.last_action_view_height.set(height);
        self.max_action_scroll.set(max_scroll);
        if self.action_scroll.get() > max_scroll {
            self.action_scroll.set(max_scroll);
        }
    }

    pub(crate) fn last_action_view_height(&self) -> u16 {
        self.last_action_view_height.get()
    }

    pub(crate) fn max_action_scroll(&self) -> u16 {
        self.max_action_scroll.get()
    }
}

#[derive(Default)]
pub(crate) struct LimitsState {
    pub(crate) cached_content: Option<LimitsOverlayContent>,
}

pub(crate) struct HelpOverlay {
    pub(crate) lines: Vec<RtLine<'static>>,
    pub(crate) scroll: u16,
}

impl HelpOverlay {
    pub(crate) fn new(lines: Vec<RtLine<'static>>) -> Self {
        Self { lines, scroll: 0 }
    }
}
#[derive(Default)]
pub(crate) struct PerfState {
    pub(crate) enabled: bool,
    pub(crate) stats: RefCell<PerfStats>,
    pub(crate) pending_scroll_rows: Cell<u64>,
}

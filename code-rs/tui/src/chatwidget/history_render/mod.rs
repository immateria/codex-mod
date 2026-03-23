use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Instant;

use ratatui::buffer::Cell as BufferCell;
use ratatui::text::Line;

use crate::history::state::{HistoryId, HistoryRecord, HistoryState};
use crate::history_cell::{
    assistant_markdown_lines,
    compute_assistant_layout,
    diff_lines_from_record,
    explore_lines_from_record_with_force,
    explore_lines_without_truncation,
    exec_display_lines_from_record,
    merged_exec_lines_from_record,
    stream_lines_from_state,
    AssistantLayoutCache,
    AssistantMarkdownCell,
    HistoryCell,
};
use code_core::config::Config;
#[cfg(feature = "code-fork")]
use crate::foundation::wrapping::word_wrap_lines;
#[cfg(not(feature = "code-fork"))]
use crate::insert_history::word_wrap_lines;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

include!("cache_stats.rs");
include!("render_state.rs");
include!("layout_cache.rs");
include!("requests.rs");

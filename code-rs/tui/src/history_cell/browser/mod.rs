mod actions;
mod model;
mod render;
mod screenshot;

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use super::card_style::{
    BORDER_TOP,
    BORDER_BODY,
    BORDER_BOTTOM,
    DEFAULT_TEXT_INDENT,
    MEDIA_GAP as SCREENSHOT_GAP,
    MEDIA_LEFT_PAD as SCREENSHOT_LEFT_PAD,
    MEDIA_MAX_ROWS as MAX_SCREENSHOT_ROWS,
    MEDIA_MAX_WIDTH as SCREENSHOT_MAX_WIDTH,
    MEDIA_MIN_TEXT_WIDTH as MIN_TEXT_WIDTH,
    MEDIA_MIN_WIDTH as SCREENSHOT_MIN_WIDTH,
    MEDIA_TEXT_RIGHT_PADDING as TEXT_RIGHT_PADDING,
};

const MAX_ACTIONS: usize = 24;
const MAX_CONSOLE: usize = 12;
const ACTION_DISPLAY_HEAD: usize = 4;
const ACTION_DISPLAY_TAIL: usize = 4;
const MIN_SCREENSHOT_ROWS: usize = 6;
const ACTION_LABEL_GAP: usize = 2;
const ACTION_TIME_GAP: usize = 2;
const ACTION_TIME_COLUMN_MIN_WIDTH: usize = 2;
const MAX_SCREENSHOT_HISTORY: usize = 24;

#[derive(Clone)]
pub(crate) struct BrowserScreenshotRecord {
    pub path: PathBuf,
    pub url: Option<String>,
    pub timestamp: Duration,
}

pub(crate) struct BrowserSessionCell {
    url: Option<String>,
    title: Option<String>,
    actions: Vec<actions::BrowserAction>,
    console_messages: Vec<String>,
    screenshot_path: Option<String>,
    screenshot_history: Vec<BrowserScreenshotRecord>,
    total_duration: Duration,
    completed: bool,
    cell_key: Option<String>,
    pub(crate) parent_call_id: Option<String>,
    headless: Option<bool>,
    status_code: Option<String>,
    cached_picker: Rc<RefCell<Option<ratatui_image::picker::Picker>>>,
    cached_image_protocol:
        Rc<RefCell<Option<(PathBuf, ratatui::layout::Rect, ratatui_image::protocol::Protocol)>>>,
}

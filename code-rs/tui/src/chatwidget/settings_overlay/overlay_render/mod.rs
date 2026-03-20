use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Layout, Margin, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Widget, Wrap};
use unicode_width::UnicodeWidthStr;

use crate::bottom_pane::{
    settings_ui::panel::{SettingsPanel, SettingsPanelStyle},
    SettingsSection,
};
use crate::live_wrap::take_prefix_by_width;
use crate::ui_interaction::ListWindow;
use crate::util::buffer::fill_rect;

use super::types::{LABEL_COLUMN_WIDTH, SettingsHelpOverlay};
use super::{SettingsContent, SettingsOverlayView};

include!("core.rs");
include!("overview_hints.rs");
include!("section.rs");
include!("sidebar_content.rs");

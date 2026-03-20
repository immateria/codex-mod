use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget, Wrap};
use std::cell::Cell;
use std::ops::Range;

use code_core::config_types::LimitsLayoutMode as ConfigLimitsLayoutMode;

use super::super::limits_overlay::{LimitsOverlay, LimitsOverlayContent, LimitsTab, LimitsTabBody};
use super::SettingsContent;
use crate::util::buffer::fill_rect;
use unicode_width::UnicodeWidthStr;

include!("model.rs");
include!("widgets.rs");
include!("content_impl.rs");
include!("content_trait.rs");

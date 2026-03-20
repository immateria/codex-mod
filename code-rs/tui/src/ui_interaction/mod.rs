use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::buffer::Buffer;
use ratatui::widgets::{Scrollbar, ScrollbarOrientation, ScrollbarState, StatefulWidget};
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};

use crate::bottom_pane::ConditionalUpdate;

include!("layout.rs");
include!("scrollbar.rs");
include!("list.rs");
include!("selectable_list_mouse.rs");
include!("tests.rs");

use std::borrow::Cow;
use std::time::Duration;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::auto_drive_style::FrameStyle;
use crate::bottom_pane::ChatComposer;

use super::{AutoActiveViewModel, AutoCoordinatorView};

mod active;
mod buttons;
mod clear;
mod estimation;
mod header;
mod prompt;

struct ButtonContext {
    label: String,
    enabled: bool,
}

struct VariantContext {
    button: Option<ButtonContext>,
    ctrl_hint: String,
    manual_hint: Option<String>,
}

struct IntroState<'a> {
    header_text: Cow<'a, str>,
    body_visible: bool,
    schedule_next_in: Option<Duration>,
}

struct HeaderRenderParams<'a> {
    area: Rect,
    model: &'a AutoActiveViewModel,
    frame_style: &'a FrameStyle,
    display_message: &'a str,
    header_label: &'a str,
    full_title: &'a str,
    intro: &'a IntroState<'a>,
}

impl AutoCoordinatorView {
    pub(super) fn estimated_height_active(
        &self,
        width: u16,
        model: &AutoActiveViewModel,
        composer_height: u16,
    ) -> u16 {
        estimation::estimated_height_active_inner(self, width, model, composer_height)
    }

    pub(super) fn render_active(
        &self,
        area: Rect,
        buf: &mut Buffer,
        model: &AutoActiveViewModel,
        composer: Option<&ChatComposer>,
    ) {
        active::render_active_inner(self, area, buf, model, composer);
    }
}


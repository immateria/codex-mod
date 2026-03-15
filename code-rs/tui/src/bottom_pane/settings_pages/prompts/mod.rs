use code_protocol::custom_prompts::CustomPrompt;
use crate::app_event_sender::AppEventSender;
use ratatui::style::Style;

use crate::components::form_text_field::FormTextField;

mod input;
mod model;
mod mouse;
mod pages;
mod pane_impl;
mod render;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Focus {
    List,
    Name,
    Body,
    Save,
    Delete,
    Cancel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    List,
    Edit,
}

pub(crate) struct PromptsSettingsView {
    prompts: Vec<CustomPrompt>,
    selected: usize,
    focus: Focus,
    name_field: FormTextField,
    body_field: FormTextField,
    status: Option<(String, Style)>,
    app_event_tx: AppEventSender,
    is_complete: bool,
    mode: Mode,
}

pub(crate) type PromptsSettingsViewFramed<'v> = crate::bottom_pane::chrome_view::Framed<'v, PromptsSettingsView>;
pub(crate) type PromptsSettingsViewContentOnly<'v> =
    crate::bottom_pane::chrome_view::ContentOnly<'v, PromptsSettingsView>;
pub(crate) type PromptsSettingsViewFramedMut<'v> =
    crate::bottom_pane::chrome_view::FramedMut<'v, PromptsSettingsView>;
pub(crate) type PromptsSettingsViewContentOnlyMut<'v> =
    crate::bottom_pane::chrome_view::ContentOnlyMut<'v, PromptsSettingsView>;

use crate::app_event_sender::AppEventSender;
use crate::bottom_pane::settings_ui::buttons::SettingsButtonKind;
use crate::components::form_text_field::FormTextField;
use code_common::shell_presets::ShellPreset;
use code_core::config_types::{ShellConfig, ShellScriptStyle};

mod input;
mod model;
mod mouse;
mod pages;
mod pane_impl;
mod render;
#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EditFocus {
    Field,
    Actions,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EditAction {
    Apply,
    Pick,
    Show,
    Resolve,
    Style,
    Back,
}

const EDIT_ACTION_ITEMS: [(EditAction, SettingsButtonKind); 6] = [
    (EditAction::Apply, SettingsButtonKind::Apply),
    (EditAction::Pick, SettingsButtonKind::Pick),
    (EditAction::Show, SettingsButtonKind::Show),
    (EditAction::Resolve, SettingsButtonKind::Resolve),
    (EditAction::Style, SettingsButtonKind::Style),
    (EditAction::Back, SettingsButtonKind::Back),
];

const EDIT_ACTION_ITEMS_NO_PICKER: [(EditAction, SettingsButtonKind); 4] = [
    (EditAction::Apply, SettingsButtonKind::Apply),
    (EditAction::Resolve, SettingsButtonKind::Resolve),
    (EditAction::Style, SettingsButtonKind::Style),
    (EditAction::Back, SettingsButtonKind::Back),
];

/// A shell option with availability status
#[derive(Clone, Debug)]
struct ShellOption {
    preset: ShellPreset,
    available: bool,
    resolved_path: Option<String>,
}

pub(crate) struct ShellSelectionView {
    shells: Vec<ShellOption>,
    selected_index: usize,
    current_shell: Option<ShellConfig>,
    app_event_tx: AppEventSender,
    is_complete: bool,
    custom_input_mode: bool,
    custom_field: FormTextField,
    custom_style_override: Option<ShellScriptStyle>,
    native_picker_notice: Option<String>,
    edit_focus: EditFocus,
    selected_action: EditAction,
    hovered_action: Option<EditAction>,
}

pub(crate) type ShellSelectionViewFramed<'v> =
    crate::bottom_pane::chrome_view::Framed<'v, ShellSelectionView>;
pub(crate) type ShellSelectionViewContentOnly<'v> =
    crate::bottom_pane::chrome_view::ContentOnly<'v, ShellSelectionView>;
pub(crate) type ShellSelectionViewFramedMut<'v> =
    crate::bottom_pane::chrome_view::FramedMut<'v, ShellSelectionView>;
pub(crate) type ShellSelectionViewContentOnlyMut<'v> =
    crate::bottom_pane::chrome_view::ContentOnlyMut<'v, ShellSelectionView>;

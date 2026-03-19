use super::*;

use crate::bottom_pane::settings_ui::buttons::StandardButtonSpec;

mod buttons;
mod input;
mod mouse;
mod paste;
mod scroll;
mod style_profile;

impl SkillsSettingsView {
    pub fn handle_key_event_direct(&mut self, key: KeyEvent) -> bool {
        if self.complete {
            return true;
        }

        let handled = input::handle_key_event_direct_inner(self, key);
        if handled && matches!(self.mode, Mode::Edit) {
            let _ = scroll::ensure_edit_focus_visible_from_last_render(self);
        }

        handled
    }

    pub fn handle_paste_direct(&mut self, text: String) -> bool {
        if self.complete {
            return false;
        }

        if !matches!(self.mode, Mode::Edit) {
            return false;
        }

        paste::handle_paste_direct_inner(self, text)
    }

    pub(super) fn handle_mouse_event_direct_framed(
        &mut self,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> bool {
        mouse::handle_mouse_event_direct_impl(self, mouse_event, area, ChromeMode::Framed)
    }

    pub(super) fn handle_mouse_event_direct_content_only(
        &mut self,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> bool {
        mouse::handle_mouse_event_direct_impl(self, mouse_event, area, ChromeMode::ContentOnly)
    }

    pub(super) fn action_button_specs(&self) -> Vec<StandardButtonSpec<ActionButton>> {
        buttons::action_button_specs_inner(self)
    }

    pub(super) fn set_style_resource_fields_from_profile(&mut self, style: Option<ShellScriptStyle>) {
        style_profile::set_style_resource_fields_from_profile_inner(self, style);
    }

    pub(super) fn infer_style_profile_mode(
        &self,
        shell_style: &str,
        slug: &str,
        display_name: &str,
    ) -> StyleProfileMode {
        style_profile::infer_style_profile_mode_inner(self, shell_style, slug, display_name)
    }

    pub(super) fn parse_shell_style(
        &self,
        shell_style_raw: &str,
    ) -> Result<Option<ShellScriptStyle>, String> {
        style_profile::parse_shell_style_inner(shell_style_raw)
    }
}


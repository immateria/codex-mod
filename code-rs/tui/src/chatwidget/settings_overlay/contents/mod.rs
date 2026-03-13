macro_rules! impl_settings_content {
    ($ty:ty) => {
        impl super::super::SettingsContent for $ty {
            fn render(&self, area: ratatui::layout::Rect, buf: &mut ratatui::buffer::Buffer) {
                self.view.content_only().render(area, buf);
            }

            fn handle_key(&mut self, key: crossterm::event::KeyEvent) -> bool {
                self.view.handle_key_event_direct(key)
            }

            fn is_complete(&self) -> bool {
                self.view.is_complete()
            }

            fn handle_mouse(
                &mut self,
                mouse_event: crossterm::event::MouseEvent,
                area: ratatui::layout::Rect,
            ) -> bool {
                self.view
                    .content_only_mut()
                    .handle_mouse_event_direct(mouse_event, area)
            }
        }
    };
}

macro_rules! impl_settings_content_with_paste {
    ($ty:ty) => {
        impl super::super::SettingsContent for $ty {
            fn render(&self, area: ratatui::layout::Rect, buf: &mut ratatui::buffer::Buffer) {
                self.view.content_only().render(area, buf);
            }

            fn handle_key(&mut self, key: crossterm::event::KeyEvent) -> bool {
                self.view.handle_key_event_direct(key)
            }

            fn is_complete(&self) -> bool {
                self.view.is_complete()
            }

            fn handle_paste(&mut self, text: String) -> bool {
                self.view.handle_paste_direct(text)
            }

            fn handle_mouse(
                &mut self,
                mouse_event: crossterm::event::MouseEvent,
                area: ratatui::layout::Rect,
            ) -> bool {
                self.view
                    .content_only_mut()
                    .handle_mouse_event_direct(mouse_event, area)
            }
        }
    };
}

macro_rules! impl_settings_content_view_complete {
    ($ty:ty) => {
        impl super::super::SettingsContent for $ty {
            fn render(&self, area: ratatui::layout::Rect, buf: &mut ratatui::buffer::Buffer) {
                self.view.content_only().render(area, buf);
            }

            fn handle_key(&mut self, key: crossterm::event::KeyEvent) -> bool {
                self.view.handle_key_event_direct(key)
            }

            fn is_complete(&self) -> bool {
                self.view.is_view_complete()
            }

            fn handle_mouse(
                &mut self,
                mouse_event: crossterm::event::MouseEvent,
                area: ratatui::layout::Rect,
            ) -> bool {
                self.view
                    .content_only_mut()
                    .handle_mouse_event_direct(mouse_event, area)
            }
        }
    };
}

macro_rules! impl_settings_content_conditional_mouse {
    ($ty:ty) => {
        impl super::super::SettingsContent for $ty {
            fn render(&self, area: ratatui::layout::Rect, buf: &mut ratatui::buffer::Buffer) {
                self.view.content_only().render(area, buf);
            }

            fn handle_key(&mut self, key: crossterm::event::KeyEvent) -> bool {
                self.view.handle_key_event_direct(key)
            }

            fn is_complete(&self) -> bool {
                self.view.is_complete()
            }

            fn handle_mouse(
                &mut self,
                mouse_event: crossterm::event::MouseEvent,
                area: ratatui::layout::Rect,
            ) -> bool {
                matches!(
                    self.view
                        .content_only_mut()
                        .handle_mouse_event_direct(mouse_event, area),
                    crate::bottom_pane::ConditionalUpdate::NeedsRedraw
                )
            }
        }
    };
}

macro_rules! impl_settings_content_view_complete_key_always_true {
    ($ty:ty) => {
        impl super::super::SettingsContent for $ty {
            fn render(&self, area: ratatui::layout::Rect, buf: &mut ratatui::buffer::Buffer) {
                self.view.content_only().render(area, buf);
            }

            fn handle_key(&mut self, key: crossterm::event::KeyEvent) -> bool {
                self.view.handle_key_event_direct(key);
                true
            }

            fn is_complete(&self) -> bool {
                self.view.is_view_complete()
            }

            fn handle_mouse(
                &mut self,
                mouse_event: crossterm::event::MouseEvent,
                area: ratatui::layout::Rect,
            ) -> bool {
                self.view
                    .content_only_mut()
                    .handle_mouse_event_direct(mouse_event, area)
            }
        }
    };
}

mod accounts;
mod auto_drive;
mod exec_limits;
mod interface;
mod js_repl;
mod memories;
mod mcp;
mod model;
mod network;
mod notifications;
mod planning;
mod prompts;
mod review;
mod shell;
mod shell_profiles;
mod skills;
mod theme;
mod updates;
mod validation;

pub(crate) use accounts::AccountsSettingsContent;
pub(crate) use auto_drive::AutoDriveSettingsContent;
pub(crate) use exec_limits::ExecLimitsSettingsContent;
pub(crate) use interface::InterfaceSettingsContent;
pub(crate) use js_repl::JsReplSettingsContent;
pub(crate) use memories::MemoriesSettingsContent;
pub(crate) use mcp::McpSettingsContent;
pub(crate) use model::ModelSettingsContent;
pub(crate) use network::NetworkSettingsContent;
pub(crate) use notifications::NotificationsSettingsContent;
pub(crate) use planning::PlanningSettingsContent;
pub(crate) use prompts::PromptsSettingsContent;
pub(crate) use review::ReviewSettingsContent;
pub(crate) use shell::ShellSettingsContent;
pub(crate) use shell_profiles::ShellProfilesSettingsContent;
pub(crate) use skills::SkillsSettingsContent;
pub(crate) use theme::ThemeSettingsContent;
pub(crate) use updates::UpdatesSettingsContent;
pub(crate) use validation::ValidationSettingsContent;

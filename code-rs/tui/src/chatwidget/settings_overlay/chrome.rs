#[cfg(not(target_os = "android"))]
mod imp {
    use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
    use ratatui::buffer::Buffer;
    use ratatui::layout::{Alignment, Rect};
    use ratatui::style::{Modifier, Style};
    use ratatui::text::{Line, Span};
    use ratatui::widgets::{Block, Borders, Clear, Paragraph, Widget};

    use crate::app_event::AppEvent;
    use crate::app_event_sender::AppEventSender;
    use crate::chrome_launch::{CHROME_LAUNCH_CHOICES, ChromeLaunchOption};
    use crate::ui_interaction::{hit_test_repeating_rows, wrap_next, wrap_prev};

    use super::super::SettingsContent;

    pub(crate) struct ChromeSettingsContent {
        selected_index: usize,
        app_event_tx: AppEventSender,
        port: Option<u16>,
        is_complete: bool,
    }

    impl ChromeSettingsContent {
        pub(crate) fn new(app_event_tx: AppEventSender, port: Option<u16>) -> Self {
            Self {
                selected_index: 0,
                app_event_tx,
                port,
                is_complete: false,
            }
        }

        fn options() -> &'static [(ChromeLaunchOption, &'static str, &'static str)] {
            CHROME_LAUNCH_CHOICES
        }

        fn move_up(&mut self) {
            let len = Self::options().len();
            self.selected_index = wrap_prev(self.selected_index, len);
        }

        fn move_down(&mut self) {
            let len = Self::options().len();
            self.selected_index = wrap_next(self.selected_index, len);
        }

        fn confirm(&mut self) {
            if let Some((option, _, _)) = Self::options().get(self.selected_index) {
                self.app_event_tx
                    .send(AppEvent::ChromeLaunchOptionSelected(*option, self.port));
                self.is_complete = true;
            }
        }

        fn cancel(&mut self) {
            self.app_event_tx.send(AppEvent::ChromeLaunchOptionSelected(
                ChromeLaunchOption::Cancel,
                self.port,
            ));
            self.is_complete = true;
        }

        fn option_at(&self, area: Rect, mouse_event: MouseEvent) -> Option<usize> {
            if area.width == 0 || area.height == 0 {
                return None;
            }

            let inner = Block::default().borders(Borders::ALL).inner(area);
            let content_area = inner.inner(crate::ui_consts::UNIFORM_PAD);
            if content_area.width == 0 || content_area.height == 0 {
                return None;
            }

            if mouse_event.column < content_area.x
                || mouse_event.column >= content_area.x.saturating_add(content_area.width)
                || mouse_event.row < content_area.y
                || mouse_event.row >= content_area.y.saturating_add(content_area.height)
            {
                return None;
            }

            // Header uses 4 lines, each option consumes 3 lines (title + description + spacer).
            hit_test_repeating_rows(
                content_area,
                mouse_event.column,
                mouse_event.row,
                4,
                3,
                3,
                Self::options().len(),
            )
        }
    }

    impl SettingsContent for ChromeSettingsContent {
        fn render(&self, area: Rect, buf: &mut Buffer) {
            if area.width == 0 || area.height == 0 {
                return;
            }

            Clear.render(area, buf);

            let block = crate::components::popup_frame::themed_block()
                .title(Line::from(" Chrome Launch Options "))
                .title_alignment(Alignment::Center);
            let inner = block.inner(area);
            block.render(area, buf);

            if inner.width == 0 || inner.height == 0 {
                return;
            }

            let mut lines: Vec<Line<'static>> = vec![
                Line::from(vec![Span::styled(
                    "Chrome is already running or CDP connection failed",
                    Style::default()
                        .fg(crate::colors::warning())
                        .add_modifier(Modifier::BOLD),
                )]),
                Line::from(""),
                Line::from("Select an option:"),
                Line::from(""),
            ];

            for (idx, (_, label, description)) in Self::options().iter().enumerate() {
                let selected = idx == self.selected_index;
                if selected {
                    lines.push(Line::from(vec![Span::styled(
                        format!("› {label}"),
                        Style::default()
                            .fg(crate::colors::success())
                            .add_modifier(Modifier::BOLD),
                    )]));
                    lines.push(Line::from(vec![Span::styled(
                        format!("  {description}"),
                        crate::colors::style_secondary(),
                    )]));
                } else {
                    lines.push(Line::from(vec![Span::styled(
                        format!("  {label}"),
                        crate::colors::style_text(),
                    )]));
                    lines.push(Line::from(vec![Span::styled(
                        format!("  {description}"),
                        crate::colors::style_text_dim(),
                    )]));
                }
                lines.push(Line::from(""));
            }

            lines.push(crate::bottom_pane::settings_ui::hints::shortcut_line(&[
                crate::bottom_pane::settings_ui::hints::KeyHint::new(
                    format!("{ud}/jk", ud = crate::icons::nav_up_down()),
                    " move",
                ).with_key_style(crate::colors::style_function()),
                crate::bottom_pane::settings_ui::hints::hint_enter(" select"),
                crate::bottom_pane::settings_ui::hints::KeyHint::new(
                    format!("{}/q", crate::icons::escape()),
                    " cancel",
                ).with_key_style(crate::colors::style_error()),
            ]));

            let content_area = inner.inner(crate::ui_consts::UNIFORM_PAD);
            if content_area.width == 0 || content_area.height == 0 {
                return;
            }

            Paragraph::new(lines)
                .alignment(Alignment::Left)
                .style(crate::colors::style_text_on_bg())
                .render(content_area, buf);
        }

        fn handle_key(&mut self, key: KeyEvent) -> bool {
            match key.code {
                KeyCode::Up | KeyCode::Char('k') => {
                    self.move_up();
                    true
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    self.move_down();
                    true
                }
                KeyCode::Enter => {
                    self.confirm();
                    true
                }
                KeyCode::Esc | KeyCode::Char('q') => {
                    self.cancel();
                    true
                }
                _ => false,
            }
        }

        fn handle_mouse(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
            match mouse_event.kind {
                MouseEventKind::Moved => {
                    let Some(index) = self.option_at(area, mouse_event) else {
                        return false;
                    };
                    if self.selected_index == index {
                        return false;
                    }
                    self.selected_index = index;
                    true
                }
                MouseEventKind::Down(MouseButton::Left) => {
                    let Some(index) = self.option_at(area, mouse_event) else {
                        return false;
                    };
                    self.selected_index = index;
                    self.confirm();
                    true
                }
                MouseEventKind::ScrollUp => {
                    self.move_up();
                    true
                }
                MouseEventKind::ScrollDown => {
                    self.move_down();
                    true
                }
                _ => false,
            }
        }

        fn is_complete(&self) -> bool {
            self.is_complete
        }
    }
}

#[cfg(not(target_os = "android"))]
pub(crate) use imp::ChromeSettingsContent;

#[cfg(target_os = "android")]
mod android_stub {
    use crossterm::event::{KeyCode, KeyEvent, MouseEvent};
    use ratatui::buffer::Buffer;
    use ratatui::layout::{Alignment, Rect};
    use ratatui::style::{Modifier, Style};
    use ratatui::text::{Line, Span};
    use ratatui::widgets::{Block, Borders, Clear, Paragraph, Widget};

    use crate::app_event_sender::AppEventSender;

    use super::super::SettingsContent;

    pub(crate) struct ChromeSettingsContent {
        is_complete: bool,
    }

    impl ChromeSettingsContent {
        pub(crate) fn new(_app_event_tx: AppEventSender, _port: Option<u16>) -> Self {
            Self { is_complete: false }
        }
    }

    impl SettingsContent for ChromeSettingsContent {
        fn render(&self, area: Rect, buf: &mut Buffer) {
            if area.width == 0 || area.height == 0 {
                return;
            }

            Clear.render(area, buf);

            let block = crate::components::popup_frame::themed_block()
                .title(Line::from(" Chrome Launch Options "))
                .title_alignment(Alignment::Center);
            let inner = block.inner(area);
            block.render(area, buf);

            if inner.width == 0 || inner.height == 0 {
                return;
            }

            let content_area = inner.inner(crate::ui_consts::UNIFORM_PAD);
            if content_area.width == 0 || content_area.height == 0 {
                return;
            }

            let lines: Vec<Line<'static>> = vec![
                Line::from(vec![Span::styled(
                    "Chrome/CDP is not available on Android builds.",
                    Style::default()
                        .fg(crate::colors::warning())
                        .add_modifier(Modifier::BOLD),
                )]),
                Line::from(""),
                Line::from("Use the internal browser tooling on desktop builds instead."),
                Line::from(""),
                Line::from(vec![
                    Span::styled("Enter", crate::colors::style_success()),
                    Span::styled("/", crate::colors::style_text_dim()),
                    Span::styled(crate::icons::escape(), crate::colors::style_error()),
                    Span::styled(" Close", crate::colors::style_text_dim()),
                ]),
            ];

            Paragraph::new(lines)
                .alignment(Alignment::Left)
                .style(crate::colors::style_text_on_bg())
                .render(content_area, buf);
        }

        fn handle_key(&mut self, key: KeyEvent) -> bool {
            match key.code {
                KeyCode::Enter | KeyCode::Esc | KeyCode::Char('q') => {
                    self.is_complete = true;
                    true
                }
                _ => false,
            }
        }

        fn handle_mouse(&mut self, _mouse_event: MouseEvent, _area: Rect) -> bool {
            false
        }

        fn is_complete(&self) -> bool {
            self.is_complete
        }
    }
}

#[cfg(target_os = "android")]
pub(crate) use android_stub::ChromeSettingsContent;

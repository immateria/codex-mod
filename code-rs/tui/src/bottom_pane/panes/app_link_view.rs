use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ratatui::widgets::Widget;
use ratatui::widgets::Wrap;

use crate::app_event::AppEvent;
use crate::app_event::AppLinkViewParams;
use crate::app_event_sender::AppEventSender;
use crate::bottom_pane::popup_consts::MAX_POPUP_ROWS;
use crate::bottom_pane::BottomPane;
use crate::bottom_pane::BottomPaneView;
use crate::bottom_pane::CancellationEvent;
use crate::components::popup_frame::render_popup_frame;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AppLinkScreen {
    Link,
    InstallConfirmation,
}

pub(crate) struct AppLinkView {
    title: String,
    description: Option<String>,
    url: String,
    screen: AppLinkScreen,
    selected_action: usize,
    complete: bool,
    app_event_tx: AppEventSender,
}

impl AppLinkView {
    pub(crate) fn new(params: AppLinkViewParams, app_event_tx: AppEventSender) -> Self {
        let title = params.app.name.clone();
        let description = params.app.description.clone();
        let url = params
            .app
            .install_url
            .clone()
            .unwrap_or_else(|| format!("https://chatgpt.com/apps/{}", params.app.id));
        Self {
            title,
            description,
            url,
            screen: AppLinkScreen::Link,
            selected_action: 0,
            complete: false,
            app_event_tx,
        }
    }

    fn action_labels(&self) -> [&'static str; 2] {
        match self.screen {
            AppLinkScreen::Link => ["Install on ChatGPT", "Back"],
            AppLinkScreen::InstallConfirmation => ["I already installed it", "Back"],
        }
    }

    fn move_up(&mut self) {
        self.selected_action = self.selected_action.saturating_sub(1);
    }

    fn move_down(&mut self) {
        self.selected_action = (self.selected_action + 1).min(self.action_labels().len() - 1);
    }

    fn open_chatgpt_link(&mut self) {
        self.app_event_tx
            .send(AppEvent::OpenUrlInBrowser { url: self.url.clone() });
        self.screen = AppLinkScreen::InstallConfirmation;
        self.selected_action = 0;
    }

    fn refresh_and_close(&mut self) {
        self.app_event_tx
            .send(AppEvent::FetchAppsDirectory { force_refetch: true });
        self.complete = true;
    }

    fn activate_selected_action(&mut self) {
        match self.screen {
            AppLinkScreen::Link => match self.selected_action {
                0 => self.open_chatgpt_link(),
                _ => self.complete = true,
            },
            AppLinkScreen::InstallConfirmation => match self.selected_action {
                0 => self.refresh_and_close(),
                _ => self.complete = true,
            },
        }
    }

    fn wrapped_lines_for(text: &str, width: u16) -> u16 {
        if text.is_empty() || width == 0 {
            return 0;
        }
        let w = width as usize;
        let mut lines: u16 = 0;
        for part in text.split('\n') {
            let len = part.chars().count();
            if len == 0 {
                lines = lines.saturating_add(1);
                continue;
            }
            let mut l = (len / w) as u16;
            if len % w != 0 {
                l = l.saturating_add(1);
            }
            if l == 0 {
                l = 1;
            }
            lines = lines.saturating_add(l);
        }
        lines
    }

    fn content_text(&self) -> String {
        let mut lines = Vec::new();
        if let Some(desc) = self
            .description
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            lines.push(desc.to_string());
            lines.push(String::new());
        }

        match self.screen {
            AppLinkScreen::Link => {
                lines.push("This app is not installed yet.".to_string());
                lines.push("Open ChatGPT to install it, then refresh the Apps list.".to_string());
            }
            AppLinkScreen::InstallConfirmation => {
                lines.push(
                    "After installing the app on ChatGPT, choose \"I already installed it\" to refresh."
                        .to_string(),
                );
            }
        }

        lines.push(String::new());
        lines.push(format!("URL: {}", self.url));
        lines.join("\n")
    }
}

impl BottomPaneView<'_> for AppLinkView {
    fn handle_key_event(&mut self, _pane: &mut BottomPane<'_>, key_event: KeyEvent) {
        match key_event.code {
            KeyCode::Up => self.move_up(),
            KeyCode::Down => self.move_down(),
            KeyCode::Enter | KeyCode::Char(' ') => self.activate_selected_action(),
            KeyCode::Esc => self.complete = true,
            _ => {}
        }
    }

    fn is_complete(&self) -> bool {
        self.complete
    }

    fn on_ctrl_c(&mut self, _pane: &mut BottomPane<'_>) -> CancellationEvent {
        self.complete = true;
        CancellationEvent::Handled
    }

    fn desired_height(&self, width: u16) -> u16 {
        let content = self.content_text();
        let content_width = width.saturating_sub(3); // borders + left padding
        let content_rows = Self::wrapped_lines_for(&content, content_width);
        let actions_rows = self.action_labels().len() as u16;
        let total = 2 // border
            + content_rows
            + 1 // spacer
            + actions_rows;
        total.clamp(8, MAX_POPUP_ROWS as u16)
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let Some(inner) = render_popup_frame(area, buf, &self.title) else {
            return;
        };

        let content = self.content_text();
        let content_area = Rect {
            x: inner.x.saturating_add(1),
            y: inner.y,
            width: inner.width.saturating_sub(1),
            height: inner.height.saturating_sub(3), // leave room for spacer + actions
        };
        Paragraph::new(content)
            .style(Style::default().fg(crate::colors::text()))
            .wrap(Wrap { trim: true })
            .render(content_area, buf);

        let actions = self.action_labels();
        let mut y = inner.y + inner.height.saturating_sub(actions.len() as u16);
        for (idx, label) in actions.iter().enumerate() {
            let selected = idx == self.selected_action;
            let prefix = if selected { '›' } else { ' ' };
            let line = Line::from(format!("{prefix} {label}"));
            let style = if selected {
                Style::default().fg(crate::colors::primary())
            } else {
                Style::default().fg(crate::colors::text_dim())
            };
            Paragraph::new(line)
                .style(style)
                .render(
                    Rect {
                        x: inner.x.saturating_add(1),
                        y,
                        width: inner.width.saturating_sub(1),
                        height: 1,
                    },
                    buf,
                );
            y = y.saturating_add(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::app_event_sender::AppEventSender;
    use crate::auto_drive_style::AutoDriveVariant;
    use crate::bottom_pane::{BottomPane, BottomPaneParams};
    use crossterm::event::KeyModifiers;
    use std::sync::mpsc;

    fn make_app_info(id: &str, name: &str, install_url: Option<String>) -> code_app_server_protocol::AppInfo {
        code_app_server_protocol::AppInfo {
            id: id.to_string(),
            name: name.to_string(),
            description: None,
            logo_url: None,
            logo_url_dark: None,
            distribution_channel: None,
            branding: None,
            app_metadata: None,
            labels: None,
            install_url,
            is_accessible: false,
            is_enabled: false,
            plugin_display_names: Vec::new(),
        }
    }

    fn make_pane(app_event_tx: AppEventSender) -> BottomPane<'static> {
        BottomPane::new(BottomPaneParams {
            app_event_tx,
            has_input_focus: true,
            using_chatgpt_auth: true,
            auto_drive_variant: AutoDriveVariant::default(),
        })
    }

    #[test]
    fn install_action_opens_url_and_switches_to_confirmation() {
        let (tx, rx) = mpsc::channel::<AppEvent>();
        let app_event_tx = AppEventSender::new(tx);

        let app = make_app_info(
            "gdrive",
            "Google Drive",
            Some("https://chatgpt.com/apps/google-drive/gdrive".to_string()),
        );
        let expected_url = app.install_url.clone().expect("install url");

        let mut view = AppLinkView::new(AppLinkViewParams { app }, app_event_tx.clone());
        let mut pane = make_pane(app_event_tx.clone());

        view.handle_key_event(
            &mut pane,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        );

        match rx.try_recv().expect("OpenUrlInBrowser") {
            AppEvent::OpenUrlInBrowser { url } => assert_eq!(url, expected_url),
            other => panic!("unexpected event: {other:?}"),
        }

        assert!(!view.is_complete());
        assert_eq!(view.action_labels()[0], "I already installed it");
    }

    #[test]
    fn confirm_action_refreshes_directory_and_closes() {
        let (tx, rx) = mpsc::channel::<AppEvent>();
        let app_event_tx = AppEventSender::new(tx);

        let app = make_app_info(
            "gdrive",
            "Google Drive",
            Some("https://chatgpt.com/apps/google-drive/gdrive".to_string()),
        );

        let mut view = AppLinkView::new(AppLinkViewParams { app }, app_event_tx.clone());
        let mut pane = make_pane(app_event_tx.clone());

        // Enter on first screen emits OpenUrlInBrowser and moves into confirmation.
        view.handle_key_event(
            &mut pane,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        );
        let _ = rx.try_recv().expect("OpenUrlInBrowser");
        assert_eq!(view.action_labels()[0], "I already installed it");

        // Enter on confirmation triggers refresh and closes.
        view.handle_key_event(
            &mut pane,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        );
        match rx.try_recv().expect("FetchAppsDirectory") {
            AppEvent::FetchAppsDirectory { force_refetch } => assert!(force_refetch),
            other => panic!("unexpected event: {other:?}"),
        }
        assert!(view.is_complete());
    }

    #[test]
    fn esc_closes_without_emitting() {
        let (tx, rx) = mpsc::channel::<AppEvent>();
        let app_event_tx = AppEventSender::new(tx);

        let app = make_app_info(
            "gdrive",
            "Google Drive",
            Some("https://chatgpt.com/apps/google-drive/gdrive".to_string()),
        );

        // Link screen.
        let mut view = AppLinkView::new(AppLinkViewParams { app: app.clone() }, app_event_tx.clone());
        let mut pane = make_pane(app_event_tx.clone());
        view.handle_key_event(&mut pane, KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(view.is_complete());
        assert!(rx.try_recv().is_err());

        // Confirmation screen.
        let mut view = AppLinkView::new(AppLinkViewParams { app }, app_event_tx.clone());
        view.handle_key_event(&mut pane, KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        let _ = rx.try_recv().expect("OpenUrlInBrowser");
        view.handle_key_event(&mut pane, KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(view.is_complete());
        assert!(rx.try_recv().is_err());
    }
}

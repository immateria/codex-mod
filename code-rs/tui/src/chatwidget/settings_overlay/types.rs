use crossterm::event::{KeyEvent, MouseEvent};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};

use crate::bottom_pane::SettingsSection;

pub(super) const LABEL_COLUMN_WIDTH: usize = 18;

pub(crate) trait SettingsContent {
    fn render(&self, area: Rect, buf: &mut Buffer);
    fn handle_key(&mut self, key: KeyEvent) -> bool;
    fn is_complete(&self) -> bool;
    fn on_close(&mut self) {}
    fn handle_paste(&mut self, _text: String) -> bool {
        false
    }
    /// Handle mouse events in the content area. Returns true if handled/needs redraw.
    fn handle_mouse(&mut self, _mouse_event: MouseEvent, _area: Rect) -> bool {
        false
    }
    /// Returns `true` when the content is in a sub-view that Esc should navigate
    /// back from (e.g. a detail pane or edit form) rather than returning focus to
    /// the settings sidebar.
    fn has_back_navigation(&self) -> bool {
        false
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SettingsOverlayFocus {
    Sidebar,
    Content,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct MenuState {
    selected: SettingsSection,
}

impl MenuState {
    pub(super) fn new(selected: SettingsSection) -> Self {
        Self { selected }
    }

    pub(super) fn selected(self) -> SettingsSection {
        self.selected
    }

    pub(super) fn set_selected(&mut self, section: SettingsSection) {
        self.selected = section;
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct SectionState {
    active: SettingsSection,
}

impl SectionState {
    pub(super) fn new(active: SettingsSection) -> Self {
        Self { active }
    }

    pub(super) fn active(self) -> SettingsSection {
        self.active
    }

    pub(super) fn set_active(&mut self, section: SettingsSection) {
        self.active = section;
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SettingsOverlayMode {
    Menu(MenuState),
    Section(SectionState),
}

#[derive(Clone, Debug)]
pub(crate) struct SettingsOverviewRow {
    pub(crate) section: SettingsSection,
    pub(crate) summary: Option<String>,
}

impl SettingsOverviewRow {
    pub(crate) fn new(section: SettingsSection, summary: Option<String>) -> Self {
        Self { section, summary }
    }
}

#[derive(Clone, Debug)]
pub(super) struct SettingsHelpOverlay {
    pub(super) lines: Vec<Line<'static>>,
}

impl SettingsHelpOverlay {
    pub(super) fn overview() -> Self {
        let title = crate::colors::style_text_bold();
        let hint = crate::colors::style_text_dim();
        let mut lines = vec![Line::from(vec![Span::styled("Settings Overview", title)]), Line::default()];
        let nav = crate::icons::nav_up_down();
        let items: &[String] = &[
            format!("• {nav}  Move between sections"),
            "• Enter  Open selected section".into(),
            "• Tab    Jump forward between sections".into(),
            "• Esc    Close settings".into(),
            "• ?      Toggle this help".into(),
        ];
        for text in items {
            lines.push(Line::from(vec![Span::styled(text.clone(), hint)]));
        }
        lines.push(Line::default());
        lines.push(Line::from(vec![Span::styled(
            "Press Esc to close",
            crate::colors::style_text_dim(),
        )]));
        Self { lines }
    }

    pub(super) fn section(section: SettingsSection) -> Self {
        let title = crate::colors::style_text_bold();
        let hint = crate::colors::style_text_dim();
        let mut lines = vec![
            Line::from(vec![Span::styled(
                format!("{} Shortcuts", section.label()),
                title,
            )]),
            Line::default(),
            Line::from(vec![Span::styled("• Esc    Return to overview", hint)]),
            Line::from(vec![Span::styled("• Tab    Focus content", hint)]),
            Line::from(vec![Span::styled("• Shift+Tab  Focus sidebar", hint)]),
            Line::from(vec![Span::styled(
                format!("• {}    Change section (sidebar focus)", crate::icons::nav_up_down()),
                hint,
            )]),
        ];
        if matches!(section, SettingsSection::Shell | SettingsSection::ShellProfiles) {
            lines.push(Line::from(vec![Span::styled(
                "• Ctrl+P  Toggle Shell/Profiles",
                hint,
            )]));
        }
        let show_activate = matches!(
            section,
            SettingsSection::Agents
                | SettingsSection::Mcp
                | SettingsSection::JsRepl
                | SettingsSection::ExecLimits
                | SettingsSection::Accounts
                | SettingsSection::Apps
                | SettingsSection::Memories
                | SettingsSection::Experimental
                | SettingsSection::Shell
                | SettingsSection::ShellEscalation
                | SettingsSection::ShellProfiles
                | SettingsSection::Skills
        ) || {
            #[cfg(feature = "managed-network-proxy")]
            {
                matches!(section, SettingsSection::Network)
            }
            #[cfg(not(feature = "managed-network-proxy"))]
            {
                false
            }
        };
        if show_activate {
            lines.push(Line::from(vec![Span::styled(
                "• Enter  Activate focused action",
                hint,
            )]));
        }
        lines.push(Line::from(vec![Span::styled("• ?      Toggle this help", hint)]));
        lines.push(Line::default());
        lines.push(Line::from(vec![Span::styled(
            "Press Esc twice to close",
            crate::colors::style_text_dim(),
        )]));
        Self { lines }
    }
}

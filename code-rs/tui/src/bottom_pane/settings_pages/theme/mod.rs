use std::borrow::Cow;

use code_core::config_types::ThemeName;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use unicode_segmentation::UnicodeSegmentation;
use ratatui::buffer::Buffer;
use ratatui::layout::Alignment;
use ratatui::layout::Margin;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::text::Span;
use ratatui::widgets::Block;
use ratatui::widgets::Borders;
use ratatui::widgets::Paragraph;
// Cleanup: remove unused imports to satisfy warning-as-error policy
use ratatui::widgets::Clear;
use ratatui::widgets::Widget;

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::chatwidget::BackgroundOrderTicket;
use crate::theme::{custom_theme_is_dark, map_theme_for_palette, palette_mode, PaletteMode};
use crate::thread_spawner;

use crate::bottom_pane::BottomPane;
use crate::bottom_pane::BottomPaneView;

type ThemeOption = (ThemeName, &'static str, &'static str);

const THEME_OPTIONS_ANSI16: &[ThemeOption] = &[
    (
        ThemeName::LightPhotonAnsi16,
        "Light (16-color)",
        "High-contrast light palette for limited terminals",
    ),
    (
        ThemeName::DarkCarbonAnsi16,
        "Dark (16-color)",
        "High-contrast dark palette for limited terminals",
    ),
];

const THEME_OPTIONS_ANSI256: &[ThemeOption] = &[
    // Light themes (at top)
    (
        ThemeName::LightPhoton,
        "Light - Photon",
        "Clean professional light theme",
    ),
    (
        ThemeName::LightPrismRainbow,
        "Light - Prism Rainbow",
        "Vibrant rainbow accents",
    ),
    (
        ThemeName::LightVividTriad,
        "Light - Vivid Triad",
        "Cyan, pink, amber triad",
    ),
    (
        ThemeName::LightPorcelain,
        "Light - Porcelain",
        "Refined porcelain tones",
    ),
    (
        ThemeName::LightSandbar,
        "Light - Sandbar",
        "Warm sandy beach colors",
    ),
    (
        ThemeName::LightGlacier,
        "Light - Glacier",
        "Cool glacier blues",
    ),
    (
        ThemeName::DarkPaperLightPro,
        "Light - Paper Pro",
        "Premium paper-like",
    ),
    // Dark themes (below)
    (
        ThemeName::DarkCarbonNight,
        "Dark - Carbon Night",
        "Sleek modern dark theme",
    ),
    (
        ThemeName::DarkShinobiDusk,
        "Dark - Shinobi Dusk",
        "Japanese-inspired twilight",
    ),
    (
        ThemeName::DarkOledBlackPro,
        "Dark - OLED Black Pro",
        "True black for OLED displays",
    ),
    (
        ThemeName::DarkAmberTerminal,
        "Dark - Amber Terminal",
        "Retro amber CRT aesthetic",
    ),
    (
        ThemeName::DarkAuroraFlux,
        "Dark - Aurora Flux",
        "Northern lights inspired",
    ),
    (
        ThemeName::DarkCharcoalRainbow,
        "Dark - Charcoal Rainbow",
        "High-contrast accessible",
    ),
    (
        ThemeName::DarkZenGarden,
        "Dark - Zen Garden",
        "Calm and peaceful",
    ),
];

/// Interactive UI for selecting appearance (Theme & Spinner)
pub(crate) struct ThemeSelectionView {
    original_theme: ThemeName, // Theme to restore on cancel
    current_theme: ThemeName,  // Currently displayed theme
    selected_theme_index: usize,
    hovered_theme_index: Option<usize>,
    // Spinner tab state
    _original_spinner: String,
    current_spinner: String,
    selected_spinner_index: usize,
    // UI mode/state
    mode: Mode,
    overview_selected_index: usize, // 0 = Theme, 1 = Spinner
    // Revert points when backing out of detail views
    revert_theme_on_back: ThemeName,
    revert_spinner_on_back: String,
    // One-shot flags to show selection at top on first render of detail views
    just_entered_themes: bool,
    just_entered_spinner: bool,
    app_event_tx: AppEventSender,
    tail_ticket: BackgroundOrderTicket,
    before_ticket: BackgroundOrderTicket,
    is_complete: bool,
}


mod core;
mod input;
mod pane_impl;
mod render;
mod render_create;
mod render_overview;
mod render_spinner;
mod render_themes;

enum Mode {
    Overview,
    Themes,
    Spinner,
    CreateSpinner(Box<CreateState>),
    CreateTheme(Box<CreateThemeState>),
}

struct CreateState {
    step: std::cell::Cell<CreateStep>,
    /// Freeform prompt describing the desired spinner
    prompt: String,
    /// While true, we render a loading indicator and disable input
    is_loading: std::cell::Cell<bool>,
    action_idx: usize, // 0 = Create/Save, 1 = Cancel/Retry
    /// Live stream messages from the background task
    rx: Option<std::sync::mpsc::Receiver<ProgressMsg>>,
    /// Accumulated thinking/output lines for live display (completed)
    thinking_lines: std::cell::RefCell<Vec<String>>,
    /// In‑progress line assembled from deltas
    thinking_current: std::cell::RefCell<String>,
    /// Parsed proposal waiting for review
    proposed_interval: std::cell::Cell<Option<u64>>,
    proposed_frames: std::cell::RefCell<Option<Vec<String>>>,
    proposed_name: std::cell::RefCell<Option<String>>,
    /// Last raw model output captured (for debugging parse errors)
    last_raw_output: std::cell::RefCell<Option<String>>,
}

struct CreateThemeState {
    step: std::cell::Cell<CreateStep>,
    prompt: String,
    is_loading: std::cell::Cell<bool>,
    action_idx: usize, // 0 = Create/Save, 1 = Cancel/Retry
    rx: Option<std::sync::mpsc::Receiver<ProgressMsg>>,
    thinking_lines: std::cell::RefCell<Vec<String>>,
    thinking_current: std::cell::RefCell<String>,
    proposed_name: std::cell::RefCell<Option<String>>,
    proposed_colors: std::cell::RefCell<Option<code_core::config_types::ThemeColors>>,
    preview_on: std::cell::Cell<bool>,
    review_focus_is_toggle: std::cell::Cell<bool>,
    last_raw_output: std::cell::RefCell<Option<String>>,
    proposed_is_dark: std::cell::Cell<Option<bool>>,
}

impl ThemeSelectionView {
    pub(crate) fn is_complete(&self) -> bool {
        self.is_complete
    }

    pub(crate) fn framed(&self) -> ThemeSelectionViewFramed<'_> {
        crate::bottom_pane::chrome_view::Framed::new(self)
    }

    pub(crate) fn content_only(&self) -> ThemeSelectionViewContentOnly<'_> {
        crate::bottom_pane::chrome_view::ContentOnly::new(self)
    }

    pub(crate) fn framed_mut(&mut self) -> ThemeSelectionViewFramedMut<'_> {
        crate::bottom_pane::chrome_view::FramedMut::new(self)
    }

    pub(crate) fn content_only_mut(&mut self) -> ThemeSelectionViewContentOnlyMut<'_> {
        crate::bottom_pane::chrome_view::ContentOnlyMut::new(self)
    }
}

#[derive(Copy, Clone, PartialEq)]
enum CreateStep {
    Prompt,
    Action,
    Review,
}

enum ProgressMsg {
    ThinkingDelta(String),
    OutputDelta(String),
    RawOutput(String),
    SetStatus(String),
    CompletedOk {
        name: String,
        interval: u64,
        frames: Vec<String>,
    },
    CompletedThemeOk(Box<ThemeGenerationResult>),
    // `_raw_snippet` is captured for potential future display/debugging
    CompletedErr {
        error: String,
        _raw_snippet: String,
    },
}

struct ThemeGenerationResult {
    name: String,
    colors: code_core::config_types::ThemeColors,
    is_dark: Option<bool>,
}

pub(crate) type ThemeSelectionViewFramed<'v> = crate::bottom_pane::chrome_view::Framed<'v, ThemeSelectionView>;
pub(crate) type ThemeSelectionViewContentOnly<'v> =
    crate::bottom_pane::chrome_view::ContentOnly<'v, ThemeSelectionView>;
pub(crate) type ThemeSelectionViewFramedMut<'v> =
    crate::bottom_pane::chrome_view::FramedMut<'v, ThemeSelectionView>;
pub(crate) type ThemeSelectionViewContentOnlyMut<'v> =
    crate::bottom_pane::chrome_view::ContentOnlyMut<'v, ThemeSelectionView>;

impl crate::bottom_pane::chrome_view::ChromeRenderable for ThemeSelectionView {
    fn render_in_framed_chrome(&self, area: Rect, buf: &mut Buffer) {
        self.render_content(area, buf);
    }

    fn render_in_content_only_chrome(&self, area: Rect, buf: &mut Buffer) {
        self.render_content_only(area, buf);
    }
}

impl crate::bottom_pane::chrome_view::ChromeMouseHandler for ThemeSelectionView {
    fn handle_mouse_event_direct_in_framed_chrome(
        &mut self,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> bool {
        self.handle_mouse_event_direct_framed(mouse_event, area)
    }

    fn handle_mouse_event_direct_in_content_only_chrome(
        &mut self,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> bool {
        self.handle_mouse_event_direct_content_only(mouse_event, area)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use std::sync::mpsc;

    use crossterm::event::{KeyModifiers, MouseEventKind};
    use ratatui::layout::Rect;

    use super::{Mode, ThemeSelectionView};
    use crate::app_event_sender::AppEventSender;
    use crate::chatwidget::BackgroundOrderTicket;

    #[test]
    fn content_only_mouse_uses_content_geometry_not_framed_geometry() {
        let area = Rect::new(0, 0, 40, 12);
        let event = crossterm::event::MouseEvent {
            kind: MouseEventKind::Moved,
            column: area.x,
            row: area.y.saturating_add(1),
            modifiers: KeyModifiers::NONE,
        };

        let make_view = || {
            let (tx, _rx) = mpsc::channel();
            let mut view = ThemeSelectionView::new(
                code_core::config_types::ThemeName::LightPhoton,
                AppEventSender::new(tx),
                BackgroundOrderTicket::test_ticket(1),
                BackgroundOrderTicket::test_ticket(1),
            );
            view.mode = Mode::Overview;
            view.overview_selected_index = 1;
            view
        };

        let mut content_view = make_view();
        assert!(content_view
            .content_only_mut()
            .handle_mouse_event_direct(event, area));
        assert_eq!(content_view.overview_selected_index, 0);

        let mut framed_view = make_view();
        assert!(!framed_view
            .framed_mut()
            .handle_mouse_event_direct(event, area));
        assert_eq!(framed_view.overview_selected_index, 1);
    }
}

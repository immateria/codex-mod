#![allow(dead_code)]

use std::borrow::Cow;

use code_core::config_types::ThemeName;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use unicode_segmentation::UnicodeSegmentation;
use ratatui::buffer::Buffer;
use ratatui::layout::Alignment;
use ratatui::layout::Constraint;
use ratatui::layout::Direction;
use ratatui::layout::Layout;
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
use crate::theme::{custom_theme_is_dark, map_theme_for_palette, palette_mode, resolved_theme, PaletteMode};
use crate::thread_spawner;

use super::BottomPane;
use super::bottom_pane_view::BottomPaneView;

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

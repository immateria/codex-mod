use std::env;

use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::BorderType;

use crate::{card_theme, colors};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AutoDriveVariant {
    Sentinel,
    Whisper,
    Beacon,
    Horizon,
    Pulse,
}

impl AutoDriveVariant {
    const ALL: [Self; 5] = [
        Self::Sentinel,
        Self::Whisper,
        Self::Beacon,
        Self::Horizon,
        Self::Pulse,
    ];

    pub(crate) fn default() -> Self {
        Self::Sentinel
    }

    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Sentinel => "Sentinel",
            Self::Whisper => "Whisper",
            Self::Beacon => "Beacon",
            Self::Horizon => "Horizon",
            Self::Pulse => "Pulse",
        }
    }

    pub(crate) fn index(self) -> usize {
        match self {
            Self::Sentinel => 0,
            Self::Whisper => 1,
            Self::Beacon => 2,
            Self::Horizon => 3,
            Self::Pulse => 4,
        }
    }

    pub(crate) fn from_index(index: usize) -> Self {
        let clamped = index % Self::ALL.len();
        Self::ALL[clamped]
    }

    pub(crate) fn from_env() -> Self {
        env::var("CODEX_AUTO_DRIVE_VARIANT")
            .ok()
            .and_then(|raw| raw.trim().parse::<usize>().ok()).map_or_else(Self::default, Self::from_index)
    }

    pub(crate) fn next(self) -> Self {
        let idx = self.index();
        let next = (idx + 1) % Self::ALL.len();
        Self::from_index(next)
    }

    pub(crate) fn style(self) -> AutoDriveStyle {
        match self {
            Self::Sentinel => sentinel_style(),
            Self::Whisper => whisper_style(),
            Self::Beacon => beacon_style(),
            Self::Horizon => horizon_style(),
            Self::Pulse => pulse_style(),
        }
    }
}

#[derive(Clone)]
pub(crate) struct AutoDriveStyle {
    pub(crate) frame: FrameStyle,
    pub(crate) button: ButtonStyle,
    pub(crate) composer: ComposerStyle,
}

#[derive(Clone)]
pub(crate) struct FrameStyle {
    pub(crate) title_text: &'static str,
    pub(crate) title_style: Style,
    pub(crate) border_style: Style,
}

#[derive(Clone)]
pub(crate) struct ButtonStyle {
    pub(crate) glyphs: ButtonGlyphs,
    pub(crate) enabled_style: Style,
    pub(crate) disabled_style: Style,
}

#[derive(Clone, Copy)]
pub(crate) struct ButtonGlyphs {
    pub(crate) top_left: char,
    pub(crate) top_right: char,
    pub(crate) bottom_left: char,
    pub(crate) bottom_right: char,
    pub(crate) horizontal: char,
    pub(crate) vertical: char,
}

impl ButtonGlyphs {
    pub(crate) const fn heavy() -> Self {
        Self {
            top_left: '╭',
            top_right: '╮',
            bottom_left: '╰',
            bottom_right: '╯',
            horizontal: '─',
            vertical: '│',
        }
    }

    pub(crate) const fn light() -> Self {
        Self {
            top_left: '+',
            top_right: '+',
            bottom_left: '+',
            bottom_right: '+',
            horizontal: '-',
            vertical: '|',
        }
    }

    pub(crate) const fn bold() -> Self {
        Self {
            top_left: '┏',
            top_right: '┓',
            bottom_left: '┗',
            bottom_right: '┛',
            horizontal: '━',
            vertical: '┃',
        }
    }

    pub(crate) const fn double() -> Self {
        Self {
            top_left: '╔',
            top_right: '╗',
            bottom_left: '╚',
            bottom_right: '╝',
            horizontal: '═',
            vertical: '║',
        }
    }
}

#[derive(Clone)]
pub(crate) struct ComposerStyle {
    pub(crate) border_style: Style,
    pub(crate) border_type: BorderType,
    pub(crate) background_style: Style,
    pub(crate) goal_title_prefix: &'static str,
    pub(crate) goal_title_suffix: &'static str,
    pub(crate) title_style: Style,
    pub(crate) border_gradient: Option<BorderGradient>,
}

#[derive(Clone, Copy)]
pub(crate) struct BorderGradient {
    pub(crate) left: Color,
    pub(crate) right: Color,
}

/// Pure white used for celebration/glow effects in Auto Drive animations.
#[allow(clippy::disallowed_methods, reason = "animation effect needs exact color")]
pub(crate) const EFFECT_WHITE: Color = Color::Rgb(255, 255, 255);

/// Title label shared across all Auto Drive style variants.
pub(crate) const AUTO_DRIVE_TITLE: &str = "Auto Drive";

/// Title label for Auto Drive goal status.
pub(crate) const AUTO_DRIVE_GOAL_TITLE: &str = "Auto Drive Goal";

fn auto_drive_accent_color() -> Color {
    if colors::is_dark_theme() {
        card_theme::auto_drive_dark_theme().theme.gradient.left
    } else {
        card_theme::auto_drive_light_theme().theme.gradient.left
    }
}

fn sentinel_style() -> AutoDriveStyle {
    let primary = colors::primary();
    let accent = auto_drive_accent_color();
    AutoDriveStyle {
        frame: FrameStyle {
            title_text: AUTO_DRIVE_TITLE,
            title_style: Style::default()
                .fg(colors::text())
                .add_modifier(Modifier::BOLD),
            border_style: Style::default()
                .fg(primary)
                .add_modifier(Modifier::BOLD),
        },
        button: ButtonStyle {
            glyphs: ButtonGlyphs::heavy(),
            enabled_style: Style::default()
                .fg(primary)
                .add_modifier(Modifier::BOLD),
            disabled_style: Style::default().fg(colors::text_dim()),
        },
        composer: ComposerStyle {
            border_style: Style::default().fg(accent),
            border_type: BorderType::Rounded,
            background_style: Style::default().bg(colors::background()),
            goal_title_prefix: " ▶ Goal ",
            goal_title_suffix: " ",
            title_style: Style::default()
                .fg(accent)
                .add_modifier(Modifier::BOLD),
            border_gradient: Some(auto_drive_border_gradient()),
        },
    }
}

fn whisper_style() -> AutoDriveStyle {
    let border = colors::border_dim();
    AutoDriveStyle {
        frame: FrameStyle {
            title_text: AUTO_DRIVE_TITLE,
            title_style: Style::default()
                .fg(colors::text_dim())
                .add_modifier(Modifier::ITALIC),
            border_style: Style::default().fg(border),
        },
        button: ButtonStyle {
            glyphs: ButtonGlyphs::light(),
            enabled_style: Style::default().fg(colors::text_dim()),
            disabled_style: Style::default()
                .fg(colors::text_dim())
                .add_modifier(Modifier::DIM),
        },
        composer: ComposerStyle {
            border_style: Style::default().fg(border),
            border_type: BorderType::Plain,
            background_style: Style::default().bg(colors::background()),
            goal_title_prefix: " ∙ Goal ",
            goal_title_suffix: " ∙",
            title_style: Style::default()
                .fg(colors::text_dim())
                .add_modifier(Modifier::ITALIC),
            border_gradient: Some(auto_drive_border_gradient()),
        },
    }
}

fn beacon_style() -> AutoDriveStyle {
    AutoDriveStyle {
        frame: FrameStyle {
            title_text: AUTO_DRIVE_TITLE,
            title_style: Style::default()
                .fg(colors::keyword())
                .add_modifier(Modifier::BOLD),
            border_style: Style::default().fg(colors::border()),
        },
        button: ButtonStyle {
            glyphs: ButtonGlyphs::heavy(),
            enabled_style: Style::default()
                .fg(colors::warning())
                .add_modifier(Modifier::BOLD),
            disabled_style: Style::default().fg(colors::text_dim()),
        },
        composer: ComposerStyle {
            border_style: Style::default()
                .fg(colors::primary())
                .add_modifier(Modifier::BOLD),
            border_type: BorderType::Thick,
            background_style: Style::default().bg(colors::background()),
            goal_title_prefix: " █ Goal ",
            goal_title_suffix: " ",
            title_style: Style::default()
                .fg(colors::keyword())
                .add_modifier(Modifier::BOLD),
            border_gradient: Some(auto_drive_border_gradient()),
        },
    }
}

fn horizon_style() -> AutoDriveStyle {
    let info = colors::info();
    AutoDriveStyle {
        frame: FrameStyle {
            title_text: AUTO_DRIVE_TITLE,
            title_style: Style::default()
                .fg(info)
                .add_modifier(Modifier::BOLD),
            border_style: Style::default()
                .fg(info)
                .add_modifier(Modifier::BOLD),
        },
        button: ButtonStyle {
            glyphs: ButtonGlyphs::double(),
            enabled_style: Style::default()
                .fg(info)
                .add_modifier(Modifier::BOLD),
            disabled_style: Style::default().fg(colors::text_dim()),
        },
        composer: ComposerStyle {
            border_style: Style::default()
                .fg(info)
                .add_modifier(Modifier::BOLD),
            border_type: BorderType::Double,
            background_style: Style::default().bg(colors::assistant_bg()),
            goal_title_prefix: " ═ Goal ",
            goal_title_suffix: " ═",
            title_style: Style::default()
                .fg(info)
                .add_modifier(Modifier::BOLD),
            border_gradient: Some(auto_drive_border_gradient()),
        },
    }
}

fn pulse_style() -> AutoDriveStyle {
    let success = colors::success();
    AutoDriveStyle {
        frame: FrameStyle {
            title_text: AUTO_DRIVE_TITLE,
            title_style: Style::default()
                .fg(success)
                .add_modifier(Modifier::BOLD),
            border_style: Style::default()
                .fg(success)
                .add_modifier(Modifier::BOLD),
        },
        button: ButtonStyle {
            glyphs: ButtonGlyphs::bold(),
            enabled_style: Style::default()
                .fg(success)
                .add_modifier(Modifier::BOLD),
            disabled_style: Style::default().fg(colors::text_dim()),
        },
        composer: ComposerStyle {
            border_style: Style::default()
                .fg(success)
                .add_modifier(Modifier::BOLD),
            border_type: BorderType::Rounded,
            background_style: Style::default().bg(colors::background()),
            goal_title_prefix: " ◆ Goal ",
            goal_title_suffix: " ◆",
            title_style: Style::default()
                .fg(success)
                .add_modifier(Modifier::BOLD),
            border_gradient: Some(auto_drive_border_gradient()),
        },
    }
}

#[allow(clippy::disallowed_methods)]
fn auto_drive_border_gradient() -> BorderGradient {
    if colors::is_dark_theme() {
        BorderGradient {
            left: Color::Rgb(0, 150, 255),
            right: Color::Rgb(255, 162, 0),
        }
    } else {
        BorderGradient {
            left: Color::Rgb(206, 235, 254),
            right: Color::Rgb(255, 232, 206),
        }
    }
}

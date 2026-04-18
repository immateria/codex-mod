use ratatui::style::Style;

use code_core::config_types::{Personality, Tone};

use crate::bottom_pane::settings_ui::menu_rows::SettingsMenuRow;
use crate::bottom_pane::settings_ui::rows::StyledText;
use crate::colors;

use super::{PersonalityRow, PersonalitySettingsView};

pub(crate) fn personality_label(p: Option<Personality>) -> &'static str {
    match p {
        Some(Personality::None) | None => "None",
        Some(Personality::Friendly) => "Friendly",
        Some(Personality::Pragmatic) => "Pragmatic",
        Some(Personality::Concise) => "Concise",
        Some(Personality::Enthusiastic) => "Enthusiastic",
        Some(Personality::Mentor) => "Mentor",
    }
}

pub(crate) fn tone_label(t: Option<Tone>) -> &'static str {
    match t {
        Some(Tone::Neutral) | None => "Neutral",
        Some(Tone::Formal) => "Formal",
        Some(Tone::Casual) => "Casual",
        Some(Tone::Direct) => "Direct",
        Some(Tone::Encouraging) => "Encouraging",
    }
}

fn personality_description(p: Option<Personality>) -> &'static str {
    match p {
        Some(Personality::None) | None => "No personality overlay",
        Some(Personality::Friendly) => "Warm, collaborative, empathetic teammate",
        Some(Personality::Pragmatic) => "Effective, direct, clarity-focused engineer",
        Some(Personality::Concise) => "Terse, no-nonsense, signal over noise",
        Some(Personality::Enthusiastic) => "Passionate, energetic, curiosity-driven",
        Some(Personality::Mentor) => "Patient teacher, deep explanations",
    }
}

fn tone_description(t: Option<Tone>) -> &'static str {
    match t {
        Some(Tone::Neutral) | None => "Default tone, no modifier",
        Some(Tone::Formal) => "Professional, measured language",
        Some(Tone::Casual) => "Relaxed, conversational, pair-programming style",
        Some(Tone::Direct) => "Straightforward, no hedging",
        Some(Tone::Encouraging) => "Supportive, positive, constructive",
    }
}

fn cycle_personality_forward(p: Option<Personality>) -> Option<Personality> {
    Some(match p {
        Some(Personality::None) | None => Personality::Friendly,
        Some(Personality::Friendly) => Personality::Pragmatic,
        Some(Personality::Pragmatic) => Personality::Concise,
        Some(Personality::Concise) => Personality::Enthusiastic,
        Some(Personality::Enthusiastic) => Personality::Mentor,
        Some(Personality::Mentor) => Personality::None,
    })
}

fn cycle_personality_backward(p: Option<Personality>) -> Option<Personality> {
    Some(match p {
        Some(Personality::None) | None => Personality::Mentor,
        Some(Personality::Friendly) => Personality::None,
        Some(Personality::Pragmatic) => Personality::Friendly,
        Some(Personality::Concise) => Personality::Pragmatic,
        Some(Personality::Enthusiastic) => Personality::Concise,
        Some(Personality::Mentor) => Personality::Enthusiastic,
    })
}

fn cycle_tone_forward(t: Option<Tone>) -> Option<Tone> {
    Some(match t {
        Some(Tone::Neutral) | None => Tone::Formal,
        Some(Tone::Formal) => Tone::Casual,
        Some(Tone::Casual) => Tone::Direct,
        Some(Tone::Direct) => Tone::Encouraging,
        Some(Tone::Encouraging) => Tone::Neutral,
    })
}

fn cycle_tone_backward(t: Option<Tone>) -> Option<Tone> {
    Some(match t {
        Some(Tone::Neutral) | None => Tone::Encouraging,
        Some(Tone::Formal) => Tone::Neutral,
        Some(Tone::Casual) => Tone::Formal,
        Some(Tone::Direct) => Tone::Casual,
        Some(Tone::Encouraging) => Tone::Direct,
    })
}

impl PersonalitySettingsView {
    pub(super) fn rows(&self) -> Vec<PersonalityRow> {
        vec![
            PersonalityRow::Archetype,
            PersonalityRow::TonePreference,
            PersonalityRow::TraitsInfo,
        ]
    }

    pub(super) fn menu_rows(&self) -> Vec<SettingsMenuRow<'static, PersonalityRow>> {
        let p_label = personality_label(self.personality);
        let p_desc = personality_description(self.personality);
        let t_label = tone_label(self.tone);
        let t_desc = tone_description(self.tone);
        let traits_value = if self.has_traits { "Custom (config.toml)" } else { "Not set" };

        vec![
            SettingsMenuRow::new(PersonalityRow::Archetype, "Personality")
                .with_value(StyledText::new(
                    format!("{p_label} — {p_desc}"),
                    Style::new().fg(colors::function()),
                ))
                .with_selected_hint("←→ or Enter to cycle"),
            SettingsMenuRow::new(PersonalityRow::TonePreference, "Tone")
                .with_value(StyledText::new(
                    format!("{t_label} — {t_desc}"),
                    Style::new().fg(colors::function()),
                ))
                .with_selected_hint("←→ or Enter to cycle"),
            SettingsMenuRow::new(PersonalityRow::TraitsInfo, "Traits")
                .with_value(StyledText::new(
                    traits_value.to_owned(),
                    Style::new().fg(colors::text_dim()),
                ))
                .with_selected_hint("Set via [personality_traits] in config.toml"),
        ]
    }

    pub(super) fn selected_row(&self) -> Option<PersonalityRow> {
        self.rows().get(self.state.selected_idx.unwrap_or(0)).copied()
    }

    pub(super) fn cycle_forward(&mut self) {
        match self.selected_row() {
            Some(PersonalityRow::Archetype) => {
                self.personality = cycle_personality_forward(self.personality);
            }
            Some(PersonalityRow::TonePreference) => {
                self.tone = cycle_tone_forward(self.tone);
            }
            _ => {}
        }
    }

    pub(super) fn cycle_backward(&mut self) {
        match self.selected_row() {
            Some(PersonalityRow::Archetype) => {
                self.personality = cycle_personality_backward(self.personality);
            }
            Some(PersonalityRow::TonePreference) => {
                self.tone = cycle_tone_backward(self.tone);
            }
            _ => {}
        }
    }
}

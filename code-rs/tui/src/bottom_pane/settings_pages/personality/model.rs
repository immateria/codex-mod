use ratatui::style::Style;

use code_core::config_types::{Personality, Tone};
use code_core::personality_traits::{PersonalityTraits, TRAIT_MAX, TRAIT_MIN, TRAIT_NEUTRAL};

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

fn personality_hint(p: Option<Personality>) -> &'static str {
    match p {
        Some(Personality::None) | None => "no overlay",
        Some(Personality::Friendly) => "warm, empathetic",
        Some(Personality::Pragmatic) => "direct, effective",
        Some(Personality::Concise) => "terse, no-nonsense",
        Some(Personality::Enthusiastic) => "energetic, curious",
        Some(Personality::Mentor) => "patient teacher",
    }
}

fn tone_hint(t: Option<Tone>) -> &'static str {
    match t {
        Some(Tone::Neutral) | None => "default",
        Some(Tone::Formal) => "professional",
        Some(Tone::Casual) => "conversational",
        Some(Tone::Direct) => "no hedging",
        Some(Tone::Encouraging) => "supportive",
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

/// Render a trait value as a compact visual bar: `[●●●○○]`
fn trait_bar(value: u8) -> String {
    let v = value.clamp(TRAIT_MIN, TRAIT_MAX) as usize;
    let filled = "●".repeat(v);
    let empty = "○".repeat((TRAIT_MAX as usize).saturating_sub(v));
    format!("[{filled}{empty}]")
}

/// Short label for the low and high extremes shown as hint.
fn trait_pole_hint(row: PersonalityRow) -> &'static str {
    match row {
        PersonalityRow::TraitConciseness  => "detailed ← → terse",
        PersonalityRow::TraitThoroughness => "trust & ship ← → triple-check",
        PersonalityRow::TraitAutonomy     => "always ask ← → act alone",
        PersonalityRow::TraitPedagogy     => "just answers ← → deep explain",
        PersonalityRow::TraitEnthusiasm   => "reserved ← → high energy",
        PersonalityRow::TraitFormality    => "casual ← → formal",
        PersonalityRow::TraitBoldness     => "conservative ← → bold refactor",
        _ => "",
    }
}

fn trait_display_label(row: PersonalityRow) -> &'static str {
    match row {
        PersonalityRow::TraitConciseness  => "Conciseness",
        PersonalityRow::TraitThoroughness => "Thoroughness",
        PersonalityRow::TraitAutonomy     => "Autonomy",
        PersonalityRow::TraitPedagogy     => "Pedagogy",
        PersonalityRow::TraitEnthusiasm   => "Enthusiasm",
        PersonalityRow::TraitFormality    => "Formality",
        PersonalityRow::TraitBoldness     => "Boldness",
        _ => "",
    }
}

fn get_trait_value(traits: &PersonalityTraits, row: PersonalityRow) -> u8 {
    match row {
        PersonalityRow::TraitConciseness  => traits.conciseness,
        PersonalityRow::TraitThoroughness => traits.thoroughness,
        PersonalityRow::TraitAutonomy     => traits.autonomy,
        PersonalityRow::TraitPedagogy     => traits.pedagogy,
        PersonalityRow::TraitEnthusiasm   => traits.enthusiasm,
        PersonalityRow::TraitFormality    => traits.formality,
        PersonalityRow::TraitBoldness     => traits.boldness,
        _ => TRAIT_NEUTRAL,
    }
}

fn set_trait_value(traits: &mut PersonalityTraits, row: PersonalityRow, value: u8) {
    let v = value.clamp(TRAIT_MIN, TRAIT_MAX);
    match row {
        PersonalityRow::TraitConciseness  => traits.conciseness = v,
        PersonalityRow::TraitThoroughness => traits.thoroughness = v,
        PersonalityRow::TraitAutonomy     => traits.autonomy = v,
        PersonalityRow::TraitPedagogy     => traits.pedagogy = v,
        PersonalityRow::TraitEnthusiasm   => traits.enthusiasm = v,
        PersonalityRow::TraitFormality    => traits.formality = v,
        PersonalityRow::TraitBoldness     => traits.boldness = v,
        _ => {}
    }
}

impl PersonalitySettingsView {
    pub(super) fn rows(&self) -> Vec<PersonalityRow> {
        vec![
            PersonalityRow::Archetype,
            PersonalityRow::TonePreference,
            PersonalityRow::TraitSeparator,
            PersonalityRow::TraitConciseness,
            PersonalityRow::TraitThoroughness,
            PersonalityRow::TraitAutonomy,
            PersonalityRow::TraitPedagogy,
            PersonalityRow::TraitEnthusiasm,
            PersonalityRow::TraitFormality,
            PersonalityRow::TraitBoldness,
        ]
    }

    pub(super) fn menu_rows(&self) -> Vec<SettingsMenuRow<'static, PersonalityRow>> {
        let p_label = personality_label(self.personality);
        let p_hint = personality_hint(self.personality);
        let t_label = tone_label(self.tone);
        let t_hint = tone_hint(self.tone);

        let mut rows = vec![
            SettingsMenuRow::new(PersonalityRow::Archetype, "Personality")
                .with_label_pad_cols(14)
                .with_value(StyledText::new(
                    p_label.to_owned(),
                    Style::new().fg(colors::function()),
                ))
                .with_selected_hint(p_hint),
            SettingsMenuRow::new(PersonalityRow::TonePreference, "Tone")
                .with_label_pad_cols(14)
                .with_value(StyledText::new(
                    t_label.to_owned(),
                    Style::new().fg(colors::function()),
                ))
                .with_selected_hint(t_hint),
        ];

        // Separator
        let mut sep = SettingsMenuRow::new(PersonalityRow::TraitSeparator, "── Traits ──");
        sep.enabled = false;
        rows.push(sep);

        // Trait sliders
        let trait_rows = [
            PersonalityRow::TraitConciseness,
            PersonalityRow::TraitThoroughness,
            PersonalityRow::TraitAutonomy,
            PersonalityRow::TraitPedagogy,
            PersonalityRow::TraitEnthusiasm,
            PersonalityRow::TraitFormality,
            PersonalityRow::TraitBoldness,
        ];

        for tr in trait_rows {
            let value = get_trait_value(&self.traits, tr);
            let bar = trait_bar(value);
            let style = if value == TRAIT_NEUTRAL {
                Style::new().fg(colors::text_dim())
            } else {
                Style::new().fg(colors::function())
            };
            rows.push(
                SettingsMenuRow::new(tr, trait_display_label(tr))
                    .with_label_pad_cols(14)
                    .with_value(StyledText::new(bar, style))
                    .with_selected_hint(trait_pole_hint(tr)),
            );
        }

        rows
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
            Some(row) if self.is_trait_row(row) => {
                let v = get_trait_value(&self.traits, row);
                if v < TRAIT_MAX {
                    set_trait_value(&mut self.traits, row, v + 1);
                }
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
            Some(row) if self.is_trait_row(row) => {
                let v = get_trait_value(&self.traits, row);
                if v > TRAIT_MIN {
                    set_trait_value(&mut self.traits, row, v - 1);
                }
            }
            _ => {}
        }
    }

    pub(super) fn is_trait_row(&self, row: PersonalityRow) -> bool {
        matches!(
            row,
            PersonalityRow::TraitConciseness
                | PersonalityRow::TraitThoroughness
                | PersonalityRow::TraitAutonomy
                | PersonalityRow::TraitPedagogy
                | PersonalityRow::TraitEnthusiasm
                | PersonalityRow::TraitFormality
                | PersonalityRow::TraitBoldness
        )
    }

    pub(super) fn current_traits(&self) -> PersonalityTraits {
        self.traits
    }
}

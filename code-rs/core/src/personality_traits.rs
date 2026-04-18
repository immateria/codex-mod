//! Trait-based personality system for fine-grained behavioral control.
//!
//! Instead of picking a single personality archetype, users can tune individual
//! behavioral dimensions like RPG stats. Each trait is a 1–5 scale where 3 is
//! neutral/balanced. The existing `Personality` enum archetypes map to preset
//! trait profiles.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────┐
//! │                    config.toml                          │
//! │  [personality_traits]                                   │
//! │  conciseness = 4                                        │
//! │  thoroughness = 3                                       │
//! │  autonomy = 4                                           │
//! │  pedagogy = 1                                           │
//! │  enthusiasm = 2                                         │
//! │  formality = 3                                          │
//! │  boldness = 4                                           │
//! └─────────────┬───────────────────────────────────────────┘
//!               │
//!               ▼
//! ┌─────────────────────────────────────────────────────────┐
//! │  PersonalityTraits::to_prompt_instructions()            │
//! │  → Generates per-trait instruction snippets             │
//! │  → Only emits text for non-neutral (≠3) traits         │
//! │  → Composes into a single personality block             │
//! └─────────────┬───────────────────────────────────────────┘
//!               │
//!               ▼
//! ┌─────────────────────────────────────────────────────────┐
//! │  ModelAdjustments::adjust()                             │
//! │  → Tweaks instructions based on model capabilities      │
//! │  → Reasoning models: skip explicit chain-of-thought     │
//! │  → Small models: simplify, reduce trait count           │
//! │  → High-capacity: use full nuanced instructions        │
//! └─────────────┬───────────────────────────────────────────┘
//!               │
//!               ▼
//! ┌─────────────────────────────────────────────────────────┐
//! │  {{ personality_traits }} placeholder in template       │
//! │  (injected alongside {{ personality }} and {{ tone }})  │
//! └─────────────────────────────────────────────────────────┘
//! ```
//!
//! # Trait Dimensions
//!
//! | Trait          | 1 (Low)                | 3 (Neutral)    | 5 (High)                |
//! |----------------|------------------------|----------------|-------------------------|
//! | Conciseness    | Very detailed/verbose  | Balanced       | Extremely terse         |
//! | Thoroughness   | Trust and ship fast    | Balanced       | Triple-check everything |
//! | Autonomy       | Always ask first       | Balanced       | Act independently       |
//! | Pedagogy       | Just give answers      | Balanced       | Deep explanations       |
//! | Enthusiasm     | Reserved/understated   | Balanced       | High energy/excited     |
//! | Formality      | Very casual            | Balanced       | Very formal/structured  |
//! | Boldness       | Conservative/minimal   | Balanced       | Bold refactoring        |
//!
//! # Model-Dependent Behavior
//!
//! Different model tiers handle personality instructions differently:
//!
//! - **Reasoning models** (o-series, gpt-5+, codex): Already do internal
//!   chain-of-thought, so thoroughness instructions focus on *output* checking
//!   rather than explicit "think step by step" prompts.
//! - **High-capacity models** (gpt-5.2-codex, opus): Can handle full nuanced
//!   trait instructions without degradation.
//! - **Smaller/faster models** (gpt-5-mini, haiku): Benefit from simpler,
//!   more direct trait instructions. Complex personality text wastes context
//!   and may be partially ignored.
//! - **Non-reasoning models** (older chat models): May need explicit
//!   verification prompts for high thoroughness.
//!
//! # Interaction with Archetypes
//!
//! Archetype presets (`Personality::Friendly`, etc.) can be expressed as trait
//! profiles. When both an archetype and custom traits are set, traits override
//! the archetype defaults for any non-neutral values:
//!
//! ```text
//! base = Personality::Mentor (pedagogy=5, enthusiasm=3, formality=3, ...)
//! user override: enthusiasm=5, formality=1
//! result: pedagogy=5, enthusiasm=5, formality=1, ...
//! ```

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Range for trait values. 1 = minimum, 5 = maximum, 3 = neutral/balanced.
pub const TRAIT_MIN: u8 = 1;
pub const TRAIT_MAX: u8 = 5;
pub const TRAIT_NEUTRAL: u8 = 3;

/// Fine-grained personality trait dimensions.
///
/// Each trait is a 1–5 scale where 3 is neutral (no special instruction
/// emitted). Values below 3 push toward one extreme, above 3 toward the
/// other. This gives 5^7 = 78,125 unique personality configurations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PersonalityTraits {
    /// Response length/detail. 1=very detailed, 5=extremely terse.
    #[serde(default = "default_neutral")]
    pub conciseness: u8,

    /// Self-verification intensity. 1=trust and ship, 5=triple-check.
    #[serde(default = "default_neutral")]
    pub thoroughness: u8,

    /// Independence level. 1=always ask, 5=act without asking.
    #[serde(default = "default_neutral")]
    pub autonomy: u8,

    /// Teaching depth. 1=just answers, 5=deep explanations.
    #[serde(default = "default_neutral")]
    pub pedagogy: u8,

    /// Energy level. 1=reserved/understated, 5=high energy.
    #[serde(default = "default_neutral")]
    pub enthusiasm: u8,

    /// Communication formality. 1=very casual, 5=very formal.
    #[serde(default = "default_neutral")]
    pub formality: u8,

    /// Change scope. 1=conservative/minimal, 5=bold refactoring.
    #[serde(default = "default_neutral")]
    pub boldness: u8,
}

fn default_neutral() -> u8 {
    TRAIT_NEUTRAL
}

impl Default for PersonalityTraits {
    fn default() -> Self {
        Self {
            conciseness: TRAIT_NEUTRAL,
            thoroughness: TRAIT_NEUTRAL,
            autonomy: TRAIT_NEUTRAL,
            pedagogy: TRAIT_NEUTRAL,
            enthusiasm: TRAIT_NEUTRAL,
            formality: TRAIT_NEUTRAL,
            boldness: TRAIT_NEUTRAL,
        }
    }
}

impl PersonalityTraits {
    /// Clamp all values to the valid 1–5 range.
    pub fn clamped(mut self) -> Self {
        self.conciseness = self.conciseness.clamp(TRAIT_MIN, TRAIT_MAX);
        self.thoroughness = self.thoroughness.clamp(TRAIT_MIN, TRAIT_MAX);
        self.autonomy = self.autonomy.clamp(TRAIT_MIN, TRAIT_MAX);
        self.pedagogy = self.pedagogy.clamp(TRAIT_MIN, TRAIT_MAX);
        self.enthusiasm = self.enthusiasm.clamp(TRAIT_MIN, TRAIT_MAX);
        self.formality = self.formality.clamp(TRAIT_MIN, TRAIT_MAX);
        self.boldness = self.boldness.clamp(TRAIT_MIN, TRAIT_MAX);
        self
    }

    /// True if all traits are at the neutral default (no instructions needed).
    pub fn is_neutral(&self) -> bool {
        *self == Self::default()
    }

    /// Merge another set of traits on top of this one: any non-neutral value
    /// in `overlay` replaces the corresponding value in `self`.
    pub fn merge_overlay(&self, overlay: &PersonalityTraits) -> Self {
        Self {
            conciseness: if overlay.conciseness != TRAIT_NEUTRAL { overlay.conciseness } else { self.conciseness },
            thoroughness: if overlay.thoroughness != TRAIT_NEUTRAL { overlay.thoroughness } else { self.thoroughness },
            autonomy: if overlay.autonomy != TRAIT_NEUTRAL { overlay.autonomy } else { self.autonomy },
            pedagogy: if overlay.pedagogy != TRAIT_NEUTRAL { overlay.pedagogy } else { self.pedagogy },
            enthusiasm: if overlay.enthusiasm != TRAIT_NEUTRAL { overlay.enthusiasm } else { self.enthusiasm },
            formality: if overlay.formality != TRAIT_NEUTRAL { overlay.formality } else { self.formality },
            boldness: if overlay.boldness != TRAIT_NEUTRAL { overlay.boldness } else { self.boldness },
        }
    }

    // ── Archetype presets ───────────────────────────────────────────────

    pub fn friendly() -> Self {
        Self {
            conciseness: 2,
            thoroughness: 3,
            autonomy: 3,
            pedagogy: 3,
            enthusiasm: 4,
            formality: 2,
            boldness: 3,
        }
    }

    pub fn pragmatic() -> Self {
        Self {
            conciseness: 4,
            thoroughness: 4,
            autonomy: 4,
            pedagogy: 2,
            enthusiasm: 2,
            formality: 3,
            boldness: 4,
        }
    }

    pub fn concise() -> Self {
        Self {
            conciseness: 5,
            thoroughness: 3,
            autonomy: 4,
            pedagogy: 1,
            enthusiasm: 1,
            formality: 3,
            boldness: 3,
        }
    }

    pub fn enthusiastic() -> Self {
        Self {
            conciseness: 2,
            thoroughness: 3,
            autonomy: 3,
            pedagogy: 3,
            enthusiasm: 5,
            formality: 2,
            boldness: 4,
        }
    }

    pub fn mentor() -> Self {
        Self {
            conciseness: 2,
            thoroughness: 4,
            autonomy: 2,
            pedagogy: 5,
            enthusiasm: 3,
            formality: 3,
            boldness: 2,
        }
    }

    // ── Prompt generation ───────────────────────────────────────────────

    /// Generate instruction text from trait levels. Only emits instructions
    /// for non-neutral traits (≠3). Returns `None` if all traits are neutral.
    pub fn to_prompt_instructions(&self) -> Option<String> {
        let clamped = self.clamped();
        if clamped.is_neutral() {
            return None;
        }

        let mut parts = Vec::new();

        if let Some(text) = conciseness_instruction(clamped.conciseness) {
            parts.push(text);
        }
        if let Some(text) = thoroughness_instruction(clamped.thoroughness) {
            parts.push(text);
        }
        if let Some(text) = autonomy_instruction(clamped.autonomy) {
            parts.push(text);
        }
        if let Some(text) = pedagogy_instruction(clamped.pedagogy) {
            parts.push(text);
        }
        if let Some(text) = enthusiasm_instruction(clamped.enthusiasm) {
            parts.push(text);
        }
        if let Some(text) = formality_instruction(clamped.formality) {
            parts.push(text);
        }
        if let Some(text) = boldness_instruction(clamped.boldness) {
            parts.push(text);
        }

        if parts.is_empty() {
            None
        } else {
            Some(format!(
                "## Behavioral Traits\n{}",
                parts.join("\n")
            ))
        }
    }
}

// ── Per-trait instruction generators ────────────────────────────────────────
//
// Each returns None for neutral (3). Instructions are concise single-line
// directives that compose well together.

fn conciseness_instruction(level: u8) -> Option<&'static str> {
    match level {
        1 => Some("- Be very detailed and thorough in your explanations. Include context, alternatives, and reasoning."),
        2 => Some("- Lean toward detailed responses. Include helpful context and explain your reasoning."),
        3 => None,
        4 => Some("- Keep responses concise. Skip unnecessary context and get to the point quickly."),
        5 => Some("- Be extremely terse. Use the fewest words possible. Prefer one-liners, code, and bullet points over prose."),
        _ => None,
    }
}

fn thoroughness_instruction(level: u8) -> Option<&'static str> {
    match level {
        1 => Some("- Optimize for speed over certainty. Trust your first instinct and ship quickly."),
        2 => Some("- Favor moving fast. Do a quick sanity check but don't over-verify."),
        3 => None,
        4 => Some("- Verify your work carefully. Re-read changes, check for edge cases, and confirm correctness before presenting."),
        5 => Some("- Triple-check everything. Verify all assumptions, test edge cases, re-read your output, and explicitly confirm correctness."),
        _ => None,
    }
}

fn autonomy_instruction(level: u8) -> Option<&'static str> {
    match level {
        1 => Some("- Always ask before taking action. Present options and wait for the user to choose."),
        2 => Some("- Ask before significant actions. Make small obvious decisions independently."),
        3 => None,
        4 => Some("- Act independently for routine decisions. Only ask for genuinely ambiguous or high-impact choices."),
        5 => Some("- Act fully autonomously. Make all reasonable decisions yourself and only stop for truly critical ambiguities."),
        _ => None,
    }
}

fn pedagogy_instruction(level: u8) -> Option<&'static str> {
    match level {
        1 => Some("- Just provide answers and code. Skip explanations unless explicitly asked."),
        2 => Some("- Keep explanations minimal. A brief 'why' is fine but don't lecture."),
        3 => None,
        4 => Some("- Explain your reasoning and the principles behind decisions. Help the user learn, not just get answers."),
        5 => Some("- Teach deeply. Explain concepts, trade-offs, and mental models. Connect specific tasks to broader principles. Ask questions to check understanding."),
        _ => None,
    }
}

fn enthusiasm_instruction(level: u8) -> Option<&'static str> {
    match level {
        1 => Some("- Be reserved and understated. No exclamation marks, celebrations, or emotional language."),
        2 => Some("- Keep energy low-key. Acknowledge good work quietly without fanfare."),
        3 => None,
        4 => Some("- Show genuine enthusiasm for clever solutions and interesting problems. Let positive energy come through."),
        5 => Some("- Be highly energetic and expressive. Celebrate discoveries, get excited about elegant code, and bring infectious enthusiasm."),
        _ => None,
    }
}

fn formality_instruction(level: u8) -> Option<&'static str> {
    match level {
        1 => Some("- Be very casual and conversational. Contractions, fragments, and informal phrasing are all fine."),
        2 => Some("- Keep it relaxed and natural. Write like you're talking to a colleague."),
        3 => None,
        4 => Some("- Use professional language. Complete sentences, proper terminology, and structured formatting."),
        5 => Some("- Be highly formal and structured. Use precise technical language, proper grammar, and organized formatting throughout."),
        _ => None,
    }
}

fn boldness_instruction(level: u8) -> Option<&'static str> {
    match level {
        1 => Some("- Be very conservative with changes. Prefer minimal, surgical edits. Never refactor beyond what's asked."),
        2 => Some("- Lean toward minimal changes. Only expand scope if directly relevant."),
        3 => None,
        4 => Some("- Don't be afraid to refactor or improve adjacent code when it makes the overall change better."),
        5 => Some("- Be bold. Refactor aggressively when it improves the codebase. Suggest architectural improvements proactively."),
        _ => None,
    }
}

// ── Model-aware adjustments ─────────────────────────────────────────────────

/// Describes a model's capability tier for personality instruction purposes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelCapabilityTier {
    /// Reasoning models (o-series, gpt-5+, codex): internal chain-of-thought,
    /// high instruction following, large context.
    Reasoning,
    /// Standard high-capacity chat models: good instruction following,
    /// moderate context.
    Standard,
    /// Small/fast models (mini, haiku): limited instruction following,
    /// personality text should be kept simple.
    Compact,
}

/// Adjustments to trait instructions based on model capabilities.
///
/// This is the future integration point where the trait system meets model
/// intelligence. The idea:
///
/// - **Reasoning models**: Thoroughness instructions shift from "think step
///   by step" to "verify your output" since the model already reasons
///   internally. Pedagogy can be richer since the model can maintain
///   coherent explanations.
///
/// - **Compact models**: Reduce the number of active traits to avoid
///   overwhelming the model. Prioritize the traits with the highest
///   deviation from neutral. Simplify instruction language.
///
/// - **Standard models**: Full trait instructions work well. May benefit
///   from explicit examples for extreme trait values.
pub struct ModelAdjustments;

impl ModelAdjustments {
    /// Given a model capability tier and traits, return potentially modified
    /// trait instructions. For now this is a passthrough — the infrastructure
    /// is here for future refinement.
    pub fn adjust_instructions(
        tier: ModelCapabilityTier,
        traits: &PersonalityTraits,
    ) -> Option<String> {
        let base = traits.to_prompt_instructions()?;

        match tier {
            ModelCapabilityTier::Reasoning => {
                // Reasoning models already think internally, so we could
                // rephrase thoroughness from "think carefully" to "verify
                // your output". For now, pass through.
                Some(base)
            }
            ModelCapabilityTier::Standard => {
                // Full instructions work well for standard models.
                Some(base)
            }
            ModelCapabilityTier::Compact => {
                // For compact models, we could reduce to only the top 3
                // most-deviated traits. For now, pass through.
                Some(base)
            }
        }
    }

    /// Infer capability tier from model slug.
    pub fn tier_from_model_slug(model: &str) -> ModelCapabilityTier {
        // Reasoning models
        if model.starts_with("o1")
            || model.starts_with("o3")
            || model.starts_with("o4")
            || model.contains("codex")
            || model.starts_with("gpt-5")
            || model.starts_with("bengalfox")
            || model.starts_with("claude-opus")
            || model.starts_with("claude-sonnet")
        {
            return ModelCapabilityTier::Reasoning;
        }

        // Compact models
        if model.contains("mini")
            || model.contains("haiku")
            || model.contains("flash")
            || model.starts_with("gpt-4o-mini")
            || model.starts_with("gpt-4.1-mini")
            || model.starts_with("gpt-4.1-nano")
        {
            return ModelCapabilityTier::Compact;
        }

        // Everything else is standard
        ModelCapabilityTier::Standard
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_all_neutral() {
        let traits = PersonalityTraits::default();
        assert!(traits.is_neutral());
        assert_eq!(traits.to_prompt_instructions(), None);
    }

    #[test]
    fn archetype_presets_are_not_neutral() {
        assert!(!PersonalityTraits::friendly().is_neutral());
        assert!(!PersonalityTraits::pragmatic().is_neutral());
        assert!(!PersonalityTraits::concise().is_neutral());
        assert!(!PersonalityTraits::enthusiastic().is_neutral());
        assert!(!PersonalityTraits::mentor().is_neutral());
    }

    #[test]
    fn prompt_instructions_only_include_non_neutral() {
        let traits = PersonalityTraits {
            conciseness: 5,
            ..Default::default()
        };
        let instructions = traits.to_prompt_instructions().unwrap();
        assert!(instructions.contains("terse"));
        assert!(!instructions.contains("formal"));
        assert!(!instructions.contains("conservative"));
    }

    #[test]
    fn merge_overlay_replaces_non_neutral() {
        let base = PersonalityTraits::mentor();
        let overlay = PersonalityTraits {
            enthusiasm: 5,
            formality: 1,
            ..Default::default()
        };
        let merged = base.merge_overlay(&overlay);
        assert_eq!(merged.pedagogy, 5); // from mentor
        assert_eq!(merged.enthusiasm, 5); // from overlay
        assert_eq!(merged.formality, 1); // from overlay
    }

    #[test]
    fn clamp_keeps_values_in_range() {
        let traits = PersonalityTraits {
            conciseness: 0,
            thoroughness: 10,
            ..Default::default()
        };
        let clamped = traits.clamped();
        assert_eq!(clamped.conciseness, 1);
        assert_eq!(clamped.thoroughness, 5);
    }

    #[test]
    fn model_tier_classification() {
        assert_eq!(
            ModelAdjustments::tier_from_model_slug("gpt-5.2-codex"),
            ModelCapabilityTier::Reasoning
        );
        assert_eq!(
            ModelAdjustments::tier_from_model_slug("gpt-4o-mini"),
            ModelCapabilityTier::Compact
        );
        assert_eq!(
            ModelAdjustments::tier_from_model_slug("gpt-4o"),
            ModelCapabilityTier::Standard
        );
        assert_eq!(
            ModelAdjustments::tier_from_model_slug("claude-haiku-4.5"),
            ModelCapabilityTier::Compact
        );
    }

    #[test]
    fn full_trait_profile_generates_all_instructions() {
        let traits = PersonalityTraits {
            conciseness: 5,
            thoroughness: 5,
            autonomy: 5,
            pedagogy: 5,
            enthusiasm: 5,
            formality: 5,
            boldness: 5,
        };
        let text = traits.to_prompt_instructions().unwrap();
        assert!(text.contains("terse"));
        assert!(text.contains("Triple-check"));
        assert!(text.contains("autonomously"));
        assert!(text.contains("Teach deeply"));
        assert!(text.contains("energetic"));
        assert!(text.contains("formal"));
        assert!(text.contains("bold"));
    }

    #[test]
    fn roundtrip_serde() {
        let traits = PersonalityTraits::pragmatic();
        let json = serde_json::to_string(&traits).unwrap();
        let deserialized: PersonalityTraits = serde_json::from_str(&json).unwrap();
        assert_eq!(traits, deserialized);
    }

    #[test]
    fn toml_deserialization() {
        let toml_str = r#"
conciseness = 5
thoroughness = 2
"#;
        let traits: PersonalityTraits = toml::from_str(toml_str).unwrap();
        assert_eq!(traits.conciseness, 5);
        assert_eq!(traits.thoroughness, 2);
        // Unspecified fields default to neutral
        assert_eq!(traits.autonomy, TRAIT_NEUTRAL);
        assert_eq!(traits.pedagogy, TRAIT_NEUTRAL);
    }
}

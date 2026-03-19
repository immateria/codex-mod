use std::cmp::Ordering;

use code_common::model_picker_order::picker_rank_for_model;
use code_common::model_presets::ModelPreset;
use code_core::config_types::ReasoningEffort;

/// Flattened preset entry combining a model with a specific reasoning effort.
#[derive(Clone, Debug)]
pub(crate) struct FlatPreset {
    pub(crate) model: String,
    pub(crate) display_name: String,
    pub(crate) effort: ReasoningEffort,
    pub(crate) label: String,
    pub(crate) description: String,
    pub(crate) model_description: String,
    pub(crate) picker_rank: u16,
}

impl FlatPreset {
    pub(crate) fn from_model_preset(preset: &ModelPreset) -> Vec<Self> {
        preset
            .supported_reasoning_efforts
            .iter()
            .map(|effort_preset| {
                let effort_label = reasoning_effort_label(effort_preset.effort.into());
                FlatPreset {
                    model: preset.model.to_string(),
                    display_name: preset.display_name.to_string(),
                    effort: effort_preset.effort.into(),
                    label: format!("{} {}", preset.display_name, effort_label.to_lowercase()),
                    description: effort_preset.description.to_string(),
                    model_description: preset.description.to_string(),
                    picker_rank: picker_rank_for_model(&preset.model),
                }
            })
            .collect()
    }
}

pub(crate) fn reasoning_effort_label(effort: ReasoningEffort) -> &'static str {
    match effort {
        ReasoningEffort::XHigh => "XHigh",
        ReasoningEffort::High => "High",
        ReasoningEffort::Medium => "Medium",
        ReasoningEffort::Low => "Low",
        ReasoningEffort::Minimal => "Minimal",
        ReasoningEffort::None => "None",
    }
}

pub(crate) fn compare_presets(a: &FlatPreset, b: &FlatPreset) -> Ordering {
    a.picker_rank
        .cmp(&b.picker_rank)
        .then_with(|| a.display_name.cmp(&b.display_name))
        .then_with(|| a.model.cmp(&b.model))
        .then_with(|| effort_rank(a.effort).cmp(&effort_rank(b.effort)))
        .then_with(|| a.label.cmp(&b.label))
}

fn effort_rank(effort: ReasoningEffort) -> u8 {
    match effort {
        ReasoningEffort::XHigh => 0,
        ReasoningEffort::High => 1,
        ReasoningEffort::Medium => 2,
        ReasoningEffort::Low => 3,
        ReasoningEffort::Minimal => 4,
        ReasoningEffort::None => 5,
    }
}


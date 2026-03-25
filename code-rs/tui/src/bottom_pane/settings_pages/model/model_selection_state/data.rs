use std::sync::OnceLock;

use code_common::model_presets::ModelPreset;
use code_core::config_types::{ContextMode, ReasoningEffort, ServiceTier};
use code_core::model_family::{
    STANDARD_CONTEXT_WINDOW_272K, default_auto_compact_limit_for_context_window,
    derive_default_model_family, resolve_context_settings, supports_extended_context,
};
use code_protocol::num_format::format_with_separators_u64;

use super::presets::{compare_presets, FlatPreset};
use super::target::ModelSelectionTarget;

const SUMMARY_HEADER_LINES: u16 = 3;
const FAST_MODE_SECTION_HEIGHT: u16 = 5;
const CONTEXT_MODE_SECTION_HEIGHT: u16 = 7;
const CONTEXT_MODE_UNAVAILABLE_NOTICE_HEIGHT: u16 = 1;
const FOLLOW_CHAT_SECTION_HEIGHT: u16 = 4;
const FOOTER_HEIGHT: u16 = 2;
const FAST_MODE_ROW_OFFSET: usize = 2;
const CONTEXT_MODE_ROW_OFFSET: usize = 3;
const CONTEXT_WINDOW_ROW_OFFSET: usize = 4;
const AUTO_COMPACT_ROW_OFFSET: usize = 5;
const FOLLOW_CHAT_ROW_OFFSET: usize = 2;

pub(crate) struct ModelSelectionViewParams {
    pub(crate) presets: Vec<ModelPreset>,
    pub(crate) current_model: String,
    pub(crate) current_effort: ReasoningEffort,
    pub(crate) current_service_tier: Option<ServiceTier>,
    pub(crate) current_context_mode: Option<ContextMode>,
    pub(crate) current_context_window: Option<u64>,
    pub(crate) current_auto_compact_token_limit: Option<i64>,
    pub(crate) use_chat_model: bool,
    pub(crate) target: ModelSelectionTarget,
}

#[derive(Clone, Debug)]
pub(crate) struct CurrentSelection {
    pub(crate) current_model: String,
    pub(crate) current_effort: ReasoningEffort,
    pub(crate) current_service_tier: Option<ServiceTier>,
    pub(crate) current_context_mode: Option<ContextMode>,
    pub(crate) current_context_window: Option<u64>,
    pub(crate) current_auto_compact_token_limit: Option<i64>,
    pub(crate) use_chat_model: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct ModelSelectionData {
    pub(crate) flat_presets: Vec<FlatPreset>,
    sorted_preset_indices: Vec<usize>,
    pub(crate) current: CurrentSelection,
    pub(crate) target: ModelSelectionTarget,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum EntryKind {
    FastMode,
    ContextMode,
    ContextWindow,
    AutoCompact,
    FollowChat,
    Preset(usize),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SelectionAction {
    ToggleFastMode(Option<ServiceTier>),
    SetContextMode(Option<ContextMode>),
    UseChatModel,
    SetPreset {
        model: String,
        effort: ReasoningEffort,
    },
}

impl SelectionAction {
    pub(crate) fn closes_view(&self) -> bool {
        matches!(self, SelectionAction::UseChatModel | SelectionAction::SetPreset { .. })
    }
}

impl ModelSelectionData {
    pub(crate) fn supports_fast_mode(&self) -> bool {
        self.target.supports_fast_mode(&self.current.current_model)
    }

    fn build_sorted_preset_indices(flat_presets: &[FlatPreset]) -> Vec<usize> {
        let mut indices: Vec<usize> = (0..flat_presets.len()).collect();
        indices.sort_by(|&a, &b| compare_presets(&flat_presets[a], &flat_presets[b]));
        indices
    }

    pub(crate) fn context_mode_intro_lines() -> &'static [String; 2] {
        static CONTEXT_MODE_INTRO_LINES: OnceLock<[String; 2]> = OnceLock::new();
        CONTEXT_MODE_INTRO_LINES.get_or_init(|| {
            let threshold = format_with_separators_u64(STANDARD_CONTEXT_WINDOW_272K);
            [
                "Fast mode speeds up replies. 1M Context is available on supported models."
                    .to_string(),
                format!(
                    "Auto uses 1M limits and pre-turn compaction checks. Past {threshold} input tokens, the session is billed at 2x input and 1.5x output."
                ),
            ]
        })
    }

    pub(crate) fn new(params: ModelSelectionViewParams) -> Self {
        let ModelSelectionViewParams {
            presets,
            current_model,
            current_effort,
            current_service_tier,
            current_context_mode,
            current_context_window,
            current_auto_compact_token_limit,
            use_chat_model,
            target,
        } = params;
        let flat_presets: Vec<FlatPreset> =
            presets.iter().flat_map(FlatPreset::from_model_preset).collect();
        let sorted_preset_indices = Self::build_sorted_preset_indices(&flat_presets);

        Self {
            flat_presets,
            sorted_preset_indices,
            current: CurrentSelection {
                current_model,
                current_effort,
                current_service_tier,
                current_context_mode,
                current_context_window,
                current_auto_compact_token_limit,
                use_chat_model,
            },
            target,
        }
    }

    pub(crate) fn initial_selection(&self) -> usize {
        Self::initial_selection_for(
            self.supports_fast_mode(),
            self.target.supports_context_mode(),
            self.target.supports_follow_chat(),
            self.current.use_chat_model,
            &self.flat_presets,
            &self.current.current_model,
            self.current.current_effort,
        )
    }

    pub(crate) fn update_presets(
        &mut self,
        presets: Vec<ModelPreset>,
        selected_index: usize,
    ) -> usize {
        let include_fast_mode = self.supports_fast_mode();
        let include_context_mode = self.target.supports_context_mode();
        let context_entry_count = if include_context_mode { 3 } else { 0 };
        let include_follow_chat = self.target.supports_follow_chat();
        let previous_selected = self.entry_at(selected_index);
        let previous_preset = match previous_selected {
            Some(EntryKind::Preset(idx)) => self
                .flat_presets
                .get(idx)
                .map(|preset| (preset.model.clone(), preset.effort)),
            _ => None,
        };

        self.flat_presets = presets
            .iter()
            .flat_map(FlatPreset::from_model_preset)
            .collect();
        self.sorted_preset_indices = Self::build_sorted_preset_indices(&self.flat_presets);

        let mut next_selected: Option<usize> = None;
        match previous_selected {
            Some(EntryKind::FastMode) => {
                if include_fast_mode {
                    next_selected = Some(0);
                }
            }
            Some(EntryKind::ContextMode) => {
                if include_context_mode {
                    next_selected = Some(usize::from(include_fast_mode));
                }
            }
            Some(EntryKind::ContextWindow) => {
                if include_context_mode {
                    next_selected = Some(usize::from(include_fast_mode) + 1);
                }
            }
            Some(EntryKind::AutoCompact) => {
                if include_context_mode {
                    next_selected = Some(usize::from(include_fast_mode) + 2);
                }
            }
            Some(EntryKind::FollowChat) => {
                if include_follow_chat {
                    next_selected = Some(usize::from(include_fast_mode) + context_entry_count);
                }
            }
            Some(EntryKind::Preset(_)) => {
                if let Some((previous_model, previous_effort)) = previous_preset
                    && let Some((new_idx, _)) =
                        self.flat_presets.iter().enumerate().find(|(_, preset)| {
                            preset.model.eq_ignore_ascii_case(&previous_model)
                                && preset.effort == previous_effort
                        })
                {
                    let prefix =
                        usize::from(include_fast_mode) + context_entry_count + usize::from(include_follow_chat);
                    next_selected = Some(new_idx + prefix);
                }
            }
            None => {}
        }

        let mut next_selected = next_selected.unwrap_or_else(|| {
            Self::initial_selection_for(
                include_fast_mode,
                include_context_mode,
                include_follow_chat,
                self.current.use_chat_model,
                &self.flat_presets,
                &self.current.current_model,
                self.current.current_effort,
            )
        });

        let total = self.entry_count();
        if total == 0 {
            next_selected = 0;
        } else if next_selected >= total {
            next_selected = total - 1;
        }

        next_selected
    }

    fn initial_selection_for(
        include_fast_mode: bool,
        include_context_mode: bool,
        include_follow_chat: bool,
        use_chat_model: bool,
        flat_presets: &[FlatPreset],
        current_model: &str,
        current_effort: ReasoningEffort,
    ) -> usize {
        let context_entry_count = if include_context_mode { 3 } else { 0 };

        if include_follow_chat && use_chat_model {
            return usize::from(include_fast_mode) + context_entry_count;
        }

        if let Some((idx, _)) = flat_presets.iter().enumerate().find(|(_, preset)| {
            preset.model.eq_ignore_ascii_case(current_model) && preset.effort == current_effort
        }) {
            return idx
                + usize::from(include_fast_mode)
                + context_entry_count
                + usize::from(include_follow_chat);
        }

        if let Some((idx, _)) = flat_presets
            .iter()
            .enumerate()
            .find(|(_, preset)| preset.model.eq_ignore_ascii_case(current_model))
        {
            return idx
                + usize::from(include_fast_mode)
                + context_entry_count
                + usize::from(include_follow_chat);
        }

        if include_follow_chat {
            if flat_presets.is_empty() {
                usize::from(include_fast_mode) + context_entry_count
            } else {
                usize::from(include_fast_mode) + context_entry_count + 1
            }
        } else if include_fast_mode {
            if flat_presets.is_empty() {
                0
            } else {
                usize::from(include_fast_mode) + context_entry_count
            }
        } else {
            0
        }
    }

    pub(crate) fn supports_extended_context(&self) -> bool {
        supports_extended_context(&self.current.current_model)
    }

    pub(crate) fn current_model_display_name(&self) -> String {
        self.flat_presets
            .iter()
            .find(|preset| preset.model.eq_ignore_ascii_case(&self.current.current_model))
            .map(|preset| preset.display_name.clone())
            .unwrap_or_else(|| self.current.current_model.clone())
    }

    pub(crate) fn entries(&self) -> Vec<EntryKind> {
        let mut entries = Vec::new();
        if self.supports_fast_mode() {
            entries.push(EntryKind::FastMode);
        }
        if self.target.supports_context_mode() {
            entries.push(EntryKind::ContextMode);
            entries.push(EntryKind::ContextWindow);
            entries.push(EntryKind::AutoCompact);
        }
        if self.target.supports_follow_chat() {
            entries.push(EntryKind::FollowChat);
        }
        for idx in self.sorted_preset_indices.iter().copied() {
            entries.push(EntryKind::Preset(idx));
        }
        entries
    }

    pub(crate) fn entry_count(&self) -> usize {
        usize::from(self.supports_fast_mode())
            + usize::from(self.target.supports_context_mode()) * 3
            + usize::from(self.target.supports_follow_chat())
            + self.flat_presets.len()
    }

    pub(crate) fn context_mode_entry_index(&self) -> Option<usize> {
        self.target
            .supports_context_mode()
            .then(|| usize::from(self.supports_fast_mode()))
    }

    pub(crate) fn context_window_entry_index(&self) -> Option<usize> {
        self.context_mode_entry_index().map(|index| index + 1)
    }

    pub(crate) fn auto_compact_entry_index(&self) -> Option<usize> {
        self.context_mode_entry_index().map(|index| index + 2)
    }

    pub(crate) fn follow_chat_entry_index(&self) -> Option<usize> {
        self.target.supports_follow_chat().then(|| {
            usize::from(self.supports_fast_mode())
                + usize::from(self.target.supports_context_mode()) * 3
        })
    }

    pub(crate) fn entry_at(&self, entry_index: usize) -> Option<EntryKind> {
        let mut next_index = 0;
        if self.supports_fast_mode() {
            if entry_index == next_index {
                return Some(EntryKind::FastMode);
            }
            next_index += 1;
        }
        if self.target.supports_context_mode() {
            if entry_index == next_index {
                return Some(EntryKind::ContextMode);
            }
            next_index += 1;
            if entry_index == next_index {
                return Some(EntryKind::ContextWindow);
            }
            next_index += 1;
            if entry_index == next_index {
                return Some(EntryKind::AutoCompact);
            }
            next_index += 1;
        }
        if self.target.supports_follow_chat() {
            if entry_index == next_index {
                return Some(EntryKind::FollowChat);
            }
            next_index += 1;
        }

        let preset_index = entry_index.checked_sub(next_index)?;
        let flat_index = *self.sorted_preset_indices.get(preset_index)?;
        Some(EntryKind::Preset(flat_index))
    }

    pub(crate) fn content_line_count(&self) -> u16 {
        let mut lines = SUMMARY_HEADER_LINES;
        if self.supports_fast_mode() {
            lines = lines.saturating_add(FAST_MODE_SECTION_HEIGHT);
        }
        if self.target.supports_context_mode() {
            lines = lines.saturating_add(CONTEXT_MODE_SECTION_HEIGHT);
            if !self.supports_extended_context() {
                lines = lines.saturating_add(CONTEXT_MODE_UNAVAILABLE_NOTICE_HEIGHT);
            }
        }
        if self.target.supports_follow_chat() {
            lines = lines.saturating_add(FOLLOW_CHAT_SECTION_HEIGHT);
        }

        let mut previous_model: Option<&str> = None;
        for idx in self.sorted_preset_indices.iter().copied() {
            let flat_preset = &self.flat_presets[idx];
            let is_new_model = previous_model
                .map(|prev| !prev.eq_ignore_ascii_case(&flat_preset.model))
                .unwrap_or(true);

            if is_new_model {
                if previous_model.is_some() {
                    lines = lines.saturating_add(1);
                }
                lines = lines.saturating_add(1);
                if !flat_preset.model_description.trim().is_empty() {
                    lines = lines.saturating_add(1);
                }
                previous_model = Some(&flat_preset.model);
            }

            lines = lines.saturating_add(1);
        }

        lines.saturating_add(FOOTER_HEIGHT)
    }

    pub(crate) fn entry_line(&self, entry_index: usize) -> usize {
        debug_assert!(entry_index < self.entry_count());
        let mut line = usize::from(SUMMARY_HEADER_LINES);

        if self.supports_fast_mode() {
            if entry_index == 0 {
                return line + FAST_MODE_ROW_OFFSET;
            }
            line += usize::from(FAST_MODE_SECTION_HEIGHT);
        }

        if let Some(context_entry_index) = self.context_mode_entry_index() {
            if entry_index == context_entry_index {
                return line + CONTEXT_MODE_ROW_OFFSET;
            }
            if self.context_window_entry_index() == Some(entry_index) {
                return line + CONTEXT_WINDOW_ROW_OFFSET;
            }
            if self.auto_compact_entry_index() == Some(entry_index) {
                return line + AUTO_COMPACT_ROW_OFFSET;
            }
            line += usize::from(CONTEXT_MODE_SECTION_HEIGHT);
            if !self.supports_extended_context() {
                line += usize::from(CONTEXT_MODE_UNAVAILABLE_NOTICE_HEIGHT);
            }
        }

        if self.target.supports_follow_chat() {
            if self.follow_chat_entry_index() == Some(entry_index) {
                return line + FOLLOW_CHAT_ROW_OFFSET;
            }
            line += usize::from(FOLLOW_CHAT_SECTION_HEIGHT);
        }

        let preset_prefix = self.entry_count() - self.sorted_preset_indices.len();
        let mut previous_model: Option<&str> = None;
        for (preset_pos, preset_index) in self.sorted_preset_indices.iter().copied().enumerate() {
            let flat_preset = &self.flat_presets[preset_index];
            let is_new_model = previous_model
                .map(|model| !model.eq_ignore_ascii_case(&flat_preset.model))
                .unwrap_or(true);

            if is_new_model {
                if previous_model.is_some() {
                    line += 1;
                }
                line += 1;
                if !flat_preset.model_description.trim().is_empty() {
                    line += 1;
                }
                previous_model = Some(&flat_preset.model);
            }

            if preset_prefix + preset_pos == entry_index {
                return line;
            }
            line += 1;
        }

        line
    }

    pub(crate) fn apply_selection(&mut self, entry: EntryKind) -> Option<SelectionAction> {
        match entry {
            EntryKind::FastMode => {
                let next_service_tier =
                    if matches!(self.current.current_service_tier, Some(ServiceTier::Fast)) {
                        None
                    } else {
                        Some(ServiceTier::Fast)
                    };
                self.current.current_service_tier = next_service_tier;
                Some(SelectionAction::ToggleFastMode(next_service_tier))
            }
            EntryKind::ContextMode => {
                let next_context_mode = match self.current.current_context_mode {
                    None | Some(ContextMode::Disabled) => Some(ContextMode::OneM),
                    Some(ContextMode::OneM) => Some(ContextMode::Auto),
                    Some(ContextMode::Auto) => Some(ContextMode::Disabled),
                };
                let family = derive_default_model_family(&self.current.current_model);
                let (next_context_window, next_auto_compact_token_limit) =
                    resolve_context_settings(
                        &self.current.current_model,
                        next_context_mode,
                        None,
                        None,
                        &family,
                    );
                self.current.current_context_mode = next_context_mode;
                self.current.current_context_window = next_context_window;
                self.current.current_auto_compact_token_limit = next_auto_compact_token_limit;
                Some(SelectionAction::SetContextMode(next_context_mode))
            }
            EntryKind::ContextWindow | EntryKind::AutoCompact => None,
            EntryKind::FollowChat => {
                self.current.use_chat_model = true;
                Some(SelectionAction::UseChatModel)
            }
            EntryKind::Preset(idx) => {
                let flat_preset = self.flat_presets.get(idx)?.clone();
                self.current.current_model = flat_preset.model.clone();
                self.current.current_effort = flat_preset.effort;
                self.current.use_chat_model = false;
                Some(SelectionAction::SetPreset {
                    model: flat_preset.model,
                    effort: flat_preset.effort,
                })
            }
        }
    }

    pub(crate) fn context_window_is_default(&self) -> bool {
        let family = derive_default_model_family(&self.current.current_model);
        let (default_context_window, _) = resolve_context_settings(
            &self.current.current_model,
            self.current.current_context_mode,
            None,
            None,
            &family,
        );
        self.current.current_context_window == default_context_window
    }

    pub(crate) fn auto_compact_is_default(&self) -> bool {
        match (
            self.current.current_context_window,
            self.current.current_auto_compact_token_limit,
        ) {
            (Some(window), Some(limit)) => {
                limit == default_auto_compact_limit_for_context_window(window)
            }
            _ => true,
        }
    }
}

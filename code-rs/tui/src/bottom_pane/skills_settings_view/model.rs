use super::*;

pub(super) const BUTTON_GAP_WIDTH: u16 =
    crate::bottom_pane::settings_ui::layout::DEFAULT_BUTTON_GAP.len() as u16;
pub(super) const SKILLS_SETTINGS_VIEW_HEIGHT: u16 = 28;
pub(super) const GENERATE_BUTTON_LABEL: &str = "Generate draft";
pub(super) const SAVE_BUTTON_LABEL: &str = "Save";
pub(super) const DELETE_BUTTON_LABEL: &str = "Delete";
pub(super) const CANCEL_BUTTON_LABEL: &str = "Cancel";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Focus {
    List,
    Name,
    Description,
    Style,
    StyleProfile,
    StyleReferences,
    StyleSkillRoots,
    StyleMcpInclude,
    StyleMcpExclude,
    Examples,
    Body,
    Generate,
    Save,
    Delete,
    Cancel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Mode {
    List,
    Edit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ActionButton {
    Generate,
    Save,
    Delete,
    Cancel,
}

impl ActionButton {
    pub(super) fn focus(self) -> Focus {
        match self {
            Self::Generate => Focus::Generate,
            Self::Save => Focus::Save,
            Self::Delete => Focus::Delete,
            Self::Cancel => Focus::Cancel,
        }
    }
}

#[derive(Clone)]
pub(super) struct SkillsFormLayout {
    // `*_top` / `*_h` values are virtual scroll-content offsets, not screen
    // coordinates. They are used to keep the focused field visible within the
    // clipped viewport.
    pub(super) viewport_inner: Rect,
    pub(super) scroll_top: usize,
    pub(super) max_scroll: usize,
    pub(super) status_row: Rect,
    pub(super) name_field: Rect,
    pub(super) name_top: usize,
    pub(super) name_h: usize,
    pub(super) description_field: Rect,
    pub(super) description_top: usize,
    pub(super) description_h: usize,
    pub(super) style_field: Rect,
    pub(super) style_top: usize,
    pub(super) style_h: usize,
    pub(super) style_profile_row: Rect,
    pub(super) style_profile_top: usize,
    pub(super) style_profile_h: usize,
    pub(super) style_references_outer: Rect,
    pub(super) style_references_inner: Rect,
    pub(super) style_references_top: usize,
    pub(super) style_references_h: usize,
    pub(super) style_skill_roots_outer: Rect,
    pub(super) style_skill_roots_inner: Rect,
    pub(super) style_skill_roots_top: usize,
    pub(super) style_skill_roots_h: usize,
    pub(super) style_mcp_include_outer: Rect,
    pub(super) style_mcp_include_inner: Rect,
    pub(super) style_mcp_include_top: usize,
    pub(super) style_mcp_include_h: usize,
    pub(super) style_mcp_exclude_outer: Rect,
    pub(super) style_mcp_exclude_inner: Rect,
    pub(super) style_mcp_exclude_top: usize,
    pub(super) style_mcp_exclude_h: usize,
    pub(super) examples_outer: Rect,
    pub(super) examples_inner: Rect,
    pub(super) examples_top: usize,
    pub(super) examples_h: usize,
    pub(super) body_outer: Rect,
    pub(super) body_inner: Rect,
    pub(super) body_top: usize,
    pub(super) body_h: usize,
    pub(super) buttons_row: Rect,
}

impl SkillsFormLayout {
    pub(super) fn focus_bounds(&self, focus: Focus) -> Option<(usize, usize)> {
        match focus {
            Focus::Name => Some((self.name_top, self.name_h)),
            Focus::Description => Some((self.description_top, self.description_h)),
            Focus::Style => Some((self.style_top, self.style_h)),
            Focus::StyleProfile => Some((self.style_profile_top, self.style_profile_h)),
            Focus::StyleReferences => Some((self.style_references_top, self.style_references_h)),
            Focus::StyleSkillRoots => Some((self.style_skill_roots_top, self.style_skill_roots_h)),
            Focus::StyleMcpInclude => Some((self.style_mcp_include_top, self.style_mcp_include_h)),
            Focus::StyleMcpExclude => Some((self.style_mcp_exclude_top, self.style_mcp_exclude_h)),
            Focus::Examples => Some((self.examples_top, self.examples_h)),
            Focus::Body => Some((self.body_top, self.body_h)),
            Focus::Generate | Focus::Save | Focus::Delete | Focus::Cancel => None,
            Focus::List => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum StyleProfileMode {
    Inherit,
    Enable,
    Disable,
}

impl StyleProfileMode {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Inherit => "inherit",
            Self::Enable => "enable for style",
            Self::Disable => "disable for style",
        }
    }

    pub(super) fn hint(self) -> &'static str {
        match self {
            Self::Inherit => "Skill follows style defaults (not pinned in this style profile).",
            Self::Enable => "Add skill to shell_style_profiles.<style>.skills allow-list.",
            Self::Disable => "Add skill to shell_style_profiles.<style>.disabled_skills override list.",
        }
    }

    pub(super) fn next(self) -> Self {
        match self {
            Self::Inherit => Self::Enable,
            Self::Enable => Self::Disable,
            Self::Disable => Self::Inherit,
        }
    }

    pub(super) fn previous(self) -> Self {
        match self {
            Self::Inherit => Self::Disable,
            Self::Enable => Self::Inherit,
            Self::Disable => Self::Enable,
        }
    }

    pub(super) fn into_config_mode(self) -> ShellStyleSkillMode {
        match self {
            Self::Inherit => ShellStyleSkillMode::Inherit,
            Self::Enable => ShellStyleSkillMode::Enabled,
            Self::Disable => ShellStyleSkillMode::Disabled,
        }
    }
}

pub(super) struct SkillEditorState {
    pub(super) focus: Focus,
    pub(super) name_field: FormTextField,
    pub(super) description_field: FormTextField,
    pub(super) style_field: FormTextField,
    pub(super) style_references_field: FormTextField,
    pub(super) style_skill_roots_field: FormTextField,
    pub(super) style_mcp_include_field: FormTextField,
    pub(super) style_mcp_exclude_field: FormTextField,
    pub(super) examples_field: FormTextField,
    pub(super) body_field: FormTextField,
    pub(super) style_references_dirty: bool,
    pub(super) style_skill_roots_dirty: bool,
    pub(super) style_mcp_include_dirty: bool,
    pub(super) style_mcp_exclude_dirty: bool,
    pub(super) style_profile_mode: StyleProfileMode,
    pub(super) hovered_button: Option<ActionButton>,
    pub(super) edit_scroll_top: usize,
}

impl SkillEditorState {
    pub(super) fn new() -> Self {
        let mut name_field = FormTextField::new_single_line();
        name_field.set_filter(InputFilter::Id);
        let mut description_field = FormTextField::new_single_line();
        description_field.set_placeholder("Describe when this skill should be used.");
        let style_field = FormTextField::new_single_line();
        let style_references_field = FormTextField::new_multi_line();
        let style_skill_roots_field = FormTextField::new_multi_line();
        let style_mcp_include_field = FormTextField::new_multi_line();
        let style_mcp_exclude_field = FormTextField::new_multi_line();
        let mut examples_field = FormTextField::new_multi_line();
        examples_field
            .set_placeholder("- User asks for ...\n- User needs ...\n- Trigger when ...");
        let mut body_field = FormTextField::new_multi_line();
        body_field.set_placeholder(
            "# Overview\n\nSummarize what this skill does and why.\n\n## Workflow\n\n1. Outline the first step.\n2. Outline the second step.\n",
        );
        Self {
            focus: Focus::Name,
            name_field,
            description_field,
            style_field,
            style_references_field,
            style_skill_roots_field,
            style_mcp_include_field,
            style_mcp_exclude_field,
            examples_field,
            body_field,
            style_references_dirty: false,
            style_skill_roots_dirty: false,
            style_mcp_include_dirty: false,
            style_mcp_exclude_dirty: false,
            style_profile_mode: StyleProfileMode::Inherit,
            hovered_button: None,
            edit_scroll_top: 0,
        }
    }

    pub(super) fn style_resource_paths_dirty(&self) -> bool {
        self.style_references_dirty || self.style_skill_roots_dirty
    }

    pub(super) fn style_mcp_filters_dirty(&self) -> bool {
        self.style_mcp_include_dirty || self.style_mcp_exclude_dirty
    }

    pub(super) fn style_profile_fields_dirty(&self) -> bool {
        self.style_resource_paths_dirty() || self.style_mcp_filters_dirty()
    }
}

impl Default for SkillEditorState {
    fn default() -> Self {
        Self::new()
    }
}

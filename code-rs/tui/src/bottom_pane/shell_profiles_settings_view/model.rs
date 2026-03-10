use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RowKind {
    Style,
    Summary,
    References,
    SkillRoots,
    SkillsAllowlist,
    DisabledSkills,
    McpInclude,
    McpExclude,
    OpenSkills,
    Apply,
    Close,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ListTarget {
    Summary,
    References,
    SkillRoots,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PickTarget {
    SkillsAllowlist,
    DisabledSkills,
    McpInclude,
    McpExclude,
}

#[derive(Clone, Debug)]
pub(super) struct SkillOption {
    pub(super) name: String,
    pub(super) description: Option<String>,
}

#[derive(Debug)]
pub(super) struct PickListItem {
    pub(super) name: String,
    pub(super) description: Option<String>,
    pub(super) is_unknown: bool,
    pub(super) is_no_filter_option: bool,
}

#[derive(Debug)]
pub(super) struct PickListState {
    pub(super) target: PickTarget,
    pub(super) items: Vec<PickListItem>,
    pub(super) checked: Vec<bool>,
    pub(super) other_values: HashSet<String>,
    pub(super) scroll: ScrollState,
}

#[derive(Debug)]
pub(super) enum ViewMode {
    Main,
    EditList { target: ListTarget, before: String },
    PickList(PickListState),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum EditorFooterAction {
    Save,
    Generate,
    Pick,
    Show,
    Cancel,
}


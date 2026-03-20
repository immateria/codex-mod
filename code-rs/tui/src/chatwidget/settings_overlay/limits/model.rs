pub(crate) struct LimitsSettingsContent {
    overlay: LimitsOverlay,
    layout_mode: LimitsLayoutMode,
    pane_focus: LimitsPaneFocus,
    left_scroll: Cell<u16>,
    right_scroll: Cell<u16>,
    left_max_scroll: Cell<u16>,
    right_max_scroll: Cell<u16>,
    last_wide_active: Cell<bool>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LimitsLayoutMode {
    Auto,
    SingleColumn,
}

impl LimitsLayoutMode {
    fn from_config(mode: ConfigLimitsLayoutMode) -> Self {
        match mode {
            ConfigLimitsLayoutMode::Auto => Self::Auto,
            ConfigLimitsLayoutMode::SingleColumn => Self::SingleColumn,
        }
    }

    fn to_config(self) -> ConfigLimitsLayoutMode {
        match self {
            Self::Auto => ConfigLimitsLayoutMode::Auto,
            Self::SingleColumn => ConfigLimitsLayoutMode::SingleColumn,
        }
    }

    fn next(self) -> Self {
        match self {
            Self::Auto => Self::SingleColumn,
            Self::SingleColumn => Self::Auto,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::SingleColumn => "single",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LimitsPaneFocus {
    Sync,
    Left,
    Right,
}

impl LimitsPaneFocus {
    fn next(self) -> Self {
        match self {
            Self::Sync => Self::Left,
            Self::Left => Self::Right,
            Self::Right => Self::Sync,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Sync => "sync",
            Self::Left => "left",
            Self::Right => "right",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HintAction {
    ToggleLayout,
    CycleFocus,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ScrollTarget {
    Sync,
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PaneHit {
    Left,
    Right,
}

struct WideLayoutSnapshot {
    left_area: Rect,
    right_area: Rect,
    left_lines: Vec<Line<'static>>,
    right_lines: Vec<Line<'static>>,
}

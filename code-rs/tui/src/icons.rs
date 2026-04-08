//! Centralized icon/glyph system with opt-in NerdFont support.
//!
//! When `tui.nerd_fonts = true` in config.toml, icons are drawn from
//! the NerdFont private-use-area codepoints.  Otherwise standard Unicode
//! symbols are used (the default, which works in any terminal).
//!
//! # Customization
//!
//! Individual icons can be overridden via `[tui.icons]` in config.toml:
//!
//! ```toml
//! [tui]
//! nerd_fonts = true
//!
//! [tui.icons]
//! gutter_user  = ">"          # override user-message gutter
//! gutter_exec  = "$"          # override exec prompt
//! bullet       = "·"          # override list-separator bullet
//! ```
//!
//! Any key from [`ALL_ICONS`] can be used.  Custom values take precedence
//! over both the NerdFont and plain defaults.
//!
//! Call [`init`] once at startup before the first render.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

// ── Global state ─────────────────────────────────────────────────────

static NERD_FONTS_ENABLED: AtomicBool = AtomicBool::new(false);
static OVERRIDES: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();

// ── Icon descriptor ──────────────────────────────────────────────────

/// A single icon with a config key, NerdFont codepoint, and plain fallback.
#[derive(Clone, Copy)]
pub struct Icon {
    /// Config key used in `[tui.icons]` (matches the accessor function name).
    pub key: &'static str,
    /// NerdFont private-use-area glyph.
    pub nerd: &'static str,
    /// Standard Unicode fallback.
    pub plain: &'static str,
}

impl Icon {
    const fn new(key: &'static str, nerd: &'static str, plain: &'static str) -> Self {
        Self { key, nerd, plain }
    }

    /// Resolve the icon: custom override → NerdFont → plain.
    pub fn resolve(self) -> &'static str {
        if let Some(overrides) = OVERRIDES.get() {
            if let Some(&custom) = overrides.get(self.key) {
                return custom;
            }
        }
        if NERD_FONTS_ENABLED.load(Ordering::Relaxed) { self.nerd } else { self.plain }
    }

    /// Check whether `symbol` matches any variant of this icon
    /// (custom override, NerdFont, or plain).
    pub fn matches(self, symbol: &str) -> bool {
        if symbol == self.nerd || symbol == self.plain {
            return true;
        }
        if let Some(overrides) = OVERRIDES.get() {
            if let Some(&custom) = overrides.get(self.key) {
                return symbol == custom;
            }
        }
        false
    }
}

// ── Declarative icon registry ────────────────────────────────────────

macro_rules! define_icon_functions {
    ($(
        $(#[$meta:meta])*
        $fn_name:ident => $const_name:ident ($nerd:literal, $plain:literal);
    )+) => {
        $(
            const $const_name: Icon = Icon::new(stringify!($fn_name), $nerd, $plain);

            $(#[$meta])*
            pub fn $fn_name() -> &'static str {
                $const_name.resolve()
            }
        )+

        /// Every registered icon, for iteration/documentation/export.
        pub const ALL_ICONS: &[Icon] = &[$($const_name),+];
    };
}

// ── Public API ───────────────────────────────────────────────────────

/// Initialise the icon system.  Call once at startup.
///
/// * `nerd_fonts` – enable NerdFont glyphs.
/// * `overrides` – per-key overrides from `[tui.icons]` (may be empty).
pub fn init(nerd_fonts: bool, overrides: HashMap<String, String>) {
    NERD_FONTS_ENABLED.store(nerd_fonts, Ordering::Relaxed);
    if !overrides.is_empty() {
        let leaked: HashMap<&'static str, &'static str> = overrides
            .into_iter()
            .map(|(k, v)| {
                let k: &'static str = Box::leak(k.into_boxed_str());
                let v: &'static str = Box::leak(v.into_boxed_str());
                (k, v)
            })
            .collect();
        let _ = OVERRIDES.set(leaked);
    }
}

/// Toggle NerdFont mode at runtime (e.g. from the settings UI).
pub fn set_nerd_fonts(enabled: bool) {
    NERD_FONTS_ENABLED.store(enabled, Ordering::Relaxed);
}

/// Whether NerdFont mode is currently active.
pub fn nerd_fonts_enabled() -> bool {
    NERD_FONTS_ENABLED.load(Ordering::Relaxed)
}

pub fn ctrl_combo(key: &str) -> String {
    format!("{}+{key}", control())
}

pub fn alt_combo(key: &str) -> String {
    format!("{}+{key}", option())
}

pub fn shift_combo(key: &str) -> String {
    format!("{}+{key}", shift())
}

// ── Icon definitions ─────────────────────────────────────────────────

define_icon_functions! {
    // ── Gutter indicators (history cell types) ─────────────────────────

    /// User input message.
    gutter_user      => GUTTER_USER       ("\u{f007}", "›");       // nf-fa-user
    /// Assistant / AI response.
    gutter_assistant => GUTTER_ASSISTANT   ("\u{f108}", "•");       // nf-fa-desktop
    /// Proposed plan.
    gutter_plan      => GUTTER_PLAN        ("\u{f0c5}", "≡");       // nf-fa-copy
    /// Error.
    gutter_error     => GUTTER_ERROR       ("\u{f057}", "✗");       // nf-fa-times_circle
    /// Tool / operation running.
    gutter_running   => GUTTER_RUNNING     ("\u{f110}", "…");       // nf-fa-spinner
    /// Tool / operation success.
    gutter_success   => GUTTER_SUCCESS     ("\u{f058}", "✓");       // nf-fa-check_circle
    /// Tool / operation failure.
    gutter_failure   => GUTTER_FAILURE     ("\u{f057}", "✗");       // nf-fa-times_circle
    /// Shell / exec prompt.
    gutter_exec      => GUTTER_EXEC        ("\u{f120}", "❯");       // nf-fa-terminal
    /// Patch / diff.
    gutter_patch     => GUTTER_PATCH       ("\u{f126}", "↯");       // nf-fa-code_fork
    /// Background event.
    gutter_background => GUTTER_BACKGROUND ("\u{f0e7}", "»");       // nf-fa-bolt
    /// Notice / important.
    gutter_notice    => GUTTER_NOTICE      ("\u{f005}", "★");       // nf-fa-star
    /// Compaction summary.
    gutter_compaction => GUTTER_COMPACTION ("\u{f066}", "§");       // nf-fa-compress
    /// Context / info.
    gutter_context   => GUTTER_CONTEXT     ("\u{f05a}", "◆");       // nf-fa-info_circle

    // ── Status indicators ──────────────────────────────────────────────

    /// Operation succeeded.
    status_ok        => STATUS_OK          ("\u{f058}", "✓");       // nf-fa-check_circle
    /// Operation failed.
    status_fail      => STATUS_FAIL        ("\u{f057}", "✗");       // nf-fa-times_circle
    /// Warning.
    status_warn      => STATUS_WARN        ("\u{f06a}", "⚠");       // nf-fa-exclamation_circle
    /// Informational.
    status_info      => STATUS_INFO        ("\u{f05a}", "•");       // nf-fa-info_circle

    // ── Navigation arrows ──────────────────────────────────────────────

    /// Left navigation.
    arrow_left       => ARROW_LEFT         ("\u{f053}", "◂");       // nf-fa-chevron_left
    /// Right navigation.
    arrow_right      => ARROW_RIGHT        ("\u{f054}", "▸");       // nf-fa-chevron_right
    /// Up navigation.
    arrow_up         => ARROW_UP           ("\u{f077}", "↑");       // nf-fa-chevron_up
    /// Down navigation.
    arrow_down       => ARROW_DOWN         ("\u{f078}", "↓");       // nf-fa-chevron_down
    /// Collapse indicator.
    arrow_collapse   => ARROW_COLLAPSE     ("\u{f053}", "◂");       // nf-fa-chevron_left
    /// Expand indicator.
    arrow_expand     => ARROW_EXPAND       ("\u{f054}", "▸");       // nf-fa-chevron_right

    // ── Sidebar collapse/expand ────────────────────────────────────────

    /// Sidebar hide (with label).
    sidebar_hide     => SIDEBAR_HIDE       ("\u{f104} hide", "◂ hide");  // nf-fa-angle_double_left
    /// Sidebar show (chevron only).
    sidebar_show     => SIDEBAR_SHOW       ("\u{f105}", "▸");            // nf-fa-angle_double_right

    // ── Plan progress ──────────────────────────────────────────────────

    /// Idea / lightbulb.
    plan_lightbulb   => PLAN_LIGHTBULB     ("\u{f0eb}", "!");       // nf-fa-lightbulb_o
    /// Launch / rocket.
    plan_rocket      => PLAN_ROCKET        ("\u{f135}", "↑");       // nf-fa-rocket
    /// Clipboard / checklist.
    plan_clipboard   => PLAN_CLIPBOARD     ("\u{f0c5}", "≡");       // nf-fa-copy
    /// Progress: empty.
    progress_empty   => PROGRESS_EMPTY     ("\u{f10c}", "○");       // nf-fa-circle_o
    /// Progress: ¼.
    progress_quarter => PROGRESS_QUARTER   ("\u{f123}", "◔");       // nf-fa-star_half
    /// Progress: ½.
    progress_half    => PROGRESS_HALF      ("\u{f042}", "◑");       // nf-fa-adjust
    /// Progress: ¾.
    progress_three_quarter => PROGRESS_THREE_QUARTER ("\u{f111}", "◕"); // nf-fa-circle
    /// Progress: complete.
    progress_full    => PROGRESS_FULL      ("\u{f058}", "●");       // nf-fa-check_circle

    // ── Agent status ───────────────────────────────────────────────────

    /// Agent running.
    agent_running    => AGENT_RUNNING      ("\u{f04b}", "▶");       // nf-fa-play
    /// Agent completed.
    agent_completed  => AGENT_COMPLETED    ("\u{f058}", "✓");       // nf-fa-check_circle
    /// Agent failed.
    agent_failed     => AGENT_FAILED       ("\u{f071}", "!");       // nf-fa-warning
    /// Agent cancelled.
    agent_cancelled  => AGENT_CANCELLED    ("\u{f04d}", "▮");       // nf-fa-stop
    /// Agent pending.
    agent_pending    => AGENT_PENDING      ("\u{f110}", "…");       // nf-fa-spinner

    // ── Web search ─────────────────────────────────────────────────────

    /// Search info.
    search_info      => SEARCH_INFO        ("\u{f05a}", "•");       // nf-fa-info_circle
    /// Search success.
    search_success   => SEARCH_SUCCESS     ("\u{f058}", "✓");       // nf-fa-check_circle
    /// Search error.
    search_error     => SEARCH_ERROR       ("\u{f057}", "✗");       // nf-fa-times_circle

    // ── Breadcrumb / hierarchy separator ───────────────────────────────

    /// Breadcrumb separator.
    breadcrumb_sep   => BREADCRUMB_SEP     ("\u{f054}", "▸");       // nf-fa-chevron_right

    // ── Keyboard / modifier labels ─────────────────────────────────────

    /// Escape key label.
    escape           => ESCAPE             ("\u{f12b7}", "Esc");
    /// Control key label.
    control          => CONTROL            ("\u{f0634}", "Ctrl");
    /// Option / Alt key label.
    option           => OPTION             ("\u{f0635}", "Alt");
    /// Shift key label.
    shift            => SHIFT              ("\u{f0636}", "Shift");
    /// Enter / return key label.
    enter            => ENTER              ("\u{f0311}", "Enter");
    /// Backspace key label.
    backspace        => BACKSPACE          ("\u{f030d}", "Backspace");
    /// Tab key label.
    tab              => TAB                ("\u{f0312}", "Tab");
    /// Reverse tab / shift+tab key label.
    reverse_tab      => REVERSE_TAB        ("\u{f0325}", "Shift+Tab");
    /// Space key label.
    space            => SPACE              ("\u{f1050}", "Space");

    // ── Selection pointer ──────────────────────────────────────────────

    /// Active item pointer.
    pointer_active   => POINTER_ACTIVE     ("\u{f054}", "›");       // nf-fa-chevron_right
    /// Focused item pointer.
    pointer_focused  => POINTER_FOCUSED    ("\u{f101}", "»");       // nf-fa-angle_double_right

    // ── Misc ───────────────────────────────────────────────────────────

    /// List bullet / separator.
    bullet           => BULLET             ("\u{f111}", "•");       // nf-fa-circle
    /// Small separator dot.
    separator_dot    => SEPARATOR_DOT      ("\u{f111}", "·");       // nf-fa-circle
    /// Version transition arrow.
    upgrade_arrow    => UPGRADE_ARROW      ("\u{f061}", "→");       // nf-fa-arrow_right
    /// Collapse toggle (▼ when expanded).
    collapse_open    => COLLAPSE_OPEN      ("\u{f078}", "▼");       // nf-fa-chevron_down
    /// Collapse toggle (▶ when collapsed).
    collapse_closed  => COLLAPSE_CLOSED    ("\u{f054}", "▶");       // nf-fa-chevron_right
    /// MCP / tools play indicator.
    tool_play        => TOOL_PLAY          ("\u{f04b}", "▶");       // nf-fa-play
    /// File tree branch connector.
    tree_branch      => TREE_BRANCH        ("\u{f105}", "└");       // nf-fa-angle_right
    /// File tree start connector.
    tree_start       => TREE_START         ("\u{f105}", "┌");       // nf-fa-angle_right
    /// Rename / transition arrow.
    rename_arrow     => RENAME_ARROW       ("\u{f061}", "→");       // nf-fa-arrow_right
    /// JavaScript language icon.
    javascript_icon  => JAVASCRIPT_ICON    ("\u{f2ee}", "JS");
    /// Rust language icon.
    rust_icon        => RUST_ICON          ("\u{e7a8}", "RS");
    /// Bash / shell language icon.
    bash_icon        => BASH_ICON          ("\u{e760}", "SH");
    /// Markdown language icon.
    markdown_icon    => MARKDOWN_ICON      ("\u{f0354}", "MD");
    /// Markdown outline icon.
    markdown_icon_outline => MARKDOWN_ICON_OUTLINE ("\u{f0f5b}", "MDOutline");
    /// Informational circle icon.
    info_circle      => INFO_CIRCLE        ("\u{f05a}", "Info");
    /// Lambda symbol icon.
    lambda           => LAMBDA             ("\u{f0627}", "λ");
    /// Undo action icon.
    undo             => UNDO               ("\u{f0e2}", "Undo");
    /// Redo action icon.
    redo             => REDO               ("\u{f01e}", "Redo");
    /// Add / create action icon.
    add              => ADD                ("\u{ea60}", "Add");

    // ── Checkboxes / toggles ───────────────────────────────────────────

    /// Checkbox checked.
    checkbox_on      => CHECKBOX_ON        ("\u{f046}", "[x]");     // nf-fa-check_square_o
    /// Checkbox unchecked.
    checkbox_off     => CHECKBOX_OFF       ("\u{f096}", "[ ]");     // nf-fa-square_o
    /// Dismiss / close button.
    dismiss          => DISMISS            ("\u{f00d}", "[x]");     // nf-fa-times
    /// Markdown task list: done.
    task_done        => TASK_DONE          ("\u{f058}", "✓");       // nf-fa-check_circle
    /// Markdown task list: pending.
    task_pending     => TASK_PENDING       ("\u{f096}", "☐");       // nf-fa-square_o
    /// Copy content action.
    copy_content     => COPY_CONTENT       ("\u{f018f}", "Copy");
    /// Paste content action.
    paste_content    => PASTE_CONTENT      ("\u{f0192}", "Paste");
    /// Cut content action.
    cut_content      => CUT_CONTENT        ("\u{f0190}", "Cut");
    /// Scroll to top of a cell.
    scroll_to_top    => SCROLL_TO_TOP      ("\u{f55c}", "↑Top"); // nf-mdi-arrow_collapse_up

    // ── Number glyphs ──────────────────────────────────────────────────

    /// Number one icon.
    number_one       => NUMBER_ONE         ("\u{f0b3a}", "1");
    /// Number two icon.
    number_two       => NUMBER_TWO         ("\u{f0b3b}", "2");
    /// Number three icon.
    number_three     => NUMBER_THREE       ("\u{f0b3c}", "3");
    /// Number four icon.
    number_four      => NUMBER_FOUR        ("\u{f0b3d}", "4");
    /// Number five icon.
    number_five      => NUMBER_FIVE        ("\u{f0b3e}", "5");
    /// Number six icon.
    number_six       => NUMBER_SIX         ("\u{f0b3f}", "6");
    /// Number seven icon.
    number_seven     => NUMBER_SEVEN       ("\u{f0b40}", "7");
    /// Number eight icon.
    number_eight     => NUMBER_EIGHT       ("\u{f0b41}", "8");
    /// Number nine icon.
    number_nine      => NUMBER_NINE        ("\u{f0b42}", "9");
    /// Number zero icon.
    number_zero      => NUMBER_ZERO        ("\u{f0b39}", "0");

    // ── Markdown list bullets ──────────────────────────────────────────

    /// Level-1 list bullet.
    list_bullet_l1   => LIST_BULLET_L1     ("\u{f111}", "-");       // nf-fa-circle
    /// Level-2 list bullet.
    list_bullet_l2   => LIST_BULLET_L2     ("\u{f10c}", "·");       // nf-fa-circle_o
    /// Level-3 list bullet.
    list_bullet_l3   => LIST_BULLET_L3     ("\u{f111}", "-");       // nf-fa-circle
    /// Level-4+ list bullet.
    list_bullet_deep => LIST_BULLET_DEEP   ("\u{f10c}", "⋅");       // nf-fa-circle_o
}

// ── Settings sidebar section icons ───────────────────────────────────

const SECTION_ICONS: &[(&str, &str)] = &[
    ("Model",            "\u{f108} "),   // nf-fa-desktop
    ("Theme",            "\u{f1fc} "),   // nf-fa-paint_brush
    ("Interface",        "\u{f085} "),   // nf-fa-cogs
    ("Experimental",     "\u{f0c3} "),   // nf-fa-flask
    ("Shell",            "\u{f120} "),   // nf-fa-terminal
    ("Shell escalation", "\u{f132} "),   // nf-fa-shield
    ("Shell profiles",   "\u{f2c0} "),   // nf-fa-id_badge
    ("Exec limits",      "\u{f023} "),   // nf-fa-lock
    ("Planning",         "\u{f073} "),   // nf-fa-calendar
    ("Updates",          "\u{f019} "),   // nf-fa-download
    ("Accounts",         "\u{f0c0} "),   // nf-fa-users
    ("Secrets",          "\u{f084} "),   // nf-fa-key
    ("Apps",             "\u{f1b2} "),   // nf-fa-cube
    ("Agents",           "\u{f1b0} "),   // nf-fa-paw
    ("Memories",         "\u{f1c0} "),   // nf-fa-database
    ("Auto Drive",       "\u{f04b} "),   // nf-fa-play
    ("Review",           "\u{f002} "),   // nf-fa-search
    ("Validation",       "\u{f00c} "),   // nf-fa-check
    ("Limits",           "\u{f0e4} "),   // nf-fa-tachometer
    ("Chrome",           "\u{f268} "),   // nf-fa-chrome
    ("MCP",              "\u{f1e0} "),   // nf-fa-share_alt
    ("JS REPL",          "\u{f121} "),   // nf-fa-code
    ("Network",          "\u{f0ac} "),   // nf-fa-globe
    ("Notifications",    "\u{f0f3} "),   // nf-fa-bell
    ("Prompts",          "\u{f27a} "),   // nf-fa-commenting
    ("Skills",           "\u{f0ad} "),   // nf-fa-wrench
    ("Plugins",          "\u{f12e} "),   // nf-fa-puzzle_piece
];

pub fn section_icon(section: &str) -> &'static str {
    if !nerd_fonts_enabled() {
        return "";
    }
    SECTION_ICONS
        .iter()
        .find_map(|(name, icon)| (*name == section).then_some(*icon))
        .unwrap_or("")
}

// ── Symbol recognizers ───────────────────────────────────────────────

const PROGRESS_ICONS: &[Icon] = &[
    PROGRESS_EMPTY,
    PROGRESS_QUARTER,
    PROGRESS_HALF,
    PROGRESS_THREE_QUARTER,
    PROGRESS_FULL,
];

pub fn is_exec_prompt(s: &str) -> bool { GUTTER_EXEC.matches(s) }
pub fn is_patch(s:       &str) -> bool { GUTTER_PATCH.matches(s) }
pub fn is_user(s:        &str) -> bool { GUTTER_USER.matches(s) }
pub fn is_assistant(s:   &str) -> bool { GUTTER_ASSISTANT.matches(s) }
pub fn is_running(s:     &str) -> bool { GUTTER_RUNNING.matches(s) }
pub fn is_success(s:     &str) -> bool { GUTTER_SUCCESS.matches(s) || STATUS_OK.matches(s) }
pub fn is_failure(s:     &str) -> bool { GUTTER_FAILURE.matches(s) || STATUS_FAIL.matches(s) }
pub fn is_notice(s:      &str) -> bool { GUTTER_NOTICE.matches(s) }
pub fn is_progress(s:    &str) -> bool { PROGRESS_ICONS.iter().any(|icon| icon.matches(s)) }
pub fn is_spinner(s:     &str) -> bool { matches!(s, "◐" | "◓" | "◑" | "◒") }
pub fn is_context(s:     &str) -> bool { GUTTER_CONTEXT.matches(s) }
pub fn is_compaction(s:  &str) -> bool { GUTTER_COMPACTION.matches(s) }
pub fn is_background(s:  &str) -> bool { GUTTER_BACKGROUND.matches(s) }

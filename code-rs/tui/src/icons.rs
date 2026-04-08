//! Three-tier icon/glyph system: **NerdFont → Unicode → ASCII**.
//!
//! The active tier is controlled by `tui.icon_mode` in config.toml:
//!
//! | Value         | Description |
//! |---------------|-------------|
//! | `"nerd_fonts"` | NerdFont PUA glyphs (requires a patched font) |
//! | `"unicode"`    | Standard Unicode symbols (**default**) |
//! | `"ascii"`      | Pure ASCII fallbacks (for minimal terminals) |
//!
//! The legacy `tui.nerd_fonts = true` is still honoured when `icon_mode`
//! is absent, mapping to `"nerd_fonts"`.
//!
//! # Customization
//!
//! Individual icons can be overridden via `[tui.icons]` in config.toml:
//!
//! ```toml
//! [tui]
//! icon_mode = "nerd_fonts"
//!
//! [tui.icons]
//! gutter_user = ">"             # override all tiers
//!
//! [tui.icons.gutter_exec]       # override individual tiers
//! ascii   = "$"
//! unicode = "❯"
//! ```
//!
//! Any key from [`ALL_ICONS`] can be used.  Custom values take precedence
//! over the built-in defaults for the active tier.
//!
//! Call [`init`] once at startup before the first render.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::OnceLock;

use code_core::config_types::{IconMode, IconOverrideValue};

// ── Global state ─────────────────────────────────────────────────────

/// Stores IconMode as u8: NerdFonts=0, Unicode=1, Ascii=2.
static ICON_MODE: AtomicU8 = AtomicU8::new(1); // default: Unicode

/// Tiered override maps built from `[tui.icons]` config.
struct IconOverrides {
    /// Override that applies to all tiers (`key = "value"`).
    all: HashMap<&'static str, &'static str>,
    /// NerdFont-specific overrides (`key.nerd = "value"`).
    nerd: HashMap<&'static str, &'static str>,
    /// Unicode-specific overrides (`key.unicode = "value"`).
    unicode: HashMap<&'static str, &'static str>,
    /// ASCII-specific overrides (`key.ascii = "value"`).
    ascii: HashMap<&'static str, &'static str>,
}

static OVERRIDES: OnceLock<IconOverrides> = OnceLock::new();

// ── Icon descriptor ──────────────────────────────────────────────────

/// A single icon with a config key and three glyph tiers.
#[derive(Clone, Copy)]
pub struct Icon {
    /// Config key used in `[tui.icons]` (matches the accessor function name).
    pub key: &'static str,
    /// NerdFont private-use-area glyph.
    pub nerd: &'static str,
    /// Standard Unicode symbol.
    pub unicode: &'static str,
    /// Pure ASCII fallback.
    pub ascii: &'static str,
}

impl Icon {
    const fn new(
        key: &'static str,
        nerd: &'static str,
        unicode: &'static str,
        ascii: &'static str,
    ) -> Self {
        Self { key, nerd, unicode, ascii }
    }

    /// Resolve the icon: tier-specific override → all-tier override → default.
    pub fn resolve(self) -> &'static str {
        let mode = ICON_MODE.load(Ordering::Relaxed);
        if let Some(ovr) = OVERRIDES.get() {
            let tier_map = match mode {
                0 => &ovr.nerd,
                2 => &ovr.ascii,
                _ => &ovr.unicode,
            };
            if let Some(&s) = tier_map.get(self.key) {
                return s;
            }
            if let Some(&s) = ovr.all.get(self.key) {
                return s;
            }
        }
        match mode {
            0 => self.nerd,
            2 => self.ascii,
            _ => self.unicode,
        }
    }

    /// Check whether `symbol` matches any built-in variant or custom override.
    pub fn matches(self, symbol: &str) -> bool {
        if symbol == self.nerd || symbol == self.unicode || symbol == self.ascii {
            return true;
        }
        if let Some(ovr) = OVERRIDES.get() {
            if let Some(&s) = ovr.all.get(self.key) {
                if symbol == s { return true; }
            }
            for tier in [&ovr.nerd, &ovr.unicode, &ovr.ascii] {
                if let Some(&s) = tier.get(self.key) {
                    if symbol == s { return true; }
                }
            }
        }
        false
    }
}

// ── Declarative icon registry ────────────────────────────────────────

macro_rules! define_icon_functions {
    ($(
        $(#[$meta:meta])*
        $fn_name:ident => $const_name:ident ($nerd:literal, $unicode:literal, $ascii:literal);
    )+) => {
        $(
            const $const_name: Icon = Icon::new(
                stringify!($fn_name), $nerd, $unicode, $ascii,
            );

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

fn leak_str(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

/// Initialise the icon system.  Call once at startup.
///
/// * `mode`      – which glyph tier to display.
/// * `overrides` – per-key overrides from `[tui.icons]` (may be empty).
pub fn init(mode: IconMode, overrides: HashMap<String, IconOverrideValue>) {
    ICON_MODE.store(mode.as_u8(), Ordering::Relaxed);
    if !overrides.is_empty() {
        let mut all     = HashMap::new();
        let mut nerd    = HashMap::new();
        let mut unicode = HashMap::new();
        let mut ascii   = HashMap::new();

        for (key, value) in overrides {
            let key: &'static str = leak_str(key);
            match value {
                IconOverrideValue::All(s) => {
                    all.insert(key, leak_str(s));
                }
                IconOverrideValue::PerTier(tiers) => {
                    if let Some(s) = tiers.nerd    { nerd.insert(key, leak_str(s)); }
                    if let Some(s) = tiers.unicode { unicode.insert(key, leak_str(s)); }
                    if let Some(s) = tiers.ascii   { ascii.insert(key, leak_str(s)); }
                }
            }
        }
        let _ = OVERRIDES.set(IconOverrides { all, nerd, unicode, ascii });
    }
}

/// Change the icon mode at runtime (e.g. from the settings UI).
pub fn set_icon_mode(mode: IconMode) {
    ICON_MODE.store(mode.as_u8(), Ordering::Relaxed);
}

/// The currently active icon mode.
pub fn icon_mode() -> IconMode {
    IconMode::from_u8(ICON_MODE.load(Ordering::Relaxed))
}

/// Whether NerdFont mode is currently active (convenience wrapper).
pub fn nerd_fonts_enabled() -> bool {
    ICON_MODE.load(Ordering::Relaxed) == 0
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
    gutter_user               => GUTTER_USER                 ("\u{f007}", "›", ">");                     //     nf-fa-user
    /// Assistant / AI response.
    gutter_assistant          => GUTTER_ASSISTANT            ("\u{f108}", "•", "*");                     //     nf-fa-desktop
    /// Proposed plan.
    gutter_plan               => GUTTER_PLAN                 ("\u{f0c5}", "≡", "=");                     //     nf-fa-copy
    /// Error.
    gutter_error              => GUTTER_ERROR                ("\u{f057}", "✗", "x");                     //     nf-fa-times_circle
    /// Tool / operation running.
    gutter_running            => GUTTER_RUNNING              ("\u{f110}", "…", "..");                    //     nf-fa-spinner
    /// Tool / operation success.
    gutter_success            => GUTTER_SUCCESS              ("\u{f058}", "✓", "+");                     //     nf-fa-check_circle
    /// Tool / operation failure.
    gutter_failure            => GUTTER_FAILURE              ("\u{f057}", "✗", "x");                     //     nf-fa-times_circle
    /// Shell / exec prompt.
    gutter_exec               => GUTTER_EXEC                 ("\u{f120}", "❯", ">");                     //     nf-fa-terminal
    /// Patch / diff.
    gutter_patch              => GUTTER_PATCH                ("\u{f126}", "↯", "~");                     //     nf-fa-code_fork
    /// Background event.
    gutter_background         => GUTTER_BACKGROUND           ("\u{f0e7}", "»", ">>");                    //     nf-fa-bolt (alias: nf-fa-flash)
    /// Notice / important.
    gutter_notice             => GUTTER_NOTICE               ("\u{f005}", "★", "*");                     //     nf-fa-star
    /// Compaction summary.
    gutter_compaction         => GUTTER_COMPACTION           ("\u{f066}", "§", "S");                     //     nf-fa-compress
    /// Context / info.
    gutter_context            => GUTTER_CONTEXT              ("\u{f05a}", "◆", "*");                     //     nf-fa-info_circle

    // ── Status indicators ──────────────────────────────────────────────

    /// Operation succeeded.
    status_ok                 => STATUS_OK                   ("\u{f058}", "✓", "+");                     //     nf-fa-check_circle
    /// Operation failed.
    status_fail               => STATUS_FAIL                 ("\u{f057}", "✗", "x");                     //     nf-fa-times_circle
    /// Warning.
    status_warn               => STATUS_WARN                 ("\u{f06a}", "⚠", "!");                     //     nf-fa-exclamation_circle
    /// Informational.
    status_info               => STATUS_INFO                 ("\u{f05a}", "•", "*");                     //     nf-fa-info_circle

    // ── Navigation arrows ──────────────────────────────────────────────

    /// Left navigation.
    arrow_left                => ARROW_LEFT                  ("\u{f053}", "◂", "<");                     //     nf-fa-chevron_left
    /// Right navigation.
    arrow_right               => ARROW_RIGHT                 ("\u{f054}", "▸", ">");                     //     nf-fa-chevron_right
    /// Up navigation.
    arrow_up                  => ARROW_UP                    ("\u{f077}", "↑", "^");                     //     nf-fa-chevron_up
    /// Down navigation.
    arrow_down                => ARROW_DOWN                  ("\u{f078}", "↓", "v");                     //     nf-fa-chevron_down
    /// Collapse indicator.
    arrow_collapse            => ARROW_COLLAPSE              ("\u{f053}", "◂", "<");                     //     nf-fa-chevron_left
    /// Expand indicator.
    arrow_expand              => ARROW_EXPAND                ("\u{f054}", "▸", ">");                     //     nf-fa-chevron_right

    // ── Sidebar collapse/expand ────────────────────────────────────────

    /// Sidebar hide (with label).
    sidebar_hide              => SIDEBAR_HIDE                ("\u{f104} hide", "◂ hide", "< hide");      //  hide   hide  nf-fa-angle_left
    /// Sidebar show (chevron only).
    sidebar_show              => SIDEBAR_SHOW                ("\u{f105}", "▸", ">");                     //     nf-fa-angle_right

    // ── Plan progress ──────────────────────────────────────────────────

    /// Idea / lightbulb.
    plan_lightbulb            => PLAN_LIGHTBULB              ("\u{f0eb}", "!", "!");                     //     nf-fa-lightbulb_o
    /// Launch / rocket.
    plan_rocket               => PLAN_ROCKET                 ("\u{f135}", "↑", "^");                     //     nf-fa-rocket
    /// Clipboard / checklist.
    plan_clipboard            => PLAN_CLIPBOARD              ("\u{f0c5}", "≡", "=");                     //     nf-fa-copy
    /// Progress: empty.
    progress_empty            => PROGRESS_EMPTY              ("\u{f10c}", "○", "o");                     //     nf-fa-circle_o
    /// Progress: ¼.
    progress_quarter          => PROGRESS_QUARTER            ("\u{f123}", "◔", "o");                     //     nf-fa-star_half_o
    /// Progress: ½.
    progress_half             => PROGRESS_HALF               ("\u{f042}", "◑", "O");                     //     nf-fa-circle_half_stroke (alias: nf-fa-adjust)
    /// Progress: ¾.
    progress_three_quarter    => PROGRESS_THREE_QUARTER      ("\u{f111}", "◕", "O");                     //     nf-fa-circle
    /// Progress: complete.
    progress_full             => PROGRESS_FULL               ("\u{f058}", "●", "@");                     //     nf-fa-check_circle

    // ── Agent status ───────────────────────────────────────────────────

    /// Agent running.
    agent_running             => AGENT_RUNNING               ("\u{f04b}", "▶", ">");                     //     nf-fa-play
    /// Agent completed.
    agent_completed           => AGENT_COMPLETED             ("\u{f058}", "✓", "+");                     //     nf-fa-check_circle
    /// Agent failed.
    agent_failed              => AGENT_FAILED                ("\u{f071}", "!", "!");                     //     nf-fa-triangle_exclamation (alias: nf-fa-warning)
    /// Agent cancelled.
    agent_cancelled           => AGENT_CANCELLED             ("\u{f04d}", "▮", "|");                     //     nf-fa-stop
    /// Agent pending.
    agent_pending             => AGENT_PENDING               ("\u{f110}", "…", "..");                    //     nf-fa-spinner

    // ── Web search ─────────────────────────────────────────────────────

    /// Search info.
    search_info               => SEARCH_INFO                 ("\u{f05a}", "•", "*");                     //     nf-fa-info_circle
    /// Search success.
    search_success            => SEARCH_SUCCESS              ("\u{f058}", "✓", "+");                     //     nf-fa-check_circle
    /// Search error.
    search_error              => SEARCH_ERROR                ("\u{f057}", "✗", "x");                     //     nf-fa-times_circle

    // ── Breadcrumb / hierarchy separator ───────────────────────────────

    /// Breadcrumb separator.
    breadcrumb_sep            => BREADCRUMB_SEP              ("\u{f054}", "▸", ">");                     //     nf-fa-chevron_right

    // ── Keyboard / modifier labels ─────────────────────────────────────

    /// Escape key label.
    escape                    => ESCAPE                      ("\u{f12b7}", "Esc", "Esc");                // 󱊷  󱊷  nf-md-keyboard_esc
    /// Control key label.
    control                   => CONTROL                     ("\u{f0634}", "Ctrl", "Ctrl");              // 󰘴  󰘴  nf-md-apple_keyboard_control
    /// Option / Alt key label.
    option                    => OPTION                      ("\u{f0635}", "Alt", "Alt");                // 󰘵  󰘵  nf-md-apple_keyboard_option
    /// Shift key label.
    shift                     => SHIFT                       ("\u{f0636}", "Shift", "Shift");            // 󰘶  󰘶  nf-md-apple_keyboard_shift
    /// Enter / return key label.
    enter                     => ENTER                       ("\u{f0311}", "Enter", "Enter");            // 󰌑  󰌑  nf-md-keyboard_return
    /// Backspace key label.
    backspace                 => BACKSPACE                   ("\u{f030d}", "Backspace", "Backspace");    // 󰌍  󰌍  nf-md-keyboard_backspace
    /// Tab key label.
    tab                       => TAB                         ("\u{f0312}", "Tab", "Tab");                // 󰌒  󰌒  nf-md-keyboard_tab
    /// Reverse tab / shift+tab key label.
    reverse_tab               => REVERSE_TAB                 ("\u{f0325}", "Shift+Tab", "Shift+Tab");    // 󰌥  󰌥  nf-md-keyboard_tab_reverse
    /// Space key label.
    space                     => SPACE                       ("\u{f1050}", "Space", "Space");            // 󱁐  󱁐  nf-md-keyboard_space

    // ── Selection pointer ──────────────────────────────────────────────

    /// Active item pointer.
    pointer_active            => POINTER_ACTIVE              ("\u{f054}", "›", ">");                     //     nf-fa-chevron_right
    /// Focused item pointer.
    pointer_focused           => POINTER_FOCUSED             ("\u{f101}", "»", ">>");                    //     nf-fa-angle_double_right

    // ── Misc ───────────────────────────────────────────────────────────

    /// List bullet / separator.
    bullet                    => BULLET                      ("\u{f111}", "•", "*");                     //     nf-fa-circle
    /// Small separator dot.
    separator_dot             => SEPARATOR_DOT               ("\u{f111}", "·", "-");                     //     nf-fa-circle
    /// Version transition arrow.
    upgrade_arrow             => UPGRADE_ARROW               ("\u{f061}", "→", "->");                    //     nf-fa-arrow_right
    /// Collapse toggle (▼ when expanded).
    collapse_open             => COLLAPSE_OPEN               ("\u{f078}", "▼", "v");                     //     nf-fa-chevron_down
    /// Collapse toggle (▶ when collapsed).
    collapse_closed           => COLLAPSE_CLOSED             ("\u{f054}", "▶", ">");                     //     nf-fa-chevron_right
    /// MCP / tools play indicator.
    tool_play                 => TOOL_PLAY                   ("\u{f04b}", "▶", ">");                     //     nf-fa-play
    /// File tree branch connector.
    tree_branch               => TREE_BRANCH                 ("\u{f105}", "└", "`");                     //     nf-fa-angle_right
    /// File tree start connector.
    tree_start                => TREE_START                  ("\u{f105}", "┌", ",");                     //     nf-fa-angle_right
    /// Rename / transition arrow.
    rename_arrow              => RENAME_ARROW                ("\u{f061}", "→", "->");                    //     nf-fa-arrow_right
    /// JavaScript language icon.
    javascript_icon           => JAVASCRIPT_ICON             ("\u{f2ee}", "JS", "JS");                   //     nf-fa-js
    /// Python language icon.
    python_icon               => PYTHON_ICON                 ("\u{e606}", "PY", "PY");                   //     nf-seti-python
    /// TypeScript language icon.
    typescript_icon           => TYPESCRIPT_ICON             ("\u{e628}", "TS", "TS");                   //     nf-seti-typescript
    /// Go language icon.
    go_icon                   => GO_ICON                     ("\u{e626}", "GO", "GO");                   //     nf-custom-go
    /// HTML language icon.
    html_icon                 => HTML_ICON                   ("\u{f13b}", "HT", "HT");                   //     nf-fa-html5
    /// CSS language icon.
    css_icon                  => CSS_ICON                    ("\u{f13c}", "CS", "CS");                   //     nf-fa-css3
    /// Rust language icon.
    rust_icon                 => RUST_ICON                   ("\u{e7a8}", "RS", "RS");                   //     nf-dev-rust
    /// Bash / shell language icon.
    bash_icon                 => BASH_ICON                   ("\u{e760}", "SH", "SH");                   //     nf-dev-bash
    /// Markdown language icon.
    markdown_icon             => MARKDOWN_ICON               ("\u{f0354}", "MD", "MD");                  // 󰍔  󰍔  nf-md-language_markdown
    /// Markdown outline icon.
    markdown_icon_outline     => MARKDOWN_ICON_OUTLINE       ("\u{f0f5b}", "MDO", "MDO");                // 󰽛  󰽛  nf-md-language_markdown_outline
    /// Informational circle icon.
    info_circle               => INFO_CIRCLE                 ("\u{f05a}", "Info", "Info");               //     nf-fa-info_circle
    /// Lambda symbol icon.
    lambda                    => LAMBDA                      ("\u{f0627}", "λ", "\\");                    // 󰘧  󰘧  nf-md-lambda
    /// Undo action icon.
    undo                      => UNDO                        ("\u{f0e2}", "Undo", "Undo");               //     nf-fa-arrow_rotate_left
    /// Redo action icon.
    redo                      => REDO                        ("\u{f01e}", "Redo", "Redo");               //     nf-fa-arrow_rotate_right
    /// Add / create action icon.
    add                       => ADD                         ("\u{ea60}", "Add", "Add");                 //     nf-cod-add

    // ── Checkboxes / toggles ───────────────────────────────────────────

    /// Checkbox checked.
    checkbox_on               => CHECKBOX_ON                 ("\u{f046}", "[x]", "[x]");                 //     nf-fa-check_square_o
    /// Checkbox unchecked.
    checkbox_off              => CHECKBOX_OFF                ("\u{f096}", "[ ]", "[ ]");                 //     nf-fa-square_o
    /// Dismiss / close button.
    dismiss                   => DISMISS                     ("\u{f00d}", "[x]", "[x]");                 //     nf-fa-xmark (alias: nf-fa-times)
    /// Markdown task list: done.
    task_done                 => TASK_DONE                   ("\u{f058}", "✓", "+");                     //     nf-fa-check_circle
    /// Markdown task list: pending.
    task_pending              => TASK_PENDING                ("\u{f096}", "☐", "[ ]");                   //     nf-fa-square_o
    /// Copy content action.
    copy_content              => COPY_CONTENT                ("\u{f018f}", "Copy", "Copy");              // 󰆏  󰆏  nf-md-content_copy
    /// Paste content action.
    paste_content             => PASTE_CONTENT               ("\u{f0192}", "Paste", "Paste");            // 󰆒  󰆒  nf-md-content_paste
    /// Cut content action.
    cut_content               => CUT_CONTENT                 ("\u{f0190}", "Cut", "Cut");                // 󰆐  󰆐  nf-md-content_cut
    /// Scroll to top of a cell.
    scroll_to_top             => SCROLL_TO_TOP               ("\u{eaf4}", "↑Top", "^Top");               //     nf-cod-fold_up

    // ── Number glyphs ──────────────────────────────────────────────────

    /// Number zero icon.
    number_zero               => NUMBER_ZERO                 ("\u{1F100}", "0.", "0.");                  // 🄀  🄀
    /// Number one icon.
    number_one                => NUMBER_ONE                  ("\u{2488}", "1.", "1.");                   // ⒈  ⒈
    /// Number two icon.
    number_two                => NUMBER_TWO                  ("\u{2489}", "2.", "2.");                   // ⒉  ⒉
    /// Number three icon.
    number_three              => NUMBER_THREE                ("\u{248A}", "3.", "3.");                   // ⒊  ⒊
    /// Number four icon.
    number_four               => NUMBER_FOUR                 ("\u{248B}", "4.", "4.");                   // ⒋  ⒋
    /// Number five icon.
    number_five               => NUMBER_FIVE                 ("\u{248C}", "5.", "5.");                   // ⒌  ⒌
    /// Number six icon.
    number_six                => NUMBER_SIX                  ("\u{248D}", "6.", "6.");                   // ⒍  ⒍
    /// Number seven icon.
    number_seven              => NUMBER_SEVEN                ("\u{248E}", "7.", "7.");                   // ⒎  ⒎
    /// Number eight icon.
    number_eight              => NUMBER_EIGHT                ("\u{248F}", "8.", "8.");                   // ⒏  ⒏
    /// Number nine icon.
    number_nine               => NUMBER_NINE                 ("\u{2490}", "9.", "9.");                   // ⒐  ⒐
    

    // ── Markdown list bullets ──────────────────────────────────────────

    /// Level-1 list bullet.
    list_bullet_l1            => LIST_BULLET_L1              ("\u{f111}", "-", "-");                     //     nf-fa-circle
    /// Level-2 list bullet.
    list_bullet_l2            => LIST_BULLET_L2              ("\u{f10c}", "·", "-");                     //     nf-fa-circle_o
    /// Level-3 list bullet.
    list_bullet_l3            => LIST_BULLET_L3              ("\u{f111}", "-", "-");                     //     nf-fa-circle
    /// Level-4+ list bullet.
    list_bullet_deep          => LIST_BULLET_DEEP            ("\u{f10c}", "⋅", ".");                     //     nf-fa-circle_o

    // ── File system ────────────────────────────────────────────────────

    /// Generic file.
    file                      => FILE                        ("\u{f15b}", "⊡", "F");                     //     nf-fa-file
    /// Closed folder.
    folder                    => FOLDER                      ("\u{f07b}", "▤", "D");                     //     nf-fa-folder
    /// Open folder.
    folder_open               => FOLDER_OPEN                 ("\u{f07c}", "▥", "D");                     //     nf-fa-folder_open

    // ── Actions ────────────────────────────────────────────────────────

    /// Edit / pencil.
    edit_pencil               => EDIT_PENCIL                 ("\u{f040}", "✎", "Ed");                    //     nf-fa-pencil
    /// Delete / trash.
    trash                     => TRASH                       ("\u{f1f8}", "✕", "X");                     //     nf-fa-trash
    /// Save / floppy disk.
    save                      => SAVE                        ("\u{f0c7}", "⊟", "Sv");                    //     nf-fa-floppy_o
    /// Refresh / reload.
    refresh                   => REFRESH                     ("\u{f021}", "↺", "~");                     //     nf-fa-refresh
    /// Search / magnify.
    search                    => SEARCH                      ("\u{f002}", "⌕", "?");                     //     nf-fa-search
    /// Filter / funnel.
    filter                    => FILTER                      ("\u{f0b0}", "▽", "Y");                     //     nf-fa-filter
    /// Hyperlink.
    link                      => LINK                        ("\u{f0c1}", "⌁", "@");                     //     nf-fa-link
    /// External link (opens outside).
    external_link             => EXTERNAL_LINK               ("\u{f08e}", "↗", "->");                    //     nf-fa-external_link
    /// Send / submit.
    send                      => SEND                        ("\u{f1d9}", "↵", "=>");                    //     nf-fa-paper_plane

    // ── State toggles ──────────────────────────────────────────────────

    /// Locked.
    lock                      => LOCK                        ("\u{f023}", "⊘", "[L]");                   //     nf-fa-lock
    /// Unlocked.
    unlock                    => UNLOCK                      ("\u{f09c}", "⊙", "[U]");                   //     nf-fa-unlock
    /// Visible / show.
    eye_show                  => EYE_SHOW                    ("\u{f06e}", "◉", "(o)");                   //     nf-fa-eye
    /// Hidden / masked.
    eye_hide                  => EYE_HIDE                    ("\u{f070}", "◎", "(-)");                   //     nf-fa-eye_slash
    /// Pinned.
    pin                       => PIN                         ("\u{f08d}", "♦", "*");                     //     nf-fa-thumb_tack
    /// Favourite (empty star).
    star_empty                => STAR_EMPTY                  ("\u{f006}", "☆", "*");                     //     nf-fa-star_o
    /// Bookmarked.
    bookmark                  => BOOKMARK                    ("\u{f02e}", "⊲", "[B]");                   //     nf-fa-bookmark

    // ── Time & reference ──────────────────────────────────────────────

    /// Clock / timestamp.
    clock                     => CLOCK                       ("\u{f017}", "◷", "Tm");                    //     nf-fa-clock_o
    /// Tag / label.
    tag                       => TAG                         ("\u{f02b}", "◈", "#");                     //     nf-fa-tag
    /// Hash / number sign.
    hash_symbol               => HASH_SYMBOL                 ("\u{f292}", "#", "#");                     //     nf-fa-hashtag

    // ── Navigation & layout ────────────────────────────────────────────

    /// Home / root.
    home                      => HOME                        ("\u{f015}", "⌂", "~");                     //     nf-fa-home
    /// Horizontal ellipsis (more items).
    ellipsis_h                => ELLIPSIS_H                  ("\u{f141}", "…", "..");                    //     nf-fa-ellipsis_h
    /// Vertical ellipsis (more items).
    ellipsis_v                => ELLIPSIS_V                  ("\u{f142}", "⋮", ":");                     //     nf-fa-ellipsis_vertical
    /// Word wrap toggle.
    word_wrap                 => WORD_WRAP                   ("\u{f035}", "↩", "<-");                    //     nf-fa-text_width

    // ── Git ────────────────────────────────────────────────────────────

    /// Git branch.
    git_branch                => GIT_BRANCH                  ("\u{e725}", "⎇", "Br");                    //     nf-dev-git_branch
    /// Git commit.
    git_commit                => GIT_COMMIT                  ("\u{e729}", "○", "Cm");                    //     nf-dev-git_commit
    /// Git merge.
    git_merge                 => GIT_MERGE                   ("\u{e727}", "⊕", "Mg");                    //     nf-dev-git_merge

    // ── System / environment ───────────────────────────────────────────

    /// Settings gear / cog.
    settings_gear             => SETTINGS_GEAR               ("\u{f013}", "⚙", "Cfg");                   //     nf-fa-gear (alias: nf-fa-cog)
    /// Cloud / remote.
    cloud                     => CLOUD                       ("\u{f0c2}", "☁", "Cld");                   //     nf-fa-cloud
    /// Notification bell.
    bell                      => BELL                        ("\u{f0f3}", "◔", "(!)");                   //     nf-fa-bell
    /// Muted bell.
    bell_off                  => BELL_OFF                    ("\u{f1f6}", "○", "(-)");                   //     nf-fa-bell_slash
    /// Robot / AI agent.
    robot                     => ROBOT                       ("\u{ee0d}", "⊛", "Bot");                   //     nf-fa-robot
}

// ── Settings sidebar section icons ───────────────────────────────────

struct SectionIcon {
    name: &'static str,
    nerd: &'static str,
    unicode: &'static str,
}

const SECTION_ICONS: &[SectionIcon] = &[
    SectionIcon { name: "Model",            nerd: "\u{f108} ", unicode: "⌂ " },
    SectionIcon { name: "Theme",            nerd: "\u{f1fc} ", unicode: "◆ " },
    SectionIcon { name: "Interface",        nerd: "\u{f085} ", unicode: "⚙ " },
    SectionIcon { name: "Experimental",     nerd: "\u{f0c3} ", unicode: "◇ " },
    SectionIcon { name: "Shell",            nerd: "\u{f120} ", unicode: "❯ " },
    SectionIcon { name: "Shell escalation", nerd: "\u{f132} ", unicode: "▲ " },
    SectionIcon { name: "Shell profiles",   nerd: "\u{f2c1} ", unicode: "◈ " },
    SectionIcon { name: "Exec limits",      nerd: "\u{f023} ", unicode: "⊘ " },
    SectionIcon { name: "Planning",         nerd: "\u{f073} ", unicode: "◷ " },
    SectionIcon { name: "Updates",          nerd: "\u{f019} ", unicode: "↓ " },
    SectionIcon { name: "Accounts",         nerd: "\u{f0c0} ", unicode: "◉ " },
    SectionIcon { name: "Secrets",          nerd: "\u{f084} ", unicode: "♦ " },
    SectionIcon { name: "Apps",             nerd: "\u{f1b2} ", unicode: "◆ " },
    SectionIcon { name: "Agents",           nerd: "\u{f1b0} ", unicode: "★ " },
    SectionIcon { name: "Memories",         nerd: "\u{f1c0} ", unicode: "▤ " },
    SectionIcon { name: "Auto Drive",       nerd: "\u{f04b} ", unicode: "▶ " },
    SectionIcon { name: "Review",           nerd: "\u{f002} ", unicode: "⌕ " },
    SectionIcon { name: "Validation",       nerd: "\u{f00c} ", unicode: "✓ " },
    SectionIcon { name: "Limits",           nerd: "\u{f0e4} ", unicode: "◷ " },
    SectionIcon { name: "Chrome",           nerd: "\u{f268} ", unicode: "▣ " },
    SectionIcon { name: "MCP",              nerd: "\u{f1e0} ", unicode: "⊕ " },
    SectionIcon { name: "JS REPL",          nerd: "\u{f121} ", unicode: "❯ " },
    SectionIcon { name: "Network",          nerd: "\u{f0ac} ", unicode: "◎ " },
    SectionIcon { name: "Notifications",    nerd: "\u{f0f3} ", unicode: "◔ " },
    SectionIcon { name: "Prompts",          nerd: "\u{f27a} ", unicode: "◆ " },
    SectionIcon { name: "Skills",           nerd: "\u{f0ad} ", unicode: "✎ " },
    SectionIcon { name: "Plugins",          nerd: "\u{f12e} ", unicode: "⊞ " },
];

pub fn section_icon(section: &str) -> &'static str {
    let mode = ICON_MODE.load(Ordering::Relaxed);
    // ASCII mode: no section icons (keep sidebar compact)
    if mode == 2 {
        return "";
    }
    SECTION_ICONS
        .iter()
        .find_map(|si| {
            if si.name != section { return None; }
            Some(if mode == 0 { si.nerd } else { si.unicode })
        })
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

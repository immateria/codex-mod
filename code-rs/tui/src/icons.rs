//! Centralized icon/glyph system with opt-in NerdFont support.
//!
//! When `tui.nerd_fonts = true` in config.toml, icons are drawn from
//! the NerdFont private-use-area codepoints. Otherwise standard Unicode
//! symbols are used (the default, which works in any terminal).
//!
//! Call [`set_nerd_fonts`] once at startup before the first render.

use std::sync::atomic::{AtomicBool, Ordering};

static NERD_FONTS_ENABLED: AtomicBool = AtomicBool::new(false);

/// Enable or disable NerdFont glyphs globally. Call once at startup.
pub fn set_nerd_fonts(enabled: bool) {
    NERD_FONTS_ENABLED.store(enabled, Ordering::Relaxed);
}

/// Whether NerdFont mode is active.
pub fn nerd_fonts_enabled() -> bool {
    NERD_FONTS_ENABLED.load(Ordering::Relaxed)
}

/// Pick between a NerdFont glyph and a plain Unicode fallback.
#[inline]
fn pick(nerd: &'static str, plain: &'static str) -> &'static str {
    if NERD_FONTS_ENABLED.load(Ordering::Relaxed) { nerd } else { plain }
}

// ── Gutter indicators (history cell types) ───────────────────────────

/// User input message.
pub fn gutter_user() -> &'static str { pick("\u{f007}", "›") }       // nf-fa-user
/// Assistant / AI response.
pub fn gutter_assistant() -> &'static str { pick("\u{f108}", "•") }   // nf-fa-desktop (model)
/// Proposed plan.
pub fn gutter_plan() -> &'static str { pick("\u{f0c5}", "≡") }       // nf-fa-copy (clipboard)
/// Error.
pub fn gutter_error() -> &'static str { pick("\u{f057}", "✗") }      // nf-fa-times_circle
/// Tool / operation running.
pub fn gutter_running() -> &'static str { pick("\u{f110}", "…") }    // nf-fa-spinner
/// Tool / operation success.
pub fn gutter_success() -> &'static str { pick("\u{f058}", "✓") }    // nf-fa-check_circle
/// Tool / operation failure.
pub fn gutter_failure() -> &'static str { pick("\u{f057}", "✗") }    // nf-fa-times_circle
/// Shell / exec prompt.
pub fn gutter_exec() -> &'static str { pick("\u{f120}", "❯") }       // nf-fa-terminal
/// Patch / diff.
pub fn gutter_patch() -> &'static str { pick("\u{f126}", "↯") }      // nf-fa-code_fork
/// Background event.
pub fn gutter_background() -> &'static str { pick("\u{f0e7}", "»") }  // nf-fa-bolt
/// Notice / important.
pub fn gutter_notice() -> &'static str { pick("\u{f005}", "★") }     // nf-fa-star
/// Compaction summary.
pub fn gutter_compaction() -> &'static str { pick("\u{f066}", "§") }  // nf-fa-compress
/// Context / info.
pub fn gutter_context() -> &'static str { pick("\u{f05a}", "◆") }    // nf-fa-info_circle

// ── Status indicators ────────────────────────────────────────────────

pub fn status_ok() -> &'static str { pick("\u{f058}", "✓") }         // nf-fa-check_circle
pub fn status_fail() -> &'static str { pick("\u{f057}", "✗") }       // nf-fa-times_circle
pub fn status_warn() -> &'static str { pick("\u{f06a}", "⚠") }       // nf-fa-exclamation_circle
pub fn status_info() -> &'static str { pick("\u{f05a}", "•") }       // nf-fa-info_circle

// ── Navigation arrows ────────────────────────────────────────────────

pub fn arrow_left() -> &'static str { pick("\u{f053}", "◂") }        // nf-fa-chevron_left
pub fn arrow_right() -> &'static str { pick("\u{f054}", "▸") }       // nf-fa-chevron_right
pub fn arrow_up() -> &'static str { pick("\u{f077}", "↑") }          // nf-fa-chevron_up
pub fn arrow_down() -> &'static str { pick("\u{f078}", "↓") }        // nf-fa-chevron_down
pub fn arrow_collapse() -> &'static str { pick("\u{f053}", "◂") }    // nf-fa-chevron_left
pub fn arrow_expand() -> &'static str { pick("\u{f054}", "▸") }      // nf-fa-chevron_right

// ── Sidebar collapse/expand ──────────────────────────────────────────

pub fn sidebar_hide() -> &'static str { pick("\u{f104} hide", "◂ hide") }  // nf-fa-angle_double_left
pub fn sidebar_show() -> &'static str { pick("\u{f105}", "▸") }            // nf-fa-angle_double_right

// ── Plan progress ────────────────────────────────────────────────────

pub fn plan_lightbulb() -> &'static str { pick("\u{f0eb}", "!") }    // nf-fa-lightbulb_o
pub fn plan_rocket() -> &'static str { pick("\u{f135}", "↑") }       // nf-fa-rocket
pub fn plan_clipboard() -> &'static str { pick("\u{f0c5}", "≡") }    // nf-fa-copy

pub fn progress_empty() -> &'static str { pick("\u{f10c}", "○") }    // nf-fa-circle_o
pub fn progress_quarter() -> &'static str { pick("\u{f123}", "◔") }  // nf-fa-star_half (approx)
pub fn progress_half() -> &'static str { pick("\u{f042}", "◑") }     // nf-fa-adjust
pub fn progress_three_quarter() -> &'static str { pick("\u{f111}", "◕") } // nf-fa-circle (mostly filled)
pub fn progress_full() -> &'static str { pick("\u{f058}", "●") }     // nf-fa-check_circle

// ── Agent status ─────────────────────────────────────────────────────

pub fn agent_running() -> &'static str { pick("\u{f04b}", "▶") }     // nf-fa-play
pub fn agent_completed() -> &'static str { pick("\u{f058}", "✓") }   // nf-fa-check_circle
pub fn agent_failed() -> &'static str { pick("\u{f071}", "!") }      // nf-fa-warning
pub fn agent_cancelled() -> &'static str { pick("\u{f04d}", "▮") }   // nf-fa-stop
pub fn agent_pending() -> &'static str { pick("\u{f110}", "…") }     // nf-fa-spinner

// ── Web search ───────────────────────────────────────────────────────

pub fn search_info() -> &'static str { pick("\u{f05a}", "•") }       // nf-fa-info_circle
pub fn search_success() -> &'static str { pick("\u{f058}", "✓") }    // nf-fa-check_circle
pub fn search_error() -> &'static str { pick("\u{f057}", "✗") }      // nf-fa-times_circle

// ── Settings sidebar section icons ───────────────────────────────────
// These return "icon label" when NerdFont is on, or just "" for plain mode
// (the caller prepends the icon to the existing label).

pub fn section_icon(section: &str) -> &'static str {
    if !NERD_FONTS_ENABLED.load(Ordering::Relaxed) {
        return "";
    }
    match section {
        "Model" => "\u{f108} ",           // nf-fa-desktop
        "Theme" => "\u{f1fc} ",           // nf-fa-paint_brush
        "Interface" => "\u{f085} ",       // nf-fa-cogs
        "Experimental" => "\u{f0c3} ",    // nf-fa-flask
        "Shell" => "\u{f120} ",           // nf-fa-terminal
        "Shell escalation" => "\u{f132} ",// nf-fa-shield
        "Shell profiles" => "\u{f2c0} ",  // nf-fa-id_badge
        "Exec limits" => "\u{f023} ",     // nf-fa-lock
        "Planning" => "\u{f073} ",        // nf-fa-calendar
        "Updates" => "\u{f019} ",         // nf-fa-download
        "Accounts" => "\u{f0c0} ",        // nf-fa-users
        "Secrets" => "\u{f084} ",         // nf-fa-key
        "Apps" => "\u{f1b2} ",            // nf-fa-cube
        "Agents" => "\u{f1b0} ",          // nf-fa-paw
        "Memories" => "\u{f1c0} ",        // nf-fa-database
        "Auto Drive" => "\u{f04b} ",      // nf-fa-play
        "Review" => "\u{f002} ",          // nf-fa-search
        "Validation" => "\u{f00c} ",      // nf-fa-check
        "Limits" => "\u{f0e4} ",          // nf-fa-tachometer
        "Chrome" => "\u{f268} ",          // nf-fa-chrome
        "MCP" => "\u{f1e0} ",             // nf-fa-share_alt
        "JS REPL" => "\u{f121} ",         // nf-fa-code
        "Network" => "\u{f0ac} ",         // nf-fa-globe
        "Notifications" => "\u{f0f3} ",   // nf-fa-bell
        "Prompts" => "\u{f27a} ",         // nf-fa-commenting
        "Skills" => "\u{f0ad} ",          // nf-fa-wrench
        "Plugins" => "\u{f12e} ",         // nf-fa-puzzle_piece
        _ => "",
    }
}

// ── Breadcrumb / hierarchy separator ─────────────────────────────────

pub fn breadcrumb_sep() -> &'static str { pick("\u{f054}", "▸") }    // nf-fa-chevron_right

// ── Selection pointer ────────────────────────────────────────────────

pub fn pointer_active() -> &'static str { pick("\u{f054}", "›") }    // nf-fa-chevron_right
pub fn pointer_focused() -> &'static str { pick("\u{f101}", "»") }   // nf-fa-angle_double_right

// ── Misc ─────────────────────────────────────────────────────────────

pub fn bullet() -> &'static str { pick("\u{f111}", "•") }            // nf-fa-circle
pub fn separator_dot() -> &'static str { pick("\u{f111}", "·") }     // nf-fa-circle (small)
pub fn upgrade_arrow() -> &'static str { pick("\u{f061}", "→") }     // nf-fa-arrow_right

//! Shared UI constants for layout and alignment within the TUI.

/// Width (in terminal columns) reserved for the left gutter/prefix used by
/// live cells and aligned widgets.
///
/// Semantics:
/// - Chat composer reserves this many columns for the left border + padding.
/// - Status indicator lines begin with this many spaces for alignment.
/// - User history lines account for this many columns (e.g., "▌ ") when wrapping.
pub(crate) const _LIVE_PREFIX_COLS: u16 = 2;

// ---------------------------------------------------------------------------
// Separator strings — used to join metadata spans and summary parts.
// ---------------------------------------------------------------------------

/// Middle-dot separator used between metadata items (model, timestamp, tokens).
pub(crate) const SEP_DOT: &str = " · ";

// ---------------------------------------------------------------------------
// Card hint strings — keyboard shortcuts shown in card footers.
// ---------------------------------------------------------------------------

/// Agent-run expand shortcut + stop hint.
pub(crate) const CARD_HINT_EXPAND_STOP: &str = " [Ctrl+A] Expand · [Esc] Stop";
/// Agent-run expand shortcut (no stop — run already finished).
pub(crate) const CARD_HINT_EXPAND: &str = " [Ctrl+A] Expand";
/// Browser view shortcut + stop hint.
pub(crate) const CARD_HINT_BROWSER_STOP: &str = " [Ctrl+B] View · [Esc] Stop";

// ---------------------------------------------------------------------------
// Standard layout margins — shared across settings panels, overlays, etc.
// ---------------------------------------------------------------------------

use ratatui::layout::Margin;

/// Standard horizontal padding (1 col left + right, 0 rows top + bottom).
/// Used by settings panels, overlay content areas, and padded render rects.
pub(crate) const HORIZONTAL_PAD: Margin = Margin::new(1, 0);

/// Uniform padding (1 col horizontal + 1 row vertical).
/// Used by overlay chrome, widget frames, and settings section boxes.
pub(crate) const UNIFORM_PAD: Margin = Margin::new(1, 1);

/// Indented horizontal padding (2 cols left + right).
/// Used for nested elements like command popups and indented form fields.
pub(crate) const NESTED_HPAD: Margin = Margin::new(2, 0);

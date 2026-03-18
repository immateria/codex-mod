use std::mem;

/// Helper for the common pattern:
/// - Temporarily replace a mode field with a sentinel (e.g. `Transition`)
/// - Run logic that may early-return or explicitly transition modes
/// - If the mode is still the sentinel at scope end, restore the previous mode
///
/// This intentionally does not borrow the slot for its entire lifetime because many handlers
/// transition modes via `self.mode = ...` while the guard is alive.
/// Do not hold references into the slot while a `ModeGuard` exists.
pub(crate) struct ModeGuard<M> {
    slot: std::ptr::NonNull<M>,
    restore: Option<M>,
    is_sentinel: fn(&M) -> bool,
}

impl<M> ModeGuard<M> {
    pub(crate) fn replace(slot: &mut M, sentinel: M, is_sentinel: fn(&M) -> bool) -> ModeGuard<M> {
        let restore = Some(mem::replace(slot, sentinel));
        ModeGuard {
            slot: std::ptr::NonNull::from(slot),
            restore,
            is_sentinel,
        }
    }

    /// Disable restoration on drop, leaving whatever is currently in the slot.
    pub(crate) fn disarm(&mut self) {
        let _ = self.restore.take();
    }

    pub(crate) fn mode_mut(&mut self) -> &mut M {
        self.restore
            .as_mut()
            .expect("ModeGuard restore already taken")
    }
}

impl<M> Drop for ModeGuard<M> {
    fn drop(&mut self) {
        let Some(restore) = self.restore.take() else {
            return;
        };

        // Safety: `slot` is derived from a `&mut M` passed to `replace`.
        // We only read/write the slot during drop.
        let slot = unsafe { self.slot.as_mut() };
        if (self.is_sentinel)(slot) {
            *slot = restore;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq, Eq)]
    enum Mode {
        Main,
        Transition,
        Other,
    }

    #[test]
    fn restores_previous_mode_when_slot_stays_sentinel() {
        let mut mode = Mode::Main;
        {
            let mut guard = ModeGuard::replace(&mut mode, Mode::Transition, |m| matches!(m, Mode::Transition));
            *guard.mode_mut() = Mode::Other;
            // Do not touch `mode` (slot) so it stays `Transition`.
        }
        assert_eq!(mode, Mode::Other);
    }

    #[test]
    fn does_not_override_explicit_mode_change() {
        let mut mode = Mode::Main;
        {
            let _guard = ModeGuard::replace(&mut mode, Mode::Transition, |m| matches!(m, Mode::Transition));
            // Explicit transition away from sentinel should win.
            mode = Mode::Other;
        }
        assert_eq!(mode, Mode::Other);
    }

    #[test]
    fn disarm_keeps_sentinel_when_slot_stays_sentinel() {
        let mut mode = Mode::Main;
        {
            let mut guard = ModeGuard::replace(&mut mode, Mode::Transition, |m| matches!(m, Mode::Transition));
            guard.disarm();
        }
        assert_eq!(mode, Mode::Transition);
    }
}

// Submodules were extracted from the former monolithic `input_pipeline.rs`.
// Keep their imports stable by providing the same chatwidget-level prelude the
// old module had via `super::*`.
mod prelude {
    pub(super) use super::super::*;
}

mod browser_overlay;
mod ghost_snapshots;
mod history;
mod key_event;
mod limits_overlay;
mod mouse;
mod slash;
mod user_input;

// Split out of the former monolithic `input_pipeline/history.rs` for readability.
// Each submodule contributes inherent methods on `ChatWidget<'_>`.
mod background_events;
mod cell_hydration;
mod debug;
mod exec_wait;
mod explore_trailing;
mod operations;
mod ordered_insert;
mod push_helpers;
mod tool_updates;

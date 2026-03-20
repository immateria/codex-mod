use super::prelude::*;

type NumstatRow = (Option<u32>, Option<u32>, String);

include!("snapshot_jobs.rs");
include!("history_snapshots.rs");
include!("undo_picker.rs");
include!("restore.rs");

mod state;
mod view;

mod account_row;
mod accounts;
mod confirm_remove;
mod list_mode;
mod render;
mod store_paths;

pub(crate) use state::LoginAccountsState;
pub(crate) use view::LoginAccountsView;

use state::{AccountRow, StorePathEditorAction, StorePathEditorState, ViewMode};

const CHATGPT_REFRESH_INTERVAL_DAYS: i64 = 28;
const ACCOUNTS_TWO_PANE_MIN_WIDTH: u16 = 96;
const ACCOUNTS_TWO_PANE_MIN_HEIGHT: u16 = 10;
const ACCOUNTS_LIST_PANE_PERCENT: u16 = 42;

mod add_account;
mod manage;
mod shared;

use ratatui::layout::Margin;

use crate::bottom_pane::settings_ui::panel::SettingsPanelStyle;

pub(crate) use add_account::{LoginAddAccountState, LoginAddAccountView};
pub(crate) use manage::{LoginAccountsState, LoginAccountsView};

fn panel_style() -> SettingsPanelStyle {
    SettingsPanelStyle::bottom_pane().with_margin(Margin::new(1, 0))
}


use super::*;
use crate::bottom_pane::settings_pages::exec_limits::ExecLimitsSettingsView;
use crate::bottom_pane::settings_pages::interface::InterfaceSettingsView;
use crate::bottom_pane::settings_pages::repl::ReplSettingsView;
use crate::bottom_pane::settings_pages::memories::MemoriesSettingsView;
use crate::bottom_pane::settings_pages::model::ModelSelectionViewParams;
#[cfg(feature = "managed-network-proxy")]
use crate::bottom_pane::settings_pages::network::NetworkSettingsView;
use crate::bottom_pane::settings_pages::overview::SettingsOverviewView;
use crate::bottom_pane::settings_pages::apps::AppsSettingsView;
use crate::bottom_pane::settings_pages::plugins::PluginsSettingsView;
use crate::bottom_pane::settings_pages::secrets::SecretsSettingsView;
use crate::bottom_pane::settings_pages::shell::ShellSelectionView;
use crate::bottom_pane::settings_pages::shell_escalation::ShellEscalationSettingsView;
use crate::bottom_pane::settings_pages::shell_profiles::ShellProfilesSettingsView;
use crate::chatwidget::settings_overlay::{
    AppsSettingsContent,
    ExecLimitsSettingsContent,
    InterfaceSettingsContent,
    ReplSettingsContent,
    MemoriesSettingsContent,
    PluginsSettingsContent,
    SecretsSettingsContent,
    ShellSettingsContent,
    ShellEscalationSettingsContent,
    ShellProfilesSettingsContent,
};
#[cfg(feature = "managed-network-proxy")]
use crate::chatwidget::settings_overlay::NetworkSettingsContent;
use crate::ui_consts::SEP_DOT;

include!("overlay.rs");
include!("builders.rs");
include!("summaries.rs");
include!("bottom_pane.rs");

use super::*;
use crate::bottom_pane::settings_pages::exec_limits::ExecLimitsSettingsView;
use crate::bottom_pane::settings_pages::interface::InterfaceSettingsView;
use crate::bottom_pane::settings_pages::js_repl::JsReplSettingsView;
use crate::bottom_pane::settings_pages::memories::MemoriesSettingsView;
use crate::bottom_pane::settings_pages::model::ModelSelectionViewParams;
use crate::bottom_pane::settings_pages::network::NetworkSettingsView;
use crate::bottom_pane::settings_pages::overview::SettingsOverviewView;
use crate::bottom_pane::settings_pages::plugins::PluginsSettingsView;
use crate::bottom_pane::settings_pages::shell::ShellSelectionView;
use crate::bottom_pane::settings_pages::shell_profiles::ShellProfilesSettingsView;
use crate::chatwidget::settings_overlay::{
    ExecLimitsSettingsContent,
    InterfaceSettingsContent,
    JsReplSettingsContent,
    MemoriesSettingsContent,
    NetworkSettingsContent,
    PluginsSettingsContent,
    ShellSettingsContent,
    ShellProfilesSettingsContent,
};

include!("overlay.rs");
include!("builders.rs");
include!("summaries.rs");
include!("bottom_pane.rs");

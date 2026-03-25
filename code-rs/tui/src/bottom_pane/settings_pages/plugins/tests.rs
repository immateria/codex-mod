use super::*;

use std::path::PathBuf;
use std::sync::{Arc, Mutex, mpsc};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use code_core::plugins::{
    ConfiguredMarketplace,
    ConfiguredMarketplacePlugin,
    MarketplacePluginAuthPolicy,
    MarketplacePluginInstallPolicy,
    MarketplacePluginPolicy,
    MarketplacePluginSource,
    PluginDetail,
    PluginReadOutcome,
};

use crate::app_event::AppEvent;
use crate::chatwidget::{PluginsDetailState, PluginsListState, PluginsSharedState};

fn abs(path: &str) -> AbsolutePathBuf {
    AbsolutePathBuf::try_from(PathBuf::from(path)).expect("absolute path")
}

fn make_policy() -> MarketplacePluginPolicy {
    MarketplacePluginPolicy {
        installation: MarketplacePluginInstallPolicy::Available,
        authentication: MarketplacePluginAuthPolicy::OnInstall,
        products: None,
    }
}

fn make_marketplace(
    name: &str,
    path: AbsolutePathBuf,
    plugin: ConfiguredMarketplacePlugin,
) -> ConfiguredMarketplace {
    ConfiguredMarketplace {
        name: name.to_string(),
        path,
        interface: None,
        plugins: vec![plugin],
    }
}

fn make_configured_plugin(
    id: &str,
    name: &str,
    source_path: AbsolutePathBuf,
    installed: bool,
    enabled: bool,
) -> ConfiguredMarketplacePlugin {
    ConfiguredMarketplacePlugin {
        id: id.to_string(),
        name: name.to_string(),
        source: MarketplacePluginSource::Local { path: source_path },
        policy: make_policy(),
        interface: None,
        installed,
        enabled,
    }
}

fn make_detail_outcome(
    marketplace_name: &str,
    marketplace_path: AbsolutePathBuf,
    plugin_id: &str,
    plugin_name: &str,
    source_path: AbsolutePathBuf,
    installed: bool,
    enabled: bool,
) -> PluginReadOutcome {
    PluginReadOutcome {
        marketplace_name: marketplace_name.to_string(),
        marketplace_path: marketplace_path.clone(),
        plugin: PluginDetail {
            id: plugin_id.to_string(),
            name: plugin_name.to_string(),
            description: None,
            source: MarketplacePluginSource::Local { path: source_path },
            policy: make_policy(),
            interface: None,
            installed,
            enabled,
            skills: Vec::new(),
            apps: Vec::new(),
            mcp_server_names: Vec::new(),
        },
    }
}

fn make_shared_state_ready(
    roots: Vec<AbsolutePathBuf>,
    marketplaces: Vec<ConfiguredMarketplace>,
) -> Arc<Mutex<PluginsSharedState>> {
    Arc::new(Mutex::new(PluginsSharedState {
        list: PluginsListState::Ready {
            roots,
            marketplaces,
            marketplace_load_errors: Vec::new(),
            remote_sync_error: None,
            featured_plugin_ids: Vec::new(),
        },
        ..Default::default()
    }))
}

#[test]
fn enter_on_list_opens_detail_and_requests_plugin_detail() {
    let root = abs("/tmp");
    let marketplace_path = abs("/tmp/marketplace");
    let plugin_source_path = abs("/tmp/marketplace/plugins/p1");

    let plugin = make_configured_plugin("p1", "plugin-one", plugin_source_path.clone(), false, false);
    let marketplaces = vec![make_marketplace("Local", marketplace_path.clone(), plugin)];
    let shared_state = make_shared_state_ready(vec![root.clone()], marketplaces);

    let (tx, rx) = mpsc::channel();
    let app_event_tx = AppEventSender::new(tx);
    let mut view = PluginsSettingsView::new(shared_state, vec![root], app_event_tx);

    assert!(rx.try_recv().is_err(), "view creation should not auto-fetch when ready");

    assert!(view.handle_key_event_direct(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));

    match rx.try_recv().expect("FetchPluginDetail") {
        AppEvent::FetchPluginDetail { request } => {
            assert_eq!(request.plugin_name, "plugin-one");
            assert_eq!(request.marketplace_path, marketplace_path);
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn detail_install_action_emits_install_event() {
    let root = abs("/tmp");
    let marketplace_path = abs("/tmp/marketplace");
    let plugin_source_path = abs("/tmp/marketplace/plugins/p1");

    let plugin = make_configured_plugin("p1", "plugin-one", plugin_source_path.clone(), false, false);
    let marketplaces = vec![make_marketplace("Local", marketplace_path.clone(), plugin)];
    let shared_state = make_shared_state_ready(vec![root.clone()], marketplaces);

    let (tx, rx) = mpsc::channel();
    let app_event_tx = AppEventSender::new(tx);
    let mut view = PluginsSettingsView::new(shared_state.clone(), vec![root], app_event_tx);

    let key = PluginDetailKey::new(marketplace_path.clone(), "plugin-one".to_string());
    let outcome = make_detail_outcome(
        "Local",
        marketplace_path.clone(),
        "p1",
        "plugin-one",
        plugin_source_path.clone(),
        false,
        false,
    );
    shared_state
        .lock()
        .unwrap_or_else(|err| err.into_inner())
        .details
        .insert(key.clone(), PluginsDetailState::Ready(outcome));

    view.mode = Mode::Detail { key: key.clone() };
    view.focused_detail_button = DetailAction::Install;

    assert!(view.handle_key_event_direct(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));

    match rx.try_recv().expect("InstallPlugin") {
        AppEvent::InstallPlugin { request, force_remote_sync } => {
            assert!(!force_remote_sync);
            assert_eq!(request.plugin_name, "plugin-one");
            assert_eq!(request.marketplace_path, marketplace_path);
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

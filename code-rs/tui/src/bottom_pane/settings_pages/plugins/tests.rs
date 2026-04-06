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
use code_core::config_types::{PluginMarketplaceRepoToml, PluginsToml};

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
            remote_sync_needs_auth: false,
            featured_plugin_ids: Vec::new(),
        },
        ..Default::default()
    }))
}

fn make_sources(
    curated_url: Option<&str>,
    curated_ref: Option<&str>,
    repos: Vec<(&str, Option<&str>)>,
) -> PluginsToml {
    PluginsToml {
        curated_repo_url: curated_url.map(ToString::to_string),
        curated_repo_ref: curated_ref.map(ToString::to_string),
        marketplace_repos: repos
            .into_iter()
            .map(|(url, git_ref)| PluginMarketplaceRepoToml {
                url: url.to_string(),
                git_ref: git_ref.map(ToString::to_string),
            })
            .collect(),
    }
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
        .unwrap_or_else(std::sync::PoisonError::into_inner)
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

#[test]
fn s_from_plugins_list_enters_sources_list_mode() {
    let root = abs("/tmp");
    let shared_state = make_shared_state_ready(vec![root.clone()], Vec::new());
    let (tx, _rx) = mpsc::channel();
    let app_event_tx = AppEventSender::new(tx);
    let mut view = PluginsSettingsView::new(shared_state, vec![root], app_event_tx);

    assert!(view.handle_key_event_direct(KeyEvent::new(
        KeyCode::Char('s'),
        KeyModifiers::NONE
    )));
    assert!(matches!(view.mode, Mode::Sources(SourcesMode::List)));
}

#[test]
fn l_when_remote_sync_needs_auth_opens_accounts_then_login() {
    let root = abs("/tmp");
    let shared_state = make_shared_state_ready(vec![root.clone()], Vec::new());
    {
        let mut state = shared_state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        state.list = PluginsListState::Ready {
            roots: vec![root.clone()],
            marketplaces: Vec::new(),
            marketplace_load_errors: Vec::new(),
            remote_sync_error: Some("chatgpt authentication required to sync remote plugins".to_string()),
            remote_sync_needs_auth: true,
            featured_plugin_ids: Vec::new(),
        };
    }

    let (tx, rx) = mpsc::channel();
    let app_event_tx = AppEventSender::new(tx);
    let mut view = PluginsSettingsView::new(shared_state, vec![root], app_event_tx);

    assert!(view.handle_key_event_direct(KeyEvent::new(
        KeyCode::Char('l'),
        KeyModifiers::NONE
    )));

    match rx.try_recv().expect("OpenSettings") {
        AppEvent::OpenSettings { section } => {
            assert_eq!(section, Some(crate::bottom_pane::SettingsSection::Accounts));
        }
        other => panic!("unexpected event: {other:?}"),
    }

    match rx.try_recv().expect("ShowLoginAccounts") {
        AppEvent::ShowLoginAccounts => {}
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn a_when_remote_sync_needs_auth_opens_accounts_settings() {
    let root = abs("/tmp");
    let shared_state = make_shared_state_ready(vec![root.clone()], Vec::new());
    {
        let mut state = shared_state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        state.list = PluginsListState::Ready {
            roots: vec![root.clone()],
            marketplaces: Vec::new(),
            marketplace_load_errors: Vec::new(),
            remote_sync_error: Some("failed to get auth token for remote plugin sync".to_string()),
            remote_sync_needs_auth: true,
            featured_plugin_ids: Vec::new(),
        };
    }

    let (tx, rx) = mpsc::channel();
    let app_event_tx = AppEventSender::new(tx);
    let mut view = PluginsSettingsView::new(shared_state, vec![root], app_event_tx);

    assert!(view.handle_key_event_direct(KeyEvent::new(
        KeyCode::Char('a'),
        KeyModifiers::NONE
    )));

    match rx.try_recv().expect("OpenSettings") {
        AppEvent::OpenSettings { section } => {
            assert_eq!(section, Some(crate::bottom_pane::SettingsSection::Accounts));
        }
        other => panic!("unexpected event: {other:?}"),
    }
    assert!(rx.try_recv().is_err(), "expected only one event");
}

#[test]
fn l_and_a_are_ignored_when_remote_sync_does_not_need_auth() {
    let root = abs("/tmp");
    let shared_state = make_shared_state_ready(vec![root.clone()], Vec::new());
    {
        let mut state = shared_state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        state.list = PluginsListState::Ready {
            roots: vec![root.clone()],
            marketplaces: Vec::new(),
            marketplace_load_errors: Vec::new(),
            remote_sync_error: Some("curated marketplace sync failed".to_string()),
            remote_sync_needs_auth: false,
            featured_plugin_ids: Vec::new(),
        };
    }

    let (tx, rx) = mpsc::channel();
    let app_event_tx = AppEventSender::new(tx);
    let mut view = PluginsSettingsView::new(shared_state, vec![root], app_event_tx);

    assert!(!view.handle_key_event_direct(KeyEvent::new(
        KeyCode::Char('l'),
        KeyModifiers::NONE
    )));
    assert!(!view.handle_key_event_direct(KeyEvent::new(
        KeyCode::Char('a'),
        KeyModifiers::NONE
    )));
    assert!(rx.try_recv().is_err(), "expected no events");
}

#[test]
fn sources_curated_editor_save_empty_url_clears_curated_fields() {
    let root = abs("/tmp");
    let shared_state = make_shared_state_ready(vec![root.clone()], Vec::new());
    {
        let mut state = shared_state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        state.sources = make_sources(
            Some("https://example.com/curated.git"),
            Some("stable"),
            vec![("https://example.com/repo.git", Some("main"))],
        );
    }

    let (tx, rx) = mpsc::channel();
    let app_event_tx = AppEventSender::new(tx);
    let mut view = PluginsSettingsView::new(shared_state, vec![root.clone()], app_event_tx);

    assert!(view.handle_key_event_direct(KeyEvent::new(
        KeyCode::Char('s'),
        KeyModifiers::NONE
    )));
    assert!(view.handle_key_event_direct(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
    assert!(matches!(
        view.mode,
        Mode::Sources(SourcesMode::EditCurated)
    ));

    view.sources_editor.url_field.set_text("");
    view.sources_editor.ref_field.set_text("should-clear");

    assert!(view.handle_key_event_direct(KeyEvent::new(
        KeyCode::Char('s'),
        KeyModifiers::CONTROL
    )));

    match rx.try_recv().expect("SetPluginMarketplaceSources") {
        AppEvent::SetPluginMarketplaceSources { roots, sources } => {
            assert_eq!(roots, vec![root]);
            assert!(sources.curated_repo_url.is_none());
            assert!(sources.curated_repo_ref.is_none());
            assert_eq!(sources.marketplace_repos.len(), 1);
            assert_eq!(sources.marketplace_repos[0].url, "https://example.com/repo.git");
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn sources_add_repo_flow_emits_set_sources_with_appended_repo() {
    let root = abs("/tmp");
    let shared_state = make_shared_state_ready(vec![root.clone()], Vec::new());

    let (tx, rx) = mpsc::channel();
    let app_event_tx = AppEventSender::new(tx);
    let mut view = PluginsSettingsView::new(shared_state, vec![root.clone()], app_event_tx);

    assert!(view.handle_key_event_direct(KeyEvent::new(
        KeyCode::Char('s'),
        KeyModifiers::NONE
    )));
    assert!(view.handle_key_event_direct(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE)));
    assert!(matches!(
        view.mode,
        Mode::Sources(SourcesMode::EditMarketplaceRepo { index: None })
    ));

    view.sources_editor
        .url_field
        .set_text("https://github.com/acme/marketplace.git");
    view.sources_editor.ref_field.set_text("main");

    assert!(view.handle_key_event_direct(KeyEvent::new(
        KeyCode::Char('s'),
        KeyModifiers::CONTROL
    )));

    match rx.try_recv().expect("SetPluginMarketplaceSources") {
        AppEvent::SetPluginMarketplaceSources { roots, sources } => {
            assert_eq!(roots, vec![root]);
            assert_eq!(sources.marketplace_repos.len(), 1);
            assert_eq!(
                sources.marketplace_repos[0].url,
                "https://github.com/acme/marketplace.git"
            );
            assert_eq!(
                sources.marketplace_repos[0].git_ref.as_deref(),
                Some("main")
            );
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn sources_delete_repo_confirm_flow_emits_set_sources_with_repo_removed() {
    let root = abs("/tmp");
    let shared_state = make_shared_state_ready(vec![root.clone()], Vec::new());
    {
        let mut state = shared_state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        state.sources = make_sources(
            None,
            None,
            vec![
                ("https://example.com/one.git", Some("main")),
                ("https://example.com/two.git", None),
            ],
        );
    }

    let (tx, rx) = mpsc::channel();
    let app_event_tx = AppEventSender::new(tx);
    let mut view = PluginsSettingsView::new(shared_state, vec![root.clone()], app_event_tx);

    assert!(view.handle_key_event_direct(KeyEvent::new(
        KeyCode::Char('s'),
        KeyModifiers::NONE
    )));
    assert!(view.handle_key_event_direct(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)));
    assert!(view.handle_key_event_direct(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)));
    assert!(view.handle_key_event_direct(KeyEvent::new(KeyCode::Delete, KeyModifiers::NONE)));
    assert!(matches!(
        view.mode,
        Mode::Sources(SourcesMode::ConfirmRemoveRepo { index: 0 })
    ));

    assert!(view.handle_key_event_direct(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)));
    assert!(view.handle_key_event_direct(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));

    match rx.try_recv().expect("SetPluginMarketplaceSources") {
        AppEvent::SetPluginMarketplaceSources { roots, sources } => {
            assert_eq!(roots, vec![root]);
            assert_eq!(sources.marketplace_repos.len(), 1);
            assert_eq!(sources.marketplace_repos[0].url, "https://example.com/two.git");
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn sources_list_r_emits_sync_marketplaces_event() {
    let root = abs("/tmp");
    let shared_state = make_shared_state_ready(vec![root.clone()], Vec::new());

    let (tx, rx) = mpsc::channel();
    let app_event_tx = AppEventSender::new(tx);
    let mut view = PluginsSettingsView::new(shared_state, vec![root.clone()], app_event_tx);

    assert!(view.handle_key_event_direct(KeyEvent::new(
        KeyCode::Char('s'),
        KeyModifiers::NONE
    )));
    assert!(view.handle_key_event_direct(KeyEvent::new(
        KeyCode::Char('R'),
        KeyModifiers::NONE
    )));

    match rx.try_recv().expect("SyncPluginMarketplaces") {
        AppEvent::SyncPluginMarketplaces { roots, refresh_list_after } => {
            assert_eq!(roots, vec![root]);
            assert!(refresh_list_after);
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

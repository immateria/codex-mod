use std::collections::HashMap;

use code_app_server_protocol::AppInfo;

use crate::app_event::AppEvent;
use crate::app_event::AppLinkViewParams;
use crate::bottom_pane::SettingsSection;
use crate::components::list_selection_view::ListSelectionView;
use crate::components::list_selection_view::SelectionAction;
use crate::components::list_selection_view::SelectionItem;

use super::AppsDirectoryCacheState;
use super::ChatWidget;

const APPS_PICKER_VIEW_ID: &str = "apps_picker";

impl ChatWidget<'_> {
    pub(crate) fn show_apps_picker(&mut self) {
        let view = match &self.apps_directory_cache {
            AppsDirectoryCacheState::Ready(apps) => self.build_apps_picker_view(apps),
            AppsDirectoryCacheState::Failed(err) => self.build_apps_picker_error_view(err),
            AppsDirectoryCacheState::Loading => self.build_apps_picker_loading_view(),
            AppsDirectoryCacheState::Uninitialized => {
                self.app_event_tx
                    .send(AppEvent::FetchAppsDirectory { force_refetch: false });
                self.apps_directory_cache = AppsDirectoryCacheState::Loading;
                self.build_apps_picker_loading_view()
            }
        };

        self.bottom_pane.show_list_selection(view);
    }

    pub(crate) fn apps_directory_mark_loading(&mut self, force_refetch: bool) {
        let _ = force_refetch;
        self.apps_directory_cache = AppsDirectoryCacheState::Loading;

        if self.bottom_pane.is_list_selection_open(APPS_PICKER_VIEW_ID) {
            self.bottom_pane
                .show_list_selection(self.build_apps_picker_loading_view());
        }
    }

    pub(crate) fn apps_directory_apply_loaded(
        &mut self,
        _force_refetch: bool,
        result: Result<Vec<AppInfo>, String>,
    ) {
        match result {
            Ok(apps) => {
                let merged = self.merge_accessible_apps(apps);
                self.apps_directory_cache = AppsDirectoryCacheState::Ready(merged);
            }
            Err(err) => {
                self.apps_directory_cache = AppsDirectoryCacheState::Failed(err);
            }
        }

        if self.bottom_pane.is_list_selection_open(APPS_PICKER_VIEW_ID) {
            // Refresh the open picker in place.
            self.show_apps_picker();
        }
    }

    pub(crate) fn open_app_link_view(&mut self, params: AppLinkViewParams) {
        let view = crate::bottom_pane::panes::app_link_view::AppLinkView::new(
            params,
            self.app_event_tx.clone(),
        );
        self.bottom_pane.show_app_link_view(view);
    }

    fn merge_accessible_apps(&self, mut directory: Vec<AppInfo>) -> Vec<AppInfo> {
        let active_account_id =
            code_core::apps_sources::active_chatgpt_account_id(&self.config.code_home)
                .unwrap_or_default();
        let effective_source_ids = code_core::apps_sources::effective_source_account_ids(
            &self.config.apps_sources,
            active_account_id.as_deref(),
        );

        let mut accessible: HashMap<String, (String, Option<String>)> = HashMap::new();
        for source_id in &effective_source_ids {
            for app in self.apps_connected_apps_from_mcp_snapshot(source_id) {
                accessible
                    .entry(app.id)
                    .or_insert_with(|| (app.name, app.description));
            }
        }

        let mut present_ids = std::collections::HashSet::new();
        for connector in &mut directory {
            if accessible.contains_key(&connector.id) {
                connector.is_accessible = true;
            }
            present_ids.insert(connector.id.clone());
        }

        for (id, (name, description)) in accessible {
            if present_ids.contains(&id) {
                continue;
            }
            let install_url = {
                let synthetic = AppInfo {
                    id: id.clone(),
                    name: name.clone(),
                    description: description.clone(),
                    logo_url: None,
                    logo_url_dark: None,
                    distribution_channel: None,
                    branding: None,
                    app_metadata: None,
                    labels: None,
                    install_url: None,
                    is_accessible: true,
                    is_enabled: true,
                    plugin_display_names: Vec::new(),
                };
                let slug = code_connectors::connector_mention_slug(&synthetic);
                format!("https://chatgpt.com/apps/{slug}/{id}")
            };

            directory.push(AppInfo {
                id,
                name,
                description,
                logo_url: None,
                logo_url_dark: None,
                distribution_channel: None,
                branding: None,
                app_metadata: None,
                labels: None,
                install_url: Some(install_url),
                is_accessible: true,
                is_enabled: true,
                plugin_display_names: Vec::new(),
            });
        }

        directory.sort_by(|left, right| {
            right
                .is_accessible
                .cmp(&left.is_accessible)
                .then_with(|| left.name.cmp(&right.name))
                .then_with(|| left.id.cmp(&right.id))
        });

        directory
    }

    fn build_apps_picker_loading_view(&self) -> ListSelectionView {
        ListSelectionView::new_with_id(
            APPS_PICKER_VIEW_ID,
            "Apps".to_string(),
            Some("Loading apps directory...".to_string()),
            Some("Esc close".to_string()),
            vec![SelectionItem {
                name: "Loading...".to_string(),
                description: None,
                is_current: false,
                actions: Vec::new(),
            }],
            self.app_event_tx.clone(),
            12,
        )
    }

    fn build_apps_picker_error_view(&self, error: &str) -> ListSelectionView {
        let mut items = Vec::new();
        items.push(SelectionItem {
            name: "Retry".to_string(),
            description: Some("Fetch the apps directory again.".to_string()),
            is_current: false,
            actions: vec![Box::new(|tx| {
                tx.send(AppEvent::FetchAppsDirectory { force_refetch: true });
            })],
        });
        items.push(SelectionItem {
            name: "Open Apps settings".to_string(),
            description: Some("Manage connector-source accounts.".to_string()),
            is_current: false,
            actions: vec![Box::new(|tx| {
                tx.send(AppEvent::OpenSettings {
                    section: Some(SettingsSection::Apps),
                });
            })],
        });

        ListSelectionView::new_with_id(
            APPS_PICKER_VIEW_ID,
            "Apps".to_string(),
            Some(format!("Failed to load apps directory: {error}")),
            Some("Enter select • Esc close".to_string()),
            items,
            self.app_event_tx.clone(),
            12,
        )
    }

    fn build_apps_picker_view(&self, apps: &[AppInfo]) -> ListSelectionView {
        let installed_count = apps
            .iter()
            .filter(|app| app.is_accessible && app.is_enabled)
            .count();
        let total = apps.len();
        let subtitle = format!(
            "Use $ to insert an installed app into your prompt.\nInstalled {installed_count} of {total} available apps."
        );

        let mut items = Vec::with_capacity(apps.len().saturating_add(1));
        items.push(SelectionItem {
            name: "Open Apps settings".to_string(),
            description: Some("Pin connector-source accounts.".to_string()),
            is_current: false,
            actions: vec![Box::new(|tx| {
                tx.send(AppEvent::OpenSettings {
                    section: Some(SettingsSection::Apps),
                });
            })],
        });

        for app in apps {
            let label = code_connectors::connector_display_label(app);
            let marker = if app.is_accessible && app.is_enabled { 'x' } else { ' ' };
            let name = format!("[{marker}] {label}");
            let description = app.description.clone();

            let actions: Vec<SelectionAction> = if app.is_accessible && app.is_enabled {
                let slug = code_connectors::connector_mention_slug(app);
                let insert = format!("${slug}");
                vec![Box::new(move |tx| {
                    tx.send(AppEvent::InsertText { text: insert.clone() });
                })]
            } else {
                let params = AppLinkViewParams { app: app.clone() };
                vec![Box::new(move |tx| {
                    tx.send(AppEvent::ShowAppLinkView {
                        params: params.clone(),
                    });
                })]
            };

            items.push(SelectionItem {
                name,
                description,
                is_current: false,
                actions,
            });
        }

        ListSelectionView::new_with_id(
            APPS_PICKER_VIEW_ID,
            "Apps".to_string(),
            Some(subtitle),
            Some("Enter select • Esc close".to_string()),
            items,
            self.app_event_tx.clone(),
            16,
        )
    }
}

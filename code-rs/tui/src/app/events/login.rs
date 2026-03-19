use code_login::AuthMode;

use super::super::state::{App, AppState};
use crate::app_event::AppEvent;

impl App<'_> {
    pub(super) fn handle_login_mode_change(&mut self, using_chatgpt_auth: bool) {
        self.config.using_chatgpt_auth = using_chatgpt_auth;
        if let AppState::Chat { widget } = &mut self.app_state {
            widget.set_using_chatgpt_auth(using_chatgpt_auth);
            let _ = widget.reload_auth();
        }

        self.spawn_remote_model_discovery();
    }

    pub(super) fn spawn_remote_model_discovery(&self) {
        if crate::chatwidget::is_test_mode() {
            return;
        }
        let remote_tx = self.app_event_tx.clone();
        let remote_auth_manager = self._server.auth_manager();
        let remote_provider = self.config.model_provider.clone();
        let remote_code_home = self.config.code_home.clone();
        let remote_using_chatgpt_hint = self.config.using_chatgpt_auth;
        tokio::spawn(async move {
            let remote_manager = code_core::remote_models::RemoteModelsManager::new(
                remote_auth_manager.clone(),
                remote_provider,
                remote_code_home,
            );
            remote_manager.refresh_remote_models().await;
            let remote_models = remote_manager.remote_models_snapshot().await;
            if remote_models.is_empty() {
                return;
            }

            let auth_mode = remote_auth_manager
                .auth()
                .map(|auth| auth.mode)
                .or({
                    if remote_using_chatgpt_hint {
                        Some(AuthMode::ChatGPT)
                    } else {
                        Some(AuthMode::ApiKey)
                    }
                });
            let supports_pro_only_models = remote_auth_manager.supports_pro_only_models();
            let presets = code_common::model_presets::builtin_model_presets(
                auth_mode,
                supports_pro_only_models,
            );
            let presets = crate::remote_model_presets::merge_remote_models(
                remote_models,
                presets,
                auth_mode,
                supports_pro_only_models,
            );
            let default_model = remote_manager.default_model_slug(auth_mode).await;
            remote_tx.send(AppEvent::ModelPresetsUpdated {
                presets,
                default_model,
            });
        });
    }
}

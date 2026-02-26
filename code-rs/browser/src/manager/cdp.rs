use crate::BrowserError;
use crate::Result;
use serde_json::Value;

use super::BrowserManager;

impl BrowserManager {
    /// Execute an arbitrary CDP command against the active page session.
    pub async fn execute_cdp(&self, method: &str, params: Value) -> Result<Value> {
        let page = self.get_or_create_page().await?;
        page.execute_cdp_raw(method, params).await
    }

    /// Execute an arbitrary CDP command at the browser (no session) scope.
    pub async fn execute_cdp_browser(&self, method: &str, params: Value) -> Result<Value> {
        // Ensure a browser is connected.
        self.ensure_browser().await?;
        let browser_guard = self.browser.lock().await;
        let browser = browser_guard
            .as_ref()
            .ok_or_else(|| BrowserError::CdpError("Browser not available".to_string()))?;

        // Local raw command type (serialize only params).
        #[derive(Debug, Clone)]
        struct RawCdpCommandBrowser {
            method: String,
            params: Value,
        }

        impl serde::Serialize for RawCdpCommandBrowser {
            fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                self.params.serialize(serializer)
            }
        }

        impl chromiumoxide_types::Method for RawCdpCommandBrowser {
            fn identifier(&self) -> chromiumoxide_types::MethodId {
                self.method.clone().into()
            }
        }

        impl chromiumoxide_types::Command for RawCdpCommandBrowser {
            type Response = Value;
        }

        let cmd = RawCdpCommandBrowser {
            method: method.to_string(),
            params,
        };
        let resp = browser.execute(cmd).await?;
        Ok(resp.result)
    }
}


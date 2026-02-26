use crate::Result;

use super::BrowserManager;

impl BrowserManager {
    /// Clean up injected artifacts and restore viewport/state where possible.
    /// This does not close the browser; it is safe to call when connected.
    pub async fn cleanup(&self) -> Result<()> {
        // Hide any overlay highlight.
        let _ = self
            .execute_cdp("Overlay.hideHighlight", serde_json::json!({}))
            .await;

        // Reset device metrics override (best-effort).
        let _ = self
            .execute_cdp("Emulation.clearDeviceMetricsOverride", serde_json::json!({}))
            .await;

        // Remove virtual cursor and related overlays if present.
        let page = self.get_or_create_page().await?;
        let cleanup_js = r#"
            (function(){
                try { if (window.__vc && typeof window.__vc.destroy === 'function') window.__vc.destroy(); } catch(_) {}
                try { if (window.__code_console_logs) delete window.__code_console_logs; } catch(_) {}
                return true;
            })()
        "#;
        let _ = page.inject_js(cleanup_js).await;
        Ok(())
    }
}


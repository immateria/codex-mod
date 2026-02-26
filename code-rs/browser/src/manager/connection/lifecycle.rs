use crate::Result;
use std::sync::Arc;
use tokio::time::Duration;
use tokio::time::Instant;
use tokio::time::sleep;
use tracing::info;
use tracing::warn;

use super::super::BrowserManager;

impl BrowserManager {
    pub async fn stop(&self) -> Result<()> {
        self.stop_idle_monitor().await;

        // stop event handler task cleanly
        if let Some(task) = self.event_task.lock().await.take() {
            task.abort();
        }

        self.stop_navigation_monitor().await;

        let mut page_guard = self.page.lock().await;
        *page_guard = None;

        // Also cleanup the background page
        let mut background_page_guard = self.background_page.lock().await;
        *background_page_guard = None;

        let config = self.config.read().await;
        let is_external_chrome = config.connect_port.is_some() || config.connect_ws.is_some();
        drop(config);

        let mut browser_guard = self.browser.lock().await;
        if let Some(mut browser) = browser_guard.take() {
            if is_external_chrome {
                info!("Disconnecting from external Chrome (not closing it)");
                // Just drop the connection, don't close the browser
            } else {
                info!("Stopping browser we launched");
                browser.close().await?;
            }
        }

        // When cleaning profiles, respect the flag everywhere:
        let should_cleanup = *self.cleanup_profile_on_drop.lock().await;
        if should_cleanup {
            let mut user_data_guard = self.user_data_dir.lock().await;
            if let Some(user_data_path) = user_data_guard.take() {
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                let _ = tokio::fs::remove_dir_all(&user_data_path).await;
                #[cfg(target_os = "macos")]
                {
                    let _ = tokio::process::Command::new("rm")
                        .arg("-rf")
                        .arg(&user_data_path)
                        .output()
                        .await;
                }
            }
        }

        Ok(())
    }

    pub(in super::super) async fn ensure_browser(&self) -> Result<()> {
        let mut browser_guard = self.browser.lock().await;

        // Check if we have a browser instance
        if let Some(browser) = browser_guard.as_ref() {
            // Try to verify it's still connected with a simple operation
            let check_result = tokio::time::timeout(Duration::from_secs(2), browser.version()).await;

            match check_result {
                Ok(Ok(_)) => {
                    // Browser is responsive
                    return Ok(());
                }
                Ok(Err(e)) => {
                    warn!("Browser check failed: {}, will restart", e);
                    *browser_guard = None;
                }
                Err(_) => {
                    warn!("Browser check timed out, likely disconnected. Will restart");
                    *browser_guard = None;
                }
            }
        }

        // Need to start or restart the browser
        drop(browser_guard);
        info!("Starting/restarting browser connection...");
        self.start().await?;
        Ok(())
    }

    pub(in super::super) async fn update_activity(&self) {
        let mut last_activity = self.last_activity.lock().await;
        *last_activity = Instant::now();
    }

    pub(in super::super) async fn start_idle_monitor(&self) {
        let config = self.config.read().await;
        let idle_timeout = Duration::from_millis(config.idle_timeout_ms);
        let is_external_chrome = config.connect_port.is_some() || config.connect_ws.is_some();
        let should_cleanup = *self.cleanup_profile_on_drop.lock().await; // <-- respect this
        drop(config);

        if is_external_chrome {
            info!("Skipping idle monitor for external Chrome connection");
            return;
        }

        let browser = Arc::clone(&self.browser);
        let last_activity = Arc::clone(&self.last_activity);
        let user_data_dir = Arc::clone(&self.user_data_dir);

        let handle = tokio::spawn(async move {
            loop {
                sleep(Duration::from_secs(10)).await;
                let last = *last_activity.lock().await;
                if last.elapsed() > idle_timeout {
                    warn!("Browser idle timeout reached, closing");
                    let mut browser_guard = browser.lock().await;
                    if let Some(mut browser) = browser_guard.take() {
                        let _ = browser.close().await;
                    }
                    if should_cleanup
                        && let Some(user_data_path) = user_data_dir.lock().await.take()
                    {
                        let _ = tokio::fs::remove_dir_all(&user_data_path).await;
                    }
                    break;
                }
            }
        });

        *self.idle_monitor_handle.lock().await = Some(handle);
    }

    pub(in super::super) async fn stop_idle_monitor(&self) {
        let mut handle_guard = self.idle_monitor_handle.lock().await;
        if let Some(handle) = handle_guard.take() {
            handle.abort();
        }
    }

    pub async fn close(&self) -> Result<()> {
        // Just delegate to stop() which handles cleanup properly
        self.stop().await
    }
}

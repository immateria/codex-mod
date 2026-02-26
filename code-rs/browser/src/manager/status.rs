use crate::Result;
use crate::config::BrowserConfig;
use tokio::time::Duration;

use super::BrowserManager;
use super::BrowserStatus;

impl BrowserManager {
    pub async fn is_enabled(&self) -> bool {
        self.config.read().await.enabled
    }

    pub fn is_enabled_sync(&self) -> bool {
        self.config.try_read().map(|c| c.enabled).unwrap_or(false)
    }

    /// Returns how long the browser has been idle when it exceeds the configured timeout.
    /// Used to decide whether we should avoid taking fresh screenshots (which would reset
    /// the idle timer) until the user interacts with browser_* tools again.
    pub async fn idle_elapsed_past_timeout(&self) -> Option<(Duration, Duration)> {
        let idle_timeout = Duration::from_millis(self.config.read().await.idle_timeout_ms);
        let last = *self.last_activity.lock().await;
        let elapsed = last.elapsed();
        if elapsed > idle_timeout {
            Some((elapsed, idle_timeout))
        } else {
            None
        }
    }

    /// Get a description of the browser connection type
    pub async fn get_browser_type(&self) -> String {
        let config = self.config.read().await;
        if config.connect_ws.is_some() || config.connect_port.is_some() {
            "CDP-connected to user's Chrome browser".to_string()
        } else if config.headless {
            "internal headless Chrome browser".to_string()
        } else {
            "internal Chrome browser (headed mode)".to_string()
        }
    }

    pub async fn set_enabled(&self, enabled: bool) -> Result<()> {
        let mut config = self.config.write().await;
        config.enabled = enabled;

        if enabled {
            self.start().await?;
        } else {
            self.stop().await?;
        }

        Ok(())
    }

    pub async fn update_config(&self, updates: impl FnOnce(&mut BrowserConfig)) -> Result<()> {
        let mut config = self.config.write().await;
        updates(&mut config);

        if let Some(page) = self.page.lock().await.as_ref() {
            // Avoid viewport manipulation for external CDP connections to prevent focus/flicker
            let is_external = config.connect_port.is_some() || config.connect_ws.is_some();
            if !is_external {
                page.update_viewport(config.viewport.clone()).await?;
            }
        }

        Ok(())
    }

    pub async fn get_config(&self) -> BrowserConfig {
        self.config.read().await.clone()
    }

    pub async fn get_current_url(&self) -> Option<String> {
        let page_guard = self.page.lock().await;
        if let Some(page) = page_guard.as_ref() {
            page.get_current_url().await.ok()
        } else {
            None
        }
    }

    pub async fn get_status(&self) -> BrowserStatus {
        let config = self.config.read().await;
        let browser_active = self.browser.lock().await.is_some();
        let current_url = self.get_current_url().await;

        BrowserStatus {
            enabled: config.enabled,
            browser_active,
            current_url,
            viewport: config.viewport.clone(),
            fullpage: config.fullpage,
        }
    }

    pub fn set_enabled_sync(&self, enabled: bool) {
        // Try to set immediately if possible, otherwise spawn a task
        if let Ok(mut cfg) = self.config.try_write() {
            cfg.enabled = enabled;
        } else {
            let config = self.config.clone();
            tokio::spawn(async move {
                let mut cfg = config.write().await;
                cfg.enabled = enabled;
            });
        }
    }

    pub fn set_fullpage_sync(&self, fullpage: bool) {
        if let Ok(mut cfg) = self.config.try_write() {
            cfg.fullpage = fullpage;
        } else {
            let config = self.config.clone();
            tokio::spawn(async move {
                let mut cfg = config.write().await;
                cfg.fullpage = fullpage;
            });
        }
    }

    pub fn set_viewport_sync(&self, width: u32, height: u32) {
        if let Ok(mut cfg) = self.config.try_write() {
            cfg.viewport.width = width;
            cfg.viewport.height = height;
        } else {
            let config = self.config.clone();
            tokio::spawn(async move {
                let mut cfg = config.write().await;
                cfg.viewport.width = width;
                cfg.viewport.height = height;
            });
        }
    }

    pub fn set_segments_max_sync(&self, segments_max: usize) {
        if let Ok(mut cfg) = self.config.try_write() {
            cfg.segments_max = segments_max;
        } else {
            let config = self.config.clone();
            tokio::spawn(async move {
                let mut cfg = config.write().await;
                cfg.segments_max = segments_max;
            });
        }
    }

    pub fn get_status_sync(&self) -> String {
        // Use try operations to avoid blocking - return cached/default values if locks are held
        let cfg = self
            .config
            .try_read()
            .map(|c| {
                let enabled = c.enabled;
                let viewport_width = c.viewport.width;
                let viewport_height = c.viewport.height;
                let fullpage = c.fullpage;
                (enabled, viewport_width, viewport_height, fullpage)
            })
            .unwrap_or((false, 1024, 768, false));

        let browser_active = self
            .browser
            .try_lock()
            .map(|b| b.is_some())
            .unwrap_or(false);

        let mode = if cfg.0 { "enabled" } else { "disabled" };
        let fullpage = if cfg.3 { "on" } else { "off" };

        let mut status = format!(
            "Browser status:\n• Mode: {}\n• Viewport: {}×{}\n• Full-page: {}",
            mode, cfg.1, cfg.2, fullpage
        );

        if browser_active {
            status.push_str("\n• Browser: active");
        }

        status
    }
}


use crate::BrowserError;
use crate::Result;
use std::sync::Arc;
use tokio::time::Duration;
use tracing::warn;

use super::BrowserManager;

impl BrowserManager {
    pub async fn capture_screenshot_with_url(
        &self,
    ) -> Result<(Vec<std::path::PathBuf>, Option<String>)> {
        let (paths, url) = self.capture_screenshot_internal().await?;
        Ok((paths, Some(url)))
    }

    pub async fn capture_screenshot(&self) -> Result<Vec<std::path::PathBuf>> {
        let (paths, _) = self.capture_screenshot_internal().await?;
        Ok(paths)
    }

    pub async fn capture_screenshot_mode(
        &self,
        mode: crate::page::ScreenshotMode,
    ) -> Result<(Vec<std::path::PathBuf>, String)> {
        self.capture_screenshot_with_mode(mode).await
    }

    async fn capture_screenshot_internal(&self) -> Result<(Vec<std::path::PathBuf>, String)> {
        // Always capture from the active page; do not create background tabs.
        self.capture_screenshot_regular().await
    }

    /// Capture screenshot using regular strategy (launched Chrome).
    async fn capture_screenshot_regular(&self) -> Result<(Vec<std::path::PathBuf>, String)> {
        // Determine screenshot mode from config.
        let config = self.config.read().await;
        let mode = if config.fullpage {
            crate::page::ScreenshotMode::FullPage {
                segments_max: Some(config.segments_max),
            }
        } else {
            crate::page::ScreenshotMode::Viewport
        };
        drop(config);

        self.capture_screenshot_with_mode(mode).await
    }

    async fn capture_screenshot_with_mode(
        &self,
        mode: crate::page::ScreenshotMode,
    ) -> Result<(Vec<std::path::PathBuf>, String)> {
        // For launched Chrome, use the regular approach since it's already isolated.
        let page = self.get_or_create_page().await?;

        // Viewport correction is handled inside Page::screenshot for all connections.

        // Initialize assets manager if needed.
        let mut assets_guard = self.assets.lock().await;
        if assets_guard.is_none() {
            *assets_guard = Some(Arc::new(crate::assets::AssetManager::new().await?));
        }
        let assets = assets_guard.as_ref().cloned().ok_or_else(|| {
            BrowserError::AssetError("assets manager failed to initialize".to_string())
        })?;
        drop(assets_guard);

        // Get current URL with timeout.
        let current_url = match tokio::time::timeout(Duration::from_secs(3), page.get_current_url())
            .await
        {
            Ok(Ok(url)) => url,
            Ok(Err(_)) | Err(_) => {
                warn!("Failed to get current URL, using default");
                "about:blank".to_string()
            }
        };

        // Capture screenshots with timeout.
        let screenshot_result = tokio::time::timeout(
            Duration::from_secs(15), // Allow up to 15 seconds for screenshot.
            page.screenshot(mode),
        )
        .await;

        let screenshots = match screenshot_result {
            Ok(Ok(shots)) => shots,
            Ok(Err(e)) => {
                return Err(BrowserError::ScreenshotError(format!(
                    "Screenshot capture failed: {e}"
                )));
            }
            Err(_) => {
                return Err(BrowserError::ScreenshotError(
                    "Screenshot capture timed out after 15 seconds".to_string(),
                ));
            }
        };

        // Store screenshots and get paths.
        let mut paths = Vec::new();
        for screenshot in screenshots {
            let image_ref = assets
                .store_screenshot(
                    &screenshot.data,
                    screenshot.format,
                    screenshot.width,
                    screenshot.height,
                    Self::SCREENSHOT_TTL_MS,
                )
                .await?;
            paths.push(std::path::PathBuf::from(image_ref.path));
        }

        self.update_activity().await;
        Ok((paths, current_url))
    }
}


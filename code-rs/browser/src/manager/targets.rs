use crate::BrowserError;
use crate::Result;
use crate::page::Page;
use std::sync::Arc;
use tokio::time::Duration;
use tracing::debug;
use tracing::warn;

use super::BrowserManager;
use super::BrowserTargetSummary;
use super::page_ops;

pub(super) fn is_controllable_target_url(url: &str) -> bool {
    let lu = url.to_ascii_lowercase();
    if lu.starts_with("chrome://")
        || lu.starts_with("devtools://")
        || lu.starts_with("edge://")
        || lu.starts_with("chrome-extension://")
        || lu.starts_with("brave://")
        || lu.starts_with("vivaldi://")
        || lu.starts_with("opera://")
    {
        return false;
    }
    lu.starts_with("http://")
        || lu.starts_with("https://")
        || lu.starts_with("file://")
        || lu == "about:blank"
}

impl BrowserManager {
    pub async fn list_page_targets(&self) -> Result<Vec<BrowserTargetSummary>> {
        let active_target_id = self.page.lock().await.as_ref().map(|page| page.target_id());

        let mut browser_guard = self.browser.lock().await;
        let browser = browser_guard.as_mut().ok_or(BrowserError::NotInitialized)?;

        let fetch =
            tokio::time::timeout(Duration::from_millis(1200), browser.fetch_targets()).await;
        let targets = match fetch {
            Ok(Ok(targets)) => targets,
            Ok(Err(err)) => return Err(BrowserError::CdpError(err.to_string())),
            Err(_) => {
                return Err(BrowserError::CdpError(
                    "Timed out fetching browser targets".to_string(),
                ))
            }
        };

        let mut summaries = Vec::new();
        for (idx, target) in targets
            .into_iter()
            .filter(|target| target.r#type == "page")
            .enumerate()
        {
            let target_id = target.target_id.inner().clone();
            summaries.push(BrowserTargetSummary {
                index: idx + 1,
                target_id: target_id.clone(),
                title: target.title,
                url: target.url.clone(),
                attached: target.attached,
                opener_id: target.opener_id.map(|id| id.inner().clone()),
                subtype: target.subtype,
                controllable: is_controllable_target_url(&target.url),
                active: active_target_id.as_deref() == Some(target_id.as_str()),
            });
        }

        Ok(summaries)
    }

    pub async fn new_tab(&self, url: &str) -> Result<String> {
        let url = url.trim();
        let url = if url.is_empty() { "about:blank" } else { url };
        if !is_controllable_target_url(url) {
            return Err(BrowserError::ConfigError(format!(
                "Refusing to open uncontrollable URL in new tab: {url}"
            )));
        }

        self.ensure_browser().await?;
        self.update_activity().await;

        let browser_guard = self.browser.lock().await;
        let browser = browser_guard.as_ref().ok_or(BrowserError::NotInitialized)?;
        let cdp_page = browser.new_page(url).await?;
        drop(browser_guard);

        self.apply_page_overrides(&cdp_page).await?;
        let config = self.config.read().await.clone();

        let page = Arc::new(Page::new(cdp_page, config));
        let target_id = page.target_id();
        {
            let mut page_guard = self.page.lock().await;
            *page_guard = Some(Arc::clone(&page));
        }

        debug!("Injecting virtual cursor for new tab");
        if let Err(e) = page.inject_virtual_cursor().await {
            warn!("Failed to inject virtual cursor for new tab: {e}");
        }

        // Ensure console capture is installed for the current document (matches get_or_create_page).
        page_ops::install_console_capture(&page, "for new tab").await;

        self.start_navigation_monitor(Arc::clone(&page)).await;
        self.start_viewport_monitor(Arc::clone(&page)).await;
        self.set_auto_viewport_correction(false).await;
        Ok(target_id)
    }

    pub async fn switch_to_target(&self, target_id: &str) -> Result<()> {
        self.ensure_browser().await?;
        self.update_activity().await;

        let browser_guard = self.browser.lock().await;
        let browser = browser_guard.as_ref().ok_or(BrowserError::NotInitialized)?;

        let mut pages = browser.pages().await?;
        if pages.is_empty() {
            for _ in 0..10 {
                tokio::time::sleep(Duration::from_millis(50)).await;
                pages = browser.pages().await?;
                if !pages.is_empty() {
                    break;
                }
            }
        }

        let mut selected: Option<chromiumoxide::page::Page> = None;
        for page in pages {
            if page.target_id().inner() == target_id {
                selected = Some(page);
                break;
            }
        }
        drop(browser_guard);

        let Some(cdp_page) = selected else {
            return Err(BrowserError::CdpError(format!(
                "No page found for target_id {target_id}"
            )));
        };

        // Enforce the same basic "controllable tab" rule as get_or_create_page.
        if let Ok(Ok(Some(url))) =
            tokio::time::timeout(Duration::from_millis(250), cdp_page.url()).await
            && !is_controllable_target_url(&url)
        {
            return Err(BrowserError::CdpError(format!(
                "Target {target_id} is not controllable ({url}); choose an http/https/file/about:blank tab"
            )));
        }

        self.apply_page_overrides(&cdp_page).await?;
        let config = self.config.read().await.clone();

        let page = Arc::new(Page::new(cdp_page, config));
        {
            let mut page_guard = self.page.lock().await;
            *page_guard = Some(Arc::clone(&page));
        }

        debug!("Injecting virtual cursor for switched page");
        if let Err(e) = page.inject_virtual_cursor().await {
            warn!("Failed to inject virtual cursor after switching target: {e}");
        }

        page_ops::install_console_capture(&page, "after switching target").await;

        self.start_navigation_monitor(Arc::clone(&page)).await;
        self.start_viewport_monitor(Arc::clone(&page)).await;
        self.set_auto_viewport_correction(false).await;
        Ok(())
    }

    pub async fn close_target(&self, target_id: &str) -> Result<()> {
        self.ensure_browser().await?;
        self.update_activity().await;

        let active_target_id = self.page.lock().await.as_ref().map(|page| page.target_id());

        let browser_guard = self.browser.lock().await;
        let browser = browser_guard.as_ref().ok_or(BrowserError::NotInitialized)?;
        let pages = browser.pages().await?;
        drop(browser_guard);

        let mut closed = false;
        for page in pages {
            if page.target_id().inner() == target_id {
                page.close().await?;
                closed = true;
                break;
            }
        }

        if !closed {
            return Err(BrowserError::CdpError(format!(
                "No page found for target_id {target_id}"
            )));
        }

        if active_target_id.as_deref() == Some(target_id) {
            let mut page_guard = self.page.lock().await;
            *page_guard = None;
            drop(page_guard);
            self.stop_navigation_monitor().await;
            self.stop_viewport_monitor().await;
        }

        Ok(())
    }

    pub async fn activate_target(&self, target_id: &str) -> Result<()> {
        let _ = self
            .execute_cdp_browser(
                "Target.activateTarget",
                serde_json::json!({ "targetId": target_id }),
            )
            .await?;
        Ok(())
    }

    pub async fn close_page(&self) -> Result<()> {
        let mut page_guard = self.page.lock().await;
        if let Some(page) = page_guard.take() {
            page.close().await?;
        }
        Ok(())
    }
}


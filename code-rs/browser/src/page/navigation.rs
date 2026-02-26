use super::{GotoResult, Page};
use super::ReadyStateTarget;
use super::url_looks_loaded;
use super::wait_ready_state;

use crate::BrowserError;
use crate::Result;
use crate::config::WaitStrategy;
use std::time::Duration;
use tracing::debug;
use tracing::info;
use tracing::warn;

impl Page {
    /// Returns the current page title, if available.
    pub async fn get_title(&self) -> Option<String> {
        self.cdp_page.get_title().await.ok().flatten()
    }

    pub async fn goto(&self, url: &str, wait: Option<WaitStrategy>) -> Result<GotoResult> {
        info!("Navigating to {}", url);

        let wait_strategy = wait.unwrap_or_else(|| self.config.wait.clone());

        // Navigate to the URL with retry on timeout. If Chrome reports timeouts
        // but the page URL actually updates to a real http(s) page, treat it as success.
        let max_retries = 3;
        let mut last_error = None;
        let mut fallback_navigated = false;

        for attempt in 1..=max_retries {
            // Wrap CDP navigation with a short timeout so we don't block ~30s
            // for sites that load but don't signal expected events.
            let nav_attempt =
                tokio::time::timeout(tokio::time::Duration::from_secs(5), self.cdp_page.goto(url))
                    .await;

            match nav_attempt {
                Ok(Ok(_)) => {
                    // Navigation reported success
                    last_error = None;
                    break;
                }
                Ok(Err(e)) => {
                    let error_str = e.to_string();
                    if error_str.contains("Request timed out") || error_str.contains("timeout") {
                        warn!(
                            "Navigation timeout on attempt {}/{}: {}",
                            attempt, max_retries, error_str
                        );
                        last_error = Some(e);

                        // Check if the page actually navigated despite the timeout
                        if let Ok(cur_opt) = self.cdp_page.url().await
                            && let Some(cur) = cur_opt {
                                let looks_loaded =
                                    cur.starts_with("http://") || cur.starts_with("https://");
                                if looks_loaded && cur != "about:blank" {
                                    info!(
                                        "Navigation reported timeout, but page URL is now {} — treating as success",
                                        cur
                                    );
                                    fallback_navigated = true;
                                    last_error = None;
                                    break;
                                }
                            }

                        if attempt < max_retries {
                            // Wait before retry, increasing delay each time
                            let delay_ms = 1000 * attempt as u64;
                            info!("Retrying navigation after {}ms...", delay_ms);
                            tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
                            continue;
                        }
                    } else {
                        // Non-timeout error, fail immediately
                        return Err(e.into());
                    }
                }
                Err(_) => {
                    // Our outer timeout fired; fallback to URL check, same as above.
                    warn!(
                        "Navigation attempt {}/{} exceeded 5s timeout; checking current URL...",
                        attempt, max_retries
                    );
                    // Check if the page actually navigated despite the timeout
                    if let Ok(cur_opt) = self.cdp_page.url().await
                        && let Some(cur) = cur_opt {
                            let looks_loaded =
                                cur.starts_with("http://") || cur.starts_with("https://");
                            if looks_loaded && cur != "about:blank" {
                                info!(
                                    "Navigation exceeded timeout, but page URL is now {} — treating as success",
                                    cur
                                );
                                fallback_navigated = true;
                                last_error = None;
                                break;
                            }
                        }

                    if attempt < max_retries {
                        let delay_ms = 1000 * attempt as u64;
                        info!("Retrying navigation after {}ms...", delay_ms);
                        tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
                        continue;
                    }
                    // If this was the last attempt, return a synthetic timeout error
                    return Err(BrowserError::CdpError("Navigation timed out".to_string()));
                }
            }
        }

        // If we exhausted retries and still have an error, bail out
        if let Some(e) = last_error {
            return Err(BrowserError::CdpError(format!(
                "Navigation failed after {max_retries} retries: {e}"
            )));
        }

        // Wait according to the strategy
        match wait_strategy {
            WaitStrategy::Event(event) => match event.as_str() {
                "domcontentloaded" => {
                    if fallback_navigated {
                        // Poll document.readyState instead of wait_for_navigation()
                        wait_ready_state(
                            &self.cdp_page,
                            ReadyStateTarget::InteractiveOrComplete,
                            Duration::from_secs(3),
                            Duration::from_millis(100),
                        )
                        .await;
                    } else {
                        // Wait for DOMContentLoaded event
                        let wait_timeout = Duration::from_secs(4);
                        match tokio::time::timeout(
                            wait_timeout,
                            self.cdp_page.wait_for_navigation(),
                        )
                        .await
                        {
                            Ok(Ok(_)) => {}
                            Ok(Err(e)) => {
                                warn!(
                                    "DOMContentLoaded wait failed after {:?}: {}",
                                    wait_timeout, e
                                );
                                wait_ready_state(
                                    &self.cdp_page,
                                    ReadyStateTarget::InteractiveOrComplete,
                                    Duration::from_secs(3),
                                    Duration::from_millis(100),
                                )
                                .await;
                                if !url_looks_loaded(&self.cdp_page, Duration::from_millis(400)).await
                                {
                                    return Err(BrowserError::CdpError(e.to_string()));
                                }
                            }
                            Err(_) => {
                                warn!("DOMContentLoaded wait timed out after {:?}", wait_timeout);
                                wait_ready_state(
                                    &self.cdp_page,
                                    ReadyStateTarget::InteractiveOrComplete,
                                    Duration::from_secs(3),
                                    Duration::from_millis(100),
                                )
                                .await;
                                if !url_looks_loaded(&self.cdp_page, Duration::from_millis(400)).await
                                {
                                    return Err(BrowserError::CdpError(format!(
                                        "DOMContentLoaded wait timed out after {wait_timeout:?}"
                                    )));
                                }
                            }
                        }
                    }
                }
                "networkidle" | "networkidle0" => {
                    // Wait for network to be idle
                    tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
                }
                "networkidle2" => {
                    // Wait for network to be mostly idle
                    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                }
                "load" => {
                    if fallback_navigated {
                        // Poll for complete state
                        wait_ready_state(
                            &self.cdp_page,
                            ReadyStateTarget::Complete,
                            Duration::from_secs(4),
                            Duration::from_millis(120),
                        )
                        .await;
                        // Small cushion after load
                        tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;
                    } else {
                        // Wait for load event
                        let wait_timeout = Duration::from_secs(5);
                        match tokio::time::timeout(
                            wait_timeout,
                            self.cdp_page.wait_for_navigation(),
                        )
                        .await
                        {
                            Ok(Ok(_)) => {}
                            Ok(Err(e)) => {
                                warn!("Load wait failed after {:?}: {}", wait_timeout, e);
                                wait_ready_state(
                                    &self.cdp_page,
                                    ReadyStateTarget::Complete,
                                    Duration::from_secs(4),
                                    Duration::from_millis(120),
                                )
                                .await;
                                if !url_looks_loaded(&self.cdp_page, Duration::from_millis(400)).await
                                {
                                    return Err(BrowserError::CdpError(e.to_string()));
                                }
                            }
                            Err(_) => {
                                warn!("Load wait timed out after {:?}", wait_timeout);
                                wait_ready_state(
                                    &self.cdp_page,
                                    ReadyStateTarget::Complete,
                                    Duration::from_secs(4),
                                    Duration::from_millis(120),
                                )
                                .await;
                                if !url_looks_loaded(&self.cdp_page, Duration::from_millis(400)).await
                                {
                                    return Err(BrowserError::CdpError(format!(
                                        "Load wait timed out after {wait_timeout:?}"
                                    )));
                                }
                            }
                        }
                        // Add extra delay to ensure page is fully loaded
                        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                    }
                }
                _ => {
                    return Err(BrowserError::ConfigError(format!(
                        "Unknown wait event: {event}"
                    )));
                }
            },
            WaitStrategy::Delay { delay_ms } => {
                tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
            }
        }

        // Get the final URL and title after navigation completes
        let title = self.cdp_page.get_title().await.ok().flatten();

        // Try to get the URL multiple times in case it's not immediately available
        let mut final_url = None;
        for _ in 0..3 {
            if let Ok(Some(url)) = self.cdp_page.url().await {
                final_url = Some(url);
                break;
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }

        let final_url = final_url.unwrap_or_else(|| url.to_string());

        let mut current_url = self.current_url.write().await;
        *current_url = Some(final_url.clone());
        drop(current_url); // Release lock before injecting cursor

        // Ensure the virtual cursor after navigation
        debug!("Ensuring virtual cursor after navigation");
        if let Err(e) = self.ensure_virtual_cursor().await {
            warn!("Failed to inject virtual cursor after navigation: {}", e);
            // Continue even if cursor injection fails
        }

        Ok(GotoResult {
            url: final_url,
            title,
        })
    }

    pub async fn get_url(&self) -> Result<String> {
        let url_guard = self.current_url.read().await;
        url_guard.clone().ok_or(BrowserError::PageNotLoaded)
    }

    /// Get the current URL directly from the browser (not cached)
    pub async fn get_current_url(&self) -> Result<String> {
        match self.cdp_page.url().await? {
            Some(url) => Ok(url),
            None => Err(BrowserError::PageNotLoaded),
        }
    }

    /// Scroll the page by the given delta in pixels
    pub async fn scroll_by(&self, dx: f64, dy: f64) -> Result<()> {
        debug!("Scrolling by ({}, {})", dx, dy);
        let js = format!(
            "(function() {{ window.scrollBy({dx}, {dy}); return {{ x: window.scrollX, y: window.scrollY }}; }})()"
        );
        let _ = self.execute_javascript(&js).await?;
        Ok(())
    }

    /// Navigate browser history backward one entry
    pub async fn go_back(&self) -> Result<()> {
        debug!("History back");
        let _ = self.execute_javascript("history.back();").await?;
        Ok(())
    }

    /// Navigate browser history forward one entry
    pub async fn go_forward(&self) -> Result<()> {
        debug!("History forward");
        let _ = self.execute_javascript("history.forward();").await?;
        Ok(())
    }

}

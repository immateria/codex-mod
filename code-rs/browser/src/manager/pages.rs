use crate::BrowserError;
use crate::Result;
use crate::page::Page;
use chromiumoxide::cdp::browser_protocol::emulation;
use chromiumoxide::cdp::browser_protocol::network;
use std::sync::Arc;
use tokio::time::Duration;
use tokio::time::Instant;
use tracing::debug;
use tracing::info;
use tracing::warn;

use super::BrowserManager;
use super::page_ops;
use super::targets;

impl BrowserManager {
    pub async fn get_or_create_page(&self) -> Result<Arc<Page>> {
        let overall_start = Instant::now();
        info!("[bm] get_or_create_page: begin");
        self.ensure_browser().await?;
        info!("[bm] get_or_create_page: ensure_browser in {:?}", overall_start.elapsed());
        self.update_activity().await;

        let mut page_guard = self.page.lock().await;
        if let Some(page) = page_guard.as_ref() {
            // Verify the page is still responsive
            let check_result =
                tokio::time::timeout(Duration::from_secs(2), page.get_current_url()).await;

            match check_result {
                Ok(Ok(_)) => {
                    // Page is responsive
                    info!(
                        "[bm] get_or_create_page: reused responsive page in {:?}",
                        overall_start.elapsed()
                    );
                    return Ok(Arc::clone(page));
                }
                Ok(Err(e)) => {
                    warn!("Existing page returned error: {}, will create new page", e);
                    *page_guard = None;
                }
                Err(_) => {
                    // Timeout checking URL; prefer to reuse instead of re-applying overrides repeatedly
                    warn!("Existing page timed out checking URL; reusing current page to avoid churn");
                    return Ok(Arc::clone(page));
                }
            }
        }

        let browser_guard = self.browser.lock().await;
        let browser = browser_guard.as_ref().ok_or(BrowserError::NotInitialized)?;

        let config = self.config.read().await;

        // If we're connected to an existing Chrome (via connect_port or connect_ws),
        // try to use the current active tab instead of creating a new one
        let cdp_page = if config.connect_port.is_some() || config.connect_ws.is_some() {
            info!("[bm] get_or_create_page: selecting an existing tab");
            // Try to get existing pages
            let mut pages = browser.pages().await?;
            if pages.is_empty() {
                // brief retry loop to allow targets to populate
                for _ in 0..10 {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    pages = browser.pages().await?;
                    if !pages.is_empty() {
                        break;
                    }
                }
            }

            if !pages.is_empty() {
                // Try to find the active/visible tab
                // We'll check each page to see if it's visible/focused
                let mut active_page = None; // focused && visible
                let mut first_visible: Option<chromiumoxide::page::Page> = None; // visible
                let mut last_allowed: Option<chromiumoxide::page::Page> = None; // allowed regardless of visibility

                for page in &pages {
                    // Quick URL check first to skip uninjectable pages
                    let url = match tokio::time::timeout(Duration::from_millis(200), page.url()).await
                    {
                        Ok(Ok(Some(u))) => u,
                        _ => "unknown".to_string(),
                    };
                    if !targets::is_controllable_target_url(&url) {
                        debug!("Skipping uncontrollable tab: {}", url);
                        continue;
                    } else {
                        last_allowed = Some(page.clone());
                    }
                    // Evaluate visibility/focus of the tab. We avoid focus listeners since they won't fire when attaching.
                    let eval = page.evaluate(
                        "(() => {\n"
                            .to_string()
                            + "  return {\n"
                            + "    visible: document.visibilityState === 'visible',\n"
                            + "    focused: (document.hasFocus && document.hasFocus()) || false,\n"
                            + "    url: String(window.location.href || '')\n"
                            + "  };\n"
                            + "})()",
                    );
                    // Guard against hung targets by timing out quickly
                    let is_visible = tokio::time::timeout(Duration::from_millis(300), eval).await;

                    if let Ok(Ok(result)) = is_visible {
                        if let Ok(obj) = result.into_value::<serde_json::Value>() {
                            let visible = obj
                                .get("visible")
                                .and_then(serde_json::Value::as_bool)
                                .unwrap_or(false);
                            let focused = obj
                                .get("focused")
                                .and_then(serde_json::Value::as_bool)
                                .unwrap_or(false);
                            let url = obj.get("url").and_then(|v| v.as_str()).unwrap_or("unknown");

                            debug!(
                                "Tab check - URL: {}, Visible: {}, Focused: {}",
                                url, visible, focused
                            );

                            // Selection heuristic (revised to avoid minimized windows):
                            // 1) Focused AND visible wins immediately.
                            // 2) Otherwise, remember the first visible tab.
                            // 3) Otherwise, fallback to the last allowed tab.
                            if focused && visible {
                                info!("Found focused & visible tab: {}", url);
                                active_page = Some(page.clone());
                                break;
                            } else if focused && !visible {
                                info!("Focused but not visible (likely minimized): skipping {}", url);
                            } else if visible && first_visible.is_none() {
                                info!("Found visible tab: {}", url);
                                first_visible = Some(page.clone());
                            }
                        } else {
                            debug!("Tab visibility check returned non-JSON; skipping");
                        }
                    } else {
                        debug!("Tab visibility check timed out or failed; skipping unresponsive tab");
                    }
                }

                // Use focused & visible if found, else first visible, else last allowed
                if let Some(page) = active_page {
                    info!("Using active/visible Chrome tab");
                    page
                } else if let Some(page) = first_visible {
                    info!("Using first visible Chrome tab");
                    page
                } else if let Some(p) = last_allowed {
                    info!("No active tab found, using last allowed tab");
                    p
                } else {
                    // No allowed pages at all, create an about:blank tab
                    warn!("No controllable tabs found; creating about:blank");
                    browser.new_page("about:blank").await?
                }
            } else {
                // No existing tabs found. Do NOT create a new tab for external Chrome if avoidable.
                info!("No existing tabs found; waiting briefly for targets");
                tokio::time::sleep(Duration::from_millis(200)).await;
                let mut pages2 = browser.pages().await?;
                if let Some(page) = pages2.pop() {
                    page
                } else {
                    // As a last resort, still create a tab, but log it clearly
                    warn!("Creating a new about:blank tab because none were available");
                    browser.new_page("about:blank").await?
                }
            }
        } else {
            // We launched Chrome ourselves, create a new page
            info!("[bm] get_or_create_page: creating new about:blank tab");
            browser.new_page("about:blank").await?
        };

        // Apply page overrides (UA, locale, timezone, viewport, etc.)
        let overrides_start = Instant::now();
        self.apply_page_overrides(&cdp_page).await?;
        info!("[bm] get_or_create_page: overrides in {:?}", overrides_start.elapsed());

        let page = Arc::new(Page::new(cdp_page, config.clone()));
        *page_guard = Some(Arc::clone(&page));

        // Inject the virtual cursor when page is created
        debug!("Injecting virtual cursor for new page");
        if let Err(e) = page.inject_virtual_cursor().await {
            warn!("Failed to inject virtual cursor on page creation: {}", e);
            // Continue even if cursor injection fails
        }

        // Ensure console capture is installed immediately for the current document.
        // Without this, connecting to an already-loaded tab would only register
        // the bootstrap for future documents, and an initial Browser Console read
        // would return no logs. This eagerly hooks console methods now.
        page_ops::install_console_capture(&page, "on page creation").await;

        // Start navigation monitoring for this page
        self.start_navigation_monitor(Arc::clone(&page)).await;
        // Start viewport monitor (low-frequency, non-invasive)
        self.start_viewport_monitor(Arc::clone(&page)).await;
        // TEMP: disable auto-corrections post-initial set to validate no unintended resizes
        // This affects both external and internal; explicit browser.setViewport still works
        self.set_auto_viewport_correction(false).await;
        info!(
            "[bm] get_or_create_page: complete in {:?}",
            overall_start.elapsed()
        );

        Ok(page)
    }

    /// Apply environment overrides on page creation.
    /// - For external CDP connections: set viewport once on connect; skip humanization (UA, locale, etc.).
    /// - For internal (launched) Chrome: apply humanization; skip viewport here (kept minimal).
    pub async fn apply_page_overrides(&self, page: &chromiumoxide::Page) -> Result<()> {
        let config = self.config.read().await;
        let is_external = config.connect_port.is_some() || config.connect_ws.is_some();

        // Always enable Network domain once
        page.execute(network::EnableParams::default()).await?;

        if is_external {
            // External Chrome: set viewport once on connection; skip humanization.
            let w = config.viewport.width as i64;
            let h = config.viewport.height as i64;
            let dpr = config.viewport.device_scale_factor;
            let mob = config.viewport.mobile;

            // Skip redundant overrides within a short window to prevent flash
            {
                let guard = self.last_metrics_applied.lock().await;
                if let Some((lw, lh, ldpr, lmob, ts)) = *guard {
                    let same = lw == w && lh == h && (ldpr - dpr).abs() < 0.001 && lmob == mob;
                    let recent = ts.elapsed() < std::time::Duration::from_secs(30);
                    if same && recent {
                        debug!("Skipping redundant device metrics override (external, recent)");
                        return Ok(());
                    }
                }
            }

            let viewport_params = emulation::SetDeviceMetricsOverrideParams::builder()
                .width(w)
                .height(h)
                .device_scale_factor(dpr)
                .mobile(mob)
                .build()
                .map_err(BrowserError::CdpError)?;
            info!(
                "Applying external device metrics override: {}x{} @ {} (mobile={})",
                w, h, dpr, mob
            );
            page.execute(viewport_params).await?;
            let mut guard = self.last_metrics_applied.lock().await;
            *guard = Some((w, h, dpr, mob, std::time::Instant::now()));
        } else {
            // Internal (launched) Chrome: apply human settings; avoid CDP viewport override here
            if let Some(ua) = &config.user_agent {
                let mut b = network::SetUserAgentOverrideParams::builder().user_agent(ua);
                if let Some(al) = &config.accept_language {
                    b = b.accept_language(al);
                }
                page.execute(b.build().map_err(BrowserError::CdpError)?)
                    .await?;
            }

            let mut headers_map = std::collections::HashMap::new();
            if config.user_agent.is_none() && let Some(al) = &config.accept_language {
                headers_map.insert(
                    "Accept-Language".to_string(),
                    serde_json::Value::String(al.clone()),
                );
            }
            if let Some(proxy_authorization) = &config.proxy_authorization {
                headers_map.insert(
                    "Proxy-Authorization".to_string(),
                    serde_json::Value::String(proxy_authorization.clone()),
                );
            }
            if !headers_map.is_empty() {
                let headers = network::Headers::new(serde_json::Value::Object(
                    headers_map.into_iter().collect(),
                ));
                let p = network::SetExtraHttpHeadersParams::builder()
                    .headers(headers)
                    .build()
                    .map_err(BrowserError::CdpError)?;
                page.execute(p).await?;
            }

            if let Some(tz) = &config.timezone {
                page.execute(emulation::SetTimezoneOverrideParams {
                    timezone_id: tz.clone(),
                })
                .await?;
            }
            if let Some(locale) = &config.locale {
                let p = emulation::SetLocaleOverrideParams::builder()
                    .locale(locale)
                    .build();
                page.execute(p).await?;
            }
        }

        Ok(())
    }
}


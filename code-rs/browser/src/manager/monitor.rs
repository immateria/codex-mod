use crate::page::Page;
use std::sync::Arc;
use tokio::time::Duration;
use tracing::debug;
use tracing::info;
use tracing::warn;

use super::BrowserManager;

impl BrowserManager {
    /// Get the current viewport dimensions.
    pub async fn get_viewport_size(&self) -> (u32, u32) {
        let config = self.config.read().await;
        (config.viewport.width, config.viewport.height)
    }

    /// Set a callback to be called when navigation occurs.
    pub async fn set_navigation_callback<F>(&self, callback: F)
    where
        F: Fn(String) + Send + Sync + 'static,
    {
        let mut callback_guard = self.navigation_callback.write().await;
        *callback_guard = Some(Box::new(callback));
    }

    /// Start monitoring for page navigation changes.
    pub(super) async fn start_navigation_monitor(&self, page: Arc<Page>) {
        // Stop any existing monitor.
        self.stop_navigation_monitor().await;

        let navigation_callback = Arc::clone(&self.navigation_callback);
        let page_target_id = page.target_id_debug();
        let page_session_id = page.session_id_debug();
        let page_weak = Arc::downgrade(&page);

        let assets_arc = Arc::clone(&self.assets);
        let config_arc = Arc::clone(&self.config);
        let handle = tokio::spawn(async move {
            let mut last_url = String::new();
            let mut last_seq: u64 = 0;
            let mut _check_count = 0; // reserved for future periodic checks

            debug!(
                target_id = %page_target_id,
                session_id = %page_session_id,
                "Starting navigation monitor"
            );

            loop {
                // Check if page is still alive.
                let page = match page_weak.upgrade() {
                    Some(p) => p,
                    None => {
                        debug!(
                            target_id = %page_target_id,
                            session_id = %page_session_id,
                            "Page dropped, stopping navigation monitor"
                        );
                        break;
                    }
                };

                // Get current URL.
                if let Ok(current_url) = page.get_current_url().await {
                    // Check if URL changed (ignore about:blank).
                    if current_url != last_url && current_url != "about:blank" {
                        info!(
                            "Navigation detected: {} -> {}",
                            if last_url.is_empty() {
                                "initial"
                            } else {
                                &last_url
                            },
                            current_url
                        );
                        last_url = current_url.clone();

                        // Call the callback if set (immediate).
                        if let Some(ref callback) = *navigation_callback.read().await {
                            debug!("Triggering navigation callback for URL: {}", current_url);
                            callback(current_url.clone());
                        }

                        // Schedule a delayed callback for fully loaded page.
                        let navigation_callback_delayed = Arc::clone(&navigation_callback);
                        let current_url_delayed = current_url.clone();
                        tokio::spawn(async move {
                            // Wait for page to fully load.
                            tokio::time::sleep(tokio::time::Duration::from_millis(2000)).await;

                            // Call the callback again with a marker that it's fully loaded.
                            if let Some(ref callback) = *navigation_callback_delayed.read().await {
                                info!("Page fully loaded callback for: {}", current_url_delayed);
                                callback(current_url_delayed);
                            }
                        });
                    }
                }

                // Listen for SPA changes via codex:locationchange (only when attached to external Chrome).
                let cfg_now = config_arc.read().await.clone();
                if cfg_now.connect_port.is_some() || cfg_now.connect_ws.is_some() {
                    // Install listener once and poll sequence counter.
                    let listener_script = r#"
                        (function(){
                          try {
                            if (!window.__code_nav_listening) {
                              window.__code_nav_listening = true;
                              window.__code_nav_seq = 0;
                              window.__code_nav_url = String(location.href || '');
                              window.addEventListener('codex:locationchange', function(){
                                window.__code_nav_seq += 1;
                                window.__code_nav_url = String(location.href || '');
                              }, { capture: true });
                            }
                            return { seq: Number(window.__code_nav_seq||0), url: String(window.__code_nav_url||location.href) };
                          } catch (e) { return { seq: 0, url: String(location.href||'') }; }
                        })()
                    "#;

                    if let Ok(result) = page.execute_javascript(listener_script).await {
                        let seq = result
                            .get("seq")
                            .and_then(serde_json::Value::as_u64)
                            .unwrap_or(0);
                        let url = result
                            .get("url")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();

                        if seq > last_seq {
                            info!(
                                "SPA locationchange detected: {} (seq {} -> {})",
                                url, last_seq, seq
                            );
                            last_seq = seq;

                            // Fire callback.
                            if let Some(ref callback) = *navigation_callback.read().await {
                                callback(url.clone());
                            }

                            // Capture a screenshot asynchronously.
                            let assets_arc2 = Arc::clone(&assets_arc);
                            let config_arc2 = Arc::clone(&config_arc);
                            let page_for_shot = Arc::clone(&page);
                            tokio::spawn(async move {
                                // Initialize assets manager if needed.
                                if assets_arc2.lock().await.is_none()
                                    && let Ok(am) = crate::assets::AssetManager::new().await
                                {
                                    *assets_arc2.lock().await = Some(Arc::new(am));
                                }
                                let assets_opt = assets_arc2.lock().await.clone();
                                drop(assets_arc2);
                                if let Some(assets) = assets_opt {
                                    let cfg = config_arc2.read().await.clone();
                                    let mode = if cfg.fullpage {
                                        crate::page::ScreenshotMode::FullPage {
                                            segments_max: Some(cfg.segments_max),
                                        }
                                    } else {
                                        crate::page::ScreenshotMode::Viewport
                                    };
                                    // Small delay to allow SPA content to render.
                                    tokio::time::sleep(Duration::from_millis(400)).await;
                                    if let Ok(shots) = page_for_shot.screenshot(mode).await {
                                        for s in shots {
                                            let _ = assets
                                                .store_screenshot(
                                                    &s.data,
                                                    s.format,
                                                    s.width,
                                                    s.height,
                                                    Self::SCREENSHOT_TTL_MS,
                                                )
                                                .await;
                                        }
                                    }
                                }
                            });
                        }
                    }
                }

                // Periodic counter disabled; listener-based SPA detection in place.

                // Check every 500ms.
                tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
            }
        });

        let mut handle_guard = self.navigation_monitor_handle.lock().await;
        *handle_guard = Some(handle);
    }

    /// Stop navigation monitoring.
    pub(super) async fn stop_navigation_monitor(&self) {
        let mut handle_guard = self.navigation_monitor_handle.lock().await;
        if let Some(handle) = handle_guard.take() {
            handle.abort();
        }
    }

    /// Start a low-frequency viewport monitor that checks for drift without forcing resyncs.
    /// Applies the same logic to internal and external: only correct after two consecutive
    /// mismatches and at most once per minute to avoid jank. Logs when throttled.
    pub(super) async fn start_viewport_monitor(&self, page: Arc<Page>) {
        // Stop any existing monitor first.
        self.stop_viewport_monitor().await;

        let config_arc = Arc::clone(&self.config);
        let correction_enabled = Arc::clone(&self.auto_viewport_correction_enabled);
        let handle = tokio::spawn(async move {
            let mut consecutive_mismatch = 0u32;
            let mut last_warn: Option<std::time::Instant> = None;
            let mut last_correction: Option<std::time::Instant> = None;
            let check_interval = std::time::Duration::from_secs(60);
            let warn_interval = std::time::Duration::from_secs(300);
            let min_correction_interval = std::time::Duration::from_secs(60);

            loop {
                tokio::time::sleep(check_interval).await;

                // Snapshot expected config.
                let cfg = config_arc.read().await.clone();
                let is_external = cfg.connect_port.is_some() || cfg.connect_ws.is_some();
                let expected_w = cfg.viewport.width as f64;
                let expected_h = cfg.viewport.height as f64;
                let expected_dpr = cfg.viewport.device_scale_factor;

                // Probe current viewport via JS (cheap and non-invasive).
                let probe_js = r#"(() => ({
                    w: (document.documentElement.clientWidth|0),
                    h: (document.documentElement.clientHeight|0),
                    dpr: (window.devicePixelRatio||1)
                }))()"#;

                if let Ok(val) = page.inject_js(probe_js).await {
                    let cw = val
                        .get("w")
                        .and_then(serde_json::Value::as_u64)
                        .unwrap_or(0) as f64;
                    let ch = val
                        .get("h")
                        .and_then(serde_json::Value::as_u64)
                        .unwrap_or(0) as f64;
                    let cdpr = val
                        .get("dpr")
                        .and_then(serde_json::Value::as_f64)
                        .unwrap_or(1.0);

                    let w_ok = (cw - expected_w).abs() <= 5.0;
                    let h_ok = (ch - expected_h).abs() <= 5.0;
                    let dpr_ok = (cdpr - expected_dpr).abs() <= 0.05;
                    let mismatch = !(w_ok && h_ok && dpr_ok);

                    if mismatch {
                        consecutive_mismatch += 1;
                        let now = std::time::Instant::now();
                        let can_correct = last_correction
                            .map(|t| now.duration_since(t) >= min_correction_interval)
                            .unwrap_or(true);

                        // Check gate: allow disabling auto-corrections at runtime.
                        let enabled = *correction_enabled.read().await;
                        if consecutive_mismatch >= 2 && can_correct && enabled {
                            info!(
                                "Correcting viewport: {}x{}@{} -> {}x{}@{} (external={})",
                                cw, ch, cdpr, expected_w, expected_h, expected_dpr, is_external
                            );
                            let _ = page
                                .set_viewport(crate::page::SetViewportParams {
                                    width: cfg.viewport.width,
                                    height: cfg.viewport.height,
                                    device_scale_factor: Some(cfg.viewport.device_scale_factor),
                                    mobile: Some(cfg.viewport.mobile),
                                })
                                .await;
                            last_correction = Some(now);
                            consecutive_mismatch = 0;
                        } else {
                            // Throttled: log at most every 5 minutes.
                            let should_warn = last_warn
                                .map(|t| now.duration_since(t) >= warn_interval)
                                .unwrap_or(true);
                            if should_warn {
                                warn!(
                                    "Viewport drift detected (throttled): {}x{}@{} vs expected {}x{}@{} (external={}, can_correct={})",
                                    cw, ch, cdpr, expected_w, expected_h, expected_dpr, is_external, can_correct
                                );
                                last_warn = Some(now);
                            }
                        }
                    } else {
                        consecutive_mismatch = 0;
                    }
                }
            }
        });

        *self.viewport_monitor_handle.lock().await = Some(handle);
    }

    pub(super) async fn stop_viewport_monitor(&self) {
        let mut handle_guard = self.viewport_monitor_handle.lock().await;
        if let Some(handle) = handle_guard.take() {
            handle.abort();
        }
    }

    /// Temporarily enable/disable automatic viewport correction (monitor-driven).
    pub async fn set_auto_viewport_correction(&self, enabled: bool) {
        let mut guard = self.auto_viewport_correction_enabled.write().await;
        *guard = enabled;
    }
}

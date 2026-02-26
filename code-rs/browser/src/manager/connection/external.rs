use crate::BrowserError;
use crate::Result;
use chromiumoxide::Browser;
use futures::StreamExt;
use tokio::time::Duration;
use tokio::time::sleep;
use tracing::info;
use tracing::warn;

use crate::global;

use super::super::BrowserManager;
use super::discover_ws_via_host_port;
use super::scan_for_chrome_debug_port;
use super::should_stop_handler;

impl BrowserManager {
    /// Try to connect to Chrome via CDP only - no fallback to internal browser
    pub async fn connect_to_chrome_only(&self) -> Result<()> {
        tracing::info!("[cdp/bm] connect_to_chrome_only: begin");
        // Quick check without holding the lock during IO
        if self.browser.lock().await.is_some() {
            tracing::info!("[cdp/bm] already connected; early return");
            return Ok(());
        }

        let config = self.config.read().await.clone();
        tracing::info!(
            "[cdp/bm] config: connect_host={:?}, connect_port={:?}, connect_ws={:?}",
            config.connect_host,
            config.connect_port,
            config.connect_ws
        );

        // If a WebSocket is configured explicitly, try that first
        if let Some(ws) = config.connect_ws.clone() {
            info!("[cdp/bm] Connecting to Chrome via configured WebSocket: {}", ws);
            let attempt_timeout = Duration::from_millis(config.connect_attempt_timeout_ms);
            let attempts = std::cmp::max(1, config.connect_attempts as i32);
            let mut last_err: Option<String> = None;

            for attempt in 1..=attempts {
                info!(
                    "[cdp/bm] WS connect attempt {}/{} (timeout={}ms)",
                    attempt,
                    attempts,
                    attempt_timeout.as_millis()
                );
                let ws_clone = ws.clone();
                let handle = tokio::spawn(async move { Browser::connect(ws_clone).await });
                match tokio::time::timeout(attempt_timeout, handle).await {
                    Ok(Ok(Ok((browser, mut handler)))) => {
                        info!("[cdp/bm] WS connect attempt {} succeeded", attempt);

                        // Start event handler loop
                        let browser_arc = self.browser.clone();
                        let page_arc = self.page.clone();
                        let background_page_arc = self.background_page.clone();
                        let task = tokio::spawn(async move {
                            let mut consecutive_errors = 0u32;
                            while let Some(result) = handler.next().await {
                                if should_stop_handler("[cdp/bm]", result, &mut consecutive_errors) {
                                    break;
                                }
                            }
                            warn!("[cdp/bm] event handler ended; clearing browser state so it can restart");
                            *browser_arc.lock().await = None;
                            *page_arc.lock().await = None;
                            *background_page_arc.lock().await = None;
                        });
                        *self.event_task.lock().await = Some(task);

                        {
                            let mut guard = self.browser.lock().await;
                            *guard = Some(browser);
                        }
                        *self.cleanup_profile_on_drop.lock().await = false;

                        // Fire-and-forget targets warmup
                        {
                            let browser_arc = self.browser.clone();
                            tokio::spawn(async move {
                                if let Some(browser) = browser_arc.lock().await.as_mut() {
                                    let _ = tokio::time::timeout(
                                        Duration::from_millis(100),
                                        browser.fetch_targets(),
                                    )
                                    .await;
                                }
                            });
                        }

                        self.start_idle_monitor().await;
                        self.update_activity().await;
                        // Cache last connection (ws only)
                        global::set_last_connection(None, Some(ws.clone())).await;
                        return Ok(());
                    }
                    Ok(Ok(Err(e))) => {
                        let msg = format!("CDP WebSocket connect failed: {e}");
                        warn!("[cdp/bm] {}", msg);
                        last_err = Some(msg);
                    }
                    Ok(Err(join_err)) => {
                        let msg = format!("Join error during connect attempt: {join_err}");
                        warn!("[cdp/bm] {}", msg);
                        last_err = Some(msg);
                    }
                    Err(_) => {
                        warn!(
                            "[cdp/bm] WS connect attempt {} timed out after {}ms; aborting attempt",
                            attempt,
                            attempt_timeout.as_millis()
                        );
                    }
                }
                sleep(Duration::from_millis(200)).await;
            }

            let base = "CDP WebSocket connect failed after all attempts".to_string();
            let msg = if let Some(e) = last_err {
                format!("{base}: {e}")
            } else {
                base
            };
            return Err(BrowserError::CdpError(msg));
        }

        // Only try CDP connection via port, no fallback
        if let Some(port) = config.connect_port {
            let host = config.connect_host.as_deref().unwrap_or("127.0.0.1");
            let actual_port = if port == 0 {
                info!("Auto-scanning for Chrome debug ports...");
                let start = tokio::time::Instant::now();
                let result = scan_for_chrome_debug_port().await.unwrap_or(0);
                info!(
                    "Auto-scan completed in {:?}, found port: {}",
                    start.elapsed(),
                    result
                );
                result
            } else {
                info!("[cdp/bm] Using specified Chrome debug port: {}", port);
                port
            };

            if actual_port > 0 {
                info!(
                    "[cdp/bm] Discovering Chrome WebSocket URL via {}:{}...",
                    host, actual_port
                );
                // Retry discovery for up to 15s to allow a freshly launched Chrome to initialize
                let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
                let ws = loop {
                    let discover_start = tokio::time::Instant::now();
                    match discover_ws_via_host_port(host, actual_port).await {
                        Ok(ws) => {
                            info!(
                                "[cdp/bm] WS discovered in {:?}: {}",
                                discover_start.elapsed(),
                                ws
                            );
                            break ws;
                        }
                        Err(e) => {
                            if tokio::time::Instant::now() >= deadline {
                                return Err(BrowserError::CdpError(format!(
                                    "Failed to discover Chrome WebSocket on port {actual_port} within 15s: {e}"
                                )));
                            }
                            tokio::time::sleep(Duration::from_millis(300)).await;
                        }
                    }
                };

                info!("[cdp/bm] Connecting to Chrome via WebSocket...");
                let connect_start = tokio::time::Instant::now();

                // Enforce per-attempt timeouts via spawned task to avoid hangs
                let attempt_timeout = Duration::from_millis(config.connect_attempt_timeout_ms);
                let attempts = std::cmp::max(1, config.connect_attempts as i32);
                let mut last_err: Option<String> = None;

                for attempt in 1..=attempts {
                    info!(
                        "[cdp/bm] WS connect attempt {}/{} (timeout={}ms)",
                        attempt,
                        attempts,
                        attempt_timeout.as_millis()
                    );

                    let ws_clone = ws.clone();
                    let handle = tokio::spawn(async move { Browser::connect(ws_clone).await });

                    match tokio::time::timeout(attempt_timeout, handle).await {
                        Ok(Ok(Ok((browser, mut handler)))) => {
                            info!("[cdp/bm] WS connect attempt {} succeeded", attempt);
                            info!(
                                "[cdp/bm] Connected to Chrome in {:?}",
                                connect_start.elapsed()
                            );

                            // Start event handler loop
                            let browser_arc = self.browser.clone();
                            let page_arc = self.page.clone();
                            let background_page_arc = self.background_page.clone();
                            let task = tokio::spawn(async move {
                                let mut consecutive_errors = 0u32;
                                while let Some(result) = handler.next().await {
                                    if should_stop_handler(
                                        "[cdp/bm]",
                                        result,
                                        &mut consecutive_errors,
                                    ) {
                                        break;
                                    }
                                }
                                warn!("[cdp/bm] event handler ended; clearing browser state so it can restart");
                                *browser_arc.lock().await = None;
                                *page_arc.lock().await = None;
                                *background_page_arc.lock().await = None;
                            });
                            *self.event_task.lock().await = Some(task);

                            // Install browser
                            {
                                let mut guard = self.browser.lock().await;
                                *guard = Some(browser);
                            }
                            *self.cleanup_profile_on_drop.lock().await = false;

                            // Fire-and-forget targets warmup after browser is installed
                            {
                                let browser_arc = self.browser.clone();
                                tokio::spawn(async move {
                                    if let Some(browser) = browser_arc.lock().await.as_mut() {
                                        let _ = tokio::time::timeout(
                                            Duration::from_millis(100),
                                            browser.fetch_targets(),
                                        )
                                        .await;
                                    }
                                });
                            }

                            self.start_idle_monitor().await;
                            self.update_activity().await;
                            // Update last connection cache
                            global::set_last_connection(Some(actual_port), Some(ws.clone())).await;
                            return Ok(());
                        }
                        Ok(Ok(Err(e))) => {
                            let msg = format!("CDP WebSocket connect failed: {e}");
                            warn!("[cdp/bm] {}", msg);
                            last_err = Some(msg);
                        }
                        Ok(Err(join_err)) => {
                            let msg = format!("Join error during connect attempt: {join_err}");
                            warn!("[cdp/bm] {}", msg);
                            last_err = Some(msg);
                        }
                        Err(_) => {
                            warn!(
                                "[cdp/bm] WS connect attempt {} timed out after {}ms; aborting attempt",
                                attempt,
                                attempt_timeout.as_millis()
                            );
                            // Best-effort abort; if connect is internally blocking, it may keep a worker thread busy,
                            // but our caller remains responsive and we can retry.
                            // We cannot await the handle here without risking another stall.
                        }
                    }

                    // Small backoff between attempts
                    sleep(Duration::from_millis(200)).await;
                }

                let base = "CDP WebSocket connect failed after all attempts".to_string();
                let msg = if let Some(e) = last_err {
                    format!("{base}: {e}")
                } else {
                    base
                };
                Err(BrowserError::CdpError(msg))
            } else {
                Err(BrowserError::CdpError(
                    "No Chrome instance found with debug port".to_string(),
                ))
            }
        } else {
            Err(BrowserError::CdpError(
                "No CDP port configured for Chrome connection".to_string(),
            ))
        }
    }
}

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;

use super::args::ChromeCommandArgs;
use super::args::choose_cdp_connect_target;
use super::args::parse_chrome_command_args;
use super::screenshots::finish_cdp_connection;
use super::screenshots::send_browser_screenshot_update;
use super::screenshots::set_latest_screenshot;
use super::super::{BackgroundOrderTicket, ChatWidget};

impl ChatWidget<'_> {
    pub(super) fn connect_to_chrome_after_launch(
        &mut self,
        port: u16,
        ticket: BackgroundOrderTicket,
    ) {
        // Wait a moment for Chrome to start, then reuse the existing connection logic
        let app_event_tx = self.app_event_tx.clone();
        let latest_screenshot = self.latest_browser_screenshot.clone();

        tokio::spawn(async move {
            // Wait for Chrome to fully start
            tokio::time::sleep(super::CHROME_LAUNCH_CONNECT_DELAY).await;

            // Now try to connect using the shared CDP connection logic
            ChatWidget::connect_to_cdp_chrome(
                None,
                Some(port),
                latest_screenshot,
                app_event_tx,
                ticket,
            )
            .await;
        });
    }

    // Shared CDP connection logic used by both /chrome command and Chrome launch options
    async fn connect_to_cdp_chrome(
        host: Option<String>,
        port: Option<u16>,
        latest_screenshot: Arc<Mutex<Option<(PathBuf, String)>>>,
        app_event_tx: AppEventSender,
        ticket: BackgroundOrderTicket,
    ) {
        tracing::info!(
            "[cdp] connect_to_cdp_chrome() begin, host={:?}, port={:?}",
            host,
            port
        );
        let browser_manager = ChatWidget::get_browser_manager().await;
        browser_manager.set_enabled_sync(true);

        // Configure for CDP connection (prefer cached ws/port on auto-detect)
        // Track whether we're attempting via cached WS and retain a cached port for fallback.
        let (cached_port, cached_ws) = if port.is_none() {
            match super::super::read_cached_connection().await {
                Some(v) => v,
                None => code_browser::global::get_last_connection().await,
            }
        } else {
            (None, None)
        };
        let connect_choice = choose_cdp_connect_target(host.clone(), port, cached_port, cached_ws);

        let attempted_via_cached_ws = connect_choice.attempted_via_cached_ws;
        let cached_port_for_fallback = connect_choice.cached_port_for_fallback;

        if port.is_none() {
            if connect_choice.connect_ws.is_some() {
                tracing::info!("[cdp] using cached Chrome WS endpoint");
            } else if let Some(p) = connect_choice.connect_port
                && p != 0
            {
                tracing::info!("[cdp] using cached Chrome debug port: {p}");
            }
        }

        {
            let mut config = browser_manager.config.write().await;
            config.headless = false;
            config.persist_profile = true;
            config.enabled = true;

            config.connect_ws = connect_choice.connect_ws;
            config.connect_host = connect_choice.connect_host;
            config.connect_port = connect_choice.connect_port;
        }

        // Try to connect to existing Chrome (no fallback to internal browser) with timeout
        tracing::info!("[cdp] calling BrowserManager::connect_to_chrome_only()…");
        let connect_result = tokio::time::timeout(
            super::CDP_CONNECT_DEADLINE,
            browser_manager.connect_to_chrome_only(),
        )
        .await;
        match connect_result {
            Err(_) => {
                tracing::error!(
                    "[cdp] connect_to_chrome_only timed out after {:?}",
                    super::CDP_CONNECT_DEADLINE
                );
                app_event_tx.send_background_event_with_ticket(
                    &ticket,
                    format!(
                        "CDP: connect timed out after {}s. Ensure Chrome is running with --remote-debugging-port={} and http://127.0.0.1:{}/json/version is reachable",
                        super::CDP_CONNECT_DEADLINE.as_secs(),
                        port.unwrap_or(0),
                        port.unwrap_or(0)
                    ),
                );
                // Offer launch options popup to help recover quickly
                app_event_tx.send(AppEvent::ShowChromeOptions(port));
            }
            Ok(result) => match result {
                Ok(_) => {
                    tracing::info!("[cdp] Connected to Chrome via CDP");
                    finish_cdp_connection(
                        browser_manager.clone(),
                        latest_screenshot.clone(),
                        &app_event_tx,
                        &ticket,
                    )
                    .await;
                }
                Err(e) => {
                    let err_msg = format!("{e}");
                    // If we attempted via a cached WS, clear it and fallback to port-based discovery once.
                    if attempted_via_cached_ws {
                        tracing::warn!(
                            "[cdp] cached WS connect failed: {err_msg} — clearing WS cache and retrying via port discovery",
                        );
                        let port_to_keep = cached_port_for_fallback;
                        // Clear WS in-memory and on-disk
                        code_browser::global::set_last_connection(port_to_keep, None).await;
                        let _ = super::super::write_cached_connection(port_to_keep, None).await;

                        // Reconfigure to use port (prefer cached port, else auto-detect)
                        {
                            let mut cfg = browser_manager.config.write().await;
                            cfg.connect_ws = None;
                            cfg.connect_host = host.clone();
                            cfg.connect_port = Some(port_to_keep.unwrap_or(0));
                        }

                        tracing::info!("[cdp] retrying connect via port discovery after WS failure…");
                        let retry = tokio::time::timeout(
                            super::CDP_CONNECT_DEADLINE,
                            browser_manager.connect_to_chrome_only(),
                        )
                        .await;
                        match retry {
                            Ok(Ok(_)) => {
                                tracing::info!(
                                    "[cdp] Fallback connect succeeded after clearing cached WS"
                                );
                                finish_cdp_connection(
                                    browser_manager.clone(),
                                    latest_screenshot.clone(),
                                    &app_event_tx,
                                    &ticket,
                                )
                                .await;
                            }
                            Ok(Err(e2)) => {
                                tracing::error!("[cdp] Fallback connect failed: {e2}");
                                app_event_tx.send_background_event_with_ticket(
                                    &ticket,
                                    format!(
                                        "CDP: failed to connect after WS fallback: {e2} (original: {err_msg})"
                                    ),
                                );
                                // Also surface the Chrome launch options UI to assist the user
                                app_event_tx.send(AppEvent::ShowChromeOptions(port));
                            }
                            Err(_) => {
                                tracing::error!(
                                    "[cdp] Fallback connect timed out after {:?}",
                                    super::CDP_CONNECT_DEADLINE
                                );
                                app_event_tx.send_background_event_with_ticket(
                                    &ticket,
                                    format!(
                                        "CDP: connect timed out after {}s during fallback. Ensure Chrome is running with --remote-debugging-port and /json/version is reachable",
                                        super::CDP_CONNECT_DEADLINE.as_secs()
                                    ),
                                );
                                // Also surface the Chrome launch options UI to assist the user
                                app_event_tx.send(AppEvent::ShowChromeOptions(port));
                            }
                        }
                    } else {
                        tracing::error!("[cdp] connect_to_chrome_only failed immediately: {err_msg}",);
                        app_event_tx.send_background_event_with_ticket(
                            &ticket,
                            format!("CDP: failed to connect to Chrome: {err_msg}"),
                        );
                        // Offer launch options popup to help recover quickly
                        app_event_tx.send(AppEvent::ShowChromeOptions(port));
                    }
                }
            },
        }
    }

    #[allow(dead_code)]
    fn switch_to_internal_browser(&mut self) {
        // Switch to internal browser mode
        self.browser_is_external = false;
        let latest_screenshot = self.latest_browser_screenshot.clone();
        let app_event_tx = self.app_event_tx.clone();
        let ticket = self.make_background_tail_ticket();

        tokio::spawn(async move {
            let ticket = ticket;
            let browser_manager = ChatWidget::get_browser_manager().await;

            // First, close any existing Chrome connection
            if browser_manager.is_enabled().await {
                let _ = browser_manager.close().await;
            }

            // Configure for internal browser
            {
                let mut config = browser_manager.config.write().await;
                config.connect_port = None;
                config.connect_ws = None;
                config.headless = true;
                config.persist_profile = false;
                config.enabled = true;
            }

            // Enable internal browser
            browser_manager.set_enabled_sync(true);

            // Explicitly (re)start the internal browser session now
            if let Err(e) = browser_manager.start().await {
                tracing::error!("Failed to start internal browser: {e}");
                app_event_tx.send_background_event_with_ticket(
                    &ticket,
                    format!("Failed to start internal browser: {e}"),
                );
                return;
            }

            // Set as global manager so core/session share the same instance
            code_browser::global::set_global_browser_manager(browser_manager.clone()).await;

            // Notify about successful switch/reconnect
            app_event_tx.send_background_event_with_ticket(
                &ticket,
                "Switched to internal browser mode (reconnected)".to_string(),
            );

            // Clear any existing screenshot
            if let Ok(mut screenshot) = latest_screenshot.lock() {
                *screenshot = None;
            }

            // Proactively navigate to about:blank, then capture a first screenshot to populate HUD
            let _ = browser_manager.goto("about:blank").await;
            // Capture an initial screenshot to populate HUD
            tokio::time::sleep(super::BROWSER_INITIAL_SCREENSHOT_DELAY).await;
            match browser_manager.capture_screenshot_with_url().await {
                Ok((paths, url)) => {
                    if let Some(first_path) = paths.first() {
                        let url_text = url.unwrap_or_else(|| "Browser".to_string());
                        set_latest_screenshot(
                            &latest_screenshot,
                            first_path.clone(),
                            url_text.clone(),
                        );
                        send_browser_screenshot_update(&app_event_tx, first_path.clone(), url_text);
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to capture initial internal browser screenshot: {e}");
                }
            }
        });
    }

    fn handle_chrome_connection(
        &mut self,
        host: Option<String>,
        port: Option<u16>,
        ticket: BackgroundOrderTicket,
    ) {
        tracing::info!(
            "[cdp] handle_chrome_connection begin, host={:?}, port={:?}",
            host,
            port
        );
        self.browser_is_external = true;
        let latest_screenshot = self.latest_browser_screenshot.clone();
        let app_event_tx = self.app_event_tx.clone();
        let port_display = port.map_or("auto-detect".to_string(), |p| p.to_string());
        let host_display = host.clone().unwrap_or_else(|| "127.0.0.1".to_string());

        // Add status message to chat (use BackgroundEvent with header so it renders reliably)
        let status_msg = format!(
            "CDP: connecting to Chrome DevTools Protocol ({host_display}:{port_display})..."
        );
        self.push_background_before_next_output(status_msg);

        // Connect in background with a single, unified flow (no double-connect)
        tokio::spawn(async move {
            tracing::info!(
                "[cdp] connect task spawned, host={:?}, port={:?}",
                host,
                port
            );
            // Unified connect flow; emits success/failure messages internally
            ChatWidget::connect_to_cdp_chrome(
                host,
                port,
                latest_screenshot.clone(),
                app_event_tx.clone(),
                ticket,
            )
            .await;
        });
    }

    pub(crate) fn handle_chrome_command(&mut self, command_text: String) {
        tracing::info!("[cdp] handle_chrome_command start: '{command_text}'");
        let chrome_ticket = self.make_background_tail_ticket();
        self.consume_pending_prompt_for_ui_only_turn();

        // Handle empty command - just "/chrome"
        if command_text.trim().is_empty() {
            tracing::info!("[cdp] no args provided; toggle connect/disconnect");

            // Toggle behavior: if an external Chrome connection is active, disconnect it.
            // Otherwise, start a connection (auto-detect).
            let (tx, rx) = std::sync::mpsc::channel();
            let app_event_tx = self.app_event_tx.clone();
            let ticket = chrome_ticket.clone();
            tokio::spawn(async move {
                let browser_manager = ChatWidget::get_browser_manager().await;
                // Check if we're currently connected to an external Chrome
                let (is_external, browser_active) = {
                    let cfg = browser_manager.config.read().await;
                    let is_external = cfg.connect_port.is_some() || cfg.connect_ws.is_some();
                    drop(cfg);
                    let status = browser_manager.get_status().await;
                    (is_external, status.browser_active)
                };

                if is_external && browser_active {
                    // Disconnect from external Chrome (do not close Chrome itself)
                    if let Err(e) = browser_manager.stop().await {
                        tracing::warn!("[cdp] failed to stop external Chrome connection: {e}");
                    }
                    // Notify UI
                    app_event_tx.send_background_event_with_ticket(
                        &ticket,
                        "Disconnected from Chrome".to_string(),
                    );
                    let _ = tx.send(true);
                } else {
                    // Not connected externally; proceed to connect
                    let _ = tx.send(false);
                }
            });

            // If the async task handled a disconnect, stop here; otherwise connect.
            let handled_disconnect = rx.recv().unwrap_or(false);
            if !handled_disconnect {
                // Switch to external Chrome mode with default/auto-detected port
                self.handle_chrome_connection(None, None, chrome_ticket);
            } else {
                // We just disconnected; reflect in title immediately
                self.browser_is_external = false;
                self.request_redraw();
            }
            return;
        }

        match parse_chrome_command_args(&command_text) {
            ChromeCommandArgs::Status => {
                // Get status from browser manager
                let (status_tx, status_rx) = std::sync::mpsc::channel();
                tokio::spawn(async move {
                    let browser_manager = ChatWidget::get_browser_manager().await;
                    let status = browser_manager.get_status_sync();
                    let _ = status_tx.send(status);
                });
                let status = status_rx
                    .recv()
                    .unwrap_or_else(|_| "Failed to get browser status.".to_string());

                let lines: Vec<String> =
                    status.lines().map(std::string::ToString::to_string).collect();
                self.push_background_tail(lines.join("\n"));
            }
            ChromeCommandArgs::WsUrl(ws_url) => {
                tracing::info!("[cdp] /chrome provided WS endpoint: {ws_url}");
                // Configure and connect using WS
                self.browser_is_external = true;
                let latest_screenshot = self.latest_browser_screenshot.clone();
                let app_event_tx = self.app_event_tx.clone();
                tokio::spawn(async move {
                    let bm = ChatWidget::get_browser_manager().await;
                    {
                        let mut cfg = bm.config.write().await;
                        cfg.enabled = true;
                        cfg.headless = false;
                        cfg.persist_profile = true;
                        cfg.connect_ws = Some(ws_url);
                        cfg.connect_port = None;
                        cfg.connect_host = None;
                    }
                    let _ = bm.connect_to_chrome_only().await;
                    // Capture a first screenshot if possible
                    tokio::time::sleep(super::BROWSER_INITIAL_SCREENSHOT_DELAY).await;
                    match bm.capture_screenshot_with_url().await {
                        Ok((paths, url)) => {
                            if let Some(first_path) = paths.first() {
                                let url_text = url.unwrap_or_else(|| "Browser".to_string());
                                set_latest_screenshot(
                                    &latest_screenshot,
                                    first_path.clone(),
                                    url_text.clone(),
                                );
                                send_browser_screenshot_update(
                                    &app_event_tx,
                                    first_path.clone(),
                                    url_text,
                                );
                            }
                        }
                        Err(e) => {
                            tracing::warn!(
                                "Failed to capture initial external Chrome screenshot: {e}"
                            );
                        }
                    }
                });
            }
            ChromeCommandArgs::HostPort { host, port } => {
                tracing::info!("[cdp] parsed host={host:?}, port={port:?}");
                self.handle_chrome_connection(host, port, chrome_ticket);
            }
        }
    }
}

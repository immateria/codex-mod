use crate::BrowserError;
use crate::Result;
use chromiumoxide::Browser;
use chromiumoxide::BrowserConfig as CdpConfig;
use chromiumoxide::browser::HeadlessMode;
use fs2::FileExt;
use futures::StreamExt;
use once_cell::sync::Lazy;
use std::path::PathBuf;
use tokio::sync::Mutex;
use tokio::time::Duration;
use tokio::time::Instant;
use tokio::time::sleep;
use tracing::info;
use tracing::warn;

use super::super::BrowserManager;
use super::discover_ws_via_host_port;
use super::scan_for_chrome_debug_port;
use super::should_stop_handler;

static INTERNAL_BROWSER_LAUNCH_GUARD: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

struct BrowserLaunchLockFile {
    file: std::fs::File,
}

impl BrowserLaunchLockFile {
    async fn acquire(timeout: Duration) -> Result<Self> {
        let lock_path = std::env::temp_dir().join("code-browser-launch.lock");
        let file = std::fs::OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(&lock_path)?;

        let start = Instant::now();
        loop {
            match file.try_lock_exclusive() {
                Ok(()) => return Ok(Self { file }),
                Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                    if start.elapsed() >= timeout {
                        return Err(BrowserError::CdpError(format!(
                            "Timed out waiting for browser launch lock at {} (another Code instance may be launching a browser).",
                            lock_path.display()
                        )));
                    }
                    sleep(Duration::from_millis(50)).await;
                }
                Err(err) => return Err(BrowserError::IoError(err)),
            }
        }
    }
}

impl Drop for BrowserLaunchLockFile {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

fn is_temporary_internal_launch_error_message(message: &str) -> bool {
    let message = message.to_ascii_lowercase();
    // macOS: EAGAIN = os error 35
    // Linux: ENOMEM = os error 12
    // Common: "Too many open files" (EMFILE)
    message.contains("resource temporarily unavailable")
        || message.contains("temporarily unavailable")
        || message.contains("os error 35")
        || message.contains("eagain")
        || message.contains("os error 12")
        || message.contains("cannot allocate memory")
        || message.contains("too many open files")
        || message.contains("os error 24")
}

fn chrome_logging_enabled() -> bool {
    env_truthy("CODE_SUBAGENT_DEBUG") || env_truthy("CODEX_BROWSER_LOG")
}

fn env_truthy(key: &str) -> bool {
    std::env::var(key)
        .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(false)
}

fn resolve_chrome_log_path() -> Option<PathBuf> {
    if !chrome_logging_enabled() {
        return None;
    }

    if let Ok(path) = std::env::var("CODEX_BROWSER_LOG_PATH") {
        let trimmed = path.trim();
        if !trimmed.is_empty() {
            return Some(PathBuf::from(trimmed));
        }
    }

    let base = if let Ok(home) = std::env::var("CODE_HOME").or_else(|_| std::env::var("CODEX_HOME")) {
        PathBuf::from(home).join("debug_logs")
    } else if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home).join(".code").join("debug_logs")
    } else {
        return Some(std::env::temp_dir().join("code-chrome.log"));
    };

    let path = base.join("code-chrome.log");
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    Some(path)
}

impl BrowserManager {
    pub async fn start(&self) -> Result<()> {
        if self.browser.lock().await.is_some() {
            return Ok(());
        }

        let config = self.config.read().await.clone();

        // 1) Attach to a live Chrome, if requested
        if let Some(ws) = config.connect_ws.clone() {
            info!("Connecting to Chrome via WebSocket: {}", ws);
            // Use the same guarded connect strategy as connect_to_chrome_only
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
                info!("Using specified Chrome debug port: {}", port);
                port
            };

            if actual_port > 0 {
                info!(
                    "Step 1: Discovering Chrome WebSocket URL via {}:{}...",
                    host, actual_port
                );
                let ws = loop {
                    let discover_start = tokio::time::Instant::now();
                    match discover_ws_via_host_port(host, actual_port).await {
                        Ok(ws) => {
                            info!(
                                "Step 2: WebSocket URL discovered in {:?}: {}",
                                discover_start.elapsed(),
                                ws
                            );
                            break ws;
                        }
                        Err(e) => {
                            if tokio::time::Instant::now() - discover_start > Duration::from_secs(15) {
                                return Err(BrowserError::CdpError(format!(
                                    "Failed to discover Chrome WebSocket on port {actual_port} within 15s: {e}"
                                )));
                            }
                            tokio::time::sleep(Duration::from_millis(300)).await;
                        }
                    }
                };

                info!("Step 3: Connecting to Chrome via WebSocket...");
                let connect_start = tokio::time::Instant::now();
                // Use guarded connect strategy with retries
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
                            info!("Step 4: Connected to Chrome in {:?}", connect_start.elapsed());

                            // Start event handler
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

                            info!("Step 5: Starting idle monitor...");
                            self.start_idle_monitor().await;
                            self.update_activity().await;
                            info!("Step 6: Chrome connection complete!");
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
        }

        // 2) Launch a browser
        info!("Launching new browser instance");
        // Prevent redundant browser launches within the same process.
        let _launch_guard = INTERNAL_BROWSER_LAUNCH_GUARD.lock().await;
        // Also serialize launches across Code processes; concurrent Chromium spawns
        // are a common source of transient EAGAIN/ENOMEM failures on macOS.
        let _launch_lock = BrowserLaunchLockFile::acquire(Duration::from_secs(30)).await?;

        const INTERNAL_LAUNCH_RETRY_DELAYS_MS: [u64; 5] = [50, 200, 500, 1000, 2000];
        let max_attempts = INTERNAL_LAUNCH_RETRY_DELAYS_MS.len() + 1;

        // Add browser launch flags (keep minimal set for screenshot functionality)
        let log_file = resolve_chrome_log_path();

        let (browser, mut handler, user_data_path) = {
            let mut attempt = 1usize;
            loop {
                // Profile dir
                let (user_data_path, is_temp_profile) = if let Some(dir) = &config.user_data_dir {
                    (dir.to_string_lossy().to_string(), false)
                } else {
                    let pid = std::process::id();
                    let timestamp = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis();
                    let temp_path = format!("/tmp/code-browser-{pid}-{timestamp}-{attempt}");
                    if tokio::fs::metadata(&temp_path).await.is_ok() {
                        let _ = tokio::fs::remove_dir_all(&temp_path).await;
                    }
                    (temp_path, true)
                };

                let mut builder = CdpConfig::builder().user_data_dir(&user_data_path);

                // Set headless mode based on config (keep original approach for stability)
                if config.headless {
                    builder = builder.headless_mode(HeadlessMode::New);
                }

                // Configure viewport (revert to original approach for screenshot stability)
                builder = builder.window_size(config.viewport.width, config.viewport.height);

                builder = builder
                    .arg("--disable-blink-features=AutomationControlled")
                    .arg("--no-first-run")
                    .arg("--no-default-browser-check")
                    .arg("--disable-component-extensions-with-background-pages")
                    .arg("--disable-background-networking")
                    .arg("--silent-debugger-extension-api")
                    .arg("--remote-allow-origins=*")
                    .arg("--disable-features=ChromeWhatsNewUI,TriggerFirstRunUI")
                    // Disable timeout for slow networks/pages
                    .arg("--disable-hang-monitor")
                    .arg("--disable-background-timer-throttling")
                    // Suppress console output
                    .arg("--silent-launch")
                    // Set a longer timeout for CDP requests (60 seconds instead of default 30)
                    .request_timeout(Duration::from_secs(60));

                if let Some(proxy_server) = config
                    .proxy_server
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                {
                    builder = builder.arg(format!("--proxy-server={proxy_server}"));
                }

                if let Some(proxy_bypass_list) = config
                    .proxy_bypass_list
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                {
                    builder = builder.arg(format!("--proxy-bypass-list={proxy_bypass_list}"));
                }

                if let Some(ref log_file) = log_file {
                    builder = builder
                        .arg("--enable-logging")
                        .arg("--log-level=1")
                        .arg(format!("--log-file={}", log_file.display()));
                }

                let browser_config = builder.build().map_err(BrowserError::CdpError)?;

                match Browser::launch(browser_config).await {
                    Ok((browser, handler)) => break (browser, handler, user_data_path),
                    Err(e) => {
                        let message = e.to_string();

                        let is_temporary = is_temporary_internal_launch_error_message(&message);
                        if is_temp_profile {
                            let _ = tokio::fs::remove_dir_all(&user_data_path).await;
                        }

                        if is_temporary && attempt < max_attempts {
                            let delay_ms = INTERNAL_LAUNCH_RETRY_DELAYS_MS[attempt - 1];
                            warn!(
                                error = %message,
                                attempt,
                                delay_ms,
                                "Internal browser launch failed with transient error; retrying"
                            );
                            sleep(Duration::from_millis(delay_ms)).await;
                            attempt += 1;
                            continue;
                        }

                        #[cfg(target_os = "macos")]
                        let hint = "Ensure Google Chrome or Chromium is installed and runnable (e.g., /Applications/Google Chrome.app).";
                        #[cfg(target_os = "linux")]
                        let hint =
                            "Ensure google-chrome or chromium is installed and available on PATH.";
                        #[cfg(target_os = "windows")]
                        let hint = "Ensure Chrome is installed and chrome.exe is available (typically in Program Files).";
                        #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
                        let hint = "Ensure Chrome/Chromium is installed and available on PATH.";

                        let log_note = log_file
                            .as_ref()
                            .map(|path| format!(" Chrome log: {}", path.display()))
                            .unwrap_or_default();
                        return Err(BrowserError::CdpError(format!(
                            "Failed to launch internal browser: {message}. Hint: {hint}.{log_note}"
                        )));
                    }
                }
            }
        };
        // Optionally: browser.fetch_targets().await.ok();

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
        *self.user_data_dir.lock().await = Some(user_data_path.clone());

        let should_cleanup = config.user_data_dir.is_none() || !config.persist_profile;
        *self.cleanup_profile_on_drop.lock().await = should_cleanup;

        self.start_idle_monitor().await;
        self.update_activity().await;
        Ok(())
    }
}

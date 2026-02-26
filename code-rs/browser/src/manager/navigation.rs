use crate::BrowserError;
use crate::Result;
use crate::config::WaitStrategy;
use crate::page::Page;
use tokio::time::Duration;
use tracing::debug;
use tracing::info;
use tracing::warn;

use super::BrowserManager;

#[derive(Debug)]
struct PageDebugInfo {
    target_id: String,
    session_id: String,
    opener_id: Option<String>,
    cached_url: Option<String>,
    live_url: Option<String>,
}

#[derive(Debug)]
struct TargetSnapshot {
    total: usize,
    sample: Vec<String>,
    truncated: bool,
}

impl BrowserManager {
    pub async fn goto(&self, url: &str) -> Result<crate::page::GotoResult> {
        const MAX_RECOVERY_ATTEMPTS: usize = 2; // number of retries after the initial attempt
        let mut recovery_attempts = 0usize;

        loop {
            match self.goto_once(url).await {
                Ok(result) => {
                    if recovery_attempts > 0 {
                        info!(
                            "Browser navigation succeeded after {} recovery attempt(s)",
                            recovery_attempts
                        );
                    }
                    return Ok(result);
                }
                Err(err) => {
                    let should_retry = recovery_attempts < MAX_RECOVERY_ATTEMPTS
                        && self.should_retry_after_goto_error(&err).await;

                    self.log_navigation_failure(url, &err, recovery_attempts, should_retry)
                        .await;

                    if !should_retry {
                        return Err(err);
                    }

                    warn!(
                        error = %err,
                        recovery_attempt = recovery_attempts + 1,
                        "Browser navigation failed; restarting browser before retry"
                    );

                    if let Err(stop_err) = self.stop().await {
                        warn!("Failed to stop browser during recovery: {}", stop_err);
                    }

                    tokio::time::sleep(Duration::from_millis(400)).await;
                    recovery_attempts += 1;
                }
            }
        }
    }

    async fn log_navigation_failure(
        &self,
        url: &str,
        err: &BrowserError,
        recovery_attempt: usize,
        will_retry: bool,
    ) {
        let error_string = err.to_string();
        let config = self.config.read().await.clone();
        let is_external = config.connect_port.is_some() || config.connect_ws.is_some();
        let browser_active = self.browser.lock().await.is_some();
        let wait_desc = match &config.wait {
            WaitStrategy::Event(event) => format!("event:{event}"),
            WaitStrategy::Delay { delay_ms } => format!("delay:{delay_ms}ms"),
        };

        warn!(
            url = %url,
            error = %error_string,
            recovery_attempt,
            will_retry,
            browser_active,
            is_external,
            headless = config.headless,
            connect_port = ?config.connect_port,
            connect_ws = ?config.connect_ws,
            viewport_width = config.viewport.width,
            viewport_height = config.viewport.height,
            viewport_dpr = config.viewport.device_scale_factor,
            viewport_mobile = config.viewport.mobile,
            wait = %wait_desc,
            "Browser navigation failed"
        );

        let page_snapshot = self.page.lock().await.clone();
        if let Some(page) = page_snapshot.as_ref() {
            let page_debug = Self::collect_page_debug_info(page).await;
            warn!(
                target_id = %page_debug.target_id,
                session_id = %page_debug.session_id,
                opener_id = ?page_debug.opener_id,
                cached_url = ?page_debug.cached_url,
                live_url = ?page_debug.live_url,
                "Browser page debug context"
            );
        } else {
            warn!("Browser navigation failed without an active page");
        }

        if let Some(snapshot) = self.collect_target_snapshot().await {
            warn!(
                targets_total = snapshot.total,
                targets_truncated = snapshot.truncated,
                targets_sample = ?snapshot.sample,
                "Browser target snapshot"
            );
        }
    }

    async fn collect_page_debug_info(page: &Page) -> PageDebugInfo {
        let cached_url = page.get_url().await.ok();
        let live_url = match tokio::time::timeout(Duration::from_millis(600), page.get_current_url())
            .await
        {
            Ok(Ok(url)) => Some(url),
            Ok(Err(err)) => {
                debug!(error = %err, "Navigation telemetry failed to read live URL");
                None
            }
            Err(_) => {
                debug!("Navigation telemetry timed out reading live URL");
                None
            }
        };

        PageDebugInfo {
            target_id: page.target_id_debug(),
            session_id: page.session_id_debug(),
            opener_id: page.opener_id_debug(),
            cached_url,
            live_url,
        }
    }

    async fn collect_target_snapshot(&self) -> Option<TargetSnapshot> {
        let mut browser_guard = self.browser.lock().await;
        let browser = browser_guard.as_mut()?;
        let fetch = tokio::time::timeout(Duration::from_millis(1200), browser.fetch_targets()).await;
        let targets = match fetch {
            Ok(Ok(targets)) => targets,
            Ok(Err(err)) => {
                warn!(error = %err, "Failed to fetch CDP targets for navigation telemetry");
                return None;
            }
            Err(_) => {
                warn!("Timed out fetching CDP targets for navigation telemetry");
                return None;
            }
        };

        let total = targets.len();
        let mut sample = Vec::new();
        for (index, target) in targets.iter().take(12).enumerate() {
            let target_id = &target.target_id;
            let target_type = &target.r#type;
            let subtype = target.subtype.as_deref().unwrap_or("-");
            let opener = target
                .opener_id
                .as_ref()
                .map(|opener_id| format!("{opener_id:?}"))
                .unwrap_or_else(|| "-".to_string());
            let url = &target.url;
            let title = &target.title;
            let attached = target.attached;
            sample.push(format!(
                "#{index} id={target_id:?} type={target_type} subtype={subtype} attached={attached} opener={opener} url={url} title={title}",
                index = index + 1,
                target_id = target_id,
                target_type = target_type,
                subtype = subtype,
                attached = attached,
                opener = opener,
                url = url,
                title = title
            ));
        }

        Some(TargetSnapshot {
            total,
            truncated: total > sample.len(),
            sample,
        })
    }

    async fn should_retry_after_goto_error(&self, err: &BrowserError) -> bool {
        let is_internal = {
            let cfg = self.config.read().await;
            cfg.connect_port.is_none() && cfg.connect_ws.is_none()
        };

        if !is_internal {
            return false;
        }

        match err {
            BrowserError::NotInitialized => true,
            BrowserError::IoError(e) => matches!(
                e.kind(),
                std::io::ErrorKind::WouldBlock
                    | std::io::ErrorKind::TimedOut
                    | std::io::ErrorKind::ConnectionReset
                    | std::io::ErrorKind::ConnectionAborted
                    | std::io::ErrorKind::BrokenPipe
            ),
            BrowserError::CdpError(msg) => {
                let msg_lower = msg.to_ascii_lowercase();
                const RECOVERABLE_SUBSTRINGS: &[&str] = &[
                    "connection closed",
                    "browser closed",
                    "target crashed",
                    "context destroyed",
                    "no such session",
                    "disconnected",
                    "transport",
                    "timeout",
                    "timed out",
                    "oneshot error",
                    "oneshot canceled",
                    "oneshot cancelled",
                    "resource temporarily unavailable",
                    "temporarily unavailable",
                    "eagain",
                ];

                RECOVERABLE_SUBSTRINGS
                    .iter()
                    .any(|needle| msg_lower.contains(needle))
            }
            _ => false,
        }
    }

    async fn goto_once(&self, url: &str) -> Result<crate::page::GotoResult> {
        // Get or create page.
        let page = self.get_or_create_page().await?;

        let nav_start = std::time::Instant::now();
        info!("Navigating to URL: {}", url);
        let config = self.config.read().await;
        let result = page.goto(url, Some(config.wait.clone())).await?;
        info!(
            "Navigation complete to: {} in {:?}",
            result.url,
            nav_start.elapsed()
        );

        // Manually trigger navigation callback for immediate response.
        if let Some(ref callback) = *self.navigation_callback.read().await {
            debug!("Manually triggering navigation callback after goto");
            callback(result.url.clone());
        }

        self.update_activity().await;
        Ok(result)
    }
}

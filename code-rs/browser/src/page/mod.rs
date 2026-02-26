use crate::Result;
use crate::config::BrowserConfig;
use crate::config::ImageFormat;
use chromiumoxide::cdp::browser_protocol::input::MouseButton;
use chromiumoxide::cdp::browser_protocol::page::AddScriptToEvaluateOnNewDocumentParams;
use chromiumoxide::page::Page as CdpPage;
use chromiumoxide::cdp::js_protocol::runtime as cdp_runtime;
use chromiumoxide::cdp::browser_protocol::log as cdp_log;
use futures::StreamExt;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tokio::sync::Mutex;
use tracing::debug;
use tracing::warn;

mod console;
mod input;
mod navigation;
mod screenshot;
mod viewport;

// Externalized virtual cursor script (editable JS)
const VIRTUAL_CURSOR_JS: &str = include_str!("../js/virtual_cursor.js");
// Externalized per-document bootstrap (tab blocking, SPA hooks, console capture, stealth).
const PAGE_BOOTSTRAP_JS: &str = include_str!("../js/page_bootstrap.js");

// Define CursorState struct (New)
#[derive(Debug, Clone)]
pub struct CursorState {
    pub x: f64,
    pub y: f64,
    // Include button state, mirroring the TS implementation
    pub button: MouseButton,
    // Track whether mouse button is currently pressed
    pub is_mouse_down: bool,
}

pub struct Page {
    cdp_page: Arc<CdpPage>,
    config: BrowserConfig,
    current_url: Arc<RwLock<Option<String>>>,
    // Add cursor state tracking (New)
    cursor_state: Arc<Mutex<CursorState>>,
    // Buffer for CDP-captured console logs
    console_logs: Arc<Mutex<Vec<serde_json::Value>>>,
    // Screenshot path preflight cache:
    // - We strongly prefer compositor captures via from_surface(false) to avoid visible flashes in the
    //   user's real Chrome window. However, that path can be flaky or unavailable when the window is not
    //   visible/minimized. A tiny 8×8 probe (guarded by a ~350ms timeout) predicts viability and is cached
    //   for ~5 seconds to avoid repeated probes while navigating.
    // - IMPORTANT: This cache and probe logic protect both UX (no flash on visible windows) and reliability
    //   (preventing repeated long timeouts when minimized). If you change this, ensure visible windows never
    //   start with from_surface(true), and keep a short/cheap probe for hidden/minimized states.
    preflight_cache: Arc<Mutex<Option<(Instant, bool)>>>,
}

#[derive(Clone, Copy, Debug)]
enum ReadyStateTarget {
    InteractiveOrComplete,
    Complete,
}

async fn wait_ready_state(
    cdp_page: &CdpPage,
    target: ReadyStateTarget,
    timeout: Duration,
    interval: Duration,
) {
    let script = "document.readyState";
    let start = Instant::now();
    loop {
        let state = cdp_page
            .evaluate(script)
            .await
            .ok()
            .and_then(|r| r.value().and_then(|v| v.as_str().map(std::string::ToString::to_string)));
        let done = match target {
            ReadyStateTarget::InteractiveOrComplete => {
                matches!(state.as_deref(), Some("interactive") | Some("complete"))
            }
            ReadyStateTarget::Complete => matches!(state.as_deref(), Some("complete")),
        };
        if done || start.elapsed() >= timeout {
            break;
        }
        tokio::time::sleep(interval).await;
    }
}

async fn url_looks_loaded(cdp_page: &CdpPage, timeout: Duration) -> bool {
    let result = tokio::time::timeout(timeout, cdp_page.url()).await;
    match result {
        Ok(Ok(Some(url))) => {
            let url = url.trim();
            if url.is_empty() || url == "about:blank" {
                return false;
            }
            url.starts_with("http://") || url.starts_with("https://")
        }
        _ => false,
    }
}

impl Page {
    pub fn new(cdp_page: CdpPage, config: BrowserConfig) -> Self {
        // Initialize cursor position (Updated)
        let initial_cursor = CursorState {
            x: (config.viewport.width as f64 / 2.0).floor(),
            y: (config.viewport.height as f64 / 4.0).floor(),
            button: MouseButton::None,
            is_mouse_down: false,
        };

        let page = Self {
            cdp_page: Arc::new(cdp_page),
            config,
            current_url: Arc::new(RwLock::new(None)),
            cursor_state: Arc::new(Mutex::new(initial_cursor)),
            preflight_cache: Arc::new(Mutex::new(None)),
            console_logs: Arc::new(Mutex::new(Vec::new())),
        };

        // Register a unified bootstrap (runs on every new document):
        //  - Blocks _blank/tab opens
        //  - Installs minimal virtual cursor early
        //  - Hooks SPA history to signal route changes
        let cdp_page_boot = page.cdp_page.clone();
        tokio::spawn(async move {
            if let Err(e) = Self::inject_bootstrap_script(&cdp_page_boot).await {
                warn!("Failed to inject unified bootstrap script: {}", e);
            } else {
                debug!("Unified bootstrap script registered for new documents");
            }
        });

        // Enable CDP Runtime/Log and start capturing console events into an internal buffer.
        // This complements the JS hook and works even if the page overwrites console later.
        let cdp_page_events = page.cdp_page.clone();
        let logs_buf = page.console_logs.clone();
        tokio::spawn(async move {
            // Best-effort enable; ignore failures silently to avoid breaking page creation.
            let _ = cdp_page_events.execute(cdp_runtime::EnableParams::default()).await;
            let _ = cdp_page_events.execute(cdp_log::EnableParams::default()).await;

            // Listen for Runtime.consoleAPICalled
            if let Ok(mut stream) = cdp_page_events
                .event_listener::<cdp_runtime::EventConsoleApiCalled>()
                .await
            {
                while let Some(evt) = stream.next().await {
                    let ts = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_millis() as i128)
                        .unwrap_or(0);
                    // Join args into a readable string; also keep raw values
                    let text = serde_json::to_string(&evt.args).unwrap_or_default();
                    let item = serde_json::json!({
                        "ts_unix_ms": ts,
                        "level": format!("{:?}", evt.r#type),
                        "message": text,
                        "source": "cdp:runtime"
                    });
                    let mut buf = logs_buf.lock().await;
                    buf.push(item);
                    if buf.len() > 2000 { buf.remove(0); }
                }
            }

            // Also listen for Log.entryAdded (browser-side logs)
            if let Ok(mut stream) = cdp_page_events
                .event_listener::<cdp_log::EventEntryAdded>()
                .await
            {
                while let Some(evt) = stream.next().await {
                    let ts = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_millis() as i128)
                        .unwrap_or(0);
                    let entry = &evt.entry;
                    let item = serde_json::json!({
                        "ts_unix_ms": ts,
                        "level": format!("{:?}", entry.level),
                        "message": entry.text,
                        "source": "cdp:log",
                        "url": entry.url,
                        "line": entry.line_number
                    });
                    let mut buf = logs_buf.lock().await;
                    buf.push(item);
                    if buf.len() > 2000 { buf.remove(0); }
                }
            }
        });

        page
    }

    /// Ensure the virtual cursor is present; inject if missing, then update to current position.
    async fn ensure_virtual_cursor(&self) -> Result<bool> {
        // Desired runtime version of the virtual cursor script
        let desired_version: i32 = 11;
        // Quick existence check
        // Check existence and version
        let status = self
            .cdp_page
            .evaluate(format!(
                r#"(function(v) {{
                      if (typeof window.__vc === 'undefined') return 'missing';
                      try {{
                        var cur = window.__vc.__version|0;
                        if (!cur || cur !== v) {{
                          if (window.__vc && typeof window.__vc.destroy === 'function') try {{ window.__vc.destroy(); }} catch (e) {{}}
                          return 'reinstall';
                        }}
                        return 'ok';
                      }} catch (e) {{ return 'reinstall'; }}
                }})({desired_version})"#
            ))
            .await
            .ok()
            .and_then(|r| r.value().and_then(|v| v.as_str().map(std::string::ToString::to_string)))
            .unwrap_or_else(|| "missing".to_string());

        if status != "ok" {
            // Inject if missing
            if let Err(e) = self.inject_virtual_cursor().await {
                warn!("Failed to inject virtual cursor: {}", e);
                return Err(e);
            }
            return Ok(true);
        }

        Ok(false)
    }

    /// Injects a unified bootstrap for each new document: tab blocking + SPA hooks
    /// and early console capture so tools like `browser_console` can read logs reliably.
    async fn inject_bootstrap_script(cdp_page: &Arc<CdpPage>) -> Result<()> {
        // The cursor is injected on demand via runtime tooling (see `ensure_virtual_cursor()`), so the
        // bootstrap focuses on tab blocking, SPA hooks, console capture, and lightweight "stealth".
        let script = PAGE_BOOTSTRAP_JS;

        let params = AddScriptToEvaluateOnNewDocumentParams::new(script);
        cdp_page.execute(params).await?;
        Ok(())
    }

    pub fn target_id_debug(&self) -> String {
        let target_id = self.cdp_page.target_id();
        format!("{target_id:?}")
    }

    pub fn target_id(&self) -> String {
        self.cdp_page.target_id().inner().clone()
    }

    pub fn session_id_debug(&self) -> String {
        let session_id = self.cdp_page.session_id();
        format!("{session_id:?}")
    }

    pub fn opener_id_debug(&self) -> Option<String> {
        self.cdp_page
            .opener_id()
            .as_ref()
            .map(|opener_id| format!("{opener_id:?}"))
    }

}

// Raw CDP command wrapper to allow executing arbitrary methods with JSON params
#[derive(Debug, Clone)]
struct RawCdpCommand {
    method: String,
    params: serde_json::Value,
}

impl RawCdpCommand {
    fn new(method: impl Into<String>, params: serde_json::Value) -> Self {
        Self {
            method: method.into(),
            params,
        }
    }
}

impl serde::Serialize for RawCdpCommand {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        // Serialize only the params as the Command payload
        self.params.serialize(serializer)
    }
}

impl chromiumoxide_types::Method for RawCdpCommand {
    fn identifier(&self) -> chromiumoxide_types::MethodId {
        self.method.clone().into()
    }
}

impl chromiumoxide_types::Command for RawCdpCommand {
    type Response = serde_json::Value;
}

impl Page {
    /// Execute an arbitrary CDP method with the provided JSON params against this page's session
    pub async fn execute_cdp_raw(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let cmd = RawCdpCommand::new(method, params);
        let resp = self.cdp_page.execute(cmd).await?;
        Ok(resp.result)
    }
}

#[derive(Debug, Clone)]
pub enum ScreenshotMode {
    Viewport,
    FullPage { segments_max: Option<usize> },
    Region(ScreenshotRegion),
}

#[derive(Debug, Clone)]
pub struct ScreenshotRegion {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug)]
pub struct Screenshot {
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub format: ImageFormat,
}

#[derive(Debug, serde::Serialize)]
pub struct GotoResult {
    pub url: String,
    pub title: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
pub struct SetViewportParams {
    pub width: u32,
    pub height: u32,
    pub device_scale_factor: Option<f64>,
    pub mobile: Option<bool>,
}

#[derive(Debug, serde::Serialize)]
pub struct ViewportResult {
    pub width: u32,
    pub height: u32,
    pub dpr: f64,
}

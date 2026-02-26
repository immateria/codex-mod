use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;

use super::args::parse_port_from_ws;
use super::super::BackgroundOrderTicket;

pub(super) fn set_latest_screenshot(
    latest_screenshot: &Arc<Mutex<Option<(PathBuf, String)>>>,
    screenshot_path: PathBuf,
    url: String,
) {
    if let Ok(mut latest) = latest_screenshot.lock() {
        *latest = Some((screenshot_path, url));
    }
}

pub(super) fn send_browser_screenshot_update(
    app_event_tx: &AppEventSender,
    screenshot_path: PathBuf,
    url: String,
) {
    use code_core::protocol::{BrowserScreenshotUpdateEvent, Event, EventMsg};

    app_event_tx.send(AppEvent::CodexEvent(Event {
        id: uuid::Uuid::new_v4().to_string(),
        event_seq: 0,
        msg: EventMsg::BrowserScreenshotUpdate(BrowserScreenshotUpdateEvent {
            screenshot_path,
            url,
        }),
        order: None,
    }));
}

pub(super) fn spawn_initial_screenshot_capture(
    browser_manager: Arc<code_browser::BrowserManager>,
    latest_screenshot: Arc<Mutex<Option<(PathBuf, String)>>>,
    app_event_tx: AppEventSender,
    default_url: &'static str,
) {
    tokio::spawn(async move {
        tokio::time::sleep(super::CDP_SCREENSHOT_DELAY).await;
        for attempt in 1..=super::CDP_SCREENSHOT_MAX_ATTEMPTS {
            match browser_manager.capture_screenshot_with_url().await {
                Ok((paths, url)) => {
                    if let Some(first_path) = paths.first() {
                        tracing::info!(
                            "Initial CDP screenshot captured: {path}",
                            path = first_path.display()
                        );
                        let url_text = url.unwrap_or_else(|| default_url.to_string());
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
                        break;
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to capture initial CDP screenshot (attempt {attempt}): {e}"
                    );
                    if attempt >= super::CDP_SCREENSHOT_MAX_ATTEMPTS {
                        break;
                    }
                    tokio::time::sleep(super::CDP_SCREENSHOT_DELAY).await;
                }
            }
        }
    });
}

pub(super) fn spawn_navigation_screenshot_capture(
    browser_manager: Arc<code_browser::BrowserManager>,
    latest_screenshot: Arc<Mutex<Option<(PathBuf, String)>>>,
    app_event_tx: AppEventSender,
    url: String,
) {
    tokio::spawn(async move {
        tokio::time::sleep(super::CDP_SCREENSHOT_DELAY).await;
        for attempt in 1..=super::CDP_SCREENSHOT_MAX_ATTEMPTS {
            match browser_manager.capture_screenshot_with_url().await {
                Ok((paths, _)) => {
                    if let Some(first_path) = paths.first() {
                        tracing::info!(
                            "[cdp] auto-captured screenshot: {path}",
                            path = first_path.display()
                        );
                        set_latest_screenshot(
                            &latest_screenshot,
                            first_path.clone(),
                            url.clone(),
                        );
                        send_browser_screenshot_update(
                            &app_event_tx,
                            first_path.clone(),
                            url,
                        );
                        break;
                    }
                }
                Err(e) => {
                    tracing::warn!("[cdp] auto-capture failed (attempt {attempt}): {e}");
                    if attempt >= super::CDP_SCREENSHOT_MAX_ATTEMPTS {
                        break;
                    }
                    tokio::time::sleep(super::CDP_SCREENSHOT_DELAY).await;
                }
            }
        }
    });
}

pub(super) async fn build_cdp_success_message(
    browser_manager: &code_browser::BrowserManager,
) -> String {
    let (detected_port, detected_ws) = code_browser::global::get_last_connection().await;
    let port_num = detected_port.or_else(|| detected_ws.as_deref().and_then(parse_port_from_ws));

    let current_url = browser_manager.get_current_url().await;
    match (port_num, current_url) {
        (Some(p), Some(url)) if !url.is_empty() => {
            format!("CDP: connected to Chrome (port {p}) to {url}")
        }
        (Some(p), _) => format!("CDP: connected to Chrome (port {p})"),
        (None, Some(url)) if !url.is_empty() => format!("CDP: connected to Chrome to {url}"),
        _ => "CDP: connected to Chrome".to_string(),
    }
}

async fn install_cdp_navigation_callback(
    browser_manager: Arc<code_browser::BrowserManager>,
    latest_screenshot: Arc<Mutex<Option<(PathBuf, String)>>>,
    app_event_tx: AppEventSender,
) {
    let browser_manager_for_callback = browser_manager.clone();
    browser_manager
        .set_navigation_callback(move |url| {
            tracing::info!("CDP Navigation callback triggered for URL: {url}");
            spawn_navigation_screenshot_capture(
                browser_manager_for_callback.clone(),
                latest_screenshot.clone(),
                app_event_tx.clone(),
                url,
            );
        })
        .await;
}

pub(super) async fn finish_cdp_connection(
    browser_manager: Arc<code_browser::BrowserManager>,
    latest_screenshot: Arc<Mutex<Option<(PathBuf, String)>>>,
    app_event_tx: &AppEventSender,
    ticket: &BackgroundOrderTicket,
) {
    let success_msg = build_cdp_success_message(&browser_manager).await;
    app_event_tx.send_background_event_with_ticket(ticket, success_msg);

    tokio::spawn(async move {
        let (p, ws) = code_browser::global::get_last_connection().await;
        let _ = super::super::write_cached_connection(p, ws).await;
    });

    install_cdp_navigation_callback(
        browser_manager.clone(),
        latest_screenshot.clone(),
        app_event_tx.clone(),
    )
    .await;

    code_browser::global::set_global_browser_manager(browser_manager.clone()).await;

    spawn_initial_screenshot_capture(browser_manager, latest_screenshot, app_event_tx.clone(), "Chrome");
}

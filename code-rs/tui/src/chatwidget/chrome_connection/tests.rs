use std::ffi::OsString;
use std::path::PathBuf;

use tempfile::tempdir;

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;

use super::args::ChromeCommandArgs;
use super::args::CdpConnectChoice;
use super::args::choose_cdp_connect_target;
use super::args::parse_chrome_command_args;
use super::args::parse_port_from_ws;
use super::screenshots::build_cdp_success_message;
use super::screenshots::send_browser_screenshot_update;

struct EnvVarGuard {
    key: &'static str,
    previous: Option<OsString>,
}

impl EnvVarGuard {
    fn set(key: &'static str, value: OsString) -> Self {
        let previous = std::env::var_os(key);
        unsafe { std::env::set_var(key, value) };
        Self { key, previous }
    }

    fn clear(key: &'static str) -> Self {
        let previous = std::env::var_os(key);
        unsafe { std::env::remove_var(key) };
        Self { key, previous }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        if let Some(previous) = self.previous.take() {
            unsafe { std::env::set_var(self.key, previous) };
        } else {
            unsafe { std::env::remove_var(self.key) };
        }
    }
}

#[test]
fn parse_port_from_ws_parses_ipv4_and_ipv6() {
    assert_eq!(
        parse_port_from_ws("ws://127.0.0.1:9222/devtools/browser/abc"),
        Some(9222)
    );
    assert_eq!(
        parse_port_from_ws("wss://example.com:443/devtools/browser/abc"),
        Some(443)
    );
    assert_eq!(
        parse_port_from_ws("ws://[::1]:9222/devtools/browser/abc"),
        Some(9222)
    );
    assert_eq!(parse_port_from_ws("ws://127.0.0.1/devtools"), None);
    assert_eq!(parse_port_from_ws("not-a-ws-url"), None);
}

#[test]
fn parse_chrome_command_args_parses_status_ws_and_host_port() {
    assert_eq!(parse_chrome_command_args("status"), ChromeCommandArgs::Status);
    assert_eq!(
        parse_chrome_command_args("ws://127.0.0.1:9222/devtools/browser/abc"),
        ChromeCommandArgs::WsUrl("ws://127.0.0.1:9222/devtools/browser/abc".to_string())
    );
    assert_eq!(
        parse_chrome_command_args("9222"),
        ChromeCommandArgs::HostPort {
            host: None,
            port: Some(9222)
        }
    );
    assert_eq!(
        parse_chrome_command_args("example.com:9222"),
        ChromeCommandArgs::HostPort {
            host: Some("example.com".to_string()),
            port: Some(9222)
        }
    );
    assert_eq!(
        parse_chrome_command_args("example.com 9222"),
        ChromeCommandArgs::HostPort {
            host: Some("example.com".to_string()),
            port: Some(9222)
        }
    );
    assert_eq!(
        parse_chrome_command_args("nonsense"),
        ChromeCommandArgs::HostPort {
            host: None,
            port: None
        }
    );
}

#[test]
fn choose_cdp_connect_target_prefers_explicit_port_over_cache() {
    let choice = choose_cdp_connect_target(
        Some("host".to_string()),
        Some(7777),
        Some(9222),
        Some("ws://127.0.0.1:9222/devtools/browser/abc".to_string()),
    );
    assert_eq!(
        choice,
        CdpConnectChoice {
            attempted_via_cached_ws: false,
            cached_port_for_fallback: None,
            connect_ws: None,
            connect_host: Some("host".to_string()),
            connect_port: Some(7777),
        }
    );
}

#[test]
fn choose_cdp_connect_target_uses_cached_ws_then_cached_port_then_autodetect() {
    let cached_ws = choose_cdp_connect_target(
        None,
        None,
        Some(9222),
        Some("ws://127.0.0.1:9222/devtools/browser/abc".to_string()),
    );
    assert_eq!(
        cached_ws,
        CdpConnectChoice {
            attempted_via_cached_ws: true,
            cached_port_for_fallback: Some(9222),
            connect_ws: Some("ws://127.0.0.1:9222/devtools/browser/abc".to_string()),
            connect_host: None,
            connect_port: None,
        }
    );

    let cached_port = choose_cdp_connect_target(None, None, Some(9222), None);
    assert_eq!(
        cached_port,
        CdpConnectChoice {
            attempted_via_cached_ws: false,
            cached_port_for_fallback: Some(9222),
            connect_ws: None,
            connect_host: None,
            connect_port: Some(9222),
        }
    );

    let autodetect = choose_cdp_connect_target(None, None, None, None);
    assert_eq!(
        autodetect,
        CdpConnectChoice {
            attempted_via_cached_ws: false,
            cached_port_for_fallback: None,
            connect_ws: None,
            connect_host: None,
            connect_port: Some(0),
        }
    );
}

#[test]
fn send_browser_screenshot_update_emits_codex_event() {
    let (tx, rx) = std::sync::mpsc::channel::<AppEvent>();
    let app_event_tx = AppEventSender::new(tx);
    let screenshot_path = PathBuf::from("/tmp/code-test-screenshot.png");
    let url = "http://example.invalid/".to_string();

    send_browser_screenshot_update(&app_event_tx, screenshot_path.clone(), url.clone());

    let event = rx.recv().expect("event");
    let AppEvent::CodexEvent(ev) = event else {
        panic!("expected CodexEvent, got: {event:?}");
    };
    let code_core::protocol::EventMsg::BrowserScreenshotUpdate(update) = ev.msg else {
        panic!("expected BrowserScreenshotUpdate, got: {:?}", ev.msg);
    };

    assert_eq!(update.screenshot_path, screenshot_path);
    assert_eq!(update.url, url);
}

#[test]
fn cached_connection_write_skips_empty() {
    let dir = tempdir().expect("tempdir");
    let _home_guard = EnvVarGuard::set("CODE_HOME", dir.path().as_os_str().to_owned());
    let _codex_home_guard = EnvVarGuard::clear("CODEX_HOME");

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");

    rt.block_on(async {
        super::super::write_cached_connection(None, None)
            .await
            .expect("write empty");
    });

    assert!(
        !dir.path().join("cache.json").exists(),
        "expected cache.json to not be created for empty write"
    );
}

#[test]
fn cached_connection_round_trips() {
    let dir = tempdir().expect("tempdir");
    let _home_guard = EnvVarGuard::set("CODE_HOME", dir.path().as_os_str().to_owned());
    let _codex_home_guard = EnvVarGuard::clear("CODEX_HOME");

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");

    rt.block_on(async {
        let port = Some(9222);
        let ws = Some("ws://127.0.0.1:9222/devtools/browser/abc".to_string());
        super::super::write_cached_connection(port, ws.clone())
            .await
            .expect("write cached connection");
        let read_back = super::super::read_cached_connection().await;
        assert_eq!(read_back, Some((port, ws)));
    });
}

#[test]
fn build_cdp_success_message_uses_port_from_ws_cache_when_port_missing() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");

    rt.block_on(async {
        let previous = code_browser::global::get_last_connection().await;
        code_browser::global::set_last_connection(
            None,
            Some("ws://127.0.0.1:1234/devtools/browser/abc".to_string()),
        )
        .await;

        let bm = code_browser::BrowserManager::new(code_browser::BrowserConfig::default());
        let msg = build_cdp_success_message(&bm).await;
        assert!(
            msg.contains("(port 1234)"),
            "expected success message to include parsed port, got: {msg}"
        );

        code_browser::global::set_last_connection(previous.0, previous.1).await;
    });
}


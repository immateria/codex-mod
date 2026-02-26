use super::connection::JsonVersion;
use super::connection::discover_ws_via_host_port;
use super::connection::should_restart_handler;
use super::connection::should_stop_handler;
use std::io::Read;
use std::io::Write;
use std::net::TcpListener;
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::thread;
use std::time::Duration;
use std::time::Instant;

#[derive(Debug)]
struct TestError(&'static str);

impl std::fmt::Display for TestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}

#[test]
fn handler_restarts_after_repeated_errors() {
    assert!(!should_restart_handler(0));
    assert!(!should_restart_handler(1));
    assert!(!should_restart_handler(2));
    assert!(should_restart_handler(3));
}

#[test]
fn handler_ignores_oneshot_cancellations() {
    let mut consecutive_errors = 0u32;
    for _ in 0..10 {
        let should_stop = should_stop_handler("[test]", Err(TestError("oneshot canceled")), &mut consecutive_errors);
        assert!(!should_stop);
        assert_eq!(consecutive_errors, 0);
    }
    for _ in 0..10 {
        let should_stop = should_stop_handler("[test]", Err(TestError("oneshot error")), &mut consecutive_errors);
        assert!(!should_stop);
        assert_eq!(consecutive_errors, 0);
    }
}

const TEST_PROXY_WS_URL: &str = "ws://proxy.invalid/devtools/browser/proxy";
const TEST_TARGET_WS_URL: &str = "ws://target.invalid/devtools/browser/target";

fn spawn_json_version_server(ws_url: &str) -> (u16, Arc<AtomicBool>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind server");
    listener.set_nonblocking(true).expect("set non-blocking");
    let port = listener.local_addr().expect("server addr").port();
    let stop = Arc::new(AtomicBool::new(false));
    let stop_thread = Arc::clone(&stop);
    let ws_url = ws_url.to_string();

    let handle = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !stop_thread.load(Ordering::Relaxed) && Instant::now() < deadline {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let mut buffer = [0u8; 1024];
                    let _ = stream.read(&mut buffer);

                    let body = format!(r#"{{"webSocketDebuggerUrl":"{ws_url}"}}"#);
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = stream.write_all(response.as_bytes());
                }
                Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(10));
                }
                Err(_) => break,
            }
        }
    });

    (port, stop, handle)
}

#[test]
fn cdp_discovery_ignores_proxy_env_vars() {
    let (proxy_port, proxy_stop, proxy_handle) = spawn_json_version_server(TEST_PROXY_WS_URL);
    let (target_port, target_stop, target_handle) = spawn_json_version_server(TEST_TARGET_WS_URL);

    let exe = std::env::current_exe().expect("current exe");
    let proxy_url = format!("http://127.0.0.1:{proxy_port}");

    let output = Command::new(exe)
        .arg("--exact")
        .arg("manager::tests::cdp_discovery_ignores_proxy_env_vars_child")
        .arg("--ignored")
        .arg("--nocapture")
        .env("CODE_BROWSER_TEST_TARGET_PORT", target_port.to_string())
        .env("HTTP_PROXY", &proxy_url)
        .env("HTTPS_PROXY", &proxy_url)
        .env("ALL_PROXY", &proxy_url)
        .env("http_proxy", &proxy_url)
        .env("https_proxy", &proxy_url)
        .env("all_proxy", &proxy_url)
        .env("NO_PROXY", "example.invalid")
        .env("no_proxy", "example.invalid")
        .output()
        .expect("spawn child test");

    proxy_stop.store(true, Ordering::Relaxed);
    target_stop.store(true, Ordering::Relaxed);
    let _ = proxy_handle.join();
    let _ = target_handle.join();

    if !output.status.success() {
        panic!(
            "child test failed: status={:?}\nstdout:\n{}\nstderr:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
    }
}

#[ignore]
#[tokio::test]
async fn cdp_discovery_ignores_proxy_env_vars_child() {
    let target_port: u16 = std::env::var("CODE_BROWSER_TEST_TARGET_PORT")
        .expect("CODE_BROWSER_TEST_TARGET_PORT")
        .parse()
        .expect("valid port");

    let url = format!("http://127.0.0.1:{target_port}/json/version");
    let default_client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .expect("build default client");
    let resp = default_client
        .get(&url)
        .send()
        .await
        .expect("default request");

    let proxy_version: JsonVersion = resp.json().await.expect("parse proxy json");
    assert_eq!(proxy_version.web_socket_debugger_url, TEST_PROXY_WS_URL);

    let discovered = discover_ws_via_host_port("127.0.0.1", target_port)
        .await
        .expect("discover ws url");
    assert_eq!(discovered, TEST_TARGET_WS_URL);
}


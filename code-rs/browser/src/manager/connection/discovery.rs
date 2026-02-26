use crate::BrowserError;
use crate::Result;
use reqwest::Client;
use serde::Deserialize;
use tokio::time::Duration;
use tracing::debug;
use tracing::info;
use tracing::warn;

#[derive(Deserialize)]
pub(in super::super) struct JsonVersion {
    #[serde(rename = "webSocketDebuggerUrl")]
    pub(in super::super) web_socket_debugger_url: String,
}

pub(in super::super) async fn discover_ws_via_host_port(host: &str, port: u16) -> Result<String> {
    let url = format!("http://{host}:{port}/json/version");
    debug!("Requesting Chrome version info from: {}", url);

    let client_start = tokio::time::Instant::now();
    let client = Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5)) // Allow Chrome time to bring up /json/version on fresh launch
        .build()
        .map_err(|e| BrowserError::CdpError(format!("Failed to build HTTP client: {e}")))?;
    debug!("HTTP client created in {:?}", client_start.elapsed());

    let req_start = tokio::time::Instant::now();
    let resp = client.get(&url).send().await.map_err(|e| {
        BrowserError::CdpError(format!("Failed to connect to Chrome debug port: {e}"))
    })?;
    debug!(
        "HTTP request completed in {:?}, status: {}",
        req_start.elapsed(),
        resp.status()
    );

    if !resp.status().is_success() {
        return Err(BrowserError::CdpError(format!(
            "Chrome /json/version returned {}",
            resp.status()
        )));
    }

    let parse_start = tokio::time::Instant::now();
    let body: JsonVersion = resp.json().await.map_err(|e| {
        BrowserError::CdpError(format!("Failed to parse Chrome debug response: {e}"))
    })?;
    debug!("Response parsed in {:?}", parse_start.elapsed());

    Ok(body.web_socket_debugger_url)
}

/// Scan for Chrome processes with debug ports and verify accessibility
pub(in super::super) async fn scan_for_chrome_debug_port() -> Option<u16> {
    use std::process::Command;

    // Use ps to find Chrome processes with remote-debugging-port
    let output = Command::new("ps").args(["aux"]).output().ok()?;

    let ps_output = String::from_utf8_lossy(&output.stdout);

    // Find all Chrome processes with debug ports
    let mut found_ports = Vec::new();
    for line in ps_output.lines() {
        // Look for Chrome/Chromium processes with remote-debugging-port
        if (line.contains("chrome") || line.contains("Chrome") || line.contains("chromium"))
            && line.contains("--remote-debugging-port=")
        {
            // Extract the port number
            if let Some(port_str) = line.split("--remote-debugging-port=").nth(1) {
                // Take everything up to the next space or end of line
                let port_str = port_str.split_whitespace().next().unwrap_or(port_str);

                // Parse the port number
                if let Ok(port) = port_str.parse::<u16>() {
                    // Skip port 0 (means random port, not accessible)
                    if port > 0 {
                        found_ports.push(port);
                    }
                }
            }
        }
    }

    // Remove duplicates
    found_ports.sort_unstable();
    found_ports.dedup();

    info!(
        "Found {} Chrome process(es) with debug ports: {:?}",
        found_ports.len(),
        found_ports
    );

    // Test each found port to see if it's accessible (test in parallel for speed)
    if found_ports.is_empty() {
        return None;
    }

    debug!("Testing {} port(s) for accessibility...", found_ports.len());
    let test_start = tokio::time::Instant::now();

    // Create futures for testing all ports in parallel
    let mut port_tests = Vec::new();
    for port in found_ports {
        let test_future = async move {
            let url = format!("http://127.0.0.1:{port}/json/version");
            let client = Client::builder()
                .no_proxy()
                .timeout(Duration::from_millis(200)) // Shorter timeout for parallel tests
                .build()
                .ok()?;

            match client.get(&url).send().await {
                Ok(resp) if resp.status().is_success() => {
                    debug!("Chrome port {} is accessible", port);
                    Some(port)
                }
                Ok(resp) => {
                    debug!("Chrome port {} returned status: {}", port, resp.status());
                    None
                }
                Err(_) => {
                    debug!("Could not connect to Chrome port {}", port);
                    None
                }
            }
        };
        port_tests.push(test_future);
    }

    // Test all ports in parallel and return the first accessible one
    let results = futures::future::join_all(port_tests).await;
    debug!(
        "Port accessibility tests completed in {:?}",
        test_start.elapsed()
    );

    if let Some(port) = results.into_iter().flatten().next() {
        info!("Verified Chrome debug port at {} is accessible", port);
        return Some(port);
    }

    warn!("No accessible Chrome debug ports found");
    None
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ChromeCommandArgs {
    Status,
    WsUrl(String),
    HostPort { host: Option<String>, port: Option<u16> },
}

pub(super) fn parse_port_from_ws(ws: &str) -> Option<u16> {
    // Best-effort parse: ws://host:port/... or ws://[::1]:port/...
    let after_scheme = ws.split("//").nth(1)?;
    let hostport = after_scheme.split('/').next()?;
    let port_str = hostport.rsplit_once(':')?.1;
    port_str.parse::<u16>().ok()
}

pub(super) fn parse_chrome_command_args(command_text: &str) -> ChromeCommandArgs {
    let parts: Vec<&str> = command_text.split_whitespace().collect();
    if parts.first().is_some_and(|first| *first == "status") {
        return ChromeCommandArgs::Status;
    }

    if let Some(first) = parts.first()
        && (first.starts_with("ws://") || first.starts_with("wss://"))
    {
        return ChromeCommandArgs::WsUrl((*first).to_string());
    }

    let mut host: Option<String> = None;
    let mut port: Option<u16> = None;

    if let Some(first) = parts.first()
        && let Some((h, p)) = first.rsplit_once(':')
        && let Ok(pn) = p.parse::<u16>()
    {
        host = Some(h.to_string());
        port = Some(pn);
    }

    if host.is_none() && port.is_none() {
        if let Some(first) = parts.first()
            && let Ok(pn) = first.parse::<u16>()
        {
            port = Some(pn);
        } else if parts.len() >= 2
            && let Some(second) = parts.get(1)
            && let Ok(pn) = second.parse::<u16>()
        {
            host = parts.first().map(|v| (*v).to_string());
            port = Some(pn);
        }
    }

    ChromeCommandArgs::HostPort { host, port }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CdpConnectChoice {
    pub(super) attempted_via_cached_ws: bool,
    pub(super) cached_port_for_fallback: Option<u16>,
    pub(super) connect_ws: Option<String>,
    pub(super) connect_host: Option<String>,
    pub(super) connect_port: Option<u16>,
}

pub(super) fn choose_cdp_connect_target(
    host: Option<String>,
    port: Option<u16>,
    cached_port: Option<u16>,
    cached_ws: Option<String>,
) -> CdpConnectChoice {
    if let Some(p) = port {
        return CdpConnectChoice {
            attempted_via_cached_ws: false,
            cached_port_for_fallback: None,
            connect_ws: None,
            connect_host: host,
            connect_port: Some(p),
        };
    }

    let cached_port_for_fallback = cached_port;
    if let Some(ws) = cached_ws {
        CdpConnectChoice {
            attempted_via_cached_ws: true,
            cached_port_for_fallback,
            connect_ws: Some(ws),
            connect_host: host,
            connect_port: None,
        }
    } else if let Some(p) = cached_port_for_fallback {
        CdpConnectChoice {
            attempted_via_cached_ws: false,
            cached_port_for_fallback,
            connect_ws: None,
            connect_host: host,
            connect_port: Some(p),
        }
    } else {
        CdpConnectChoice {
            attempted_via_cached_ws: false,
            cached_port_for_fallback: None,
            connect_ws: None,
            connect_host: host,
            connect_port: Some(0), // auto-detect
        }
    }
}


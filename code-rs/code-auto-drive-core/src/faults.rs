use anyhow::anyhow;
use chrono::Duration as ChronoDuration;
use code_core::error::{CodexErr, UnexpectedResponseError, UsageLimitReachedError};
use once_cell::sync::OnceCell;

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use reqwest::StatusCode;
use serde_json::json;

/// Scope flag – currently only `auto_drive` is recognised.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FaultScope {
    AutoDrive,
}

#[derive(Debug, Default)]
struct FaultConfig {
    disconnect: AtomicUsize,
    rate_limit: AtomicUsize,
    rate_limit_reset: Mutex<Option<FaultReset>>, // optional per-call reset hint
}

#[derive(Debug, Clone)]
pub(crate) enum FaultReset {
    Seconds(u64),
    Timestamp(Instant),
}

static CONFIG: OnceCell<HashMap<FaultScope, FaultConfig>> = OnceCell::new();

fn parse_fault_scope() -> Option<FaultScope> {
    match std::env::var("CODEX_FAULTS_SCOPE").ok().as_deref() {
        Some("auto_drive") => Some(FaultScope::AutoDrive),
        _ => None,
    }
}

fn parse_reset_hint() -> Option<FaultReset> {
    if let Ok(seconds) = std::env::var("CODEX_FAULTS_429_RESET") {
        if let Ok(value) = seconds.parse::<u64>() {
            return Some(FaultReset::Seconds(value));
        }
        if let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(&seconds) {
            let instant = Instant::now()
                + Duration::from_secs(parsed.signed_duration_since(chrono::Utc::now()).num_seconds().clamp(0, i64::MAX) as u64);
            return Some(FaultReset::Timestamp(instant));
        }
        if let Some(stripped) = seconds.strip_prefix("now+")
            && let Ok(value) = stripped.trim_end_matches('s').parse::<u64>()
        {
            return Some(FaultReset::Seconds(value));
        }
    }
    None
}

fn init_config() -> HashMap<FaultScope, FaultConfig> {
    let mut map = HashMap::new();
    if let Some(scope) = parse_fault_scope()
        && let Ok(spec) = std::env::var("CODEX_FAULTS")
    {
        let cfg = FaultConfig::default();
        for entry in spec.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            if let Some((label, count)) = entry.split_once(':')
                && let Ok(num) = count.parse::<usize>()
            {
                match label {
                    "disconnect" => cfg.disconnect.store(num, Ordering::Relaxed),
                    "429" => cfg.rate_limit.store(num, Ordering::Relaxed),
                    _ => {}
                }
            }
        }
        *cfg.rate_limit_reset.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = parse_reset_hint();
        map.insert(scope, cfg);
    }
    map
}

fn config() -> &'static HashMap<FaultScope, FaultConfig> {
    CONFIG.get_or_init(init_config)
}

/// Represents a fault to inject.
#[derive(Debug)]
pub enum InjectedFault {
    Disconnect,
    RateLimit { reset_hint: Option<FaultReset> },
}

/// Determine whether a fault should fire for the given scope.
pub fn next_fault(scope: FaultScope) -> Option<InjectedFault> {
    let cfg = config().get(&scope)?;
    if cfg.disconnect.load(Ordering::Relaxed) > 0 {
        let remaining = cfg.disconnect.fetch_sub(1, Ordering::Relaxed);
        if remaining > 0 {
            tracing::warn!("[faults] inject transient disconnect (remaining {})", remaining - 1);
            return Some(InjectedFault::Disconnect);
        }
    }
    if cfg.rate_limit.load(Ordering::Relaxed) > 0 {
        let remaining = cfg.rate_limit.fetch_sub(1, Ordering::Relaxed);
        if remaining > 0 {
            tracing::warn!("[faults] inject 429 rate limit (remaining {})", remaining - 1);
            return Some(InjectedFault::RateLimit {
                reset_hint: cfg.rate_limit_reset.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone(),
            });
        }
    }
    None
}

/// Convert a fault into an `anyhow::Error` matching production failures.
pub fn fault_to_error(fault: InjectedFault) -> anyhow::Error {
    match fault {
        InjectedFault::Disconnect => anyhow!("model stream error: stream disconnected before completion"),
        InjectedFault::RateLimit { reset_hint } => match reset_hint {
            Some(FaultReset::Seconds(secs)) => anyhow!(CodexErr::UsageLimitReached(UsageLimitReachedError {
                plan_type: None,
                resets_in_seconds: Some(secs),
            })),
            Some(FaultReset::Timestamp(instant)) => {
                let reset_at = chrono::Utc::now()
                    + ChronoDuration::from_std(instant.saturating_duration_since(Instant::now()))
                        .unwrap_or_else(|_| ChronoDuration::seconds(0));
                let body = json!({
                    "error": {
                        "reset_at": reset_at.to_rfc3339(),
                    }
                })
                .to_string();
                anyhow!(CodexErr::UnexpectedStatus(UnexpectedResponseError {
                    status: StatusCode::TOO_MANY_REQUESTS,
                    body,
                    request_id: None,
                }))
            }
            None => anyhow!(CodexErr::UnexpectedStatus(UnexpectedResponseError {
                status: StatusCode::TOO_MANY_REQUESTS,
                body: json!({ "error": { "message": "fault injector 429" } }).to_string(),
                request_id: None,
            })),
        },
    }
}

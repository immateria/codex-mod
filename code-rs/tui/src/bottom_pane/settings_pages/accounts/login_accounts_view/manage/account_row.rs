use base64::Engine;
use chrono::{DateTime, Utc};
use code_core::auth_accounts::StoredAccount;
use code_login::AuthMode;
use serde_json::Value as JsonValue;

use crate::account_label::account_display_label;

use super::{AccountRow, CHATGPT_REFRESH_INTERVAL_DAYS};

impl AccountRow {
    pub(super) fn from_stored(account: StoredAccount, active_id: Option<&str>) -> Self {
        let id = account.id.clone();
        let label = account_display_label(&account);
        let mode = account.mode;
        let mut detail_parts: Vec<String> = Vec::new();
        let now = Utc::now();

        if mode.is_chatgpt()
            && let Some(plan) = account
                .tokens
                .as_ref()
                .and_then(|t| t.id_token.get_chatgpt_plan_type())
            {
                detail_parts.push(format!("Plan: {plan}"));
            }

        if let Some(email) = account
            .tokens
            .as_ref()
            .and_then(|token| token.id_token.email.as_ref())
        {
            detail_parts.push(format!("Email: {email}"));
        }

        if let Some(last_refresh) = account.last_refresh {
            detail_parts.push(format!("Refreshed: {}", format_timestamp(last_refresh)));
            let refresh_due = last_refresh + chrono::Duration::days(CHATGPT_REFRESH_INTERVAL_DAYS);
            detail_parts.push(format!(
                "Renews: {} ({})",
                format_timestamp(refresh_due),
                format_relative_time(refresh_due, now)
            ));
        }

        if let Some(access_expires_at) = account
            .tokens
            .as_ref()
            .and_then(|token| jwt_expiry(&token.access_token))
        {
            detail_parts.push(format!(
                "Access expires: {} ({})",
                format_timestamp(access_expires_at),
                format_relative_time(access_expires_at, now)
            ));
        }

        if mode == AuthMode::ApiKey {
            detail_parts.push("Type: API key account".to_string());
        }

        if let Some(created_at) = account.created_at {
            detail_parts.push(format!("Connected: {}", format_timestamp(created_at)));
        }

        let is_active = active_id.is_some_and(|candidate| candidate == id);

        Self {
            id,
            label,
            detail_items: detail_parts,
            mode,
            is_active,
        }
    }
}

fn format_timestamp(ts: DateTime<Utc>) -> String {
    ts.with_timezone(&chrono::Local)
        .format("%Y-%m-%d %H:%M")
        .to_string()
}

fn format_relative_time(target: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let delta = target - now;
    let seconds = delta.num_seconds();
    if seconds >= 0 {
        if seconds < 60 {
            "in <1m".to_string()
        } else if seconds < 3600 {
            format!("in {}m", seconds / 60)
        } else if seconds < 86_400 {
            format!("in {}h", seconds / 3600)
        } else {
            format!("in {}d", seconds / 86_400)
        }
    } else {
        let past = seconds.saturating_abs();
        if past < 60 {
            "<1m ago".to_string()
        } else if past < 3600 {
            format!("{}m ago", past / 60)
        } else if past < 86_400 {
            format!("{}h ago", past / 3600)
        } else {
            format!("{}d ago", past / 86_400)
        }
    }
}

fn jwt_expiry(token: &str) -> Option<DateTime<Utc>> {
    let mut parts = token.split('.');
    let (Some(header), Some(payload), Some(sig), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return None;
    };
    if header.is_empty() || payload.is_empty() || sig.is_empty() {
        return None;
    }

    let payload_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .ok()?;
    let payload_json = serde_json::from_slice::<JsonValue>(&payload_bytes).ok()?;
    let exp = payload_json.get("exp")?.as_i64()?;
    DateTime::from_timestamp(exp, 0)
}

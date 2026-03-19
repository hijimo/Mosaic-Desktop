use crate::protocol::types::{CreditsSnapshot, PlanType, RateLimitSnapshot, RateLimitWindow};
use http::HeaderMap;
use serde::Deserialize;
use std::collections::BTreeSet;
use std::fmt::Display;

#[derive(Debug)]
pub struct RateLimitError {
    pub message: String,
}

impl Display for RateLimitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

pub fn parse_all_rate_limits(headers: &HeaderMap) -> Vec<RateLimitSnapshot> {
    let mut snapshots = Vec::new();
    if let Some(snapshot) = parse_rate_limit_for_limit(headers, None) {
        snapshots.push(snapshot);
    }

    let mut limit_ids: BTreeSet<String> = BTreeSet::new();
    for name in headers.keys() {
        let header_name = name.as_str().to_ascii_lowercase();
        if let Some(limit_id) = header_name_to_limit_id(&header_name) {
            if limit_id != "codex" {
                limit_ids.insert(limit_id);
            }
        }
    }

    for limit_id in limit_ids {
        if let Some(snapshot) = parse_rate_limit_for_limit(headers, Some(limit_id.as_str())) {
            if has_rate_limit_data(&snapshot) {
                snapshots.push(snapshot);
            }
        }
    }

    snapshots
}

pub fn parse_rate_limit_for_limit(
    headers: &HeaderMap,
    limit_id: Option<&str>,
) -> Option<RateLimitSnapshot> {
    let normalized_limit = limit_id
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or("codex")
        .to_ascii_lowercase()
        .replace('_', "-");
    let prefix = format!("x-{normalized_limit}");
    let primary = parse_rate_limit_window(
        headers,
        &format!("{prefix}-primary-used-percent"),
        &format!("{prefix}-primary-window-minutes"),
        &format!("{prefix}-primary-reset-at"),
    );

    let secondary = parse_rate_limit_window(
        headers,
        &format!("{prefix}-secondary-used-percent"),
        &format!("{prefix}-secondary-window-minutes"),
        &format!("{prefix}-secondary-reset-at"),
    );

    let normalized_limit_id = normalize_limit_id(normalized_limit);
    let credits = parse_credits_snapshot(headers);

    Some(RateLimitSnapshot {
        limit_id: Some(normalized_limit_id),
        limit_name: None,
        primary,
        secondary,
        credits,
        plan_type: None,
    })
}

#[derive(Debug, Deserialize)]
struct RateLimitEventWindow {
    used_percent: f64,
    window_minutes: Option<i64>,
    reset_at: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct RateLimitEventDetails {
    primary: Option<RateLimitEventWindow>,
    secondary: Option<RateLimitEventWindow>,
}

#[derive(Debug, Deserialize)]
struct RateLimitEventCredits {
    has_credits: bool,
    unlimited: bool,
    balance: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RateLimitEvent {
    #[serde(rename = "type")]
    kind: String,
    plan_type: Option<PlanType>,
    rate_limits: Option<RateLimitEventDetails>,
    credits: Option<RateLimitEventCredits>,
    metered_limit_name: Option<String>,
    limit_name: Option<String>,
}

pub fn parse_rate_limit_event(payload: &str) -> Option<RateLimitSnapshot> {
    let event: RateLimitEvent = serde_json::from_str(payload).ok()?;
    if event.kind != "codex.rate_limits" {
        return None;
    }
    let (primary, secondary) = match event.rate_limits.as_ref() {
        Some(details) => (
            map_event_window(details.primary.as_ref()),
            map_event_window(details.secondary.as_ref()),
        ),
        None => (None, None),
    };
    let credits = event.credits.map(|credits| CreditsSnapshot {
        has_credits: credits.has_credits,
        unlimited: credits.unlimited,
        balance: credits.balance,
    });
    let limit_id = event
        .metered_limit_name
        .or(event.limit_name)
        .map(normalize_limit_id);
    Some(RateLimitSnapshot {
        limit_id: Some(limit_id.unwrap_or_else(|| "codex".to_string())),
        limit_name: None,
        primary,
        secondary,
        credits,
        plan_type: event.plan_type,
    })
}

fn map_event_window(window: Option<&RateLimitEventWindow>) -> Option<RateLimitWindow> {
    let window = window?;
    Some(RateLimitWindow {
        used_percent: window.used_percent,
        window_minutes: window.window_minutes,
        resets_at: window.reset_at,
    })
}

fn parse_rate_limit_window(
    headers: &HeaderMap,
    used_percent_header: &str,
    window_minutes_header: &str,
    resets_at_header: &str,
) -> Option<RateLimitWindow> {
    let used_percent: Option<f64> = parse_header_f64(headers, used_percent_header);

    used_percent.and_then(|used_percent| {
        let window_minutes = parse_header_i64(headers, window_minutes_header);
        let resets_at = parse_header_i64(headers, resets_at_header);

        let has_data = used_percent != 0.0
            || window_minutes.map_or(false, |minutes| minutes != 0)
            || resets_at.is_some();

        if has_data {
            Some(RateLimitWindow {
                used_percent,
                window_minutes,
                resets_at,
            })
        } else {
            None
        }
    })
}

fn parse_credits_snapshot(headers: &HeaderMap) -> Option<CreditsSnapshot> {
    let has_credits = parse_header_bool(headers, "x-codex-credits-has-credits")?;
    let unlimited = parse_header_bool(headers, "x-codex-credits-unlimited")?;
    let balance = parse_header_str(headers, "x-codex-credits-balance")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(std::string::ToString::to_string);
    Some(CreditsSnapshot {
        has_credits,
        unlimited,
        balance,
    })
}

fn parse_header_f64(headers: &HeaderMap, name: &str) -> Option<f64> {
    parse_header_str(headers, name)?
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
}

fn parse_header_i64(headers: &HeaderMap, name: &str) -> Option<i64> {
    parse_header_str(headers, name)?.parse::<i64>().ok()
}

fn parse_header_bool(headers: &HeaderMap, name: &str) -> Option<bool> {
    let raw = parse_header_str(headers, name)?;
    if raw.eq_ignore_ascii_case("true") || raw == "1" {
        Some(true)
    } else if raw.eq_ignore_ascii_case("false") || raw == "0" {
        Some(false)
    } else {
        None
    }
}

fn parse_header_str<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name)?.to_str().ok()
}

fn has_rate_limit_data(snapshot: &RateLimitSnapshot) -> bool {
    snapshot.primary.is_some() || snapshot.secondary.is_some() || snapshot.credits.is_some()
}

fn header_name_to_limit_id(header_name: &str) -> Option<String> {
    let suffix = "-primary-used-percent";
    let prefix = header_name.strip_suffix(suffix)?;
    let limit = prefix.strip_prefix("x-")?;
    Some(normalize_limit_id(limit.to_string()))
}

fn normalize_limit_id(name: impl Into<String>) -> String {
    name.into().trim().to_ascii_lowercase().replace('-', "_")
}

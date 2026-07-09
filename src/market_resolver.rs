use chrono::{DateTime, Duration, TimeZone, Utc};
use serde::{Deserialize, Serialize};

use crate::types::{Event, Market};

pub const OUTCOME_UP: &str = "up";
pub const OUTCOME_DOWN: &str = "down";
pub const OUTCOME_UNKNOWN: &str = "unknown";

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Filter {
    pub asset: String,
    pub interval: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Candidate {
    pub event: Event,
    pub market: Market,
    pub token_ids: Vec<String>,
}

pub fn normalize_outcome(value: &str) -> &'static str {
    match value.trim().to_ascii_lowercase().as_str() {
        "up" => OUTCOME_UP,
        "down" => OUTCOME_DOWN,
        _ => OUTCOME_UNKNOWN,
    }
}

pub fn outcome_for_token(
    winning_token_id: &str,
    up_token_id: &str,
    down_token_id: &str,
) -> &'static str {
    let winning = winning_token_id.trim();
    if winning.is_empty() {
        return OUTCOME_UNKNOWN;
    }
    if winning == up_token_id.trim() {
        OUTCOME_UP
    } else if winning == down_token_id.trim() {
        OUTCOME_DOWN
    } else {
        OUTCOME_UNKNOWN
    }
}

pub fn up_down_token_ids(outcomes: &[String], token_ids: &[String]) -> (String, String) {
    if outcomes.len() != token_ids.len() {
        return (String::new(), String::new());
    }
    let mut up = String::new();
    let mut down = String::new();
    for (outcome, token) in outcomes.iter().zip(token_ids) {
        match outcome.trim().to_ascii_lowercase().as_str() {
            "up" | "yes" => up = token.clone(),
            "down" | "no" => down = token.clone(),
            _ => {}
        }
    }
    (up, down)
}

pub fn infer_timeframe(parts: &[&str]) -> String {
    let text = parts.join(" ").to_ascii_lowercase();
    for tf in ["15m", "15 min", "15-minute", "5m", "5 min", "5-minute"] {
        if text.contains(tf) {
            return if tf.starts_with('5') { "5m" } else { "15m" }.into();
        }
    }
    String::new()
}

pub fn infer_timeframe_from_window(start: DateTime<Utc>, end: DateTime<Utc>) -> String {
    if start >= end {
        return String::new();
    }
    let d = end - start;
    if d <= Duration::minutes(6) {
        "5m".into()
    } else if d <= Duration::minutes(16) {
        "15m".into()
    } else {
        String::new()
    }
}

pub fn window_from_slug(slug: &str, timeframe: &str) -> Option<(DateTime<Utc>, DateTime<Utc>)> {
    let epoch = slug.trim().rsplit('-').next()?.parse::<i64>().ok()?;
    if epoch <= 0 {
        return None;
    }
    let duration = parse_timeframe(timeframe)?;
    let start = Utc.timestamp_opt(epoch, 0).single()?;
    Some((start, start + duration))
}

pub fn crypto_window_slug(asset: &str, timeframe: &str, window_start: DateTime<Utc>) -> String {
    let prefix = match asset.trim().to_ascii_uppercase().as_str() {
        "BTC" => "btc",
        "ETH" => "eth",
        "SOL" => "sol",
        "XRP" => "xrp",
        "DOGE" => "doge",
        "BNB" => "bnb",
        "HYPE" => "hype",
        _ => return String::new(),
    };
    if !matches!(timeframe, "5m" | "15m" | "4h") {
        return String::new();
    }
    format!("{prefix}-updown-{timeframe}-{}", window_start.timestamp())
}

pub fn asset_search_queries(asset: &str) -> Vec<String> {
    asset_search_names(asset)
        .into_iter()
        .flat_map(|name| [format!("{name} 5m"), format!("{name} 15m")])
        .collect()
}

pub fn asset_mentioned(asset: &str, text: &str) -> bool {
    let text = text.to_ascii_lowercase();
    text.contains(&asset.trim().to_ascii_lowercase())
        || asset_search_names(asset)
            .iter()
            .any(|name| text.contains(name))
}

pub fn parse_json_string_list(raw: &str) -> serde_json::Result<Vec<String>> {
    if raw.trim().is_empty() {
        return Ok(vec![]);
    }
    serde_json::from_str(raw)
}

pub fn parse_token_ids(raw: &str) -> Vec<String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return vec![];
    }
    serde_json::from_str(raw).unwrap_or_else(|_| vec![raw.into()])
}

pub fn query(filter: &Filter) -> String {
    let asset = filter.asset.trim();
    let interval = filter.interval.trim();
    let mut q = if interval.is_empty() {
        asset.to_string()
    } else {
        crypto_search_name(asset).unwrap_or(asset).to_string()
    };
    if !interval.is_empty() {
        if !q.is_empty() {
            q.push(' ');
        }
        q.push_str(interval);
        q.push_str(" updown");
    }
    if q.is_empty() {
        "crypto".into()
    } else {
        q
    }
}

pub fn select(events: &[Event], filter: &Filter) -> Vec<Candidate> {
    let mut out = Vec::new();
    for event in events.iter().filter(|e| e.active && !e.closed) {
        for market in event.markets.iter().filter(|m| m.active && !m.closed) {
            if matches_asset(event, market, &filter.asset)
                && matches_interval(event, market, &filter.interval)
            {
                out.push(Candidate {
                    event: event.clone(),
                    market: market.clone(),
                    token_ids: parse_token_ids(&market.clob_token_ids),
                });
            }
        }
    }
    out
}

fn parse_timeframe(timeframe: &str) -> Option<Duration> {
    match timeframe {
        "5m" => Some(Duration::minutes(5)),
        "15m" => Some(Duration::minutes(15)),
        "4h" => Some(Duration::hours(4)),
        _ => None,
    }
}

fn asset_search_names(asset: &str) -> Vec<String> {
    match asset.trim().to_ascii_uppercase().as_str() {
        "BTC" => vec!["bitcoin".into()],
        "ETH" => vec!["ethereum".into()],
        "SOL" => vec!["solana".into()],
        "XRP" => vec!["xrp".into()],
        "DOGE" => vec!["dogecoin".into(), "doge".into()],
        "BNB" => vec!["bnb".into(), "binance coin".into()],
        other => vec![other.to_ascii_lowercase()],
    }
}

fn crypto_search_name(asset: &str) -> Option<&'static str> {
    match asset.trim().to_ascii_uppercase().as_str() {
        "BTC" => Some("bitcoin"),
        "ETH" => Some("ethereum"),
        "SOL" => Some("solana"),
        "XRP" => Some("xrp"),
        "DOGE" => Some("doge"),
        "BNB" => Some("bnb"),
        "HYPE" => Some("hyperliquid"),
        _ => None,
    }
}

fn matches_asset(event: &Event, market: &Market, asset: &str) -> bool {
    let asset = asset.trim();
    if asset.is_empty() {
        return true;
    }
    let text = format!(
        "{} {} {} {}",
        event.title, event.slug, market.question, market.slug
    )
    .to_ascii_lowercase();
    text.contains(&asset.to_ascii_lowercase())
        || crypto_search_name(asset).is_some_and(|name| text.contains(name))
}

fn matches_interval(event: &Event, market: &Market, interval: &str) -> bool {
    let interval = interval.trim().to_ascii_lowercase();
    if interval.is_empty() {
        return true;
    }
    format!(
        "{} {} {} {}",
        event.title, event.slug, market.question, market.slug
    )
    .to_ascii_lowercase()
    .contains(&interval)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_up_down_tokens_and_outcome() {
        let (up, down) = up_down_token_ids(&["Yes".into(), "No".into()], &["1".into(), "2".into()]);
        assert_eq!((up.as_str(), down.as_str()), ("1", "2"));
        assert_eq!(outcome_for_token("2", &up, &down), OUTCOME_DOWN);
    }

    #[test]
    fn timeframe_slug_and_queries_match_go_rules() {
        assert_eq!(
            infer_timeframe(&["BTC 15-minute updown", "5m distractor"]),
            "15m"
        );
        let start = Utc.timestamp_opt(1_700_000_000, 0).unwrap();
        assert_eq!(
            crypto_window_slug("BTC", "5m", start),
            "btc-updown-5m-1700000000"
        );
        assert_eq!(
            window_from_slug("btc-updown-5m-1700000000", "5m")
                .unwrap()
                .1,
            start + Duration::minutes(5)
        );
        assert_eq!(
            asset_search_queries("DOGE"),
            vec!["dogecoin 5m", "dogecoin 15m", "doge 5m", "doge 15m"]
        );
    }

    #[test]
    fn selects_active_matching_crypto_markets() {
        let market = Market {
            active: true,
            closed: false,
            question: "Bitcoin Up or Down 5m".into(),
            slug: "btc-updown-5m-1".into(),
            clob_token_ids: r#"["u","d"]"#.into(),
            ..Default::default()
        };
        let event = Event {
            active: true,
            closed: false,
            title: "Bitcoin 5m".into(),
            markets: vec![market],
            ..Default::default()
        };
        let got = select(
            &[event],
            &Filter {
                asset: "BTC".into(),
                interval: "5m".into(),
            },
        );
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].token_ids, vec!["u", "d"]);
        assert_eq!(
            query(&Filter {
                asset: "BTC".into(),
                interval: "5m".into()
            }),
            "bitcoin 5m updown"
        );
    }
}

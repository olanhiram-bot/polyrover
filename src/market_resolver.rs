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

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CryptoMarket {
    pub id: String,
    pub condition_id: String,
    pub asset: String,
    pub timeframe: String,
    pub slug: String,
    pub question: String,
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub active: bool,
    pub closed: bool,
    pub accepting: bool,
    pub up_token_id: String,
    pub down_token_id: String,
}

pub fn discover_window_markets(
    client: &crate::Client,
    assets: &[String],
    start: DateTime<Utc>,
    through: DateTime<Utc>,
) -> crate::Result<Vec<CryptoMarket>> {
    if start > through {
        return Ok(Vec::new());
    }
    let mut targets = std::collections::BTreeMap::new();
    let mut window = start;
    while window <= through {
        for asset in assets {
            let slug = crypto_window_slug(asset, "5m", window);
            if !slug.is_empty() {
                targets.insert(slug, (asset.trim().to_ascii_uppercase(), window));
            }
        }
        window += Duration::minutes(5);
    }
    let rows = client.markets(&crate::gamma::MarketParams {
        slug: targets.keys().cloned().collect(),
        active: Some(true),
        closed: Some(false),
        ..Default::default()
    })?;
    let mut markets = rows
        .into_iter()
        .filter_map(|row| crypto_market_from_row(row, &targets))
        .collect::<Vec<_>>();
    markets.sort_by(|left, right| {
        (left.start_time, &left.asset, &left.condition_id).cmp(&(
            right.start_time,
            &right.asset,
            &right.condition_id,
        ))
    });
    markets.dedup_by(|left, right| left.condition_id == right.condition_id);
    Ok(markets)
}

pub fn discover_complete_window_markets(
    client: &crate::Client,
    assets: &[String],
    start: DateTime<Utc>,
    through: DateTime<Utc>,
) -> crate::Result<Vec<CryptoMarket>> {
    if assets.is_empty()
        || start > through
        || start.timestamp().rem_euclid(300) != 0
        || through.timestamp().rem_euclid(300) != 0
    {
        return Err(crate::Error::Invalid(
            "invalid complete 5m discovery range or assets".into(),
        ));
    }
    let normalized = assets
        .iter()
        .map(|asset| asset.trim().to_ascii_uppercase())
        .collect::<std::collections::BTreeSet<_>>();
    if normalized.len() != assets.len() || normalized.iter().any(|asset| asset.is_empty()) {
        return Err(crate::Error::Invalid(
            "complete 5m discovery assets must be nonempty and unique".into(),
        ));
    }

    let mut targets = std::collections::BTreeMap::new();
    let mut window = start;
    while window <= through {
        for asset in &normalized {
            let slug = crypto_window_slug(asset, "5m", window);
            if slug.is_empty() {
                return Err(crate::Error::Invalid(format!(
                    "unsupported complete 5m discovery asset {asset}"
                )));
            }
            targets.insert(slug, (asset.clone(), window));
        }
        window += Duration::minutes(5);
    }

    let mut found = discover_window_markets(client, assets, start, through)?
        .into_iter()
        .filter(|market| complete_market_valid(market, &targets))
        .map(|market| (market.slug.clone(), market))
        .collect::<std::collections::BTreeMap<_, _>>();
    remove_identity_conflicts(&mut found);
    let batch_missing = targets
        .keys()
        .filter(|slug| !found.contains_key(*slug))
        .cloned()
        .collect::<Vec<_>>();
    for slug in &batch_missing {
        match client.market_by_slug(slug) {
            Ok(row) => {
                if let Some(market) = crypto_market_from_row(row, &targets)
                    .filter(|market| complete_market_valid(market, &targets))
                {
                    found.insert(slug.clone(), market);
                }
            }
            Err(crate::Error::Api { status: 404, .. }) => {}
            Err(error) => return Err(error),
        }
    }

    remove_identity_conflicts(&mut found);
    let missing = targets
        .keys()
        .filter(|slug| !found.contains_key(*slug))
        .cloned()
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(crate::Error::Invalid(format!(
            "incomplete 5m market discovery: found {}/{}; missing {}",
            targets.len() - missing.len(),
            targets.len(),
            missing.join(",")
        )));
    }
    let mut markets = found.into_values().collect::<Vec<_>>();
    markets.sort_by(|left, right| {
        (left.start_time, &left.asset, &left.condition_id).cmp(&(
            right.start_time,
            &right.asset,
            &right.condition_id,
        ))
    });
    Ok(markets)
}

fn remove_identity_conflicts(markets: &mut std::collections::BTreeMap<String, CryptoMarket>) {
    let mut conditions = std::collections::BTreeSet::new();
    let mut tokens = std::collections::BTreeSet::new();
    markets.retain(|_, market| {
        let unique = !conditions.contains(&market.condition_id)
            && !tokens.contains(&market.up_token_id)
            && !tokens.contains(&market.down_token_id);
        if unique {
            conditions.insert(market.condition_id.clone());
            tokens.insert(market.up_token_id.clone());
            tokens.insert(market.down_token_id.clone());
        }
        unique
    });
}

fn crypto_market_from_row(
    row: Market,
    targets: &std::collections::BTreeMap<String, (String, DateTime<Utc>)>,
) -> Option<CryptoMarket> {
    let (asset, fallback_start) = targets.get(row.slug.trim())?;
    let condition_id = row.condition_id.trim();
    if condition_id.is_empty() {
        return None;
    }
    let tokens = parse_token_ids(&row.clob_token_ids);
    let (mut up_token_id, mut down_token_id) = up_down_token_ids(&row.outcomes.0, &tokens);
    if (up_token_id.is_empty() || down_token_id.is_empty()) && tokens.len() >= 2 {
        up_token_id = tokens[0].clone();
        down_token_id = tokens[1].clone();
    }
    if up_token_id.is_empty() || down_token_id.is_empty() {
        return None;
    }
    let (window_start, window_end) = window_from_slug(&row.slug, "5m")
        .unwrap_or((*fallback_start, *fallback_start + Duration::minutes(5)));
    Some(CryptoMarket {
        id: row.id,
        condition_id: condition_id.into(),
        asset: asset.clone(),
        timeframe: "5m".into(),
        slug: row.slug,
        question: row.question,
        start_time: Some(window_start),
        end_time: Some(window_end),
        active: row.active,
        closed: row.closed,
        accepting: row.active && !row.closed,
        up_token_id,
        down_token_id,
    })
}

fn complete_market_valid(
    market: &CryptoMarket,
    targets: &std::collections::BTreeMap<String, (String, DateTime<Utc>)>,
) -> bool {
    targets.get(&market.slug).is_some_and(|(asset, start)| {
        market.asset == *asset
            && market.start_time == Some(*start)
            && market.end_time == Some(*start + Duration::minutes(5))
            && market.active
            && !market.closed
            && market.accepting
            && market.up_token_id != market.down_token_id
    })
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
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
    };

    fn market_json(slug: &str) -> serde_json::Value {
        serde_json::json!({
            "id": format!("gamma-{slug}"),
            "conditionId": format!("condition-{slug}"),
            "slug": slug,
            "question": "Up or Down",
            "active": true,
            "closed": false,
            "outcomes": ["Up", "Down"],
            "clobTokenIds": format!("[\"{slug}-up\",\"{slug}-down\"]")
        })
    }

    fn mock_client(
        responses: Vec<(u16, String)>,
    ) -> (crate::Client, thread::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let mut requests = Vec::new();
            for (status, body) in responses {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = [0; 65_536];
                let read = stream.read(&mut request).unwrap();
                let request = String::from_utf8_lossy(&request[..read]);
                requests.push(request.lines().next().unwrap_or_default().to_string());
                let reason = if status == 200 { "OK" } else { "Not Found" };
                write!(
                    stream,
                    "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                )
                .unwrap();
            }
            requests
        });
        let base = format!("http://{address}");
        let client = crate::Client::new(crate::ClientConfig {
            gamma_base_url: base.clone(),
            clob_base_url: base.clone(),
            data_base_url: base,
        })
        .unwrap();
        (client, server)
    }

    fn three_window_slugs(start: DateTime<Utc>) -> Vec<String> {
        [0, 5, 10]
            .into_iter()
            .map(|minutes| crypto_window_slug("BTC", "5m", start + Duration::minutes(minutes)))
            .collect()
    }

    #[test]
    fn complete_discovery_uses_one_batch_without_fallback() {
        let start = Utc.timestamp_opt(1_700_000_100, 0).unwrap();
        let slugs = three_window_slugs(start);
        let body = serde_json::to_string(
            &slugs
                .iter()
                .map(|slug| market_json(slug))
                .collect::<Vec<_>>(),
        )
        .unwrap();
        let (client, server) = mock_client(vec![(200, body)]);

        let rows = discover_complete_window_markets(
            &client,
            &["BTC".into()],
            start,
            start + Duration::minutes(10),
        )
        .unwrap();
        let requests = server.join().unwrap();

        assert_eq!(rows.len(), 3);
        assert_eq!(requests.len(), 1);
    }

    #[test]
    fn partial_batch_fetches_only_missing_slug() {
        let start = Utc.timestamp_opt(1_700_000_100, 0).unwrap();
        let slugs = three_window_slugs(start);
        let batch = serde_json::to_string(
            &slugs[..2]
                .iter()
                .map(|slug| market_json(slug))
                .collect::<Vec<_>>(),
        )
        .unwrap();
        let fallback = serde_json::to_string(&market_json(&slugs[2])).unwrap();
        let (client, server) = mock_client(vec![(200, batch), (200, fallback)]);

        let rows = discover_complete_window_markets(
            &client,
            &["BTC".into()],
            start,
            start + Duration::minutes(10),
        )
        .unwrap();
        let requests = server.join().unwrap();

        assert_eq!(rows.len(), 3);
        assert_eq!(requests.len(), 2);
        assert!(requests[1].contains(&format!("/markets/slug/{}", slugs[2])));
    }

    #[test]
    fn duplicate_batch_identity_is_retried_by_slug() {
        let start = Utc.timestamp_opt(1_700_000_100, 0).unwrap();
        let slugs = three_window_slugs(start);
        let mut duplicate = market_json(&slugs[2]);
        duplicate["conditionId"] = market_json(&slugs[0])["conditionId"].clone();
        let batch = serde_json::to_string(&vec![
            market_json(&slugs[0]),
            market_json(&slugs[1]),
            duplicate,
        ])
        .unwrap();
        let fallback = serde_json::to_string(&market_json(&slugs[2])).unwrap();
        let (client, server) = mock_client(vec![(200, batch), (200, fallback)]);

        let rows = discover_complete_window_markets(
            &client,
            &["BTC".into()],
            start,
            start + Duration::minutes(10),
        )
        .unwrap();
        let requests = server.join().unwrap();

        assert_eq!(rows.len(), 3);
        assert_eq!(requests.len(), 2);
        assert!(requests[1].contains(&format!("/markets/slug/{}", slugs[2])));
    }

    #[test]
    fn unresolved_fallback_reports_missing_slug() {
        let start = Utc.timestamp_opt(1_700_000_100, 0).unwrap();
        let slugs = three_window_slugs(start);
        let batch = serde_json::to_string(
            &slugs[..2]
                .iter()
                .map(|slug| market_json(slug))
                .collect::<Vec<_>>(),
        )
        .unwrap();
        let (client, server) = mock_client(vec![(200, batch), (404, "{}".into())]);

        let error = discover_complete_window_markets(
            &client,
            &["BTC".into()],
            start,
            start + Duration::minutes(10),
        )
        .unwrap_err();
        server.join().unwrap();

        assert_eq!(
            error.to_string(),
            format!(
                "incomplete 5m market discovery: found 2/3; missing {}",
                slugs[2]
            )
        );
    }

    #[test]
    fn failed_batch_does_not_fan_out() {
        let start = Utc.timestamp_opt(1_700_000_100, 0).unwrap();
        let (client, server) = mock_client(vec![(429, "{}".into())]);

        let error = discover_complete_window_markets(
            &client,
            &["BTC".into()],
            start,
            start + Duration::minutes(10),
        )
        .unwrap_err();
        let requests = server.join().unwrap();

        assert!(matches!(error, crate::Error::RateLimited { .. }));
        assert_eq!(requests.len(), 1);
    }

    #[test]
    fn strict_discovery_rejects_invalid_inputs_before_network_access() {
        let start = Utc.timestamp_opt(1_700_000_100, 0).unwrap();
        let cases = [
            (Vec::new(), start, start),
            (vec!["BTC".into(), "btc".into()], start, start),
            (
                vec!["BTC".into()],
                start + Duration::seconds(1),
                start + Duration::minutes(5),
            ),
            (vec!["BTC".into()], start + Duration::minutes(5), start),
        ];
        for (assets, from, through) in cases {
            let (client, server) = mock_client(Vec::new());
            assert!(discover_complete_window_markets(&client, &assets, from, through).is_err());
            assert!(server.join().unwrap().is_empty());
        }
    }

    #[test]
    fn discover_window_markets_resolves_gamma_tokens() -> crate::Result<()> {
        use std::{
            io::{Read, Write},
            net::TcpListener,
            thread,
        };

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0; 4096];
            let read = stream.read(&mut request).unwrap();
            let request = String::from_utf8_lossy(&request[..read]);
            assert!(request.contains("slug=btc-updown-5m-1700000100"));
            let body = r#"[{"id":"gamma-1","conditionId":"condition-1","slug":"btc-updown-5m-1700000100","question":"Bitcoin Up or Down","active":true,"closed":false,"outcomes":["Up","Down"],"clobTokenIds":"[\"up\",\"down\"]"}]"#;
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .unwrap();
        });
        let base = format!("http://{address}");
        let client = crate::Client::new(crate::ClientConfig {
            gamma_base_url: base.clone(),
            clob_base_url: base.clone(),
            data_base_url: base,
        })?;
        let start = Utc.timestamp_opt(1_700_000_100, 0).unwrap();
        let rows = discover_window_markets(&client, &["BTC".into()], start, start)?;
        server.join().unwrap();

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].condition_id, "condition-1");
        assert_eq!(rows[0].up_token_id, "up");
        assert_eq!(rows[0].down_token_id, "down");
        assert_eq!(rows[0].timeframe, "5m");
        Ok(())
    }

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

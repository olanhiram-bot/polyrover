use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;

use crate::{Error, Result};

pub const DEFAULT_MARKET_URL: &str = "wss://ws-subscriptions-clob.polymarket.com/ws/market";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Config {
    pub url: String,
    pub ping_interval_secs: u64,
    pub pong_timeout_secs: u64,
    pub reconnect: bool,
    pub reconnect_delay_secs: u64,
    pub reconnect_max_delay_secs: u64,
    pub reconnect_max: u32,
    pub level: u32,
    pub custom_feature_enabled: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            url: DEFAULT_MARKET_URL.into(),
            ping_interval_secs: 10,
            pong_timeout_secs: 30,
            reconnect: true,
            reconnect_delay_secs: 2,
            reconnect_max_delay_secs: 30,
            reconnect_max: 5,
            level: 0,
            custom_feature_enabled: false,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct PriceLevel {
    pub price: String,
    pub size: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct BookMessage {
    pub event_type: String,
    pub asset_id: String,
    pub market: String,
    pub timestamp: String,
    pub hash: String,
    #[serde(default)]
    pub bids: Vec<PriceLevel>,
    #[serde(default)]
    pub asks: Vec<PriceLevel>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct PriceChangeMessage {
    pub event_type: String,
    pub market: String,
    #[serde(default, rename = "price_changes")]
    pub changes: Vec<PriceChangeEntry>,
    pub timestamp: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct PriceChangeEntry {
    pub asset_id: String,
    pub price: String,
    pub side: String,
    pub size: String,
    pub hash: String,
    #[serde(default)]
    pub best_bid: String,
    #[serde(default)]
    pub best_ask: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct LastTradeMessage {
    pub event_type: String,
    pub asset_id: String,
    pub market: String,
    pub price: String,
    pub side: String,
    pub size: String,
    pub fee_rate_bps: String,
    pub timestamp: String,
    #[serde(default)]
    pub transaction_hash: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct TickSizeChangeMessage {
    pub event_type: String,
    pub asset_id: String,
    pub market: String,
    pub old_tick_size: String,
    pub new_tick_size: String,
    pub timestamp: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct BestBidAskMessage {
    pub event_type: String,
    pub asset_id: String,
    pub market: String,
    pub best_bid: String,
    pub best_ask: String,
    pub spread: String,
    pub timestamp: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct NewMarketMessage {
    pub event_type: String,
    pub id: String,
    pub question: String,
    pub market: String,
    pub slug: String,
    pub description: String,
    #[serde(rename = "assets_ids")]
    pub asset_ids: Vec<String>,
    pub outcomes: Vec<String>,
    pub event_message: HashMap<String, Value>,
    pub timestamp: String,
    pub tags: Vec<String>,
    pub condition_id: String,
    pub clob_token_ids: Vec<String>,
    pub active: bool,
    pub sports_market_type: String,
    pub line: String,
    pub game_start_time: String,
    pub order_price_min_tick_size: String,
    pub group_item_title: String,
    pub taker_base_fee: String,
    pub fees_enabled: bool,
    pub fee_schedule: HashMap<String, Value>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct MarketResolvedMessage {
    pub event_type: String,
    pub id: String,
    pub market: String,
    #[serde(rename = "assets_ids")]
    pub asset_ids: Vec<String>,
    pub winning_asset_id: String,
    pub winning_outcome: String,
    pub timestamp: String,
    pub tags: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum MarketEvent {
    Book(BookMessage),
    PriceChange(PriceChangeMessage),
    LastTrade(LastTradeMessage),
    TickSizeChange(TickSizeChangeMessage),
    BestBidAsk(BestBidAskMessage),
    NewMarket(Box<NewMarketMessage>),
    MarketResolved(MarketResolvedMessage),
    Ignored,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct RawMessage {
    pub observed_at_ms: i64,
    #[serde(default)]
    pub event_type: String,
    #[serde(default)]
    pub market: String,
    #[serde(default)]
    pub asset_id: String,
    pub payload: Value,
}

impl RawMessage {
    pub fn from_payload(payload: Value, observed_at_ms: i64) -> Self {
        Self {
            observed_at_ms,
            event_type: str_field(&payload, "event_type"),
            market: str_field(&payload, "market"),
            asset_id: str_field(&payload, "asset_id"),
            payload,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, PartialEq)]
pub struct StreamStatsSnapshot {
    pub r#type: String,
    pub stream: String,
    pub state: String,
    pub asset_ids: Vec<String>,
    pub messages_received: i64,
    pub duplicate_messages: i64,
    pub invalid_messages: i64,
    pub reconnects: i64,
    pub last_message_at_ms: i64,
    pub event_counts: HashMap<String, i64>,
}

#[derive(Clone, Debug)]
pub struct StreamStats {
    snapshot: StreamStatsSnapshot,
}

impl StreamStats {
    pub fn new(stream: &str) -> Self {
        Self {
            snapshot: StreamStatsSnapshot {
                r#type: "stream_stats".into(),
                stream: stream.into(),
                state: "idle".into(),
                ..Default::default()
            },
        }
    }
    pub fn set_subscriptions(&mut self, asset_ids: &[String]) {
        self.snapshot.asset_ids = asset_ids.to_vec();
    }
    pub fn mark_connected(&mut self) {
        self.snapshot.state = "connected".into();
    }
    pub fn mark_disconnected(&mut self) {
        self.snapshot.state = "disconnected".into();
    }
    pub fn record_event(&mut self, event_type: &str, at_ms: i64) {
        self.snapshot.messages_received += 1;
        self.snapshot.last_message_at_ms = at_ms;
        if !event_type.is_empty() {
            *self
                .snapshot
                .event_counts
                .entry(event_type.into())
                .or_default() += 1;
        }
    }
    pub fn record_duplicate(&mut self) {
        self.snapshot.duplicate_messages += 1;
    }
    pub fn record_invalid(&mut self) {
        self.snapshot.invalid_messages += 1;
    }
    pub fn record_reconnect(&mut self) {
        self.snapshot.reconnects += 1;
    }
    pub fn snapshot(&self) -> StreamStatsSnapshot {
        self.snapshot.clone()
    }
}

#[derive(Clone, Debug)]
pub struct Deduplicator {
    seen: HashMap<String, i64>,
    size: usize,
    ttl_ms: i64,
    pub input: i64,
    pub duplicates: i64,
    pub output: i64,
}

impl Deduplicator {
    pub fn new(size: usize, ttl_ms: i64) -> Self {
        Self {
            seen: HashMap::new(),
            size,
            ttl_ms,
            input: 0,
            duplicates: 0,
            output: 0,
        }
    }
    pub fn process_at(&mut self, data: &str, now_ms: i64) -> bool {
        self.input += 1;
        let Some(key) = extract_key(data) else {
            self.output += 1;
            return true;
        };
        if self
            .seen
            .get(&key)
            .is_some_and(|ts| now_ms - *ts < self.ttl_ms)
        {
            self.duplicates += 1;
            return false;
        }
        self.seen.insert(key, now_ms);
        self.evict(now_ms);
        self.output += 1;
        true
    }
    fn evict(&mut self, now_ms: i64) {
        self.seen.retain(|_, ts| now_ms - *ts < self.ttl_ms);
        if self.size == 0 {
            self.seen.clear();
            return;
        }
        while self.seen.len() > self.size {
            let oldest = self
                .seen
                .iter()
                .min_by_key(|(_, ts)| *ts)
                .map(|(k, _)| k.clone())
                .unwrap();
            self.seen.remove(&oldest);
        }
    }
}

pub fn parse_market_event(text: &str) -> Result<MarketEvent> {
    let value: Value = serde_json::from_str(text)?;
    match value
        .get("event_type")
        .and_then(Value::as_str)
        .unwrap_or_default()
    {
        "book" => Ok(MarketEvent::Book(serde_json::from_value(value)?)),
        "price_change" => Ok(MarketEvent::PriceChange(serde_json::from_value(value)?)),
        "last_trade_price" => Ok(MarketEvent::LastTrade(serde_json::from_value(value)?)),
        "tick_size_change" => Ok(MarketEvent::TickSizeChange(serde_json::from_value(value)?)),
        "best_bid_ask" => Ok(MarketEvent::BestBidAsk(serde_json::from_value(value)?)),
        "new_market" => Ok(MarketEvent::NewMarket(Box::new(serde_json::from_value(
            value,
        )?))),
        "market_resolved" => Ok(MarketEvent::MarketResolved(serde_json::from_value(value)?)),
        _ => Ok(MarketEvent::Ignored),
    }
}

pub fn market_subscription(asset_ids: &[String], cfg: &Config) -> Result<Value> {
    if asset_ids.is_empty() {
        return Err(Error::Invalid("asset_ids are required".into()));
    }
    if asset_ids.iter().any(|id| id.trim().is_empty()) {
        return Err(Error::Invalid("asset_ids cannot be blank".into()));
    }
    let mut msg = json!({"type":"market","assets_ids":asset_ids});
    if cfg.level > 0 {
        msg["level"] = json!(cfg.level);
    }
    if cfg.custom_feature_enabled {
        msg["custom_feature_enabled"] = json!(true);
    }
    Ok(msg)
}

pub fn split_array(data: &str) -> Option<Vec<Value>> {
    match serde_json::from_str::<Value>(data).ok()? {
        Value::Array(rows) => Some(rows),
        _ => None,
    }
}

pub fn extract_key(data: &str) -> Option<String> {
    let value: Value = serde_json::from_str(data).ok()?;
    let event_type = str_field(&value, "event_type");
    let hash = str_field(&value, "hash");
    let asset_id = str_field(&value, "asset_id");
    let price = str_field(&value, "price");
    let size = str_field(&value, "size");
    let market = str_field(&value, "market");
    let timestamp = str_field(&value, "timestamp");
    match event_type.as_str() {
        "book" | "tick_size_change" if !hash.is_empty() => Some(format!("{event_type}:{hash}")),
        "price_change" if !hash.is_empty() => Some(format!("pc:{hash}")),
        "price_change" if !market.is_empty() => Some(format!("pc:{market}:{timestamp}")),
        "last_trade_price" if !asset_id.is_empty() && !price.is_empty() => {
            Some(format!("ltp:{asset_id}:{price}:{size}"))
        }
        _ => None,
    }
}

fn str_field(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subscription_matches_go_shape_and_validates_assets() {
        let cfg = Config {
            level: 2,
            custom_feature_enabled: true,
            ..Default::default()
        };
        assert_eq!(
            market_subscription(&["a".into(), "b".into()], &cfg).unwrap(),
            json!({"type":"market","assets_ids":["a","b"],"level":2,"custom_feature_enabled":true})
        );
        assert!(market_subscription(&[], &cfg).is_err());
    }

    #[test]
    fn split_array_only_accepts_arrays() {
        assert_eq!(
            split_array(r#"[{"event_type":"book"},42]"#).unwrap().len(),
            2
        );
        assert!(split_array(r#"{"event_type":"book"}"#).is_none());
        assert!(split_array("not-json").is_none());
    }

    #[test]
    fn dedup_uses_source_key_rules_and_bounds_cache() {
        let mut d = Deduplicator::new(1, 1000);
        let book = r#"{"event_type":"book","hash":"h1"}"#;
        let other = r#"{"event_type":"price_change","market":"m","timestamp":"t"}"#;
        assert!(d.process_at(book, 0));
        assert!(!d.process_at(book, 1));
        assert!(d.process_at(other, 2));
        assert_eq!(d.seen.len(), 1);
        assert_eq!(d.duplicates, 1);
    }

    #[test]
    fn raw_message_preserves_payload() {
        let payload: Value = serde_json::from_str(
            r#"{"event_type":"last_trade_price","asset_id":"a","price":"0.5","size":"2"}"#,
        )
        .unwrap();
        let raw = RawMessage::from_payload(payload.clone(), 7);
        assert_eq!(raw.event_type, "last_trade_price");
        assert_eq!(raw.payload, payload);
        assert_eq!(
            extract_key(&raw.payload.to_string()).unwrap(),
            "ltp:a:0.5:2"
        );
    }

    #[test]
    fn stats_counts_events() {
        let mut stats = StreamStats::new("market");
        stats.set_subscriptions(&["a".into()]);
        stats.mark_connected();
        stats.record_event("book", 10);
        stats.record_duplicate();
        let snap = stats.snapshot();
        assert_eq!(snap.state, "connected");
        assert_eq!(snap.messages_received, 1);
        assert_eq!(snap.event_counts["book"], 1);
        assert_eq!(snap.duplicate_messages, 1);
    }
}

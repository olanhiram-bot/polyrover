use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::stream::{
    BestBidAskMessage, BookMessage, LastTradeMessage, PriceChangeEntry, PriceChangeMessage,
    PriceLevel, TickSizeChangeMessage,
};

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Level {
    pub price: String,
    pub size: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Snapshot {
    pub event_type: String,
    pub asset_id: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub market: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub timestamp: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub best_bid: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub best_ask: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub spread: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub midpoint: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub tick_size: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub previous_tick_size: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub last_trade_price: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub last_trade_size: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub last_trade_side: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub transaction_hash: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub update_hash: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub bids: Vec<Level>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub asks: Vec<Level>,
}

#[derive(Clone, Debug, Default)]
pub struct Tracker {
    snapshots: BTreeMap<String, Snapshot>,
}

impl Tracker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn apply_book(&mut self, msg: BookMessage) -> Snapshot {
        let mut snapshot = self.snapshot_for(&msg.asset_id);
        snapshot.event_type = first_non_empty(&[&msg.event_type, "book"]);
        snapshot.asset_id = msg.asset_id.clone();
        snapshot.market = first_non_empty(&[&msg.market, &snapshot.market]);
        snapshot.timestamp = msg.timestamp;
        snapshot.update_hash = msg.hash;
        snapshot.bids = levels_from_stream(&msg.bids);
        snapshot.asks = levels_from_stream(&msg.asks);
        sort_levels(&mut snapshot.bids, true);
        sort_levels(&mut snapshot.asks, false);
        refresh_prices(&mut snapshot);
        self.snapshots.insert(msg.asset_id, snapshot.clone());
        snapshot
    }

    pub fn apply_price_change(&mut self, msg: PriceChangeMessage) -> Vec<Snapshot> {
        let mut out = Vec::new();
        for change in msg.changes {
            if change.asset_id.trim().is_empty() {
                continue;
            }
            let mut snapshot = self.snapshot_for(&change.asset_id);
            snapshot.event_type = first_non_empty(&[&msg.event_type, "price_change"]);
            snapshot.asset_id = change.asset_id.clone();
            snapshot.market = first_non_empty(&[&msg.market, &snapshot.market]);
            snapshot.timestamp = msg.timestamp.clone();
            snapshot.update_hash = change.hash.clone();
            apply_level_change(&mut snapshot, &change);
            let use_stream_best = !change.best_bid.is_empty() || !change.best_ask.is_empty();
            if !change.best_bid.is_empty() {
                snapshot.best_bid = change.best_bid;
            }
            if !change.best_ask.is_empty() {
                snapshot.best_ask = change.best_ask;
            }
            if use_stream_best {
                refresh_midpoint(&mut snapshot);
            } else {
                refresh_prices(&mut snapshot);
            }
            self.snapshots.insert(change.asset_id, snapshot.clone());
            out.push(snapshot);
        }
        out
    }

    pub fn apply_last_trade(&mut self, msg: LastTradeMessage) -> Snapshot {
        let mut snapshot = self.snapshot_for(&msg.asset_id);
        snapshot.event_type = first_non_empty(&[&msg.event_type, "last_trade_price"]);
        snapshot.asset_id = msg.asset_id.clone();
        snapshot.market = first_non_empty(&[&msg.market, &snapshot.market]);
        snapshot.timestamp = msg.timestamp;
        snapshot.last_trade_price = msg.price;
        snapshot.last_trade_size = msg.size;
        snapshot.last_trade_side = msg.side;
        snapshot.transaction_hash = msg.transaction_hash;
        refresh_prices(&mut snapshot);
        self.snapshots.insert(msg.asset_id, snapshot.clone());
        snapshot
    }

    pub fn apply_best_bid_ask(&mut self, msg: BestBidAskMessage) -> Snapshot {
        let mut snapshot = self.snapshot_for(&msg.asset_id);
        snapshot.event_type = first_non_empty(&[&msg.event_type, "best_bid_ask"]);
        snapshot.asset_id = msg.asset_id.clone();
        snapshot.market = first_non_empty(&[&msg.market, &snapshot.market]);
        snapshot.timestamp = msg.timestamp;
        snapshot.best_bid = first_non_empty(&[&msg.best_bid, &snapshot.best_bid]);
        snapshot.best_ask = first_non_empty(&[&msg.best_ask, &snapshot.best_ask]);
        refresh_midpoint(&mut snapshot);
        snapshot.spread = first_non_empty(&[&msg.spread, &snapshot.spread]);
        self.snapshots.insert(msg.asset_id, snapshot.clone());
        snapshot
    }

    pub fn apply_tick_size_change(&mut self, msg: TickSizeChangeMessage) -> Snapshot {
        let mut snapshot = self.snapshot_for(&msg.asset_id);
        snapshot.event_type = first_non_empty(&[&msg.event_type, "tick_size_change"]);
        snapshot.asset_id = msg.asset_id.clone();
        snapshot.market = first_non_empty(&[&msg.market, &snapshot.market]);
        snapshot.timestamp = msg.timestamp;
        snapshot.previous_tick_size = msg.old_tick_size;
        snapshot.tick_size = msg.new_tick_size;
        self.snapshots.insert(msg.asset_id, snapshot.clone());
        snapshot
    }

    pub fn snapshot(&self, asset_id: &str) -> Option<Snapshot> {
        self.snapshots.get(asset_id).cloned()
    }
    fn snapshot_for(&self, asset_id: &str) -> Snapshot {
        self.snapshot(asset_id).unwrap_or_else(|| Snapshot {
            asset_id: asset_id.into(),
            ..Default::default()
        })
    }
}

fn levels_from_stream(rows: &[PriceLevel]) -> Vec<Level> {
    rows.iter()
        .map(|r| Level {
            price: r.price.clone(),
            size: r.size.clone(),
        })
        .collect()
}

fn apply_level_change(snapshot: &mut Snapshot, change: &PriceChangeEntry) {
    match change.side.trim().to_ascii_uppercase().as_str() {
        "BUY" | "BID" | "BIDS" => {
            upsert_level(&mut snapshot.bids, &change.price, &change.size);
            sort_levels(&mut snapshot.bids, true);
        }
        "SELL" | "ASK" | "ASKS" => {
            upsert_level(&mut snapshot.asks, &change.price, &change.size);
            sort_levels(&mut snapshot.asks, false);
        }
        _ => {}
    }
}

fn upsert_level(levels: &mut Vec<Level>, price: &str, size: &str) {
    if price.trim().is_empty() {
        return;
    }
    if is_zero_size(size) {
        levels.retain(|l| l.price != price);
        return;
    }
    if let Some(level) = levels.iter_mut().find(|l| l.price == price) {
        level.size = size.into();
    } else {
        levels.push(Level {
            price: price.into(),
            size: size.into(),
        });
    }
}

fn is_zero_size(size: &str) -> bool {
    size.trim().parse::<f64>().is_ok_and(|v| v == 0.0)
}

fn sort_levels(levels: &mut [Level], bid: bool) {
    levels.sort_by(
        |a, b| match (parse_price(&a.price), parse_price(&b.price)) {
            (Some(left), Some(right)) if bid => right.total_cmp(&left),
            (Some(left), Some(right)) => left.total_cmp(&right),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        },
    );
}

fn refresh_prices(snapshot: &mut Snapshot) {
    if let Some(level) = snapshot.bids.first() {
        snapshot.best_bid = level.price.clone();
    }
    if let Some(level) = snapshot.asks.first() {
        snapshot.best_ask = level.price.clone();
    }
    refresh_midpoint(snapshot);
}

fn refresh_midpoint(snapshot: &mut Snapshot) {
    if let Some(mid) = midpoint(&snapshot.best_bid, &snapshot.best_ask) {
        snapshot.midpoint = mid;
    }
    if let Some(s) = spread(&snapshot.best_bid, &snapshot.best_ask) {
        snapshot.spread = s;
    }
}

fn midpoint(bid: &str, ask: &str) -> Option<String> {
    Some(format_float((parse_price(bid)? + parse_price(ask)?) / 2.0))
}
fn spread(bid: &str, ask: &str) -> Option<String> {
    Some(format_float(parse_price(ask)? - parse_price(bid)?))
}
fn parse_price(value: &str) -> Option<f64> {
    value.trim().parse().ok()
}

fn format_float(value: f64) -> String {
    let mut s = format!("{value:.12}");
    while s.contains('.') && s.ends_with('0') {
        s.pop();
    }
    if s.ends_with('.') {
        s.pop();
    }
    s
}

fn first_non_empty(values: &[&str]) -> String {
    values
        .iter()
        .find(|v| !v.trim().is_empty())
        .unwrap_or(&"")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn book_computes_best_bid_ask_and_midpoint() {
        let mut tracker = Tracker::new();
        let snapshot = tracker.apply_book(BookMessage {
            event_type: "book".into(),
            asset_id: "token-1".into(),
            market: "market-1".into(),
            timestamp: "1000".into(),
            bids: vec![
                PriceLevel {
                    price: "0.49".into(),
                    size: "10".into(),
                },
                PriceLevel {
                    price: "0.51".into(),
                    size: "4".into(),
                },
            ],
            asks: vec![
                PriceLevel {
                    price: "0.55".into(),
                    size: "2".into(),
                },
                PriceLevel {
                    price: "0.53".into(),
                    size: "7".into(),
                },
            ],
            ..Default::default()
        });
        assert_eq!(snapshot.best_bid, "0.51");
        assert_eq!(snapshot.best_ask, "0.53");
        assert_eq!(snapshot.midpoint, "0.52");
        assert_eq!(snapshot.bids[0].price, "0.51");
        assert_eq!(snapshot.asks[0].price, "0.53");
    }

    #[test]
    fn price_change_updates_book_and_stream_best_prices() {
        let mut tracker = Tracker::new();
        tracker.apply_book(BookMessage {
            asset_id: "token-1".into(),
            market: "market-1".into(),
            bids: vec![PriceLevel {
                price: "0.49".into(),
                size: "10".into(),
            }],
            asks: vec![PriceLevel {
                price: "0.53".into(),
                size: "7".into(),
            }],
            ..Default::default()
        });
        let snapshots = tracker.apply_price_change(PriceChangeMessage {
            event_type: "price_change".into(),
            market: "market-1".into(),
            timestamp: "1001".into(),
            changes: vec![PriceChangeEntry {
                asset_id: "token-1".into(),
                side: "BUY".into(),
                price: "0.52".into(),
                size: "12".into(),
                best_bid: "0.52".into(),
                best_ask: "0.53".into(),
                hash: "hash-1".into(),
            }],
        });
        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].update_hash, "hash-1");
        assert_eq!(snapshots[0].midpoint, "0.525");
        assert_eq!(snapshots[0].bids[0].price, "0.52");
    }

    #[test]
    fn last_trade_preserves_book_prices_and_tick_size_updates() {
        let mut tracker = Tracker::new();
        tracker.apply_book(BookMessage {
            asset_id: "token-1".into(),
            market: "market-1".into(),
            bids: vec![PriceLevel {
                price: "0.49".into(),
                size: "10".into(),
            }],
            asks: vec![PriceLevel {
                price: "0.53".into(),
                size: "7".into(),
            }],
            ..Default::default()
        });
        let trade = tracker.apply_last_trade(LastTradeMessage {
            event_type: "last_trade_price".into(),
            asset_id: "token-1".into(),
            market: "market-1".into(),
            price: "0.5".into(),
            size: "25".into(),
            side: "BUY".into(),
            transaction_hash: "0xabc".into(),
            ..Default::default()
        });
        assert_eq!(trade.best_bid, "0.49");
        assert_eq!(trade.last_trade_price, "0.5");
        let tick = tracker.apply_tick_size_change(TickSizeChangeMessage {
            asset_id: "token-1".into(),
            old_tick_size: "0.01".into(),
            new_tick_size: "0.001".into(),
            ..Default::default()
        });
        assert_eq!(tick.tick_size, "0.001");
        assert_eq!(tick.previous_tick_size, "0.01");
    }

    #[test]
    fn best_bid_ask_updates_without_book_delta() {
        let mut tracker = Tracker::new();
        let snapshot = tracker.apply_best_bid_ask(BestBidAskMessage {
            event_type: "best_bid_ask".into(),
            asset_id: "token-1".into(),
            market: "market-1".into(),
            best_bid: "0.73".into(),
            best_ask: "0.77".into(),
            spread: "0.04".into(),
            timestamp: "1004".into(),
        });
        assert_eq!(snapshot.midpoint, "0.75");
        assert_eq!(snapshot.spread, "0.04");
    }

    #[test]
    fn ignores_blank_asset_removes_zero_size_and_keeps_bad_prices_last() {
        let mut tracker = Tracker::new();
        tracker.apply_book(BookMessage {
            asset_id: "token-1".into(),
            bids: vec![
                PriceLevel {
                    price: "bad".into(),
                    size: "1".into(),
                },
                PriceLevel {
                    price: "0.49".into(),
                    size: "10".into(),
                },
            ],
            asks: vec![PriceLevel {
                price: "0.53".into(),
                size: "7".into(),
            }],
            ..Default::default()
        });
        let empty = tracker.apply_price_change(PriceChangeMessage {
            changes: vec![PriceChangeEntry {
                price: "0.52".into(),
                size: "1".into(),
                ..Default::default()
            }],
            ..Default::default()
        });
        assert!(empty.is_empty());
        let updated = tracker.apply_price_change(PriceChangeMessage {
            changes: vec![PriceChangeEntry {
                asset_id: "token-1".into(),
                side: "BUY".into(),
                price: "0.49".into(),
                size: "0".into(),
                ..Default::default()
            }],
            ..Default::default()
        });
        assert_eq!(updated[0].bids[0].price, "bad");
        assert_eq!(updated[0].best_bid, "bad");
    }
}

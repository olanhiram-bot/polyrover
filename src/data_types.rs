//! Row types returned by the Data API.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::jsonx::{bool_or_false, float_or_zero, int_or_zero, string_or_number};

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct Position {
    #[serde(default, alias = "token_id", rename = "asset")]
    pub token_id: String,
    #[serde(default, alias = "condition_id", rename = "conditionId")]
    pub condition_id: String,
    #[serde(default, alias = "market_id", rename = "market")]
    pub market_id: String,
    #[serde(default)]
    pub side: String,
    #[serde(default, rename = "eventId")]
    pub event_id: String,
    #[serde(default, rename = "proxyWallet")]
    pub proxy_wallet: String,
    #[serde(default, deserialize_with = "float_or_zero")]
    pub size: f64,
    #[serde(
        default,
        alias = "avg_price",
        rename = "avgPrice",
        deserialize_with = "float_or_zero"
    )]
    pub avg_price: f64,
    #[serde(
        default,
        rename = "curPrice",
        alias = "current_price",
        deserialize_with = "float_or_zero"
    )]
    pub current_price: f64,
    #[serde(
        default,
        rename = "unrealizedPnl",
        alias = "unrealized_pnl",
        deserialize_with = "float_or_zero"
    )]
    pub unrealized_pnl: f64,
    #[serde(default, rename = "cashPnl", deserialize_with = "float_or_zero")]
    pub cash_pnl: f64,
    #[serde(default, rename = "realizedPnl", deserialize_with = "float_or_zero")]
    pub realized_pnl: f64,
    #[serde(default, deserialize_with = "bool_or_false")]
    pub redeemable: bool,
    #[serde(default, deserialize_with = "bool_or_false")]
    pub mergeable: bool,
    #[serde(default, rename = "negativeRisk", deserialize_with = "bool_or_false")]
    pub negative_risk: bool,
    #[serde(default)]
    pub outcome: String,
    #[serde(default, rename = "outcomeIndex", deserialize_with = "int_or_zero")]
    pub outcome_index: i64,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub slug: String,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct ClosedPosition {
    #[serde(flatten)]
    pub position: Position,
    #[serde(default, deserialize_with = "string_or_number")]
    pub timestamp: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct Trade {
    #[serde(default)]
    pub id: String,
    #[serde(
        default,
        alias = "market",
        alias = "condition_id",
        rename = "conditionId"
    )]
    pub market: String,
    #[serde(default, alias = "asset_id", rename = "asset")]
    pub asset_id: String,
    #[serde(default, rename = "proxyWallet")]
    pub proxy_wallet: String,
    #[serde(default)]
    pub side: String,
    #[serde(default, deserialize_with = "float_or_zero")]
    pub price: f64,
    #[serde(default, deserialize_with = "float_or_zero")]
    pub size: f64,
    #[serde(
        default,
        alias = "fee_rate_bps",
        rename = "feeRateBps",
        deserialize_with = "int_or_zero"
    )]
    pub fee_rate_bps: i64,
    #[serde(default)]
    pub outcome: String,
    #[serde(default)]
    pub status: String,
    #[serde(default, alias = "transaction_hash", rename = "transactionHash")]
    pub transaction_hash: String,
    #[serde(
        default,
        alias = "created_at",
        rename = "timestamp",
        deserialize_with = "string_or_number"
    )]
    pub created_at: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub slug: String,
    #[serde(default, alias = "event_slug", rename = "eventSlug")]
    pub event_slug: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub pseudonym: String,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct Activity {
    #[serde(default, rename = "type")]
    pub activity_type: String,
    #[serde(default, rename = "proxyWallet")]
    pub proxy_wallet: String,
    #[serde(default)]
    pub market: String,
    #[serde(default, alias = "condition_id", rename = "conditionId")]
    pub condition_id: String,
    #[serde(default, alias = "asset_id", rename = "asset")]
    pub asset_id: String,
    #[serde(default)]
    pub side: String,
    #[serde(default, deserialize_with = "string_or_number")]
    pub price: String,
    #[serde(default, deserialize_with = "string_or_number")]
    pub size: String,
    #[serde(
        default,
        alias = "usdc_size",
        rename = "usdcSize",
        deserialize_with = "string_or_number"
    )]
    pub usdc_size: String,
    #[serde(default, alias = "transaction_hash", rename = "transactionHash")]
    pub transaction_hash: String,
    #[serde(default, deserialize_with = "string_or_number")]
    pub timestamp: String,
    #[serde(default)]
    pub outcome: String,
    #[serde(default, rename = "outcomeIndex", deserialize_with = "int_or_zero")]
    pub outcome_index: i64,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub slug: String,
    #[serde(default, alias = "event_slug", rename = "eventSlug")]
    pub event_slug: String,
    #[serde(
        default,
        alias = "is_combo",
        rename = "isCombo",
        deserialize_with = "bool_or_false"
    )]
    pub is_combo: bool,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct Holder {
    #[serde(default)]
    pub address: String,
    #[serde(default, rename = "proxyWallet")]
    pub proxy_wallet: String,
    #[serde(default, deserialize_with = "float_or_zero")]
    pub shares: f64,
    #[serde(default, deserialize_with = "float_or_zero")]
    pub amount: f64,
    #[serde(default, deserialize_with = "float_or_zero")]
    pub pnl: f64,
    #[serde(default, deserialize_with = "float_or_zero")]
    pub volume: f64,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct PortfolioValue {
    #[serde(default)]
    pub user: String,
    #[serde(default, deserialize_with = "float_or_zero")]
    pub value: f64,
    #[serde(default)]
    pub timestamp: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct TotalMarketsTraded {
    #[serde(default)]
    pub user: String,
    #[serde(default)]
    pub markets_traded: i64,
    #[serde(default)]
    pub traded: i64,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct OpenInterest {
    #[serde(default)]
    pub market: String,
    #[serde(default)]
    pub asset_id: String,
    #[serde(default, rename = "value", deserialize_with = "float_or_zero")]
    pub open_value: f64,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct LeaderboardRow {
    #[serde(default, deserialize_with = "int_or_zero")]
    pub rank: i64,
    #[serde(default)]
    pub user: String,
    #[serde(default, rename = "proxyWallet")]
    pub proxy_wallet: String,
    #[serde(default, rename = "userName")]
    pub user_name: String,
    #[serde(default, alias = "vol", deserialize_with = "float_or_zero")]
    pub volume: f64,
    #[serde(default, deserialize_with = "float_or_zero")]
    pub pnl: f64,
    #[serde(default, rename = "roi", deserialize_with = "float_or_zero")]
    pub roi: f64,
    #[serde(default, rename = "profileImage")]
    pub profile_image: String,
    #[serde(default, rename = "xUsername")]
    pub x_username: String,
    #[serde(default, rename = "verifiedBadge", deserialize_with = "bool_or_false")]
    pub verified_badge: bool,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct LiveVolumeResponse {
    #[serde(default, deserialize_with = "float_or_zero")]
    pub total: f64,
    #[serde(default)]
    pub markets: Vec<Value>,
    #[serde(default)]
    pub events: Vec<Value>,
}

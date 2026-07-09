use chrono::{DateTime, FixedOffset, NaiveDate, TimeZone, Utc};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Map, Value};

use crate::jsonx::{
    bool_or_false, first_non_empty, scalar_to_string, string_or_number, StringOrArray,
};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct NormalizedTime(pub Option<DateTime<FixedOffset>>);

impl<'de> Deserialize<'de> for NormalizedTime {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        let text = scalar_to_string(&value);
        if text.is_empty() {
            return Ok(Self(None));
        }
        parse_time(&text)
            .map(|dt| Self(Some(dt)))
            .map_err(serde::de::Error::custom)
    }
}

impl Serialize for NormalizedTime {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match &self.0 {
            Some(dt) => serializer.serialize_str(&dt.to_rfc3339()),
            None => serializer.serialize_none(),
        }
    }
}

fn parse_time(raw: &str) -> std::result::Result<DateTime<FixedOffset>, chrono::ParseError> {
    let mut s = raw.to_string();
    if s.len() >= 3 && (s.contains(' ') || s.contains('T')) {
        let last = &s[s.len() - 3..];
        if (last.starts_with('+') || last.starts_with('-'))
            && last[1..].chars().all(|c| c.is_ascii_digit())
        {
            s = format!("{}{}:00", &s[..s.len() - 3], last);
        }
    }
    for fmt in [
        "%Y-%m-%d %H:%M:%S%:z",
        "%Y-%m-%dT%H:%M:%S%:z",
        "%Y-%m-%d %H:%M:%S%.f%:z",
        "%Y-%m-%dT%H:%M:%S%.f%:z",
    ] {
        if let Ok(dt) = DateTime::parse_from_str(&s, fmt) {
            return Ok(dt);
        }
    }
    if let Ok(dt) = DateTime::parse_from_rfc3339(&s) {
        return Ok(dt);
    }
    if let Ok(date) = NaiveDate::parse_from_str(&s, "%Y-%m-%d") {
        return Ok(Utc
            .from_utc_datetime(&date.and_hms_opt(0, 0, 0).unwrap())
            .fixed_offset());
    }
    if let Some(date) = parse_month_name_date(&s) {
        return Ok(Utc
            .from_utc_datetime(&date.and_hms_opt(0, 0, 0).unwrap())
            .fixed_offset());
    }
    DateTime::parse_from_rfc3339(&s)
}

fn parse_month_name_date(raw: &str) -> Option<NaiveDate> {
    let cleaned = raw.replace(',', "");
    let parts: Vec<_> = cleaned.split_whitespace().collect();
    if parts.len() != 3 {
        return None;
    }
    let month = match parts[0] {
        "January" => 1,
        "February" => 2,
        "March" => 3,
        "April" => 4,
        "May" => 5,
        "June" => 6,
        "July" => 7,
        "August" => 8,
        "September" => 9,
        "October" => 10,
        "November" => 11,
        "December" => 12,
        _ => return None,
    };
    let day = parts[1].parse().ok()?;
    let year = parts[2].parse().ok()?;
    NaiveDate::from_ymd_opt(year, month, day)
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct HealthResponse {
    pub data: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct Market {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub question: String,
    #[serde(default)]
    pub slug: String,
    #[serde(default, alias = "conditionId")]
    pub condition_id: String,
    #[serde(default, alias = "clobTokenIds")]
    pub clob_token_ids: String,
    #[serde(default)]
    pub active: bool,
    #[serde(default)]
    pub closed: bool,
    #[serde(default)]
    pub archived: bool,
    #[serde(default)]
    pub outcomes: StringOrArray,
    #[serde(default, alias = "outcomePrices")]
    pub outcome_prices: StringOrArray,
    #[serde(default, alias = "endDate")]
    pub end_date: NormalizedTime,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct Event {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub ticker: String,
    #[serde(default)]
    pub slug: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub active: bool,
    #[serde(default)]
    pub closed: bool,
    #[serde(default)]
    pub archived: bool,
    #[serde(default)]
    pub markets: Vec<Market>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct SearchResponse {
    #[serde(default)]
    pub events: Vec<Event>,
    #[serde(default)]
    pub tags: Vec<Value>,
    #[serde(default)]
    pub profiles: Vec<Value>,
    #[serde(default)]
    pub pagination: Value,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct ClobServerTime {
    #[serde(default)]
    pub timestamp: String,
    #[serde(default)]
    pub iso: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct ClobOrderBookLevel {
    #[serde(default, deserialize_with = "string_or_number")]
    pub price: String,
    #[serde(default, deserialize_with = "string_or_number")]
    pub size: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct ClobOrderBook {
    #[serde(default)]
    pub market: String,
    #[serde(default, alias = "assetId")]
    pub asset_id: String,
    #[serde(default, deserialize_with = "string_or_number")]
    pub timestamp: String,
    #[serde(default)]
    pub hash: String,
    #[serde(default)]
    pub bids: Vec<ClobOrderBookLevel>,
    #[serde(default)]
    pub asks: Vec<ClobOrderBookLevel>,
    #[serde(default, alias = "minOrderSize", deserialize_with = "string_or_number")]
    pub min_order_size: String,
    #[serde(default, alias = "tickSize", deserialize_with = "string_or_number")]
    pub tick_size: String,
    #[serde(default, alias = "negRisk", deserialize_with = "bool_or_false")]
    pub neg_risk: bool,
    #[serde(
        default,
        alias = "lastTradePrice",
        deserialize_with = "string_or_number"
    )]
    pub last_trade_price: String,
}

impl ClobOrderBook {
    pub fn best_bid(&self) -> Option<f64> {
        self.bids
            .iter()
            .filter_map(parse_level)
            .map(|(p, _)| p)
            .max_by(f64::total_cmp)
    }

    pub fn best_ask(&self) -> Option<f64> {
        self.asks
            .iter()
            .filter_map(parse_level)
            .map(|(p, _)| p)
            .min_by(f64::total_cmp)
    }

    pub fn available_ask_size(&self, max_price: f64) -> f64 {
        if !positive_finite(max_price) {
            return 0.0;
        }
        self.asks
            .iter()
            .filter_map(parse_level)
            .filter(|(price, _)| *price <= max_price)
            .map(|(_, size)| size)
            .sum()
    }
}

fn parse_level(level: &ClobOrderBookLevel) -> Option<(f64, f64)> {
    let price: f64 = level.price.trim().parse().ok()?;
    let size: f64 = level.size.trim().parse().ok()?;
    (positive_finite(price) && price <= 1.0 && positive_finite(size)).then_some((price, size))
}

fn positive_finite(v: f64) -> bool {
    v > 0.0 && v.is_finite()
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct ClobTickSize {
    #[serde(
        default,
        alias = "minimumTickSize",
        deserialize_with = "string_or_number"
    )]
    pub minimum_tick_size: String,
    #[serde(
        default,
        alias = "minimumOrderSize",
        deserialize_with = "string_or_number"
    )]
    pub minimum_order_size: String,
    #[serde(default, alias = "tickSize", deserialize_with = "string_or_number")]
    pub tick_size: String,
}

impl ClobTickSize {
    pub fn value(&self) -> Option<f64> {
        [self.tick_size.as_str(), self.minimum_tick_size.as_str()]
            .into_iter()
            .filter_map(|raw| raw.trim().parse::<f64>().ok())
            .find(|v| positive_finite(*v))
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct ClobNegRiskInfo {
    #[serde(default, alias = "negRisk", deserialize_with = "bool_or_false")]
    pub neg_risk: bool,
    #[serde(
        default,
        alias = "negRiskMarketID",
        deserialize_with = "string_or_number"
    )]
    pub neg_risk_market_id: String,
    #[serde(default, alias = "negRiskFeeBips")]
    pub neg_risk_fee_bips: i64,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct ClobToken {
    #[serde(default, alias = "tokenId", alias = "t")]
    pub token_id: String,
    #[serde(default, alias = "o")]
    pub outcome: String,
    #[serde(default, alias = "p", deserialize_with = "string_or_number")]
    pub price: String,
    #[serde(default, alias = "w")]
    pub winner: bool,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct ClobMarket {
    #[serde(default, alias = "conditionId", alias = "c")]
    pub condition_id: String,
    #[serde(default, alias = "questionId", alias = "q")]
    pub question_id: String,
    #[serde(default, alias = "t")]
    pub tokens: Vec<ClobToken>,
    #[serde(default)]
    pub closed: bool,
    #[serde(default, alias = "acceptingOrders", alias = "ao")]
    pub accepting_orders: bool,
    #[serde(default, alias = "negRisk", alias = "nr")]
    pub neg_risk: bool,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct ClobPaginatedMarkets {
    #[serde(default)]
    pub limit: i64,
    #[serde(default)]
    pub count: i64,
    #[serde(default)]
    pub next_cursor: String,
    #[serde(default)]
    pub data: Vec<ClobMarket>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct ClobMarketOutcome {
    pub status: String,
    pub condition_id: String,
    pub winning_token_id: String,
    pub closed: bool,
    pub source: String,
}

pub const CLOB_OUTCOME_RESOLVED: &str = "resolved";
pub const CLOB_OUTCOME_UNRESOLVED: &str = "unresolved";

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct ClobMarketByTokenResponse {
    #[serde(default)]
    pub condition_id: String,
    #[serde(default)]
    pub primary_token_id: String,
    #[serde(default)]
    pub secondary_token_id: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct ClobPriceHistory {
    #[serde(default)]
    pub history: Vec<Value>,
}

pub(crate) fn first_price(values: &[&str]) -> String {
    first_non_empty(values.iter().copied()).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clob_book_math_matches_go_rules() {
        let book: ClobOrderBook = serde_json::from_str(
            r#"{
            "bids":[{"price":"0.42","size":"10"},{"price":"1.2","size":"9"}],
            "asks":[{"price":"0.48","size":"3"},{"price":0.47,"size":2},{"price":"bad","size":"4"}]
        }"#,
        )
        .unwrap();
        assert_eq!(book.best_bid(), Some(0.42));
        assert_eq!(book.best_ask(), Some(0.47));
        assert_eq!(book.available_ask_size(0.48), 5.0);
    }

    #[test]
    fn tick_size_prefers_tick_then_minimum() {
        let tick = ClobTickSize {
            minimum_tick_size: "0.01".into(),
            tick_size: "".into(),
            ..Default::default()
        };
        assert_eq!(tick.value(), Some(0.01));
    }

    #[test]
    fn normalized_time_accepts_gamma_formats() {
        for raw in [
            "2026-07-07T10:11:12Z",
            "2026-07-07 10:11:12+00",
            "2026-07-07",
            "January 2, 2026",
        ] {
            let json = format!("\"{raw}\"");
            let parsed: NormalizedTime =
                serde_json::from_str(&json).unwrap_or_else(|err| panic!("{raw}: {err}"));
            assert!(parsed.0.is_some(), "{raw}");
        }
    }
}

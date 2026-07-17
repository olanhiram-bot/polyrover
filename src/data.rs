//! Data API client: positions, closed positions, trades, and activity.

use serde::Deserialize;

use crate::{
    data_types::{
        Activity, ClosedPosition, Holder, LeaderboardRow, LiveVolumeResponse, OpenInterest,
        PortfolioValue, Position, TotalMarketsTraded, Trade,
    },
    transport, Result,
};

pub const DEFAULT_BASE_URL: &str = "https://data-api.polymarket.com";

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ClosedPositionParams {
    pub user: String,
    pub markets: Vec<String>,
    pub title: String,
    pub event_ids: Vec<u64>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
    pub sort_by: String,
    pub sort_direction: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TradeParams {
    pub user: String,
    pub markets: Vec<String>,
    pub event_ids: Vec<u64>,
    pub side: String,
    pub start: Option<u64>,
    pub end: Option<u64>,
    pub taker_only: Option<bool>,
    pub filter_type: String,
    pub filter_amount: String,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ActivityParams {
    pub user: String,
    pub markets: Vec<String>,
    pub event_ids: Vec<u64>,
    pub activity_types: Vec<String>,
    pub side: String,
    pub start: Option<u64>,
    pub end: Option<u64>,
    pub sort_by: String,
    pub sort_direction: String,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LeaderboardParams {
    pub category: String,
    pub time_period: String,
    pub order_by: String,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
    pub user: String,
    pub user_name: String,
}

#[derive(Clone)]
pub struct Client {
    transport: transport::Client,
}

impl Client {
    pub fn new(base_url: impl Into<String>) -> Result<Self> {
        let base = match base_url.into() {
            s if s.is_empty() => DEFAULT_BASE_URL.into(),
            s => s,
        };
        Ok(Self {
            transport: transport::Client::new(transport::Config::new(base))?,
        })
    }

    pub async fn health(&self) -> Result<()> {
        self.transport.get_raw("/").await.map(|_| ())
    }

    pub async fn current_positions(&self, user: &str, limit: u32) -> Result<Vec<Position>> {
        self.transport
            .get_json(&path(
                "/positions",
                &[pair("user", user), limit_pair(limit)],
            ))
            .await
    }

    pub async fn closed_positions(&self, user: &str, limit: u32) -> Result<Vec<ClosedPosition>> {
        self.closed_positions_with(&ClosedPositionParams {
            user: user.into(),
            limit: Some(limit),
            ..ClosedPositionParams::default()
        })
        .await
    }

    pub async fn closed_positions_with(
        &self,
        params: &ClosedPositionParams,
    ) -> Result<Vec<ClosedPosition>> {
        self.transport
            .get_json(&params.path("/closed-positions"))
            .await
    }

    pub async fn trades(&self, user: &str, limit: u32) -> Result<Vec<Trade>> {
        self.trades_with(&TradeParams {
            user: user.into(),
            limit: Some(limit),
            ..TradeParams::default()
        })
        .await
    }

    pub async fn trades_with(&self, params: &TradeParams) -> Result<Vec<Trade>> {
        self.transport.get_json(&params.path("/trades")).await
    }

    pub async fn market_trades(&self, market: &str, limit: u32) -> Result<Vec<Trade>> {
        self.trades_with(&TradeParams {
            markets: vec![market.into()],
            limit: Some(limit),
            ..TradeParams::default()
        })
        .await
    }

    pub async fn activity(&self, user: &str, limit: u32) -> Result<Vec<Activity>> {
        self.activity_with(&ActivityParams {
            user: user.into(),
            limit: Some(limit),
            ..ActivityParams::default()
        })
        .await
    }

    pub async fn activity_with(&self, params: &ActivityParams) -> Result<Vec<Activity>> {
        self.transport.get_json(&params.path("/activity")).await
    }

    pub async fn top_holders(&self, market: &str, limit: u32) -> Result<Vec<Holder>> {
        let groups: Vec<HolderGroup> = self
            .transport
            .get_json(&path(
                "/holders",
                &[pair("market", market), limit_pair(limit)],
            ))
            .await?;
        let mut out = Vec::new();
        for group in groups {
            for mut holder in group.holders {
                if holder.address.is_empty() {
                    holder.address = holder.proxy_wallet.clone();
                }
                if holder.shares == 0.0 {
                    holder.shares = holder.amount;
                }
                out.push(holder);
            }
        }
        Ok(out)
    }

    pub async fn total_value(&self, user: &str) -> Result<PortfolioValue> {
        let raw = self
            .transport
            .get_raw(&path("/value", &[pair("user", user)]))
            .await?;
        let mut value = match serde_json::from_str::<PortfolioValue>(&raw) {
            Ok(row) => row,
            Err(_) => serde_json::from_str::<Vec<PortfolioValue>>(&raw)?
                .into_iter()
                .next()
                .unwrap_or_default(),
        };
        if value.user.is_empty() {
            value.user = user.into();
        }
        Ok(value)
    }

    pub async fn markets_traded(&self, user: &str) -> Result<TotalMarketsTraded> {
        let mut row: TotalMarketsTraded = self
            .transport
            .get_json(&path("/traded", &[pair("user", user)]))
            .await?;
        if row.markets_traded == 0 {
            row.markets_traded = row.traded;
        }
        Ok(row)
    }

    pub async fn open_interest(&self, market: &str) -> Result<OpenInterest> {
        Ok(self
            .transport
            .get_json::<Vec<OpenInterest>>(&path("/oi", &[pair("market", market)]))
            .await?
            .into_iter()
            .next()
            .unwrap_or(OpenInterest {
                market: market.into(),
                ..Default::default()
            }))
    }

    pub async fn trader_leaderboard(&self, limit: u32) -> Result<Vec<LeaderboardRow>> {
        self.trader_leaderboard_with(&LeaderboardParams {
            limit: Some(limit),
            ..LeaderboardParams::default()
        })
        .await
    }

    pub async fn trader_leaderboard_with(
        &self,
        params: &LeaderboardParams,
    ) -> Result<Vec<LeaderboardRow>> {
        let mut rows: Vec<LeaderboardRow> = self
            .transport
            .get_json(&params.path("/v1/leaderboard"))
            .await?;
        for row in &mut rows {
            if row.user.is_empty() {
                row.user.clone_from(if row.proxy_wallet.is_empty() {
                    &row.user_name
                } else {
                    &row.proxy_wallet
                });
            }
        }
        Ok(rows)
    }

    pub async fn live_volume(&self, event_id: u32) -> Result<LiveVolumeResponse> {
        let raw = self
            .transport
            .get_raw(&path("/live-volume", &[pair("id", &event_id.to_string())]))
            .await?;
        match serde_json::from_str::<LiveVolumeResponse>(&raw) {
            Ok(row) => Ok(row),
            Err(_) => Ok(serde_json::from_str::<Vec<LiveVolumeResponse>>(&raw)?
                .into_iter()
                .next()
                .unwrap_or_default()),
        }
    }
}

impl ClosedPositionParams {
    fn path(&self, base: &str) -> String {
        path(
            base,
            &[
                pair("user", &self.user),
                csv_pair("market", &self.markets),
                pair("title", &self.title),
                csv_pair("eventId", &self.event_ids),
                number_pair("limit", self.limit),
                number_pair("offset", self.offset),
                pair("sortBy", &self.sort_by),
                pair("sortDirection", &self.sort_direction),
            ],
        )
    }
}

impl TradeParams {
    fn path(&self, base: &str) -> String {
        path(
            base,
            &[
                number_pair("limit", self.limit),
                number_pair("offset", self.offset),
                bool_pair("takerOnly", self.taker_only),
                pair("filterType", &self.filter_type),
                pair("filterAmount", &self.filter_amount),
                csv_pair("market", &self.markets),
                csv_pair("eventId", &self.event_ids),
                pair("user", &self.user),
                pair("side", &self.side),
                number_pair("start", self.start),
                number_pair("end", self.end),
            ],
        )
    }
}

impl ActivityParams {
    fn path(&self, base: &str) -> String {
        path(
            base,
            &[
                number_pair("limit", self.limit),
                number_pair("offset", self.offset),
                pair("user", &self.user),
                csv_pair("market", &self.markets),
                csv_pair("eventId", &self.event_ids),
                csv_pair("type", &self.activity_types),
                number_pair("start", self.start),
                number_pair("end", self.end),
                pair("sortBy", &self.sort_by),
                pair("sortDirection", &self.sort_direction),
                pair("side", &self.side),
            ],
        )
    }
}

impl LeaderboardParams {
    fn path(&self, base: &str) -> String {
        path(
            base,
            &[
                pair("category", &self.category),
                pair("timePeriod", &self.time_period),
                pair("orderBy", &self.order_by),
                number_pair("limit", self.limit),
                number_pair("offset", self.offset),
                pair("user", &self.user),
                pair("userName", &self.user_name),
            ],
        )
    }
}

#[derive(Deserialize)]
struct HolderGroup {
    #[serde(default)]
    holders: Vec<Holder>,
}

fn pair(key: &str, value: &str) -> Option<(String, String)> {
    (!value.is_empty()).then(|| (key.into(), value.into()))
}

fn limit_pair(limit: u32) -> Option<(String, String)> {
    (limit > 0).then(|| ("limit".into(), limit.to_string()))
}

fn number_pair<T: ToString>(key: &str, value: Option<T>) -> Option<(String, String)> {
    value.map(|value| (key.into(), value.to_string()))
}

fn bool_pair(key: &str, value: Option<bool>) -> Option<(String, String)> {
    number_pair(key, value)
}

fn csv_pair<T: ToString>(key: &str, values: &[T]) -> Option<(String, String)> {
    (!values.is_empty()).then(|| {
        (
            key.into(),
            values
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(","),
        )
    })
}

fn path(base: &str, pairs: &[Option<(String, String)>]) -> String {
    let pairs: Vec<_> = pairs.iter().flatten().collect();
    if pairs.is_empty() {
        return base.into();
    }
    format!(
        "{}?{}",
        base,
        pairs
            .into_iter()
            .map(|(k, v)| format!("{}={}", escape(k), escape(v)))
            .collect::<Vec<_>>()
            .join("&")
    )
}

fn escape(value: &str) -> String {
    let mut out = String::new();
    for b in value.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_paths_match_go_endpoints() {
        assert_eq!(
            path("/positions", &[pair("user", "0xabc"), limit_pair(20)]),
            "/positions?user=0xabc&limit=20"
        );
        assert_eq!(
            path("/traded", &[pair("user", "a b")]),
            "/traded?user=a%20b"
        );
    }

    #[test]
    fn holders_are_flattened_like_go_client() {
        let groups: Vec<HolderGroup> = serde_json::from_str(
            r#"[{"holders":[{"proxyWallet":"0x1","amount":"3","shares":0}]}]"#,
        )
        .unwrap();
        let mut holder = groups.into_iter().next().unwrap().holders.pop().unwrap();
        if holder.address.is_empty() {
            holder.address = holder.proxy_wallet.clone();
        }
        if holder.shares == 0.0 {
            holder.shares = holder.amount;
        }
        assert_eq!(holder.address, "0x1");
        assert_eq!(holder.shares, 3.0);
    }
}

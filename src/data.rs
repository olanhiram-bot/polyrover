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

    pub fn health(&self) -> Result<()> {
        self.transport.get_raw("/").map(|_| ())
    }

    pub fn current_positions(&self, user: &str, limit: u32) -> Result<Vec<Position>> {
        self.transport.get_json(&path(
            "/positions",
            &[pair("user", user), limit_pair(limit)],
        ))
    }

    pub fn closed_positions(&self, user: &str, limit: u32) -> Result<Vec<ClosedPosition>> {
        self.transport.get_json(&path(
            "/closed-positions",
            &[pair("user", user), limit_pair(limit)],
        ))
    }

    pub fn trades(&self, user: &str, limit: u32) -> Result<Vec<Trade>> {
        self.transport
            .get_json(&path("/trades", &[pair("user", user), limit_pair(limit)]))
    }

    pub fn market_trades(&self, market: &str, limit: u32) -> Result<Vec<Trade>> {
        self.transport.get_json(&path(
            "/trades",
            &[pair("market", market), limit_pair(limit)],
        ))
    }

    pub fn activity(&self, user: &str, limit: u32) -> Result<Vec<Activity>> {
        self.transport
            .get_json(&path("/activity", &[pair("user", user), limit_pair(limit)]))
    }

    pub fn top_holders(&self, market: &str, limit: u32) -> Result<Vec<Holder>> {
        let groups: Vec<HolderGroup> = self.transport.get_json(&path(
            "/holders",
            &[pair("market", market), limit_pair(limit)],
        ))?;
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

    pub fn total_value(&self, user: &str) -> Result<PortfolioValue> {
        let raw = self
            .transport
            .get_raw(&path("/value", &[pair("user", user)]))?;
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

    pub fn markets_traded(&self, user: &str) -> Result<TotalMarketsTraded> {
        let mut row: TotalMarketsTraded = self
            .transport
            .get_json(&path("/traded", &[pair("user", user)]))?;
        if row.markets_traded == 0 {
            row.markets_traded = row.traded;
        }
        Ok(row)
    }

    pub fn open_interest(&self, market: &str) -> Result<OpenInterest> {
        Ok(self
            .transport
            .get_json::<Vec<OpenInterest>>(&path("/oi", &[pair("market", market)]))?
            .into_iter()
            .next()
            .unwrap_or(OpenInterest {
                market: market.into(),
                ..Default::default()
            }))
    }

    pub fn trader_leaderboard(&self, limit: u32) -> Result<Vec<LeaderboardRow>> {
        self.transport
            .get_json(&path("/v1/leaderboard", &[limit_pair(limit)]))
    }

    pub fn live_volume(&self, event_id: u32) -> Result<LiveVolumeResponse> {
        let raw = self
            .transport
            .get_raw(&path("/live-volume", &[pair("id", &event_id.to_string())]))?;
        match serde_json::from_str::<LiveVolumeResponse>(&raw) {
            Ok(row) => Ok(row),
            Err(_) => Ok(serde_json::from_str::<Vec<LiveVolumeResponse>>(&raw)?
                .into_iter()
                .next()
                .unwrap_or_default()),
        }
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

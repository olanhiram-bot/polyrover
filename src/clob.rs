use serde::Deserialize;

use crate::{
    gamma,
    jsonx::string_or_number,
    transport,
    types::{
        first_price, ClobMarket, ClobMarketByTokenResponse, ClobMarketOutcome, ClobNegRiskInfo,
        ClobOrderBook, ClobPaginatedMarkets, ClobServerTime, ClobTickSize, CLOB_OUTCOME_RESOLVED,
        CLOB_OUTCOME_UNRESOLVED,
    },
    Error, Result,
};

pub const DEFAULT_BASE_URL: &str = "https://clob.polymarket.com";

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

    pub fn server_time(&self) -> Result<ClobServerTime> {
        self.transport.get_json("/time")
    }

    pub fn markets(&self, next_cursor: &str) -> Result<ClobPaginatedMarkets> {
        self.transport
            .get_json(&cursor_path("/markets", next_cursor))
    }

    pub fn market(&self, condition_id: &str) -> Result<ClobMarket> {
        self.transport
            .get_json(&format!("/markets/{}", escape(condition_id)))
    }

    pub fn market_by_token(&self, token_id: &str) -> Result<ClobMarketByTokenResponse> {
        self.transport
            .get_json(&format!("/markets-by-token/{}", escape(token_id)))
    }

    pub fn market_outcome(
        &self,
        condition_id: &str,
        gamma_base_url: &str,
    ) -> Result<ClobMarketOutcome> {
        let condition_id = condition_id.trim();
        if condition_id.is_empty() {
            return Err(Error::Invalid("clob: condition_id is required".into()));
        }
        match self.market(condition_id) {
            Ok(market) => Ok(outcome_from_clob_market(condition_id, market)),
            Err(err) if !gamma_base_url.trim().is_empty() => {
                resolve_via_gamma(gamma_base_url, condition_id).or(Err(err))
            }
            Err(err) => Err(err),
        }
    }

    pub fn order_book(&self, token_id: &str) -> Result<ClobOrderBook> {
        self.transport
            .get_json(&format!("/book?token_id={}", escape(token_id)))
    }

    pub fn price(&self, token_id: &str, side: &str) -> Result<String> {
        let row: PriceResponse = self.transport.get_json(&format!(
            "/price?token_id={}&side={}",
            escape(token_id),
            escape(side)
        ))?;
        Ok(row.price)
    }

    pub fn midpoint(&self, token_id: &str) -> Result<String> {
        let row: MidpointResponse = self
            .transport
            .get_json(&format!("/midpoint?token_id={}", escape(token_id)))?;
        Ok(first_price(&[&row.mid, &row.mid_price]))
    }

    pub fn spread(&self, token_id: &str) -> Result<String> {
        let row: SpreadResponse = self
            .transport
            .get_json(&format!("/spread?token_id={}", escape(token_id)))?;
        Ok(row.spread)
    }

    pub fn tick_size(&self, token_id: &str) -> Result<ClobTickSize> {
        self.transport
            .get_json(&format!("/tick-size?token_id={}", escape(token_id)))
    }

    pub fn neg_risk(&self, token_id: &str) -> Result<ClobNegRiskInfo> {
        self.transport
            .get_json(&format!("/neg-risk?token_id={}", escape(token_id)))
    }

    pub fn simplified_markets(&self, next_cursor: &str) -> Result<ClobPaginatedMarkets> {
        self.transport
            .get_json(&cursor_path("/simplified-markets", next_cursor))
    }
}

#[derive(Deserialize)]
struct PriceResponse {
    #[serde(default, deserialize_with = "string_or_number")]
    price: String,
}

#[derive(Deserialize)]
struct MidpointResponse {
    #[serde(default, deserialize_with = "string_or_number")]
    mid: String,
    #[serde(default, deserialize_with = "string_or_number")]
    mid_price: String,
}

#[derive(Deserialize)]
struct SpreadResponse {
    #[serde(default, deserialize_with = "string_or_number")]
    spread: String,
}

fn outcome_from_clob_market(condition_id: &str, market: ClobMarket) -> ClobMarketOutcome {
    let winner = winning_token_id(&market);
    if market.closed && !winner.is_empty() {
        return ClobMarketOutcome {
            status: CLOB_OUTCOME_RESOLVED.into(),
            condition_id: condition_id.into(),
            winning_token_id: winner,
            closed: true,
            source: format!("clob:/markets/{condition_id}"),
        };
    }
    ClobMarketOutcome {
        status: CLOB_OUTCOME_UNRESOLVED.into(),
        condition_id: condition_id.into(),
        closed: market.closed,
        source: format!("clob:/markets/{condition_id}:not_closed_or_no_winner"),
        ..Default::default()
    }
}

fn winning_token_id(market: &ClobMarket) -> String {
    let mut winners = market
        .tokens
        .iter()
        .filter(|t| t.winner && !t.token_id.trim().is_empty());
    let Some(first) = winners.next() else {
        return String::new();
    };
    if winners.next().is_some() {
        String::new()
    } else {
        first.token_id.trim().into()
    }
}

fn resolve_via_gamma(gamma_base_url: &str, condition_id: &str) -> Result<ClobMarketOutcome> {
    let client = gamma::Client::new(gamma_base_url)?;
    let markets = client.markets(&gamma::MarketParams {
        condition_ids: vec![condition_id.into()],
        ..Default::default()
    })?;
    if markets.into_iter().any(|m| m.closed) {
        return Ok(ClobMarketOutcome {
            status: CLOB_OUTCOME_UNRESOLVED.into(),
            condition_id: condition_id.into(),
            closed: true,
            source: format!("gamma:closed_condition_id={condition_id}"),
            ..Default::default()
        });
    }
    Err(Error::Invalid(format!(
        "gamma: no closed market found for condition_id={condition_id}"
    )))
}

fn cursor_path(base: &str, next_cursor: &str) -> String {
    if next_cursor.is_empty() {
        base.into()
    } else {
        format!("{}?next_cursor={}", base, escape(next_cursor))
    }
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
    fn paths_match_go_clob_endpoints() {
        assert_eq!(
            cursor_path("/markets", "abc 123"),
            "/markets?next_cursor=abc%20123"
        );
        assert_eq!(
            format!("/book?token_id={}", escape("tok/1")),
            "/book?token_id=tok%2F1"
        );
    }

    #[test]
    fn midpoint_accepts_both_field_names() {
        let row: MidpointResponse = serde_json::from_str(r#"{"mid_price":0.51}"#).unwrap();
        assert_eq!(first_price(&[&row.mid, &row.mid_price]), "0.51");
    }

    #[test]
    fn market_outcome_requires_exactly_one_closed_winner() {
        let market = ClobMarket {
            closed: true,
            tokens: vec![
                crate::types::ClobToken {
                    token_id: "yes".into(),
                    winner: true,
                    ..Default::default()
                },
                crate::types::ClobToken {
                    token_id: "no".into(),
                    winner: false,
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let outcome = outcome_from_clob_market("c", market);
        assert_eq!(outcome.status, CLOB_OUTCOME_RESOLVED);
        assert_eq!(outcome.winning_token_id, "yes");

        let unresolved = outcome_from_clob_market(
            "c",
            ClobMarket {
                closed: false,
                ..Default::default()
            },
        );
        assert_eq!(unresolved.status, CLOB_OUTCOME_UNRESOLVED);
        assert!(winning_token_id(&ClobMarket {
            tokens: vec![
                crate::types::ClobToken {
                    token_id: "a".into(),
                    winner: true,
                    ..Default::default()
                },
                crate::types::ClobToken {
                    token_id: "b".into(),
                    winner: true,
                    ..Default::default()
                }
            ],
            ..Default::default()
        })
        .is_empty());
    }
}

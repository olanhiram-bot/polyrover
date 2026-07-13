//! Authoritative market outcome resolution.

use chrono::{DateTime, Utc};

use crate::{
    clob,
    gamma::{self, MarketParams},
    types::{Market, CLOB_OUTCOME_RESOLVED},
    Error, Result,
};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MarketRef {
    pub condition_id: String,
    pub slug: String,
    pub up_token_id: String,
    pub down_token_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MarketResult {
    pub condition_id: String,
    pub winning_token_id: String,
    pub resolved_at: DateTime<Utc>,
    pub observed_at: DateTime<Utc>,
    pub source: String,
}

#[derive(Clone)]
pub struct Resolver {
    clob: clob::Client,
    gamma: gamma::Client,
    gamma_base_url: String,
}

impl Resolver {
    pub fn new(
        clob_base_url: impl Into<String>,
        gamma_base_url: impl Into<String>,
    ) -> Result<Self> {
        let gamma_base_url = gamma_base_url.into();
        let gamma_base_url = if gamma_base_url.trim().is_empty() {
            gamma::DEFAULT_BASE_URL.into()
        } else {
            gamma_base_url
        };
        Ok(Self {
            clob: clob::Client::new(clob_base_url)?,
            gamma: gamma::Client::new(gamma_base_url.clone())?,
            gamma_base_url,
        })
    }

    pub fn resolve(&self, market: &MarketRef) -> Result<Option<MarketResult>> {
        self.resolve_at(market, Utc::now())
    }

    pub fn resolve_at(
        &self,
        market: &MarketRef,
        observed_at: DateTime<Utc>,
    ) -> Result<Option<MarketResult>> {
        let condition_id = market.condition_id.trim();
        if condition_id.is_empty() {
            return Err(Error::Invalid(
                "market_results: condition_id is required".into(),
            ));
        }
        let gamma_markets = if market.slug.trim().is_empty() {
            self.gamma.markets(&MarketParams {
                condition_ids: vec![condition_id.into()],
                ..Default::default()
            })?
        } else {
            vec![self.gamma.market_by_slug(market.slug.trim())?]
        };
        let Some((gamma_winner, resolved_at)) = gamma_markets
            .iter()
            .find_map(|row| exact_gamma_result(row, market))
        else {
            return Ok(None);
        };
        if resolved_at > observed_at {
            return Err(Error::Invalid(
                "market_results: observation precedes resolution".into(),
            ));
        }
        let outcome = self.clob.market_outcome(condition_id, &self.gamma_base_url);
        if let Ok(outcome) = &outcome {
            if outcome.status == CLOB_OUTCOME_RESOLVED
                && outcome.closed
                && !outcome.winning_token_id.trim().is_empty()
                && outcome.winning_token_id != gamma_winner
            {
                return Err(Error::Invalid(
                    "market_results: CLOB and Gamma winners disagree".into(),
                ));
            }
        }
        let source = match outcome {
            Ok(outcome) if outcome.winning_token_id == gamma_winner => {
                format!("{}+gamma:closedTime+exact_1_0", outcome.source)
            }
            _ => "gamma:closedTime+exact_1_0".into(),
        };
        Ok(Some(MarketResult {
            condition_id: condition_id.into(),
            winning_token_id: gamma_winner,
            resolved_at,
            observed_at,
            source,
        }))
    }
}

fn exact_gamma_result(row: &Market, market: &MarketRef) -> Option<(String, DateTime<Utc>)> {
    if row.condition_id.trim() != market.condition_id.trim() || !row.closed {
        return None;
    }
    let resolved_at = row.closed_time.0?.with_timezone(&Utc);
    let token_ids = serde_json::from_str::<Vec<String>>(&row.clob_token_ids).ok()?;
    if token_ids.len() != 2
        || row.outcome_prices.0.len() != 2
        || market.up_token_id.trim().is_empty()
        || market.down_token_id.trim().is_empty()
        || !((token_ids[0] == market.up_token_id && token_ids[1] == market.down_token_id)
            || (token_ids[1] == market.up_token_id && token_ids[0] == market.down_token_id))
    {
        return None;
    }
    let mut winner = None;
    for (index, price) in row.outcome_prices.0.iter().enumerate() {
        match price.trim().parse::<f64>().ok()? {
            1.0 if winner.is_none() => winner = Some(index),
            0.0 => {}
            _ => return None,
        }
    }
    let winner = token_ids.get(winner?)?.clone();
    (winner == market.up_token_id || winner == market.down_token_id)
        .then_some((winner, resolved_at))
}

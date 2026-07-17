//! Unified [`Client`] facade over the Gamma, CLOB, and Data API clients,
//! plus aggregate health reporting.

use crate::{
    clob,
    data::{self, ActivityParams, ClosedPositionParams, LeaderboardParams, TradeParams},
    data_types::{Activity, ClosedPosition, LeaderboardRow, Position, Trade},
    gamma::{self, MarketParams, SearchParams},
    simulation::{self, Request as SimulationRequest, ResultRow as SimulationResult},
    types::{ClobOrderBook, Market, SearchResponse},
    Result,
};
use serde::Serialize;

/// Combined reachability for the public Gamma and CLOB endpoints.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ClientHealth {
    pub gamma: String,
    pub clob: String,
}

/// Endpoint configuration for [`Client`].
#[derive(Clone, Debug)]
pub struct ClientConfig {
    pub gamma_base_url: String,
    pub clob_base_url: String,
    pub data_base_url: String,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            gamma_base_url: gamma::DEFAULT_BASE_URL.into(),
            clob_base_url: clob::DEFAULT_BASE_URL.into(),
            data_base_url: data::DEFAULT_BASE_URL.into(),
        }
    }
}

/// Unified read-only entry point for Polymarket research operations.
#[derive(Clone)]
pub struct Client {
    gamma: gamma::Client,
    clob: clob::Client,
    data: data::Client,
}

impl Client {
    /// Creates a client using the configured Gamma, CLOB, and Data endpoints.
    pub fn new(config: ClientConfig) -> Result<Self> {
        Ok(Self {
            gamma: gamma::Client::new(config.gamma_base_url)?,
            clob: clob::Client::new(config.clob_base_url)?,
            data: data::Client::new(config.data_base_url)?,
        })
    }

    pub async fn search(&self, params: &SearchParams) -> Result<SearchResponse> {
        self.gamma.search(params).await
    }

    pub async fn markets(&self, params: &MarketParams) -> Result<Vec<Market>> {
        self.gamma.markets(params).await
    }

    pub async fn market_by_slug(&self, slug: &str) -> Result<Market> {
        self.gamma.market_by_slug(slug).await
    }

    pub async fn order_book(&self, token_id: &str) -> Result<ClobOrderBook> {
        self.clob.order_book(token_id).await
    }

    pub async fn order_books(&self, token_ids: &[String]) -> Result<Vec<ClobOrderBook>> {
        self.clob.order_books(token_ids).await
    }

    pub async fn price(&self, token_id: &str, side: &str) -> Result<String> {
        self.clob.price(token_id, side).await
    }

    pub async fn current_positions(&self, user: &str, limit: u32) -> Result<Vec<Position>> {
        self.data.current_positions(user, limit).await
    }

    pub async fn closed_positions(&self, user: &str, limit: u32) -> Result<Vec<ClosedPosition>> {
        self.data.closed_positions(user, limit).await
    }

    pub async fn closed_positions_with(
        &self,
        params: &ClosedPositionParams,
    ) -> Result<Vec<ClosedPosition>> {
        self.data.closed_positions_with(params).await
    }

    pub async fn trades(&self, user: &str, limit: u32) -> Result<Vec<Trade>> {
        self.data.trades(user, limit).await
    }

    pub async fn trades_with(&self, params: &TradeParams) -> Result<Vec<Trade>> {
        self.data.trades_with(params).await
    }

    pub async fn activity(&self, user: &str, limit: u32) -> Result<Vec<Activity>> {
        self.data.activity(user, limit).await
    }

    pub async fn activity_with(&self, params: &ActivityParams) -> Result<Vec<Activity>> {
        self.data.activity_with(params).await
    }

    pub async fn trader_leaderboard(&self, limit: u32) -> Result<Vec<LeaderboardRow>> {
        self.data.trader_leaderboard(limit).await
    }

    pub async fn trader_leaderboard_with(
        &self,
        params: &LeaderboardParams,
    ) -> Result<Vec<LeaderboardRow>> {
        self.data.trader_leaderboard_with(params).await
    }

    pub async fn health(&self) -> ClientHealth {
        ClientHealth {
            gamma: health_label(self.gamma.health_check().await.is_ok()),
            clob: health_label(self.clob.health().await.is_ok()),
        }
    }

    pub async fn simulate(&self, request: SimulationRequest) -> Result<SimulationResult> {
        let book = self.order_book(&request.token_id).await?;
        simulation::simulate_book(&book, request)
    }
}

fn health_label(healthy: bool) -> String {
    if healthy { "ok" } else { "error" }.into()
}

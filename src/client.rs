use crate::{
    clob, data,
    data_types::{LeaderboardRow, Position, Trade},
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

    pub fn search(&self, params: &SearchParams) -> Result<SearchResponse> {
        self.gamma.search(params)
    }

    pub fn markets(&self, params: &MarketParams) -> Result<Vec<Market>> {
        self.gamma.markets(params)
    }

    pub fn market_by_slug(&self, slug: &str) -> Result<Market> {
        self.gamma.market_by_slug(slug)
    }

    pub fn order_book(&self, token_id: &str) -> Result<ClobOrderBook> {
        self.clob.order_book(token_id)
    }

    pub fn order_books(&self, token_ids: &[String]) -> Result<Vec<ClobOrderBook>> {
        self.clob.order_books(token_ids)
    }

    pub fn price(&self, token_id: &str, side: &str) -> Result<String> {
        self.clob.price(token_id, side)
    }

    pub fn current_positions(&self, user: &str, limit: u32) -> Result<Vec<Position>> {
        self.data.current_positions(user, limit)
    }

    pub fn trades(&self, user: &str, limit: u32) -> Result<Vec<Trade>> {
        self.data.trades(user, limit)
    }

    pub fn trader_leaderboard(&self, limit: u32) -> Result<Vec<LeaderboardRow>> {
        self.data.trader_leaderboard(limit)
    }

    pub fn health(&self) -> ClientHealth {
        ClientHealth {
            gamma: health_label(self.gamma.health_check().is_ok()),
            clob: health_label(self.clob.health().is_ok()),
        }
    }

    pub fn simulate(&self, request: SimulationRequest) -> Result<SimulationResult> {
        let book = self.order_book(&request.token_id)?;
        simulation::simulate_book(&book, request)
    }
}

fn health_label(healthy: bool) -> String {
    if healthy { "ok" } else { "error" }.into()
}

use crate::{
    transport,
    types::{Event, HealthResponse, Market, SearchResponse},
    Result,
};

pub const DEFAULT_BASE_URL: &str = "https://gamma-api.polymarket.com";

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

    pub fn health_check(&self) -> Result<HealthResponse> {
        self.transport
            .get_raw("/")
            .map(|_| HealthResponse { data: "ok".into() })
    }

    pub fn active_markets(&self) -> Result<Vec<Market>> {
        self.markets(&MarketParams {
            active: Some(true),
            closed: Some(false),
            ..Default::default()
        })
    }

    pub fn markets(&self, params: &MarketParams) -> Result<Vec<Market>> {
        self.transport.get_json(&params.path("/markets"))
    }

    pub fn market_by_id(&self, id: &str) -> Result<Market> {
        self.transport.get_json(&format!("/markets/{}", escape(id)))
    }

    pub fn events(&self, params: &EventParams) -> Result<Vec<Event>> {
        self.transport.get_json(&params.path("/events"))
    }

    pub fn event_by_id(&self, id: &str) -> Result<Event> {
        self.transport.get_json(&format!("/events/{}", escape(id)))
    }

    pub fn search(&self, params: &SearchParams) -> Result<SearchResponse> {
        self.transport.get_json(&params.path("/public-search"))
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MarketParams {
    pub limit: Option<u32>,
    pub offset: Option<u32>,
    pub order: Option<String>,
    pub ascending: Option<bool>,
    pub slug: Vec<String>,
    pub condition_ids: Vec<String>,
    pub clob_token_ids: Vec<String>,
    pub active: Option<bool>,
    pub closed: Option<bool>,
    pub tag_id: Option<i64>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct EventParams {
    pub limit: Option<u32>,
    pub offset: Option<u32>,
    pub order: Option<String>,
    pub ascending: Option<bool>,
    pub slug: Vec<String>,
    pub active: Option<bool>,
    pub closed: Option<bool>,
    pub tag_id: Option<i64>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SearchParams {
    pub q: String,
    pub limit_per_type: Option<u32>,
    pub page: Option<u32>,
    pub events_status: Option<String>,
    pub sort: Option<String>,
    pub search_profiles: Option<bool>,
}

impl MarketParams {
    pub fn path(&self, base: &str) -> String {
        let mut q = Query::new(base);
        q.opt("limit", self.limit);
        q.opt("offset", self.offset);
        q.opt_str("order", self.order.as_deref());
        q.opt("ascending", self.ascending);
        q.opt("active", self.active);
        q.opt("closed", self.closed);
        q.opt("tag_id", self.tag_id);
        q.list("slug", &self.slug);
        q.list("condition_ids", &self.condition_ids);
        q.list("clob_token_ids", &self.clob_token_ids);
        q.finish()
    }
}

impl EventParams {
    pub fn path(&self, base: &str) -> String {
        let mut q = Query::new(base);
        q.opt("limit", self.limit);
        q.opt("offset", self.offset);
        q.opt_str("order", self.order.as_deref());
        q.opt("ascending", self.ascending);
        q.opt("active", self.active);
        q.opt("closed", self.closed);
        q.opt("tag_id", self.tag_id);
        q.list("slug", &self.slug);
        q.finish()
    }
}

impl SearchParams {
    pub fn path(&self, base: &str) -> String {
        let mut q = Query::new(base);
        q.pair("q", &self.q);
        q.opt("limit_per_type", self.limit_per_type);
        q.opt("page", self.page);
        q.opt_str("events_status", self.events_status.as_deref());
        q.opt_str("sort", self.sort.as_deref());
        q.opt("search_profiles", self.search_profiles);
        q.finish()
    }
}

struct Query {
    base: String,
    pairs: Vec<(String, String)>,
}

impl Query {
    fn new(base: &str) -> Self {
        Self {
            base: base.into(),
            pairs: vec![],
        }
    }
    fn pair(&mut self, key: &str, value: &str) {
        if !value.is_empty() {
            self.pairs.push((key.into(), value.into()));
        }
    }
    fn opt<T: ToString>(&mut self, key: &str, value: Option<T>) {
        if let Some(value) = value {
            self.pair(key, &value.to_string());
        }
    }
    fn opt_str(&mut self, key: &str, value: Option<&str>) {
        if let Some(value) = value {
            self.pair(key, value);
        }
    }
    fn list(&mut self, key: &str, values: &[String]) {
        for value in values {
            self.pair(key, value);
        }
    }
    fn finish(self) -> String {
        if self.pairs.is_empty() {
            return self.base;
        }
        let query = self
            .pairs
            .into_iter()
            .map(|(k, v)| format!("{}={}", escape(&k), escape(&v)))
            .collect::<Vec<_>>()
            .join("&");
        format!("{}?{}", self.base, query)
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
    fn market_params_match_gamma_query_shape() {
        let path = MarketParams {
            limit: Some(5),
            active: Some(true),
            slug: vec!["will btc".into()],
            ..Default::default()
        }
        .path("/markets");
        assert_eq!(path, "/markets?limit=5&active=true&slug=will%20btc");
    }

    #[test]
    fn search_params_use_public_search() {
        assert_eq!(
            SearchParams {
                q: "Will BTC".into(),
                limit_per_type: Some(3),
                ..Default::default()
            }
            .path("/public-search"),
            "/public-search?q=Will%20BTC&limit_per_type=3"
        );
    }
}

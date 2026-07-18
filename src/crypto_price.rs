//! Public Polymarket web crypto reference-price client.

use chrono::{DateTime, SecondsFormat, Utc};
use serde::Deserialize;

use crate::{transport, Error, Result};

pub const DEFAULT_BASE_URL: &str = "https://polymarket.com";

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct CryptoPrice {
    #[serde(rename = "openPrice")]
    pub open_price: Option<f64>,
    #[serde(default, rename = "closePrice")]
    pub close_price: Option<f64>,
    #[serde(default)]
    pub timestamp: i64,
    #[serde(default)]
    pub completed: bool,
    #[serde(default)]
    pub incomplete: bool,
    #[serde(default)]
    pub cached: bool,
}

#[derive(Clone)]
pub struct Client {
    transport: transport::Client,
}

impl Client {
    pub fn new(base_url: impl Into<String>) -> Result<Self> {
        let base = match base_url.into() {
            s if s.trim().is_empty() => DEFAULT_BASE_URL.into(),
            s => s,
        };
        Ok(Self {
            transport: transport::Client::new(transport::Config::new(base))?,
        })
    }

    pub async fn get(
        &self,
        symbol: &str,
        event_start: DateTime<Utc>,
        variant: &str,
        end_date: DateTime<Utc>,
    ) -> Result<CryptoPrice> {
        let symbol = symbol.trim();
        let variant = variant.trim();
        if symbol.is_empty() || variant.is_empty() {
            return Err(Error::Invalid(
                "crypto price symbol and variant are required".into(),
            ));
        }
        let path = format!(
            "/api/crypto/crypto-price?symbol={}&eventStartTime={}&variant={}&endDate={}",
            escape(&symbol.to_ascii_uppercase()),
            escape(&event_start.to_rfc3339_opts(SecondsFormat::Secs, true)),
            escape(variant),
            escape(&end_date.to_rfc3339_opts(SecondsFormat::Secs, true)),
        );
        let price: CryptoPrice = self.transport.get_json(&path).await?;
        if price
            .open_price
            .is_some_and(|value| !value.is_finite() || value <= 0.0)
        {
            return Err(Error::Invalid(format!(
                "crypto price openPrice is invalid: {:?}",
                price.open_price
            )));
        }
        if price
            .close_price
            .is_some_and(|value| !value.is_finite() || value <= 0.0)
        {
            return Err(Error::Invalid("crypto price closePrice is invalid".into()));
        }
        Ok(price)
    }
}

fn escape(value: &str) -> String {
    let mut out = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

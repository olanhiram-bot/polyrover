use std::{env, time::Duration};

use crate::{Error, Result};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Config {
    pub mode: String,
    pub gamma_base_url: String,
    pub clob_base_url: String,
    pub request_timeout: Duration,
    pub live_trading_enabled: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            mode: "read-only".into(),
            gamma_base_url: "https://gamma-api.polymarket.com".into(),
            clob_base_url: "https://clob.polymarket.com".into(),
            request_timeout: Duration::from_secs(10),
            live_trading_enabled: false,
        }
    }
}

impl Config {
    pub fn from_env(prefix: &str) -> Result<Self> {
        let mut cfg = Self::default();
        let prefix = if prefix.is_empty() {
            "POLYMARKET"
        } else {
            prefix
        };
        read_env(prefix, "MODE", &mut cfg.mode);
        read_env(prefix, "GAMMA_BASE_URL", &mut cfg.gamma_base_url);
        read_env(prefix, "CLOB_BASE_URL", &mut cfg.clob_base_url);
        if let Ok(raw) = env::var(format!("{prefix}_REQUEST_TIMEOUT")) {
            cfg.request_timeout = parse_duration(&raw)?;
        }
        if let Ok(raw) = env::var(format!("{prefix}_LIVE_TRADING_ENABLED")) {
            cfg.live_trading_enabled = raw
                .parse()
                .map_err(|_| Error::Invalid("live_trading_enabled must be boolean".into()))?;
        }
        if cfg.mode != "read-only" && cfg.mode != "paper" && cfg.mode != "live" {
            return Err(Error::Invalid(format!("invalid mode: {}", cfg.mode)));
        }
        if cfg.request_timeout.is_zero() {
            return Err(Error::Invalid("request_timeout must be positive".into()));
        }
        Ok(cfg)
    }
}

fn read_env(prefix: &str, key: &str, target: &mut String) {
    if let Ok(value) = env::var(format!("{prefix}_{key}")) {
        *target = value;
    }
}

fn parse_duration(raw: &str) -> Result<Duration> {
    if let Some(ms) = raw.strip_suffix("ms") {
        return ms
            .parse::<u64>()
            .map(Duration::from_millis)
            .map_err(|_| Error::Invalid(format!("invalid duration: {raw}")));
    }
    if let Some(seconds) = raw.strip_suffix('s') {
        return seconds
            .parse::<u64>()
            .map(Duration::from_secs)
            .map_err(|_| Error::Invalid(format!("invalid duration: {raw}")));
    }
    Err(Error::Invalid(format!("invalid duration: {raw}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_read_only_and_no_live() {
        let cfg = Config::default();
        assert_eq!(cfg.mode, "read-only");
        assert!(!cfg.live_trading_enabled);
    }

    #[test]
    fn parses_seconds_duration() {
        assert_eq!(parse_duration("10s").unwrap(), Duration::from_secs(10));
    }
}

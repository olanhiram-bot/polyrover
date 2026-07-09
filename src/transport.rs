use serde::de::DeserializeOwned;

use crate::{Error, Result};

#[derive(Clone, Debug)]
pub struct Config {
    pub base_url: String,
    pub timeout_secs: u64,
    pub user_agent: String,
}

impl Config {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: trim_base(base_url.into()),
            timeout_secs: 30,
            user_agent: "polyrover/0.1".into(),
        }
    }
}

#[derive(Clone)]
pub struct Client {
    base_url: String,
    http: reqwest::blocking::Client,
}

impl Client {
    pub fn new(config: Config) -> Result<Self> {
        let http = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(config.timeout_secs))
            .user_agent(config.user_agent)
            .build()?;
        Ok(Self {
            base_url: config.base_url,
            http,
        })
    }

    pub fn get_json<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let response = self.http.get(self.url(path)?).send()?;
        let status = response.status();
        let body = response.text()?;
        if !status.is_success() {
            return Err(Error::Api {
                status: status.as_u16(),
                body,
            });
        }
        Ok(serde_json::from_str(&body)?)
    }

    pub fn get_raw(&self, path: &str) -> Result<String> {
        let response = self.http.get(self.url(path)?).send()?;
        let status = response.status();
        let body = response.text()?;
        if !status.is_success() {
            return Err(Error::Api {
                status: status.as_u16(),
                body,
            });
        }
        Ok(body)
    }

    fn url(&self, path: &str) -> Result<String> {
        if !path.starts_with('/') {
            return Err(Error::Url(format!("path must start with /: {path}")));
        }
        Ok(format!("{}{}", self.base_url, path))
    }
}

fn trim_base(mut base: String) -> String {
    while base.ends_with('/') {
        base.pop();
    }
    base
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trims_base_url_slash() {
        assert_eq!(
            Config::new("https://example.test///").base_url,
            "https://example.test"
        );
    }
}

use serde::{de::DeserializeOwned, Serialize};

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
        let body = checked_body(self.http.get(self.url(path)?).send()?)?;
        Ok(serde_json::from_str(&body)?)
    }

    pub fn post_json<B: Serialize, T: DeserializeOwned>(&self, path: &str, body: &B) -> Result<T> {
        let body = checked_body(self.http.post(self.url(path)?).json(body).send()?)?;
        Ok(serde_json::from_str(&body)?)
    }

    pub fn get_raw(&self, path: &str) -> Result<String> {
        checked_body(self.http.get(self.url(path)?).send()?)
    }

    fn url(&self, path: &str) -> Result<String> {
        if !path.starts_with('/') {
            return Err(Error::Url(format!("path must start with /: {path}")));
        }
        Ok(format!("{}{}", self.base_url, path))
    }
}

fn checked_body(response: reqwest::blocking::Response) -> Result<String> {
    let status = response.status();
    let retry_after_secs = response
        .headers()
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok());
    let body = response.text()?;
    if status.as_u16() == 429 {
        return Err(Error::RateLimited { retry_after_secs });
    }
    if !status.is_success() {
        return Err(Error::Api {
            status: status.as_u16(),
            body,
        });
    }
    Ok(body)
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

    #[test]
    fn rate_limit_preserves_retry_after() {
        use std::{
            io::{Read, Write},
            net::TcpListener,
            thread,
        };
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0; 1024];
            let _ = stream.read(&mut request).unwrap();
            stream.write_all(
                b"HTTP/1.1 429 Too Many Requests\r\nRetry-After: 3\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            )
            .unwrap();
        });
        let client = Client::new(Config::new(format!("http://{address}"))).unwrap();
        assert!(matches!(
            client.get_raw("/limited"),
            Err(Error::RateLimited {
                retry_after_secs: Some(3)
            })
        ));
        server.join().unwrap();
    }
}

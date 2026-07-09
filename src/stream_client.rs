use std::{net::TcpStream, thread, time::Duration};

use tungstenite::{connect, stream::MaybeTlsStream, Message, WebSocket};

use crate::{
    stream::{market_subscription, split_array, Config, Deduplicator, RawMessage, StreamStats},
    Error, Result,
};

pub struct MarketWsClient {
    socket: WebSocket<MaybeTlsStream<TcpStream>>,
    config: Config,
    stats: StreamStats,
    dedup: Deduplicator,
}

impl MarketWsClient {
    pub fn connect(config: Config) -> Result<Self> {
        let (socket, _) = connect(config.url.as_str()).map_err(ws_err)?;
        let mut stats = StreamStats::new("market");
        stats.mark_connected();
        Ok(Self {
            socket,
            config,
            stats,
            dedup: Deduplicator::new(4096, 120_000),
        })
    }

    pub fn connect_with_retries(config: Config) -> Result<Self> {
        let delays = reconnect_delays(&config);
        let mut last_err = None;
        for attempt in 0..=delays.len() {
            match Self::connect(config.clone()) {
                Ok(client) => return Ok(client),
                Err(err) => last_err = Some(err),
            }
            if let Some(delay) = delays.get(attempt) {
                thread::sleep(*delay);
            }
        }
        Err(last_err.unwrap_or_else(|| Error::Invalid("websocket connect failed".into())))
    }

    pub fn subscribe_assets(&mut self, asset_ids: &[String]) -> Result<()> {
        let msg = market_subscription(asset_ids, &self.config)?;
        self.socket
            .send(Message::Text(msg.to_string().into()))
            .map_err(ws_err)?;
        self.stats.set_subscriptions(asset_ids);
        Ok(())
    }

    pub fn read_raw(&mut self, observed_at_ms: i64) -> Result<Vec<RawMessage>> {
        let text = message_text(self.socket.read().map_err(ws_err)?)?;
        if !self.dedup.process_at(&text, observed_at_ms) {
            self.stats.record_duplicate();
            return Ok(vec![]);
        }
        let values = raw_values_from_text(&text);
        if values.is_empty() {
            self.stats.record_invalid();
        }
        let mut out = Vec::with_capacity(values.len());
        for value in values {
            let raw = RawMessage::from_payload(value, observed_at_ms);
            self.stats.record_event(&raw.event_type, observed_at_ms);
            out.push(raw);
        }
        Ok(out)
    }

    pub fn ping(&mut self) -> Result<()> {
        self.socket
            .send(Message::Ping(Vec::new().into()))
            .map_err(ws_err)
    }

    pub fn stats(&self) -> crate::stream::StreamStatsSnapshot {
        self.stats.snapshot()
    }

    pub fn close(mut self) -> Result<()> {
        self.socket.close(None).map_err(ws_err)
    }
}

pub fn reconnect_delays(config: &Config) -> Vec<Duration> {
    if !config.reconnect || config.reconnect_max == 0 {
        return vec![];
    }
    let base = config.reconnect_delay_secs.max(1);
    let max = config.reconnect_max_delay_secs.max(base);
    (0..config.reconnect_max)
        .map(|attempt| Duration::from_secs((base.saturating_mul(1 << attempt.min(16))).min(max)))
        .collect()
}

fn raw_values_from_text(text: &str) -> Vec<serde_json::Value> {
    split_array(text).unwrap_or_else(|| {
        serde_json::from_str(text)
            .map(|v| vec![v])
            .unwrap_or_default()
    })
}

fn message_text(message: Message) -> Result<String> {
    match message {
        Message::Text(text) => Ok(text.to_string()),
        Message::Binary(bytes) => String::from_utf8(bytes.to_vec())
            .map_err(|err| Error::Invalid(format!("ws binary is not utf8: {err}"))),
        Message::Close(_) => Err(Error::Invalid("ws closed".into())),
        _ => Ok(String::new()),
    }
}

fn ws_err(err: tungstenite::Error) -> Error {
    Error::Invalid(format!("websocket: {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn text_and_binary_messages_normalize() {
        assert_eq!(
            message_text(Message::Text("hello".into())).unwrap(),
            "hello"
        );
        assert_eq!(
            message_text(Message::Binary(b"hello".as_slice().into())).unwrap(),
            "hello"
        );
        assert!(message_text(Message::Binary(vec![0xff].into())).is_err());
        assert!(message_text(Message::Close(None)).is_err());
    }

    #[test]
    fn raw_values_handle_single_array_and_malformed_json() {
        assert_eq!(raw_values_from_text(r#"{"event_type":"book"}"#).len(), 1);
        assert_eq!(
            raw_values_from_text(r#"[{"event_type":"book"},{"event_type":"price_change"}]"#).len(),
            2
        );
        assert!(raw_values_from_text("not-json").is_empty());
    }

    #[test]
    fn reconnect_delays_backoff_and_cap() {
        let cfg = Config {
            reconnect: true,
            reconnect_delay_secs: 2,
            reconnect_max_delay_secs: 5,
            reconnect_max: 4,
            ..Default::default()
        };
        assert_eq!(
            reconnect_delays(&cfg),
            vec![
                Duration::from_secs(2),
                Duration::from_secs(4),
                Duration::from_secs(5),
                Duration::from_secs(5)
            ]
        );
        assert!(reconnect_delays(&Config {
            reconnect: false,
            ..Default::default()
        })
        .is_empty());
    }

    #[test]
    fn subscription_payload_is_client_compatible() {
        let cfg = Config {
            level: 1,
            custom_feature_enabled: true,
            ..Default::default()
        };
        let got = market_subscription(&["token".into()], &cfg).unwrap();
        assert_eq!(
            got,
            json!({"type":"market","assets_ids":["token"],"level":1,"custom_feature_enabled":true})
        );
    }
}

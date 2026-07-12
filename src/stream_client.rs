use std::{
    io::ErrorKind,
    net::TcpStream,
    thread,
    time::{Duration, Instant},
};

use tungstenite::{connect, stream::MaybeTlsStream, Message, WebSocket};

use crate::{
    market_data::{TrackedEvent, Tracker},
    stream::{
        market_subscription, parse_market_event, split_array, Config, Deduplicator, MarketEvent,
        RawMessage, StreamStats,
    },
    Error, Result,
};

#[derive(Clone, Debug)]
pub struct MarketRead {
    pub messages: Vec<RawMessage>,
    pub received_valid_frame: bool,
    pub invalid_frame: bool,
    pub reconnected: bool,
}

#[derive(Clone, Debug)]
pub struct TrackedMarketRead {
    pub events: Vec<TrackedEvent>,
    pub received_valid_frame: bool,
    pub invalid_frame: bool,
    pub reconnected: bool,
}

pub struct MarketWsClient {
    socket: WebSocket<MaybeTlsStream<TcpStream>>,
    config: Config,
    stats: StreamStats,
    dedup: Deduplicator,
    tracker: Tracker,
    subscriptions: Vec<String>,
    last_ping: Instant,
}

impl MarketWsClient {
    pub fn connect(config: Config) -> Result<Self> {
        let socket = Self::dial(&config)?;
        let mut stats = StreamStats::new("market");
        stats.mark_connected();
        Ok(Self {
            socket,
            config,
            stats,
            dedup: Deduplicator::new(4096, 120_000),
            tracker: Tracker::new(),
            subscriptions: Vec::new(),
            last_ping: Instant::now(),
        })
    }

    pub fn connect_with_retries(config: Config) -> Result<Self> {
        let socket = Self::dial_with_retries(&config)?;
        let mut stats = StreamStats::new("market");
        stats.mark_connected();
        Ok(Self {
            socket,
            config,
            stats,
            dedup: Deduplicator::new(4096, 120_000),
            tracker: Tracker::new(),
            subscriptions: Vec::new(),
            last_ping: Instant::now(),
        })
    }

    fn dial(config: &Config) -> Result<WebSocket<MaybeTlsStream<TcpStream>>> {
        let (mut socket, _) = connect(config.url.as_str()).map_err(ws_err)?;
        set_read_timeout(&mut socket, Duration::from_millis(100))?;
        Ok(socket)
    }

    fn dial_with_retries(config: &Config) -> Result<WebSocket<MaybeTlsStream<TcpStream>>> {
        let delays = reconnect_delays(config);
        let mut last_err = None;
        for attempt in 0..=delays.len() {
            match Self::dial(config) {
                Ok(socket) => return Ok(socket),
                Err(err) => last_err = Some(err),
            }
            if let Some(delay) = delays.get(attempt) {
                thread::sleep(*delay);
            }
        }
        Err(Error::ReconnectExhausted {
            attempts: delays.len() as u32 + 1,
            last_error: last_err
                .map(|err| err.to_string())
                .unwrap_or_else(|| "websocket connect failed".into()),
        })
    }

    pub fn subscribe_assets(&mut self, asset_ids: &[String]) -> Result<()> {
        let msg = market_subscription(asset_ids, &self.config)?;
        self.socket
            .send(Message::Text(msg.to_string().into()))
            .map_err(ws_err)?;
        self.stats.set_subscriptions(asset_ids);
        self.subscriptions = asset_ids.to_vec();
        Ok(())
    }

    pub fn read_raw_with_status(&mut self, observed_at_ms: i64) -> Result<MarketRead> {
        let mut reconnected = false;
        if self.last_ping.elapsed() >= Duration::from_secs(self.config.ping_interval_secs.max(1)) {
            if let Err(err) = self.ping() {
                if !self.config.reconnect {
                    return Err(err);
                }
                self.reconnect_and_resubscribe()?;
                reconnected = true;
            }
        }
        let message = match self.socket.read() {
            Ok(message) => message,
            Err(tungstenite::Error::Io(err))
                if matches!(err.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) =>
            {
                return Ok(MarketRead {
                    messages: Vec::new(),
                    received_valid_frame: false,
                    invalid_frame: false,
                    reconnected,
                });
            }
            Err(_) if self.config.reconnect => {
                self.reconnect_and_resubscribe()?;
                reconnected = true;
                self.socket.read().map_err(ws_err)?
            }
            Err(err) => return Err(ws_err(err)),
        };
        if matches!(&message, Message::Ping(_) | Message::Pong(_)) {
            return Ok(MarketRead {
                messages: Vec::new(),
                received_valid_frame: true,
                invalid_frame: false,
                reconnected,
            });
        }
        let text = message_text(message)?;
        if text.trim().eq_ignore_ascii_case("PONG") {
            return Ok(MarketRead {
                messages: Vec::new(),
                received_valid_frame: true,
                invalid_frame: false,
                reconnected,
            });
        }
        if !self.dedup.process_at(&text, observed_at_ms) {
            self.stats.record_duplicate();
            return Ok(MarketRead {
                messages: Vec::new(),
                received_valid_frame: true,
                invalid_frame: false,
                reconnected,
            });
        }
        let values = raw_values_from_text(&text);
        if values.is_empty() {
            self.stats.record_invalid();
            return Ok(MarketRead {
                messages: Vec::new(),
                received_valid_frame: false,
                invalid_frame: true,
                reconnected,
            });
        }
        let mut messages = Vec::with_capacity(values.len());
        for value in values {
            let raw = RawMessage::from_payload(value, observed_at_ms);
            self.stats.record_event(&raw.event_type, observed_at_ms);
            messages.push(raw);
        }
        Ok(MarketRead {
            messages,
            received_valid_frame: true,
            invalid_frame: false,
            reconnected,
        })
    }

    pub fn read_raw(&mut self, observed_at_ms: i64) -> Result<Vec<RawMessage>> {
        Ok(self.read_raw_with_status(observed_at_ms)?.messages)
    }

    fn reconnect_and_resubscribe(&mut self) -> Result<()> {
        self.stats.mark_disconnected();
        self.socket = Self::dial_with_retries(&self.config)?;
        self.stats.mark_connected();
        self.stats.record_reconnect();
        self.last_ping = Instant::now();
        let subscriptions = self.subscriptions.clone();
        self.subscribe_assets(&subscriptions)
    }

    pub fn read_events(&mut self, observed_at_ms: i64) -> Result<Vec<MarketEvent>> {
        self.read_raw(observed_at_ms)?
            .into_iter()
            .map(|raw| parse_market_event(&raw.payload.to_string()))
            .collect()
    }

    pub fn read_tracked_with_status(&mut self, observed_at_ms: i64) -> Result<TrackedMarketRead> {
        let read = self.read_raw_with_status(observed_at_ms)?;
        let events = read
            .messages
            .into_iter()
            .map(|raw| parse_market_event(&raw.payload.to_string()))
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .map(|event| self.tracker.apply_event(event))
            .collect();
        Ok(TrackedMarketRead {
            events,
            received_valid_frame: read.received_valid_frame,
            invalid_frame: read.invalid_frame,
            reconnected: read.reconnected,
        })
    }

    pub fn read_tracked(&mut self, observed_at_ms: i64) -> Result<Vec<TrackedEvent>> {
        Ok(self.read_tracked_with_status(observed_at_ms)?.events)
    }

    pub fn ping(&mut self) -> Result<()> {
        self.socket
            .send(Message::Text("PING".into()))
            .map_err(ws_err)?;
        self.last_ping = Instant::now();
        Ok(())
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

fn set_read_timeout(
    socket: &mut WebSocket<MaybeTlsStream<TcpStream>>,
    timeout: Duration,
) -> Result<()> {
    let result = match socket.get_mut() {
        MaybeTlsStream::Plain(stream) => stream.set_read_timeout(Some(timeout)),
        MaybeTlsStream::Rustls(stream) => stream.sock.set_read_timeout(Some(timeout)),
        _ => return Err(Error::Invalid("unsupported websocket transport".into())),
    };
    result.map_err(|err| Error::Invalid(format!("websocket read timeout: {err}")))
}

fn ws_err(err: tungstenite::Error) -> Error {
    Error::WebSocket(err.to_string())
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
    fn reads_typed_market_events_from_websocket() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut socket = tungstenite::accept(stream).unwrap();
            assert!(socket.read().unwrap().to_string().contains("token-1"));
            socket
                .send(Message::Text(
                    r#"{"event_type":"new_market","id":"market-1"}"#.into(),
                ))
                .unwrap();
        });
        let mut client = MarketWsClient::connect(Config {
            url: format!("ws://{address}"),
            ..Default::default()
        })
        .unwrap();
        client.subscribe_assets(&["token-1".into()]).unwrap();
        assert!(matches!(
            client.read_events(1).unwrap().as_slice(),
            [crate::stream::MarketEvent::NewMarket(market)] if market.id == "market-1"
        ));
        server.join().unwrap();
    }

    #[test]
    fn idle_connection_sends_text_heartbeat() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut socket = tungstenite::accept(stream).unwrap();
            assert!(socket.read().unwrap().to_string().contains("token-1"));
            assert_eq!(socket.read().unwrap().to_string(), "PING");
            socket
                .send(Message::Text(
                    r#"{"event_type":"new_market","id":"heartbeat-ok"}"#.into(),
                ))
                .unwrap();
        });
        let mut client = MarketWsClient::connect(Config {
            url: format!("ws://{address}"),
            ping_interval_secs: 1,
            ..Default::default()
        })
        .unwrap();
        client.subscribe_assets(&["token-1".into()]).unwrap();

        let mut rows = Vec::new();
        for _ in 0..12 {
            rows = client.read_raw(1).unwrap();
            if !rows.is_empty() {
                break;
            }
        }
        assert_eq!(rows[0].event_type, "new_market");
        server.join().unwrap();
    }

    #[test]
    fn reconnects_and_resubscribes_after_reset() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut socket = tungstenite::accept(stream).unwrap();
            assert!(socket.read().unwrap().to_string().contains("token-1"));
            drop(socket);

            let (stream, _) = listener.accept().unwrap();
            let mut socket = tungstenite::accept(stream).unwrap();
            assert!(socket.read().unwrap().to_string().contains("token-1"));
            socket
                .send(Message::Text(
                    r#"{"event_type":"new_market","id":"market-2"}"#.into(),
                ))
                .unwrap();
        });
        let mut client = MarketWsClient::connect(Config {
            url: format!("ws://{address}"),
            ..Default::default()
        })
        .unwrap();
        client.subscribe_assets(&["token-1".into()]).unwrap();

        let rows = client.read_raw(1).unwrap();

        assert_eq!(rows[0].event_type, "new_market");
        server.join().unwrap();
    }

    #[test]
    fn reconnect_preserves_stream_stats_and_counts_the_reconnect() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut socket = tungstenite::accept(stream).unwrap();
            assert!(socket.read().unwrap().to_string().contains("token-1"));
            socket
                .send(Message::Text(
                    r#"{"event_type":"new_market","id":"market-1"}"#.into(),
                ))
                .unwrap();
            drop(socket);

            let (stream, _) = listener.accept().unwrap();
            let mut socket = tungstenite::accept(stream).unwrap();
            assert!(socket.read().unwrap().to_string().contains("token-1"));
            socket
                .send(Message::Text(
                    r#"{"event_type":"new_market","id":"market-2"}"#.into(),
                ))
                .unwrap();
        });
        let mut client = MarketWsClient::connect(Config {
            url: format!("ws://{address}"),
            ..Default::default()
        })
        .unwrap();
        client.subscribe_assets(&["token-1".into()]).unwrap();

        let mut seen = Vec::new();
        while seen.len() < 2 {
            for raw in client.read_raw(1).unwrap() {
                seen.push(raw.payload["id"].as_str().unwrap_or_default().to_string());
            }
        }
        assert_eq!(seen, vec!["market-1", "market-2"]);

        let stats = client.stats();
        assert_eq!(stats.reconnects, 1);
        assert_eq!(stats.messages_received, 2);
        server.join().unwrap();
    }

    #[test]
    fn reconnect_preserves_dedup_so_replayed_messages_stay_suppressed() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut socket = tungstenite::accept(stream).unwrap();
            assert!(socket.read().unwrap().to_string().contains("token-1"));
            socket
                .send(Message::Text(
                    r#"{"event_type":"book","hash":"h1","asset_id":"token-1"}"#.into(),
                ))
                .unwrap();
            drop(socket);

            let (stream, _) = listener.accept().unwrap();
            let mut socket = tungstenite::accept(stream).unwrap();
            assert!(socket.read().unwrap().to_string().contains("token-1"));
            socket
                .send(Message::Text(
                    r#"{"event_type":"book","hash":"h1","asset_id":"token-1"}"#.into(),
                ))
                .unwrap();
            socket
                .send(Message::Text(
                    r#"{"event_type":"book","hash":"h2","asset_id":"token-1"}"#.into(),
                ))
                .unwrap();
        });
        let mut client = MarketWsClient::connect(Config {
            url: format!("ws://{address}"),
            ..Default::default()
        })
        .unwrap();
        client.subscribe_assets(&["token-1".into()]).unwrap();

        let mut seen = Vec::new();
        while seen.len() < 2 {
            for raw in client.read_raw(1).unwrap() {
                seen.push(raw.payload["hash"].as_str().unwrap_or_default().to_string());
            }
        }
        assert_eq!(seen, vec!["h1", "h2"]);
        assert_eq!(client.stats().duplicate_messages, 1);
        server.join().unwrap();
    }

    #[test]
    fn read_status_distinguishes_inbound_control_frame_from_timeout() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut socket = tungstenite::accept(stream).unwrap();
            let _subscription = socket.read().unwrap();
            socket.send(Message::Text("PONG".into())).unwrap();
        });
        let mut client = MarketWsClient::connect(Config {
            url: format!("ws://{address}"),
            ..Config::default()
        })
        .unwrap();
        client.subscribe_assets(&["token-1".into()]).unwrap();
        let read = client.read_raw_with_status(1_000).unwrap();
        assert!(read.received_valid_frame);
        assert!(!read.invalid_frame);
        assert!(!read.reconnected);
        assert!(read.messages.is_empty());
        server.join().unwrap();
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

//! User WSS endpoint configuration and typed order/trade message shapes.

use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::net::TcpStream;
use tokio_tungstenite::{
    connect_async,
    tungstenite::{Error as WsError, Message},
    MaybeTlsStream, WebSocketStream,
};

use crate::{
    auth::ApiKey,
    stream::{Config, StreamStats},
    Error, Result,
};

pub const DEFAULT_USER_URL: &str = "wss://ws-subscriptions-clob.polymarket.com/ws/user";

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct UserOrderMessage {
    #[serde(default)]
    pub event_type: String,
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub order_id: String,
    #[serde(default)]
    pub market: String,
    #[serde(default)]
    pub asset_id: String,
    #[serde(default)]
    pub side: String,
    #[serde(default)]
    pub price: String,
    #[serde(default)]
    pub size: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub timestamp: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct UserTradeMessage {
    #[serde(default)]
    pub event_type: String,
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub trade_id: String,
    #[serde(default)]
    pub order_id: String,
    #[serde(default)]
    pub market: String,
    #[serde(default)]
    pub asset_id: String,
    #[serde(default)]
    pub side: String,
    #[serde(default)]
    pub price: String,
    #[serde(default)]
    pub size: String,
    #[serde(default)]
    pub fee_rate_bps: String,
    #[serde(default)]
    pub timestamp: String,
    #[serde(default)]
    pub transaction_hash: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum UserEvent {
    Order(UserOrderMessage),
    Trade(UserTradeMessage),
    Ignored,
}

pub struct UserWsClient {
    socket: WebSocketStream<MaybeTlsStream<TcpStream>>,
    credentials: ApiKey,
    stats: StreamStats,
}

impl UserWsClient {
    pub async fn connect(mut config: Config, credentials: ApiKey) -> Result<Self> {
        credentials.validate()?;
        if config.url.is_empty() {
            config.url = DEFAULT_USER_URL.into();
        }
        let (socket, _) = connect_async(config.url.as_str()).await.map_err(ws_err)?;
        let mut stats = StreamStats::new("user");
        stats.mark_connected();
        Ok(Self {
            socket,
            credentials,
            stats,
        })
    }

    pub async fn subscribe_user(&mut self, markets: &[String]) -> Result<()> {
        let payload = user_subscription_payload(&self.credentials, markets)?;
        self.socket
            .send(Message::Text(payload.to_string().into()))
            .await
            .map_err(ws_err)?;
        self.stats.set_subscriptions(markets);
        Ok(())
    }

    pub async fn read_event(&mut self, observed_at_ms: i64) -> Result<UserEvent> {
        let message = self
            .socket
            .next()
            .await
            .ok_or_else(|| Error::Invalid("user ws closed".into()))?
            .map_err(ws_err)?;
        let text = match message {
            Message::Text(text) => text.to_string(),
            Message::Binary(bytes) => String::from_utf8(bytes.to_vec())
                .map_err(|err| Error::Invalid(format!("user ws binary is not utf8: {err}")))?,
            Message::Close(_) => return Err(Error::Invalid("user ws closed".into())),
            _ => return Ok(UserEvent::Ignored),
        };
        let event = parse_user_event(&text)?;
        match &event {
            UserEvent::Order(msg) => self.stats.record_event(&msg.event_type, observed_at_ms),
            UserEvent::Trade(msg) => self.stats.record_event(&msg.event_type, observed_at_ms),
            UserEvent::Ignored => self.stats.record_invalid(),
        }
        Ok(event)
    }

    pub fn stats(&self) -> crate::stream::StreamStatsSnapshot {
        self.stats.snapshot()
    }
    pub async fn close(mut self) -> Result<()> {
        self.socket.close(None).await.map_err(ws_err)
    }
}

pub fn user_subscription_payload(credentials: &ApiKey, markets: &[String]) -> Result<Value> {
    credentials.validate()?;
    Ok(json!({
        "type": "user",
        "markets": markets,
        "auth": {
            "apiKey": credentials.key,
            "secret": credentials.secret,
            "passphrase": credentials.passphrase,
        }
    }))
}

pub fn redacted_user_subscription_payload(
    credentials: &ApiKey,
    markets: &[String],
) -> Result<Value> {
    credentials.validate()?;
    let redacted = credentials.redacted();
    Ok(
        json!({"type":"user","markets":markets,"auth":{"apiKey":redacted.key,"secret":redacted.secret,"passphrase":redacted.passphrase}}),
    )
}

pub fn parse_user_event(text: &str) -> Result<UserEvent> {
    let value: Value = serde_json::from_str(text)?;
    let event_type = value
        .get("event_type")
        .or_else(|| value.get("type"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    match event_type {
        "order" => {
            let mut msg: UserOrderMessage = serde_json::from_value(value)?;
            msg.event_type = "order".into();
            Ok(UserEvent::Order(msg))
        }
        "trade" => {
            let mut msg: UserTradeMessage = serde_json::from_value(value)?;
            msg.event_type = "trade".into();
            Ok(UserEvent::Trade(msg))
        }
        _ => Ok(UserEvent::Ignored),
    }
}

fn ws_err(err: WsError) -> Error {
    Error::Invalid(format!("user websocket: {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key() -> ApiKey {
        ApiKey {
            key: "api-key-1234".into(),
            secret: "secret".into(),
            passphrase: "pass".into(),
        }
    }

    #[tokio::test]
    async fn reads_user_event_over_async_websocket() {
        use futures_util::{SinkExt, StreamExt};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
            let subscription = socket.next().await.unwrap().unwrap().to_string();
            assert!(subscription.contains("api-key-1234"));
            socket
                .send(tokio_tungstenite::tungstenite::Message::Text(
                    r#"{"event_type":"order","id":"o1"}"#.into(),
                ))
                .await
                .unwrap();
        });
        let mut client = UserWsClient::connect(
            Config {
                url: format!("ws://{address}"),
                ..Default::default()
            },
            key(),
        )
        .await
        .unwrap();
        client.subscribe_user(&["market-1".into()]).await.unwrap();
        assert!(matches!(
            client.read_event(1).await.unwrap(),
            UserEvent::Order(order) if order.id == "o1"
        ));
        server.await.unwrap();
    }

    #[test]
    fn user_payload_validates_and_redacts() {
        assert!(user_subscription_payload(&ApiKey::default(), &[]).is_err());
        let payload = user_subscription_payload(&key(), &["m1".into()]).unwrap();
        assert_eq!(payload["auth"]["apiKey"], "api-key-1234");
        let redacted = redacted_user_subscription_payload(&key(), &["m1".into()]).unwrap();
        assert_eq!(redacted["auth"]["secret"], "<redacted>");
    }

    #[test]
    fn parses_order_trade_and_unknown_events() {
        match parse_user_event(r#"{"event_type":"order","id":"o1","status":"matched"}"#).unwrap() {
            UserEvent::Order(order) => assert_eq!(order.id, "o1"),
            _ => panic!("want order"),
        }
        match parse_user_event(r#"{"type":"trade","trade_id":"t1","price":"0.5"}"#).unwrap() {
            UserEvent::Trade(trade) => assert_eq!(trade.trade_id, "t1"),
            _ => panic!("want trade"),
        }
        assert_eq!(
            parse_user_event(r#"{"event_type":"noop"}"#).unwrap(),
            UserEvent::Ignored
        );
    }
}

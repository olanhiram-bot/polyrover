#![cfg(feature = "public")]

use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::mpsc,
    thread,
};

use chrono::{TimeZone, Utc};

use polyrover::{
    data::{ActivityParams, ClosedPositionParams, LeaderboardParams, TradeParams},
    gamma::{MarketKeysetParams, MarketParams, SearchParams},
    simulation::Request,
    stream::{parse_market_event, MarketEvent},
    Client, ClientConfig,
};

fn serve_json(body: &'static str) -> (String, mpsc::Receiver<String>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let (requests, received) = mpsc::channel();
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut raw = [0; 4096];
        let length = stream.read(&mut raw).unwrap();
        requests
            .send(String::from_utf8_lossy(&raw[..length]).into_owned())
            .unwrap();
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        )
        .unwrap();
    });
    (format!("http://{address}"), received, handle)
}

#[tokio::test]
async fn client_searches_markets_through_one_public_interface() {
    let (gamma_base_url, received, server) =
        serve_json(r#"{"events":[{"id":"event-1","title":"Bitcoin"}]}"#);
    let client = Client::new(ClientConfig {
        gamma_base_url,
        ..ClientConfig::default()
    })
    .unwrap();

    let response = client
        .search(&SearchParams {
            q: "bitcoin".into(),
            limit_per_type: Some(1),
            ..SearchParams::default()
        })
        .await
        .unwrap();

    assert_eq!(response.events[0].id, "event-1");
    let request = received.recv().unwrap();
    assert!(request.starts_with("GET /public-search?"));
    assert!(request.contains("q=bitcoin"));
    server.join().unwrap();
}

#[tokio::test]
async fn client_reads_clob_books_through_one_public_interface() {
    let (clob_base_url, received, server) =
        serve_json(r#"{"asset_id":"token-1","bids":[],"asks":[]}"#);
    let client = Client::new(ClientConfig {
        clob_base_url,
        ..ClientConfig::default()
    })
    .unwrap();

    let book = client.order_book("token-1").await.unwrap();

    assert_eq!(book.asset_id, "token-1");
    assert!(received
        .recv()
        .unwrap()
        .starts_with("GET /book?token_id=token-1 "));
    server.join().unwrap();
}

#[tokio::test]
async fn client_reads_clob_books_in_one_batch() {
    let (clob_base_url, received, server) = serve_json(
        r#"[{"asset_id":"token-1","bids":[],"asks":[]},{"asset_id":"token-2","bids":[],"asks":[]}]"#,
    );
    let client = Client::new(ClientConfig {
        clob_base_url,
        ..ClientConfig::default()
    })
    .unwrap();

    let books = client
        .order_books(&["token-1".into(), "token-2".into()])
        .await
        .unwrap();

    assert_eq!(books.len(), 2);
    let request = received.recv().unwrap();
    assert!(request.starts_with("POST /books "));
    assert!(request.contains(r#"{"token_id":"token-1"}"#));
    assert!(request.contains(r#"{"token_id":"token-2"}"#));
    server.join().unwrap();
}

#[tokio::test]
async fn client_reads_positions_through_one_public_interface() {
    let (data_base_url, received, server) = serve_json(r#"[{"asset":"token-1"}]"#);
    let client = Client::new(ClientConfig {
        data_base_url,
        ..ClientConfig::default()
    })
    .unwrap();

    let positions = client.current_positions("0xuser", 5).await.unwrap();

    assert_eq!(positions[0].token_id, "token-1");
    let request = received.recv().unwrap();
    assert!(request.starts_with("GET /positions?"));
    assert!(request.contains("user=0xuser"));
    assert!(request.contains("limit=5"));
    server.join().unwrap();
}

#[tokio::test]
async fn client_lists_markets_through_one_public_interface() {
    let (gamma_base_url, received, server) = serve_json("[]");
    let client = Client::new(ClientConfig {
        gamma_base_url,
        ..ClientConfig::default()
    })
    .unwrap();

    let markets = client
        .markets(&MarketParams {
            limit: Some(5),
            ..MarketParams::default()
        })
        .await
        .unwrap();

    assert!(markets.is_empty());
    let request = received.recv().unwrap();
    assert!(request.starts_with("GET /markets?"));
    assert!(request.contains("limit=5"));
    server.join().unwrap();
}

#[tokio::test]
async fn client_pages_gamma_markets_with_keyset_cursor() {
    let (gamma_base_url, received, server) =
        serve_json(r#"{"markets":[{"id":"market-1"}],"next_cursor":"opaque next"}"#);
    let client = Client::new(ClientConfig {
        gamma_base_url,
        ..ClientConfig::default()
    })
    .unwrap();

    let page = client
        .market_page(&MarketKeysetParams {
            limit: Some(100),
            after_cursor: "opaque previous".into(),
            closed: Some(true),
            ..MarketKeysetParams::default()
        })
        .await
        .unwrap();

    assert_eq!(page.markets[0].id, "market-1");
    assert_eq!(page.next_cursor, "opaque next");
    let request = received.recv().unwrap();
    assert!(request.starts_with("GET /markets/keyset?"));
    assert!(request.contains("limit=100"));
    assert!(request.contains("after_cursor=opaque%20previous"));
    assert!(request.contains("closed=true"));
    server.join().unwrap();
}

#[tokio::test]
async fn client_reads_clob_prices_through_one_public_interface() {
    let (clob_base_url, received, server) = serve_json(r#"{"price":"0.42"}"#);
    let client = Client::new(ClientConfig {
        clob_base_url,
        ..ClientConfig::default()
    })
    .unwrap();

    let price = client.price("token-1", "buy").await.unwrap();

    assert_eq!(price, "0.42");
    let request = received.recv().unwrap();
    assert!(request.starts_with("GET /price?"));
    assert!(request.contains("token_id=token-1"));
    assert!(request.contains("side=buy"));
    server.join().unwrap();
}

#[tokio::test]
async fn client_reads_crypto_reference_price_through_one_public_interface() {
    let (crypto_price_base_url, received, server) = serve_json(
        r#"{"openPrice":64000.5,"closePrice":64010.25,"timestamp":1778745300000,"completed":false,"incomplete":false,"cached":true}"#,
    );
    let client = Client::new(ClientConfig {
        crypto_price_base_url,
        ..ClientConfig::default()
    })
    .unwrap();

    let price = client
        .crypto_price(
            "btc",
            Utc.with_ymd_and_hms(2026, 5, 14, 7, 55, 0).unwrap(),
            "fiveminute",
            Utc.with_ymd_and_hms(2026, 5, 14, 8, 0, 0).unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(price.open_price, Some(64000.5));
    assert_eq!(price.close_price, Some(64010.25));
    assert!(price.cached);
    let request = received.recv().unwrap();
    assert!(request.starts_with("GET /api/crypto/crypto-price?"));
    assert!(request.contains("symbol=BTC"));
    assert!(request.contains("eventStartTime=2026-05-14T07%3A55%3A00Z"));
    assert!(request.contains("variant=fiveminute"));
    assert!(request.contains("endDate=2026-05-14T08%3A00%3A00Z"));
    server.join().unwrap();
}

#[tokio::test]
async fn client_accepts_missing_future_crypto_reference_price() {
    let (crypto_price_base_url, received, server) = serve_json(
        r#"{"openPrice":null,"closePrice":null,"timestamp":0,"completed":false,"incomplete":true,"cached":false}"#,
    );
    let client = Client::new(ClientConfig {
        crypto_price_base_url,
        ..ClientConfig::default()
    })
    .unwrap();

    let price = client
        .crypto_price(
            "BTC",
            Utc.with_ymd_and_hms(2026, 5, 14, 8, 0, 0).unwrap(),
            "fiveminute",
            Utc.with_ymd_and_hms(2026, 5, 14, 8, 5, 0).unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(price.open_price, None);
    assert!(price.incomplete);
    received.recv().unwrap();
    server.join().unwrap();
}

#[tokio::test]
async fn client_reads_trades_through_one_public_interface() {
    let (data_base_url, received, server) = serve_json("[]");
    let client = Client::new(ClientConfig {
        data_base_url,
        ..ClientConfig::default()
    })
    .unwrap();

    let trades = client.trades("0xuser", 7).await.unwrap();

    assert!(trades.is_empty());
    let request = received.recv().unwrap();
    assert!(request.starts_with("GET /trades?"));
    assert!(request.contains("user=0xuser"));
    assert!(request.contains("limit=7"));
    server.join().unwrap();
}

#[tokio::test]
async fn client_reads_leaderboard_through_one_public_interface() {
    let (data_base_url, received, server) = serve_json("[]");
    let client = Client::new(ClientConfig {
        data_base_url,
        ..ClientConfig::default()
    })
    .unwrap();

    let rows = client.trader_leaderboard(9).await.unwrap();

    assert!(rows.is_empty());
    let request = received.recv().unwrap();
    assert!(request.starts_with("GET /v1/leaderboard?"));
    assert!(request.contains("limit=9"));
    server.join().unwrap();
}

#[tokio::test]
async fn client_reads_filtered_leaderboard_pages() {
    let (data_base_url, received, server) =
        serve_json(r#"[{"rank":"1","proxyWallet":"0xabc","userName":"alice","vol":100,"pnl":25}]"#);
    let client = Client::new(ClientConfig {
        data_base_url,
        ..ClientConfig::default()
    })
    .unwrap();

    let rows = client
        .trader_leaderboard_with(&LeaderboardParams {
            category: "POLITICS".into(),
            time_period: "MONTH".into(),
            order_by: "PNL".into(),
            limit: Some(50),
            offset: Some(100),
            ..LeaderboardParams::default()
        })
        .await
        .unwrap();

    assert_eq!(rows[0].proxy_wallet, "0xabc");
    assert_eq!(rows[0].user_name, "alice");
    assert_eq!(rows[0].user, "0xabc");
    let request = received.recv().unwrap();
    assert!(request.starts_with("GET /v1/leaderboard?"));
    assert!(request.contains("category=POLITICS"));
    assert!(request.contains("timePeriod=MONTH"));
    assert!(request.contains("orderBy=PNL"));
    assert!(request.contains("limit=50"));
    assert!(request.contains("offset=100"));
    server.join().unwrap();
}

#[tokio::test]
async fn client_reads_filtered_closed_position_pages() {
    let (data_base_url, received, server) = serve_json(
        r#"[{"proxyWallet":"0xabc","asset":"token-1","conditionId":"0xmarket","realizedPnl":25,"timestamp":123}]"#,
    );
    let client = Client::new(ClientConfig {
        data_base_url,
        ..ClientConfig::default()
    })
    .unwrap();

    let rows = client
        .closed_positions_with(&ClosedPositionParams {
            user: "0xabc".into(),
            markets: vec!["0xmarket".into(), "0xmarket2".into()],
            limit: Some(50),
            offset: Some(150),
            sort_by: "REALIZEDPNL".into(),
            sort_direction: "DESC".into(),
            ..ClosedPositionParams::default()
        })
        .await
        .unwrap();

    assert_eq!(rows[0].position.proxy_wallet, "0xabc");
    assert_eq!(rows[0].position.realized_pnl, 25.0);
    let request = received.recv().unwrap();
    assert!(request.starts_with("GET /closed-positions?"));
    assert!(request.contains("user=0xabc"));
    assert!(request.contains("market=0xmarket%2C0xmarket2"));
    assert!(request.contains("limit=50"));
    assert!(request.contains("offset=150"));
    assert!(request.contains("sortBy=REALIZEDPNL"));
    assert!(request.contains("sortDirection=DESC"));
    server.join().unwrap();
}

#[tokio::test]
async fn client_reads_filtered_trade_pages() {
    let (data_base_url, received, server) = serve_json(
        r#"[{"proxyWallet":"0xabc","asset":"token-1","conditionId":"0xmarket","transactionHash":"0xtx","timestamp":123}]"#,
    );
    let client = Client::new(ClientConfig {
        data_base_url,
        ..ClientConfig::default()
    })
    .unwrap();

    let rows = client
        .trades_with(&TradeParams {
            user: "0xabc".into(),
            side: "BUY".into(),
            start: Some(1),
            end: Some(123),
            limit: Some(100),
            offset: Some(200),
            ..TradeParams::default()
        })
        .await
        .unwrap();

    assert_eq!(rows[0].proxy_wallet, "0xabc");
    assert_eq!(rows[0].asset_id, "token-1");
    assert_eq!(rows[0].market, "0xmarket");
    assert_eq!(rows[0].transaction_hash, "0xtx");
    assert_eq!(rows[0].created_at, "123");
    let request = received.recv().unwrap();
    assert!(request.starts_with("GET /trades?"));
    assert!(request.contains("user=0xabc"));
    assert!(request.contains("side=BUY"));
    assert!(request.contains("start=1"));
    assert!(request.contains("end=123"));
    assert!(request.contains("limit=100"));
    assert!(request.contains("offset=200"));
    server.join().unwrap();
}

#[tokio::test]
async fn client_reads_filtered_activity_pages() {
    let (data_base_url, received, server) = serve_json(
        r#"[{"proxyWallet":"0xabc","type":"TRADE","conditionId":"0xmarket","usdcSize":50,"transactionHash":"0xtx","timestamp":123}]"#,
    );
    let client = Client::new(ClientConfig {
        data_base_url,
        ..ClientConfig::default()
    })
    .unwrap();

    let rows = client
        .activity_with(&ActivityParams {
            user: "0xabc".into(),
            activity_types: vec!["TRADE".into(), "REDEEM".into()],
            sort_by: "TIMESTAMP".into(),
            sort_direction: "DESC".into(),
            limit: Some(100),
            offset: Some(300),
            ..ActivityParams::default()
        })
        .await
        .unwrap();

    assert_eq!(rows[0].proxy_wallet, "0xabc");
    assert_eq!(rows[0].condition_id, "0xmarket");
    assert_eq!(rows[0].transaction_hash, "0xtx");
    assert_eq!(rows[0].usdc_size, "50");
    let request = received.recv().unwrap();
    assert!(request.starts_with("GET /activity?"));
    assert!(request.contains("user=0xabc"));
    assert!(request.contains("type=TRADE%2CREDEEM"));
    assert!(request.contains("sortBy=TIMESTAMP"));
    assert!(request.contains("sortDirection=DESC"));
    assert!(request.contains("limit=100"));
    assert!(request.contains("offset=300"));
    server.join().unwrap();
}

#[tokio::test]
async fn client_reports_combined_health_through_one_public_interface() {
    let (gamma_base_url, gamma_request, gamma_server) = serve_json("{}");
    let (clob_base_url, clob_request, clob_server) = serve_json("{}");
    let client = Client::new(ClientConfig {
        gamma_base_url,
        clob_base_url,
        ..ClientConfig::default()
    })
    .unwrap();

    let health = client.health().await;

    assert_eq!(health.gamma, "ok");
    assert_eq!(health.clob, "ok");
    assert!(gamma_request.recv().unwrap().starts_with("GET / "));
    assert!(clob_request.recv().unwrap().starts_with("GET / "));
    gamma_server.join().unwrap();
    clob_server.join().unwrap();
}

#[tokio::test]
async fn client_simulates_fills_through_one_public_interface() {
    let (clob_base_url, received, server) =
        serve_json(r#"{"asset_id":"token-1","asks":[{"price":"0.5","size":"10"}]}"#);
    let client = Client::new(ClientConfig {
        clob_base_url,
        ..ClientConfig::default()
    })
    .unwrap();

    let fill = client
        .simulate(Request {
            token_id: "token-1".into(),
            side: "buy".into(),
            amount: "1".into(),
            limit_price: String::new(),
        })
        .await
        .unwrap();

    assert!(fill.complete);
    assert_eq!(fill.filled_size, "2");
    assert!(received
        .recv()
        .unwrap()
        .starts_with("GET /book?token_id=token-1 "));
    server.join().unwrap();
}

#[test]
fn parses_existing_typed_market_events() {
    assert!(matches!(
        parse_market_event(r#"{"event_type":"book"}"#),
        Ok(MarketEvent::Book(_))
    ));
    assert!(matches!(
        parse_market_event(r#"{"event_type":"price_change"}"#),
        Ok(MarketEvent::PriceChange(_))
    ));
    assert!(matches!(
        parse_market_event(r#"{"event_type":"last_trade_price"}"#),
        Ok(MarketEvent::LastTrade(_))
    ));
    assert!(matches!(
        parse_market_event(r#"{"event_type":"tick_size_change"}"#),
        Ok(MarketEvent::TickSizeChange(_))
    ));
    assert!(matches!(
        parse_market_event(r#"{"event_type":"best_bid_ask"}"#),
        Ok(MarketEvent::BestBidAsk(_))
    ));
}

#[test]
fn parses_market_lifecycle_events() {
    let event = parse_market_event(
        r#"{"event_type":"new_market","id":"1031769","assets_ids":["yes","no"],"active":true}"#,
    )
    .unwrap();
    assert!(
        matches!(event, MarketEvent::NewMarket(market) if market.asset_ids == ["yes", "no"] && market.active)
    );

    let event = parse_market_event(
        r#"{"event_type":"market_resolved","assets_ids":["yes","no"],"winning_asset_id":"yes","winning_outcome":"Yes"}"#,
    )
    .unwrap();
    assert!(
        matches!(event, MarketEvent::MarketResolved(market) if market.winning_asset_id == "yes" && market.winning_outcome == "Yes")
    );
}

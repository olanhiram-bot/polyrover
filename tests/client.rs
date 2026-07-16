#![cfg(feature = "public")]

use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::mpsc,
    thread,
};

use polyrover::{
    gamma::{MarketParams, SearchParams},
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

#[test]
fn client_searches_markets_through_one_public_interface() {
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
        .unwrap();

    assert_eq!(response.events[0].id, "event-1");
    let request = received.recv().unwrap();
    assert!(request.starts_with("GET /public-search?"));
    assert!(request.contains("q=bitcoin"));
    server.join().unwrap();
}

#[test]
fn client_reads_clob_books_through_one_public_interface() {
    let (clob_base_url, received, server) =
        serve_json(r#"{"asset_id":"token-1","bids":[],"asks":[]}"#);
    let client = Client::new(ClientConfig {
        clob_base_url,
        ..ClientConfig::default()
    })
    .unwrap();

    let book = client.order_book("token-1").unwrap();

    assert_eq!(book.asset_id, "token-1");
    assert!(received
        .recv()
        .unwrap()
        .starts_with("GET /book?token_id=token-1 "));
    server.join().unwrap();
}

#[test]
fn client_reads_clob_books_in_one_batch() {
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
        .unwrap();

    assert_eq!(books.len(), 2);
    let request = received.recv().unwrap();
    assert!(request.starts_with("POST /books "));
    assert!(request.contains(r#"{"token_id":"token-1"}"#));
    assert!(request.contains(r#"{"token_id":"token-2"}"#));
    server.join().unwrap();
}

#[test]
fn client_reads_positions_through_one_public_interface() {
    let (data_base_url, received, server) = serve_json(r#"[{"asset":"token-1"}]"#);
    let client = Client::new(ClientConfig {
        data_base_url,
        ..ClientConfig::default()
    })
    .unwrap();

    let positions = client.current_positions("0xuser", 5).unwrap();

    assert_eq!(positions[0].token_id, "token-1");
    let request = received.recv().unwrap();
    assert!(request.starts_with("GET /positions?"));
    assert!(request.contains("user=0xuser"));
    assert!(request.contains("limit=5"));
    server.join().unwrap();
}

#[test]
fn client_lists_markets_through_one_public_interface() {
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
        .unwrap();

    assert!(markets.is_empty());
    let request = received.recv().unwrap();
    assert!(request.starts_with("GET /markets?"));
    assert!(request.contains("limit=5"));
    server.join().unwrap();
}

#[test]
fn client_reads_clob_prices_through_one_public_interface() {
    let (clob_base_url, received, server) = serve_json(r#"{"price":"0.42"}"#);
    let client = Client::new(ClientConfig {
        clob_base_url,
        ..ClientConfig::default()
    })
    .unwrap();

    let price = client.price("token-1", "buy").unwrap();

    assert_eq!(price, "0.42");
    let request = received.recv().unwrap();
    assert!(request.starts_with("GET /price?"));
    assert!(request.contains("token_id=token-1"));
    assert!(request.contains("side=buy"));
    server.join().unwrap();
}

#[test]
fn client_reads_trades_through_one_public_interface() {
    let (data_base_url, received, server) = serve_json("[]");
    let client = Client::new(ClientConfig {
        data_base_url,
        ..ClientConfig::default()
    })
    .unwrap();

    let trades = client.trades("0xuser", 7).unwrap();

    assert!(trades.is_empty());
    let request = received.recv().unwrap();
    assert!(request.starts_with("GET /trades?"));
    assert!(request.contains("user=0xuser"));
    assert!(request.contains("limit=7"));
    server.join().unwrap();
}

#[test]
fn client_reads_leaderboard_through_one_public_interface() {
    let (data_base_url, received, server) = serve_json("[]");
    let client = Client::new(ClientConfig {
        data_base_url,
        ..ClientConfig::default()
    })
    .unwrap();

    let rows = client.trader_leaderboard(9).unwrap();

    assert!(rows.is_empty());
    let request = received.recv().unwrap();
    assert!(request.starts_with("GET /v1/leaderboard?"));
    assert!(request.contains("limit=9"));
    server.join().unwrap();
}

#[test]
fn client_reports_combined_health_through_one_public_interface() {
    let (gamma_base_url, gamma_request, gamma_server) = serve_json("{}");
    let (clob_base_url, clob_request, clob_server) = serve_json("{}");
    let client = Client::new(ClientConfig {
        gamma_base_url,
        clob_base_url,
        ..ClientConfig::default()
    })
    .unwrap();

    let health = client.health();

    assert_eq!(health.gamma, "ok");
    assert_eq!(health.clob, "ok");
    assert!(gamma_request.recv().unwrap().starts_with("GET / "));
    assert!(clob_request.recv().unwrap().starts_with("GET / "));
    gamma_server.join().unwrap();
    clob_server.join().unwrap();
}

#[test]
fn client_simulates_fills_through_one_public_interface() {
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

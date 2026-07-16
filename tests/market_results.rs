#![cfg(feature = "public")]

use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::mpsc,
    thread,
};

use chrono::{TimeZone, Utc};
use polyrover::market_results::{MarketRef, Resolver};

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
fn resolves_exact_causal_market_result_through_polyrover_clients() {
    let (gamma_url, gamma_request, gamma_server) = serve_json(
        r#"{"conditionId":"condition-1","slug":"btc-updown","closed":true,"closedTime":"2026-07-10T12:05:03Z","clobTokenIds":"[\"token-up\",\"token-down\"]","outcomePrices":["1","0"]}"#,
    );
    let (clob_url, clob_request, clob_server) = serve_json(
        r#"{"condition_id":"condition-1","closed":true,"tokens":[{"token_id":"token-up","winner":true},{"token_id":"token-down","winner":false}]}"#,
    );
    let observed_at = Utc.with_ymd_and_hms(2026, 7, 10, 12, 5, 5).unwrap();
    let resolver = Resolver::new(clob_url, gamma_url).unwrap();

    let result = resolver
        .resolve_at(
            &MarketRef {
                condition_id: "condition-1".into(),
                slug: "btc-updown".into(),
                up_token_id: "token-up".into(),
                down_token_id: "token-down".into(),
            },
            observed_at,
        )
        .unwrap()
        .unwrap();

    assert_eq!(result.winning_token_id, "token-up");
    assert_eq!(
        result.resolved_at,
        Utc.with_ymd_and_hms(2026, 7, 10, 12, 5, 3).unwrap()
    );
    assert_eq!(result.observed_at, observed_at);
    assert!(result.source.contains("clob:/markets/condition-1"));
    assert!(gamma_request
        .recv()
        .unwrap()
        .starts_with("GET /markets/slug/btc-updown "));
    assert!(clob_request
        .recv()
        .unwrap()
        .starts_with("GET /markets/condition-1 "));
    gamma_server.join().unwrap();
    clob_server.join().unwrap();
}

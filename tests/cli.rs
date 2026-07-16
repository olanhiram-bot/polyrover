#![cfg(feature = "public")]

use std::{process::Command, thread};

use tungstenite::Message;

#[test]
fn stream_watch_prints_events_and_stats_from_public_websocket() {
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

    let output = Command::new(env!("CARGO_BIN_EXE_polyrover"))
        .args([
            "stream",
            "watch",
            "--token-id",
            "token-1",
            "--url",
            &format!("ws://{address}"),
            "--limit",
            "1",
            "--json",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let body: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stream watch must print one JSON envelope");
    assert_eq!(body["ok"], true);
    assert_eq!(body["data"]["events"][0]["event_type"], "new_market");
    assert_eq!(body["data"]["stats"]["messages_received"], 1);
    server.join().unwrap();
}

#[test]
fn help_remains_available_without_credentials() {
    let output = Command::new(env!("CARGO_BIN_EXE_polyrover"))
        .arg("help")
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.starts_with("polyrover read-only Polymarket CLI"));
    assert!(stdout.contains("gamma search"));
    assert!(stdout.contains("clob simulate"));
    assert!(stdout.contains("analytics positions"));
}

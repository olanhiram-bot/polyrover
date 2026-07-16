#![cfg(feature = "public")]

use std::{process::Command, thread};

use tokio_tungstenite::tungstenite::{self, Message};

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
    assert!(stdout.starts_with("polyrover async Polymarket CLI"));
    assert!(stdout.contains("gamma search"));
    assert!(stdout.contains("clob simulate"));
    assert!(stdout.contains("analytics positions"));
}

#[test]
fn top_level_help_groups_commands_and_points_to_detailed_help() {
    let output = Command::new(env!("CARGO_BIN_EXE_polyrover"))
        .arg("--help")
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Public data:"));
    assert!(stdout.contains("Streaming:"));
    assert!(stdout.contains("Local simulation:"));
    assert!(stdout.contains("polyrover help <command>"));
}

#[test]
fn help_command_shows_usage_options_defaults_and_example() {
    let output = Command::new(env!("CARGO_BIN_EXE_polyrover"))
        .args(["help", "analytics", "positions"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout
        .contains("Usage: polyrover analytics positions --user <wallet> [--limit <n>] [--json]"));
    assert!(stdout.contains("default: 20"));
    assert!(stdout.contains("Example:"));
}

#[test]
fn command_path_accepts_help_flag() {
    let output = Command::new(env!("CARGO_BIN_EXE_polyrover"))
        .args(["gamma", "search", "--help"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Usage: polyrover gamma search --query <text>"));
    assert!(stdout.contains("--limit <n>"));
    assert!(stdout.contains("polyrover gamma search --query \"bitcoin\""));
}

#[test]
fn every_command_has_detailed_help() {
    for command in [
        &["ping"][..],
        &["gamma", "search"],
        &["gamma", "markets"],
        &["clob", "book"],
        &["clob", "price"],
        &["clob", "simulate"],
        &["analytics", "positions"],
        &["analytics", "trades"],
        &["analytics", "leaderboard"],
        &["stream", "watch"],
        &["sim", "reset"],
        &["sim", "buy"],
        &["sim", "sell"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_polyrover"))
            .arg("help")
            .args(command)
            .output()
            .unwrap();
        let stdout = String::from_utf8(output.stdout).unwrap();
        let path = command.join(" ");
        assert!(output.status.success(), "help failed for {path}");
        assert!(
            stdout.contains(&format!("Usage: polyrover {path}")),
            "missing usage for {path}"
        );
        assert!(stdout.contains("Example:"), "missing example for {path}");
    }
}

#[test]
fn unknown_commands_fail_with_a_help_hint() {
    let output = Command::new(env!("CARGO_BIN_EXE_polyrover"))
        .args(["gama", "search"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("unknown command `gama search`"));
    assert!(stderr.contains("polyrover help"));
}

#[test]
fn unknown_help_targets_fail_with_the_same_hint() {
    let output = Command::new(env!("CARGO_BIN_EXE_polyrover"))
        .args(["help", "gama", "search"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("unknown command `gama search`"));
    assert!(stderr.contains("polyrover help"));
}

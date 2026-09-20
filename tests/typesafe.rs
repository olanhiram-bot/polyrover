#![cfg(feature = "typesafe")]

use std::{
    collections::BTreeMap,
    io::{Read, Write},
    net::TcpListener,
    process::Command,
    sync::mpsc,
    thread,
    time::Duration,
};

use polyrover::{
    types::Market,
    typesafe::{
        market_review_request, Answer, Client, Config, Question, Request, Response, ReviewRoute,
        Usage,
    },
    Error,
};
use serde_json::{json, Value};

fn market() -> Market {
    serde_json::from_str(include_str!("../examples/typesafe-market.json")).unwrap()
}

// Synthetic contract response following https://docs.typesafe.ai/api; no live data.
fn response(request: &Request) -> Response {
    Response {
        model: "jev-test".into(),
        usage: Usage {
            input_tokens: 320,
            output_tokens: 60,
        },
        answers: request
            .questions
            .iter()
            .map(|(id, question)| {
                let answer = match question {
                    Question::Choice { criteria, .. } => Answer::Choice {
                        choice: "crypto".into(),
                        confidence: 0.95,
                        probabilities: criteria
                            .keys()
                            .map(|k| (k.clone(), if k == "crypto" { 1.0 } else { 0.0 }))
                            .collect(),
                    },
                    Question::Score { criteria, .. } => Answer::Score {
                        score: 3.0,
                        confidence: 0.95,
                        legend: criteria
                            .iter()
                            .enumerate()
                            .map(|(i, v)| (i.to_string(), v.clone()))
                            .collect(),
                        probabilities: (0..criteria.len())
                            .map(|i| (i.to_string(), if i == 3 { 1.0 } else { 0.0 }))
                            .collect(),
                    },
                    Question::Noul { .. } => Answer::Noul { noul: 0.95 },
                };
                (id.clone(), answer)
            })
            .collect(),
    }
}

fn server(
    status: u16,
    headers: &'static str,
    body: String,
) -> (Config, mpsc::Receiver<String>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let (tx, rx) = mpsc::channel();
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut raw = Vec::new();
        loop {
            let mut chunk = [0; 4096];
            let n = stream.read(&mut chunk).unwrap();
            assert!(n > 0, "connection closed before complete request");
            raw.extend_from_slice(&chunk[..n]);
            if let Some(end) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&raw[..end]).to_lowercase();
                let length: usize = headers
                    .lines()
                    .find_map(|l| l.strip_prefix("content-length: "))
                    .unwrap()
                    .parse()
                    .unwrap();
                if raw.len() >= end + 4 + length {
                    break;
                }
            }
        }
        tx.send(String::from_utf8(raw).unwrap()).unwrap();
        write!(stream, "HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
    });
    (
        Config {
            base_url: format!("http://{address}"),
            model: "jev-test".into(),
            ..Default::default()
        },
        rx,
        handle,
    )
}

#[tokio::test]
async fn review_batches_primitives_authenticates_and_preserves_audit_fields() {
    let mut market = market();
    market
        .extra
        .insert("private_note".into(), json!("never send this"));
    let request = market_review_request(&market, "jev-test").unwrap();
    let body = serde_json::to_string(&response(&request)).unwrap();
    let (config, rx, handle) = server(200, "", body);
    let review = Client::new("test-secret", config)
        .unwrap()
        .review_market(&market, 0.8)
        .await
        .unwrap();
    assert_eq!(review.route, ReviewRoute::ResearchReady);
    assert!(review.reasons.is_empty());
    assert_eq!(review.market_id, "example-1");
    assert_eq!(review.rubric_version, "market_rules_v1");
    assert_eq!(review.evaluation.model, "jev-test");
    assert_eq!(review.evaluation.usage.input_tokens, 320);
    let raw = rx.recv().unwrap();
    assert!(raw.starts_with("POST /v1/systemone HTTP/1.1"));
    assert!(raw
        .to_lowercase()
        .contains("authorization: bearer test-secret\r\n"));
    let sent: Value = serde_json::from_str(raw.split_once("\r\n\r\n").unwrap().1).unwrap();
    assert_eq!(sent["questions"].as_object().unwrap().len(), 3);
    assert_eq!(
        sent["state"]["market"]["description"],
        market.extra["description"]
    );
    assert!(!raw.contains("never send this"));
    handle.join().unwrap();
}

#[tokio::test]
async fn missing_rules_and_uncertainty_route_to_manual_review() {
    let mut market = market();
    market.extra.remove("description");
    let request = market_review_request(&market, "jev-test").unwrap();
    let mut response = response(&request);
    if let Answer::Score {
        confidence,
        score,
        probabilities,
        ..
    } = response.answers.get_mut("resolution_clarity").unwrap()
    {
        *confidence = 0.4;
        *score = 1.5;
        *probabilities = BTreeMap::from([
            ("0".into(), 0.25),
            ("1".into(), 0.25),
            ("2".into(), 0.25),
            ("3".into(), 0.25),
        ]);
    }
    response.answers.insert(
        "resolution_source_identified".into(),
        Answer::Noul { noul: 0.5 },
    );
    let (config, rx, handle) = server(200, "", serde_json::to_string(&response).unwrap());
    let review = Client::new("test-secret", config)
        .unwrap()
        .review_market(&market, 0.8)
        .await
        .unwrap();
    assert_eq!(review.route, ReviewRoute::ManualReview);
    for reason in [
        "missing_resolution_rules",
        "low_resolution_confidence",
        "ambiguous_resolution_rules",
        "resolution_source_not_established",
    ] {
        assert!(review.reasons.iter().any(|v| v == reason));
    }
    rx.recv().unwrap();
    handle.join().unwrap();
}

#[tokio::test]
async fn errors_are_classified_and_do_not_echo_secrets_or_follow_redirects() {
    for status in [401, 422, 429, 529, 302] {
        let (config, rx, handle) = server(
            status,
            "Retry-After: 7\r\nLocation: http://127.0.0.1:1/stolen\r\n",
            "test-secret".into(),
        );
        let error = Client::new("test-secret", config)
            .unwrap()
            .evaluate(&market_review_request(&market(), "jev-test").unwrap())
            .await
            .unwrap_err();
        assert!(!error.to_string().contains("test-secret"));
        if status == 429 {
            assert!(matches!(
                error,
                Error::RateLimited {
                    retry_after_secs: Some(7)
                }
            ));
        } else {
            assert!(matches!(error, Error::Api { status: actual, .. } if actual == status));
        }
        rx.recv().unwrap();
        handle.join().unwrap();
    }
}

#[test]
fn malformed_answers_fail_closed() {
    let request = market_review_request(&market(), "jev-test").unwrap();
    let valid = serde_json::to_value(response(&request)).unwrap();
    let cases = [
        ("/answers/category/choice", json!("unknown")),
        ("/answers/category/confidence", json!(1.1)),
        ("/answers/category/probabilities/crypto", json!(0.1)),
        ("/answers/category", json!({"type":"noul","noul":0.5})),
        ("/answers/resolution_clarity/score", json!(4)),
        (
            "/answers/resolution_clarity/legend/0",
            json!("changed rubric"),
        ),
        ("/answers/resolution_source_identified/noul", json!(-0.1)),
    ];
    for (path, replacement) in cases {
        let mut value = valid.clone();
        *value.pointer_mut(path).unwrap() = replacement;
        assert!(
            serde_json::from_value::<Response>(value)
                .unwrap()
                .validate(&request)
                .is_err(),
            "{path}"
        );
    }
    let mut missing = response(&request);
    missing.answers.remove("category");
    assert!(missing.validate(&request).is_err());
}

#[test]
fn invalid_configuration_and_requests_are_rejected() {
    for key in ["", "  ", "secret\nheader"] {
        assert!(Client::new(key, Config::default()).is_err());
    }
    for base_url in [
        "http://example.com",
        "https://user:secret@example.com",
        "https://example.com?secret=x",
    ] {
        assert!(Client::new(
            "test",
            Config {
                base_url: base_url.into(),
                ..Default::default()
            }
        )
        .is_err());
    }
    assert!(market_review_request(&Market::default(), "jev-test").is_err());
    let mut request = market_review_request(&market(), "jev-test").unwrap();
    request.questions.insert(
        "invalid".into(),
        Question::Score {
            instructions: "score it".into(),
            criteria: vec!["only one".into()],
        },
    );
    assert!(request.validate().is_err());
}

#[test]
fn cli_dry_run_works_without_credentials_and_uses_the_versioned_envelope() {
    let output = Command::new(env!("CARGO_BIN_EXE_polyrover"))
        .env_remove("TYPESAFE_API_KEY")
        .args([
            "ai",
            "review-market",
            "--market-file",
            concat!(env!("CARGO_MANIFEST_DIR"), "/examples/typesafe-market.json"),
            "--dry-run",
            "--json",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["ok"], true);
    assert_eq!(value["version"], "1");
    assert_eq!(value["meta"]["command"], "ai review-market");
    assert_eq!(value["data"]["model"], "jev-latest");
    assert_eq!(value["data"]["questions"].as_object().unwrap().len(), 3);
}

#[test]
fn cli_reports_bad_arguments_and_missing_credentials_without_network() {
    for args in [
        vec![],
        vec!["--slug", "example", "--market-file", "example.json"],
        vec!["--slug"],
        vec!["--slug", "example", "--min-confidence", "NaN"],
        vec!["--slug", "example", "--min-confidence", "2"],
        vec!["--slug", "example", "--typo"],
        vec!["--slug", "example", "--slug", "duplicate"],
        vec!["--slug", "example"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_polyrover"))
            .current_dir(std::env::temp_dir())
            .env_remove("TYPESAFE_API_KEY")
            .args(["ai", "review-market"])
            .args(&args)
            .output()
            .unwrap();
        assert!(!output.status.success(), "{args:?}");
        let value: Value = serde_json::from_slice(&output.stderr).unwrap();
        assert_eq!(value["ok"], false);
    }
}

#[test]
fn news_cli_help_and_validation_are_available_without_network() {
    let binary = env!("CARGO_BIN_EXE_polyrover");
    let help = Command::new(binary)
        .args(["help", "ai", "research-market"])
        .output()
        .unwrap();
    assert!(help.status.success());
    let help = String::from_utf8(help.stdout).unwrap();
    assert!(help.contains("--news-url"));
    assert!(help.contains("--collect-only"));
    for args in [
        vec!["--question", "test", "--min-confidence", "NaN"],
        vec!["--question", "test", "--max-age-days", "0"],
        vec![
            "--question",
            "test",
            "--news-url",
            "https://news.google.com/search?q=x",
        ],
        vec![
            "--news-url",
            "https://example.com/search?q=x",
            "--collect-only",
        ],
        vec![
            "--news-url",
            "https://news.google.com/search?q=x&q=y",
            "--collect-only",
        ],
    ] {
        let output = Command::new(binary)
            .env_remove("TYPESAFE_API_KEY")
            .args(["ai", "research-market"])
            .args(args)
            .output()
            .unwrap();
        assert!(!output.status.success());
        let result: Value = serde_json::from_slice(&output.stderr).unwrap();
        assert_eq!(result["ok"], false);
    }
}

#![cfg(feature = "server")]
use chrono::{Duration, Utc};
use polyrover::decision_server::{self, project_report, DecisionService, Generator};
use serde_json::{json, Value};
use std::{
    collections::BTreeSet,
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

fn report(slug: &str) -> Value {
    json!({"rubric_version":"decision_v1","slug":slug,"market_id":"m1","question":"Will X win?","generated_at":Utc::now(),
        "decision":{"action":"buy_yes","summary_es":"Experimental recommendation","experimental":true,"orders_submitted":0,"reasons":[]},
        "forecast":{"predicted_outcome":"yes","yes_interval":[0.7,0.8],"classifier_confidence":0.9,"evaluation":{"model":"test"}},
        "yes_quote":{"book_timestamp":Utc::now(),"average_ask":0.3,"total_cost_per_share":0.32,"shares":10},"no_quote":null,
        "research":{"coverage":{"discovered":2,"evaluated":2,"unavailable":0},"articles":[]},"limitations":["Uncalibrated"]})
}
fn directory() -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "polyrover-server-test-{}-{}",
        std::process::id(),
        Utc::now().timestamp_nanos_opt().unwrap()
    ));
    std::fs::create_dir(&path).unwrap();
    path
}
fn generator(calls: Arc<AtomicUsize>) -> Generator {
    Arc::new(move |slug| {
        calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move { Ok(report(&slug)) })
    })
}
async fn server(service: DecisionService) -> (String, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        axum::serve(
            listener,
            decision_server::router(service, vec!["http://localhost:8080".parse().unwrap()]),
        )
        .await
        .unwrap();
    });
    (address, task)
}

#[test]
fn expiry_never_repeats_a_stale_buy_and_never_exposes_extra_fields() {
    let mut source = report("test-market");
    source["secret"] = json!("must-not-appear");
    source["generated_at"] = json!(Utc::now() - Duration::minutes(10));
    source["yes_quote"]["book_timestamp"] = source["generated_at"].clone();
    let public = project_report(&source, Utc::now()).unwrap();
    assert_eq!(public["action"], "wait");
    assert_eq!(public["reported_action"], "buy_yes");
    assert_eq!(public["stale"], true);
    assert!(!public.to_string().contains("must-not-appear"));
    source["slug"] = json!("../.env");
    assert!(project_report(&source, Utc::now()).is_err());
}

#[test]
fn rejects_unknown_versions_missing_identity_and_orders() {
    for (key, value) in [
        ("rubric_version", json!("unknown")),
        ("market_id", Value::Null),
        ("generated_at", json!("bad")),
    ] {
        let mut r = report("test");
        r[key] = value;
        assert!(project_report(&r, Utc::now()).is_err());
    }
    let mut r = report("test");
    r["decision"]["orders_submitted"] = json!(1);
    assert!(project_report(&r, Utc::now()).is_err());
    r = report("test");
    r["yes_quote"] = Value::Null;
    assert!(project_report(&r, Utc::now()).is_err());
}

#[tokio::test]
async fn public_read_cors_generation_and_persistent_daily_spend_gate() {
    let dir = directory();
    let calls = Arc::new(AtomicUsize::new(0));
    let allowed = BTreeSet::from(["test-market".into()]);
    let service =
        DecisionService::open(dir.clone(), allowed.clone(), generator(calls.clone())).unwrap();
    let (base, task) = server(service).await;
    let http = reqwest::Client::new();
    let url = format!("{base}/api/v1/decisions/test-market");
    let response = http
        .get(&url)
        .header("Origin", "http://localhost:8080")
        .send()
        .await
        .unwrap();
    assert_eq!(response.headers()["cache-control"], "no-store");
    assert_eq!(
        response.headers()["access-control-allow-origin"],
        "http://localhost:8080"
    );
    let state: Value = response.json().await.unwrap();
    assert_eq!(state["status"], "missing");
    assert_eq!(state["can_generate"], true);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        http.post(&url)
            .json(&json!({}))
            .send()
            .await
            .unwrap()
            .status(),
        202
    );
    for _ in 0..50 {
        let state: Value = http.get(&url).send().await.unwrap().json().await.unwrap();
        if state["status"] == "ready" {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let state: Value = http.get(&url).send().await.unwrap().json().await.unwrap();
    assert_eq!(state["status"], "ready");
    assert_eq!(state["data"]["action"], "buy_yes");
    assert_eq!(
        http.post(&url)
            .json(&json!({}))
            .send()
            .await
            .unwrap()
            .status(),
        429
    );
    assert_eq!(
        http.post(format!("{base}/api/v1/decisions/other"))
            .json(&json!({}))
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    assert_eq!(
        http.post(&url)
            .json(&json!({"command":"ignored"}))
            .send()
            .await
            .unwrap()
            .status(),
        400
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    task.abort();
    let reopened = DecisionService::open(dir.clone(), allowed, generator(calls.clone())).unwrap();
    let (base, task) = server(reopened).await;
    assert_eq!(
        http.post(format!("{base}/api/v1/decisions/test-market"))
            .json(&json!({}))
            .send()
            .await
            .unwrap()
            .status(),
        429
    );
    task.abort();
    std::fs::remove_dir_all(dir).unwrap();
}

#[tokio::test]
async fn importing_cli_reports_provides_real_read_only_data() {
    let dir = directory();
    let calls = Arc::new(AtomicUsize::new(0));
    let path = dir.join("input.json");
    std::fs::write(
        &path,
        json!({"ok":true,"version":"1","data":report("imported")}).to_string(),
    )
    .unwrap();
    let service =
        DecisionService::open(dir.clone(), BTreeSet::new(), generator(calls.clone())).unwrap();
    service.import_report(&path).await.unwrap();
    let (base, task) = server(service).await;
    let state: Value = reqwest::get(format!("{base}/api/v1/decisions/imported"))
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(state["status"], "ready");
    assert_eq!(state["generation_enabled"], false);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    task.abort();
    std::fs::remove_dir_all(dir).unwrap();
}

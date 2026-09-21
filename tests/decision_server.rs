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
fn old_reports_expose_why_the_model_was_not_consulted_without_regeneration() {
    let mut r = report("test-market");
    r["forecast"] = json!({"predicted_outcome":"insufficient_evidence","error":"no_current_relevant_articles","evaluation":null});
    r["evidence_selection"] = json!({"included_ids":[],"excluded":{"a":"context_budget"}});
    let public = project_report(&r, Utc::now()).unwrap();
    assert_eq!(public["forecast_status"], "no_eligible_evidence");
    assert_eq!(public["evidence_omitted_for_size"], 1);
    assert_eq!(public["analysis_version"], "legacy_v1");
    r["market_snapshot"] = json!({"closed":true});
    assert_eq!(
        project_report(&r, Utc::now()).unwrap()["forecast_status"],
        "not_applicable"
    );
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

#[test]
fn research_cache_expires_exactly_at_24_hours_independently_of_quotes() {
    let source = report("boundary");
    let generated = chrono::DateTime::parse_from_rfc3339(source["generated_at"].as_str().unwrap())
        .unwrap()
        .with_timezone(&Utc);
    let before = project_report(
        &source,
        generated + Duration::hours(24) - Duration::milliseconds(1),
    )
    .unwrap();
    assert_eq!(before["research_stale"], false);
    assert_eq!(before["stale"], true);
    assert_eq!(before["action"], "wait");
    assert_eq!(
        project_report(&source, generated + Duration::hours(24)).unwrap()["research_stale"],
        true
    );
}

#[tokio::test]
#[ignore = "requires POLYROVER_TEST_DATABASE_URL; CI runs PostgreSQL tests explicitly"]
async fn public_read_cors_generation_and_persistent_daily_spend_gate() {
    let dir = directory();
    let calls = Arc::new(AtomicUsize::new(0));
    let allowed = BTreeSet::from(["test-market".into()]);
    let db = test_database().await;
    let service = DecisionService::connect(&db, allowed.clone(), generator(calls.clone()))
        .await
        .unwrap();
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
        200
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
    let reopened = DecisionService::connect(&db, allowed, generator(calls.clone()))
        .await
        .unwrap();
    let (base, task) = server(reopened).await;
    assert_eq!(
        http.post(format!("{base}/api/v1/decisions/test-market"))
            .json(&json!({}))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    task.abort();
    std::fs::remove_dir_all(dir).unwrap();
}

#[tokio::test]
#[ignore = "requires POLYROVER_TEST_DATABASE_URL; CI runs PostgreSQL tests explicitly"]
async fn importing_cli_reports_provides_real_read_only_data() {
    let dir = directory();
    let calls = Arc::new(AtomicUsize::new(0));
    let path = dir.join("input.json");
    std::fs::write(
        &path,
        json!({"ok":true,"version":"1","data":report("imported")}).to_string(),
    )
    .unwrap();
    let db = test_database().await;
    let service = DecisionService::connect(&db, BTreeSet::new(), generator(calls.clone()))
        .await
        .unwrap();
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

// Each test gets its own schema inside a dedicated TEST database.
async fn test_database() -> String {
    let base = std::env::var("POLYROVER_TEST_DATABASE_URL")
        .expect("set a dedicated POLYROVER_TEST_DATABASE_URL");
    let (client, connection) = tokio_postgres::connect(&base, tokio_postgres::NoTls)
        .await
        .unwrap();
    tokio::spawn(async move {
        connection.await.unwrap();
    });
    let schema = format!(
        "test_{}_{}",
        std::process::id(),
        Utc::now().timestamp_nanos_opt().unwrap()
    );
    client
        .batch_execute(&format!("CREATE SCHEMA {schema}"))
        .await
        .unwrap();
    if base.starts_with("postgres://") || base.starts_with("postgresql://") {
        let mut url = reqwest::Url::parse(&base).unwrap();
        url.query_pairs_mut()
            .append_pair("options", &format!("-csearch_path={schema}"));
        url.to_string()
    } else {
        format!("{base} options='-csearch_path={schema}'")
    }
}

async fn sql(db: &str, statement: &str) {
    let (client, connection) = tokio_postgres::connect(db, tokio_postgres::NoTls)
        .await
        .unwrap();
    tokio::spawn(async move {
        connection.await.unwrap();
    });
    client.batch_execute(statement).await.unwrap();
}

async fn state(http: &reqwest::Client, url: &str) -> Value {
    http.get(url).send().await.unwrap().json().await.unwrap()
}

async fn ready(http: &reqwest::Client, url: &str) -> Value {
    for _ in 0..100 {
        let value = state(http, url).await;
        if value["status"] == "ready" {
            return value;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("job did not finish");
}

#[tokio::test]
#[ignore = "requires POLYROVER_TEST_DATABASE_URL"]
async fn cache_survives_reconnect_and_expiry_generates_once_preserving_history() {
    let db = test_database().await;
    let calls = Arc::new(AtomicUsize::new(0));
    let allowed = BTreeSet::from(["daily".into()]);
    let service = DecisionService::connect(&db, allowed.clone(), generator(calls.clone()))
        .await
        .unwrap();
    let (base, task) = server(service).await;
    let url = format!("{base}/api/v1/decisions/daily");
    let http = reqwest::Client::new();
    assert_eq!(
        http.post(&url)
            .json(&json!({}))
            .send()
            .await
            .unwrap()
            .status(),
        202
    );
    let first = ready(&http, &url).await;
    assert_eq!(first["cache_hit"], true);
    assert_eq!(first["can_generate"], false);
    assert_eq!(first["research_ttl_seconds"], 86400);
    let generated =
        chrono::DateTime::parse_from_rfc3339(first["data"]["generated_at"].as_str().unwrap())
            .unwrap();
    let expires = chrono::DateTime::parse_from_rfc3339(
        first["data"]["research_valid_until"].as_str().unwrap(),
    )
    .unwrap();
    assert_eq!((expires - generated).num_seconds(), 86400);
    for _ in 0..5 {
        assert_eq!(state(&http, &url).await["data"], first["data"]);
        assert_eq!(
            http.post(&url)
                .json(&json!({}))
                .send()
                .await
                .unwrap()
                .status(),
            200
        );
    }
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    task.abort();
    let service = DecisionService::connect(&db, allowed, generator(calls.clone()))
        .await
        .unwrap();
    let (base, task) = server(service).await;
    let url = format!("{base}/api/v1/decisions/daily");
    assert_eq!(state(&http, &url).await["cache_hit"], true);
    // Simulate the passage of 24h in the DB without waiting or contacting providers.
    sql(
        &db,
        "UPDATE polyrover_predictions SET generated_at=generated_at-interval '25 hours',
        research_valid_until=research_valid_until-interval '25 hours',
        report=jsonb_set(report,'{generated_at}',to_jsonb(generated_at-interval '25 hours'));
        UPDATE polyrover_research_jobs SET retry_at=clock_timestamp()-interval '1 second'",
    )
    .await;
    let expired = state(&http, &url).await;
    assert_eq!(expired["cache_hit"], false);
    assert_eq!(expired["can_generate"], true);
    assert_eq!(expired["data"]["action"], "wait");
    assert_eq!(calls.load(Ordering::SeqCst), 1); // GET never starts research.
    assert_eq!(
        http.post(&url)
            .json(&json!({}))
            .send()
            .await
            .unwrap()
            .status(),
        202
    );
    ready(&http, &url).await;
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    let (client, connection) = tokio_postgres::connect(&db, tokio_postgres::NoTls)
        .await
        .unwrap();
    tokio::spawn(async move {
        connection.await.unwrap();
    });
    let count: i64 = client
        .query_one("SELECT count(*) FROM polyrover_predictions", &[])
        .await
        .unwrap()
        .get(0);
    assert_eq!(count, 2);
    task.abort();
}

#[tokio::test]
#[ignore = "requires POLYROVER_TEST_DATABASE_URL"]
async fn simultaneous_instances_reserve_only_one_paid_job_and_failures_keep_gate() {
    let db = test_database().await;
    let calls = Arc::new(AtomicUsize::new(0));
    let counter = calls.clone();
    let release = Arc::new(tokio::sync::Notify::new());
    let gate = release.clone();
    let fail: Generator = Arc::new(move |_| {
        counter.fetch_add(1, Ordering::SeqCst);
        let gate = gate.clone();
        Box::pin(async move {
            gate.notified().await;
            Err(polyrover::Error::Invalid(
                "synthetic provider failure".into(),
            ))
        })
    });
    let allowed = BTreeSet::from(["race".into(), "other".into()]);
    let a = DecisionService::connect(&db, allowed.clone(), fail.clone())
        .await
        .unwrap();
    let b = DecisionService::connect(&db, allowed, fail).await.unwrap();
    let (base_a, task_a) = server(a).await;
    let (base_b, task_b) = server(b).await;
    let http = reqwest::Client::new();
    let url_a = format!("{base_a}/api/v1/decisions/race");
    let url_b = format!("{base_b}/api/v1/decisions/race");
    let (a, b) = tokio::join!(
        http.post(&url_a).json(&json!({})).send(),
        http.post(&url_b).json(&json!({})).send()
    );
    let mut statuses = [a.unwrap().status().as_u16(), b.unwrap().status().as_u16()];
    statuses.sort();
    assert_eq!(statuses, [202, 202]); // Both clients observe the same single job.
    assert_eq!(
        http.post(format!("{base_b}/api/v1/decisions/other"))
            .json(&json!({}))
            .send()
            .await
            .unwrap()
            .status(),
        429
    );
    release.notify_one();
    for _ in 0..100 {
        if state(&http, &url_b).await["status"] == "failed" {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert_eq!(state(&http, &url_b).await["status"], "failed");
    assert_eq!(
        http.post(&url_b)
            .json(&json!({}))
            .send()
            .await
            .unwrap()
            .status(),
        429
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    task_a.abort();
    task_b.abort();
}

#[tokio::test]
#[ignore = "requires POLYROVER_TEST_DATABASE_URL"]
async fn migration_is_idempotent_preserves_source_files_and_imported_cache() {
    let db = test_database().await;
    let dir = directory();
    let calls = Arc::new(AtomicUsize::new(0));
    let service = DecisionService::connect(
        &db,
        BTreeSet::from(["imported".into(), "attempted".into()]),
        generator(calls.clone()),
    )
    .await
    .unwrap();
    let mut r = report("imported");
    r["generated_at"] = json!(Utc::now() - Duration::hours(23));
    r["yes_quote"]["book_timestamp"] = r["generated_at"].clone();
    let path = dir.join("imported.decision.json");
    std::fs::write(&path, r.to_string()).unwrap();
    std::fs::write(
        dir.join("job-ledger.json"),
        json!({"attempted":Utc::now().timestamp()}).to_string(),
    )
    .unwrap();
    service.import_legacy_directory(&dir).await.unwrap();
    service.import_legacy_directory(&dir).await.unwrap();
    assert!(path.exists());
    let (base, task) = server(service).await;
    let http = reqwest::Client::new();
    let url = format!("{base}/api/v1/decisions/imported");
    let cached = state(&http, &url).await;
    assert_eq!(cached["cache_hit"], true);
    assert_eq!(cached["data"]["action"], "wait"); // research != executable quote lifetime
    assert_eq!(cached["data"]["research_stale"], false);
    assert_eq!(cached["can_generate"], false);
    assert_eq!(
        http.post(&url)
            .json(&json!({}))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    assert_eq!(
        http.post(format!("{base}/api/v1/decisions/attempted"))
            .json(&json!({}))
            .send()
            .await
            .unwrap()
            .status(),
        429
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    let (client, connection) = tokio_postgres::connect(&db, tokio_postgres::NoTls)
        .await
        .unwrap();
    tokio::spawn(async move {
        connection.await.unwrap();
    });
    assert_eq!(
        client
            .query_one("SELECT count(*) FROM polyrover_predictions", &[])
            .await
            .unwrap()
            .get::<_, i64>(0),
        1
    );
    task.abort();
    std::fs::remove_dir_all(dir).unwrap();
}

#[tokio::test]
#[ignore = "requires POLYROVER_TEST_DATABASE_URL"]
async fn unavailable_storage_never_falls_back_to_paid_generation() {
    let db = test_database().await;
    let calls = Arc::new(AtomicUsize::new(0));
    let service = DecisionService::connect(
        &db,
        BTreeSet::from(["fail-closed".into()]),
        generator(calls.clone()),
    )
    .await
    .unwrap();
    // Simulate missing storage inside this test's isolated schema.
    sql(
        &db,
        "ALTER TABLE polyrover_predictions RENAME TO unavailable_predictions",
    )
    .await;
    let (base, task) = server(service).await;
    let http = reqwest::Client::new();
    let url = format!("{base}/api/v1/decisions/fail-closed");
    assert_eq!(http.get(&url).send().await.unwrap().status(), 503);
    assert_eq!(
        http.post(&url)
            .json(&json!({}))
            .send()
            .await
            .unwrap()
            .status(),
        503
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    task.abort();
}

#[tokio::test]
#[ignore = "requires POLYROVER_TEST_DATABASE_URL"]
async fn arenaton_can_create_any_market_and_reuses_cache_even_at_daily_limit() {
    let db = test_database().await;
    let calls = Arc::new(AtomicUsize::new(0));
    let service = DecisionService::connect_with_generation(
        &db,
        BTreeSet::new(),
        true,
        1,
        generator(calls.clone()),
    )
    .await
    .unwrap();
    let (base, task) = server(service).await;
    let http = reqwest::Client::new();
    let first = format!("{base}/api/v1/decisions/not-preapproved");
    let second = format!("{base}/api/v1/decisions/another-market");
    let missing = state(&http, &first).await;
    assert_eq!(missing["status"], "missing");
    assert_eq!(missing["generation_enabled"], true);
    assert_eq!(missing["can_generate"], true);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        http.post(&first)
            .json(&json!({}))
            .send()
            .await
            .unwrap()
            .status(),
        202
    );
    let saved = ready(&http, &first).await;
    assert_eq!(saved["cache_hit"], true);
    assert_eq!(
        http.post(&first)
            .json(&json!({}))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let limited = state(&http, &second).await;
    assert_eq!(limited["generation_enabled"], true);
    assert_eq!(limited["can_generate"], false);
    assert!(limited["retry_after_seconds"].as_i64().unwrap() > 0);
    assert_eq!(
        http.post(&second)
            .json(&json!({}))
            .send()
            .await
            .unwrap()
            .status(),
        429
    );
    task.abort();
    // A new instance cannot reset the shared spend limit.
    let reopened = DecisionService::connect_with_generation(
        &db,
        BTreeSet::new(),
        true,
        1,
        generator(calls.clone()),
    )
    .await
    .unwrap();
    let (base, task) = server(reopened).await;
    assert_eq!(
        http.post(format!("{base}/api/v1/decisions/another-market"))
            .json(&json!({}))
            .send()
            .await
            .unwrap()
            .status(),
        429
    );
    assert_eq!(
        http.post(format!("{base}/api/v1/decisions/not-preapproved"))
            .json(&json!({}))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    task.abort();
}

//! Standalone, bounded decision API. Public reads; generation only for an explicit
//! operator allowlist, at most one attempt per market/day and one job at a time.
use std::{
    collections::{BTreeMap, BTreeSet},
    future::Future,
    path::{Path, PathBuf},
    pin::Pin,
    sync::Arc,
};

use crate::{Error, Result};
use axum::{
    extract::{DefaultBodyLimit, Path as RoutePath, State},
    http::{header, HeaderValue, Method, StatusCode},
    routing::get,
    Json, Router,
};
use chrono::{DateTime, Duration, Utc};
use serde_json::{json, Value};
use tokio::sync::Mutex;
use tower_http::{cors::CorsLayer, set_header::SetResponseHeaderLayer};

pub type Generator =
    Arc<dyn Fn(String) -> Pin<Box<dyn Future<Output = Result<Value>> + Send>> + Send + Sync>;
const DAY_SECONDS: i64 = 86_400;
const MAX_REPORT_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Clone)]
pub struct DecisionService {
    inner: Arc<Mutex<Stored>>,
    directory: PathBuf,
    allowed: BTreeSet<String>,
    generator: Generator,
}

#[derive(Default)]
struct Stored {
    reports: BTreeMap<String, Value>,
    attempts: BTreeMap<String, i64>,
    running: Option<String>,
    failed: BTreeSet<String>,
}

fn invalid(message: &str) -> Error {
    Error::Invalid(message.into())
}

pub fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 300
        && slug
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
}

fn date(value: &Value) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value.as_str()?)
        .ok()
        .map(|d| d.with_timezone(&Utc))
}

/// Only typed/allowlisted public fields cross the HTTP boundary. Old CLI reports
/// are accepted, but schema, identity and advisory-only semantics are mandatory.
pub fn project_report(report: &Value, now: DateTime<Utc>) -> Result<Value> {
    let slug = report["slug"]
        .as_str()
        .filter(|s| valid_slug(s))
        .ok_or_else(|| invalid("invalid report slug"))?;
    let market_id = report["market_id"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| invalid("missing report market identity"))?;
    let generated = date(&report["generated_at"])
        .filter(|d| *d <= now + Duration::seconds(5))
        .ok_or_else(|| invalid("invalid report timestamp"))?;
    let action = report["decision"]["action"]
        .as_str()
        .filter(|s| matches!(*s, "buy_yes" | "buy_no" | "wait"))
        .ok_or_else(|| invalid("invalid decision action"))?;
    if report["rubric_version"] != "decision_v1"
        || report["decision"]["orders_submitted"] != 0
        || report["decision"]["experimental"] != true
    {
        return Err(invalid("unsupported or non-advisory report"));
    }
    let mut valid_until = generated + Duration::seconds(120);
    let side = match action {
        "buy_yes" => Some("yes_quote"),
        "buy_no" => Some("no_quote"),
        _ => None,
    };
    if let Some(side) = side {
        let timestamp = date(&report[side]["book_timestamp"])
            .ok_or_else(|| invalid("buy decision missing venue timestamp"))?;
        valid_until = valid_until.min(timestamp + Duration::seconds(120));
    }
    let stale = now >= valid_until;
    let sources: Vec<Value> = report["research"]["articles"].as_array().into_iter().flatten().map(|entry| {
        let article = &entry["article"];
        json!({"id":article["id"],"title":article["title"],"url":article["final_url"],"status":article["status"],
            "evaluated":entry["all_extracted_text_evaluated"]})
    }).collect();
    let mut reasons = report["decision"]["reasons"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    if stale {
        reasons.push(json!("decision_expired_refresh_required"));
    }
    Ok(json!({
        "slug":slug,"market_id":market_id,"question":report["question"],"generated_at":generated,
        "valid_until":valid_until,"stale":stale,"action":if stale {"wait"} else {action},"reported_action":action,
        "summary":if stale { "Saved analysis; the priced decision has expired. Do not treat old quotes as a current recommendation." } else { report["decision"]["summary_es"].as_str().unwrap_or("") },
        "predicted_outcome":report["forecast"]["predicted_outcome"],"yes_interval":report["forecast"]["yes_interval"],
        "classifier_confidence":report["forecast"]["classifier_confidence"],"calibrated":false,"experimental":true,
        "model":report["forecast"]["evaluation"]["model"],"reasons":reasons,
        "coverage":report["research"]["coverage"],"sources":sources,
        "yes_quote":public_quote(&report["yes_quote"]),"no_quote":public_quote(&report["no_quote"]),
        "limitations":report["limitations"],"orders_submitted":0
    }))
}

fn public_quote(quote: &Value) -> Value {
    if quote.is_null() {
        return Value::Null;
    }
    json!({"average_ask":quote["average_ask"],"total_cost_per_share":quote["total_cost_per_share"],
        "shares":quote["shares"],"book_timestamp":quote["book_timestamp"]})
}

fn read_json(path: &Path) -> Result<Value> {
    use std::io::Read;
    let file = std::fs::File::open(path).map_err(|_| invalid("cannot open decision data"))?;
    let mut bytes = Vec::new();
    file.take(MAX_REPORT_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| invalid("cannot read decision data"))?;
    if bytes.len() as u64 > MAX_REPORT_BYTES {
        return Err(invalid("decision data exceeds size limit"));
    }
    Ok(serde_json::from_slice(&bytes)?)
}

fn atomic_json(path: &Path, value: &Value) -> Result<()> {
    use std::io::Write;
    let temp = path.with_extension(format!(
        "{}-{}.tmp",
        std::process::id(),
        Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)
        .map_err(|_| invalid("cannot reserve decision storage"))?;
    file.write_all(&serde_json::to_vec(value)?)
        .and_then(|_| file.sync_all())
        .map_err(|_| invalid("cannot persist decision storage"))?;
    std::fs::rename(&temp, path).map_err(|_| invalid("cannot publish decision storage"))?;
    Ok(())
}

impl DecisionService {
    /// Use a dedicated directory; the API has no file-path parameters.
    pub fn open(
        directory: PathBuf,
        allowed: BTreeSet<String>,
        generator: Generator,
    ) -> Result<Self> {
        if allowed.len() > 100 || allowed.iter().any(|s| !valid_slug(s)) {
            return Err(invalid("allowlist requires at most 100 valid slugs"));
        }
        std::fs::create_dir_all(&directory)
            .map_err(|_| invalid("cannot create decision directory"))?;
        let mut stored = Stored::default();
        let ledger = directory.join("job-ledger.json");
        if ledger.exists() {
            stored.attempts = serde_json::from_value(read_json(&ledger)?)?;
        }
        for entry in
            std::fs::read_dir(&directory).map_err(|_| invalid("cannot list decision directory"))?
        {
            let entry = entry.map_err(|_| invalid("cannot list decision entry"))?;
            if !entry
                .file_type()
                .map_err(|_| invalid("cannot inspect decision entry"))?
                .is_file()
                || !entry
                    .file_name()
                    .to_string_lossy()
                    .ends_with(".decision.json")
            {
                continue;
            }
            if stored.reports.len() >= 1000 {
                return Err(invalid("decision store exceeds 1000 reports"));
            }
            let report = read_json(&entry.path())?;
            let projected = project_report(&report, Utc::now())?;
            stored
                .reports
                .insert(projected["slug"].as_str().unwrap().into(), report);
        }
        Ok(Self {
            inner: Arc::new(Mutex::new(stored)),
            directory,
            allowed,
            generator,
        })
    }

    pub async fn import_report(&self, path: &Path) -> Result<()> {
        let value = read_json(path)?;
        let report = if value["ok"] == true && value["version"] == "1" {
            value["data"].clone()
        } else {
            value
        };
        let projected = project_report(&report, Utc::now())?;
        let slug = projected["slug"].as_str().unwrap().to_owned();
        let mut stored = self.inner.lock().await;
        if stored
            .reports
            .get(&slug)
            .is_some_and(|old| date(&old["generated_at"]) > date(&report["generated_at"]))
        {
            return Err(invalid("refusing to replace a newer decision"));
        }
        atomic_json(
            &self.directory.join(format!("{slug}.decision.json")),
            &report,
        )?;
        stored.reports.insert(slug, report);
        Ok(())
    }

    async fn snapshot(&self, slug: &str) -> Value {
        let stored = self.inner.lock().await;
        let now = Utc::now();
        let cooldown = stored
            .attempts
            .get(slug)
            .map(|ts| (ts.saturating_add(DAY_SECONDS) - now.timestamp()).max(0))
            .unwrap_or(0);
        let running = stored.running.as_deref() == Some(slug);
        let failed = stored.failed.contains(slug);
        let report = stored
            .reports
            .get(slug)
            .and_then(|r| project_report(r, now).ok());
        let status = if running {
            "running"
        } else if failed {
            "failed"
        } else if report.is_some() {
            "ready"
        } else {
            "missing"
        };
        json!({"schema_version":"polyrover_decision_v1","slug":slug,"status":status,
            "can_generate":self.allowed.contains(slug) && stored.running.is_none() && cooldown == 0,
            "generation_enabled":self.allowed.contains(slug),"retry_after_seconds":if running {3} else {cooldown},
            "error_code":if failed {Some("generation_failed")} else {None},"data":report})
    }

    async fn start(&self, slug: String) -> std::result::Result<(), StatusCode> {
        if !self.allowed.contains(&slug) {
            return Err(StatusCode::FORBIDDEN);
        }
        let mut stored = self.inner.lock().await;
        let now = Utc::now().timestamp();
        if stored.running.is_some()
            || stored
                .attempts
                .get(&slug)
                .is_some_and(|ts| now < ts.saturating_add(DAY_SECONDS))
        {
            return Err(StatusCode::TOO_MANY_REQUESTS);
        }
        let mut attempts = stored.attempts.clone();
        attempts.insert(slug.clone(), now);
        // Persist the spend gate BEFORE starting any paid request, including failures.
        atomic_json(&self.directory.join("job-ledger.json"), &json!(attempts))
            .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
        stored.attempts = attempts;
        stored.running = Some(slug.clone());
        stored.failed.remove(&slug);
        drop(stored);
        let service = self.clone();
        tokio::spawn(async move {
            let result = tokio::time::timeout(
                std::time::Duration::from_secs(1200),
                (service.generator)(slug.clone()),
            )
            .await;
            let report = result
                .ok()
                .and_then(|r| r.ok())
                .filter(|r| r["slug"] == slug && project_report(r, Utc::now()).is_ok());
            let mut stored = service.inner.lock().await;
            if let Some(report) = report.filter(|r| {
                atomic_json(&service.directory.join(format!("{slug}.decision.json")), r).is_ok()
            }) {
                stored.reports.insert(slug.clone(), report);
            } else {
                stored.failed.insert(slug.clone());
            }
            stored.running = None;
        });
        Ok(())
    }
}

pub fn router(service: DecisionService, origins: Vec<HeaderValue>) -> Router {
    Router::new()
        .route(
            "/health",
            get(|| async {
                Json(json!({"service":"polyrover","schema_version":"polyrover_decision_v1"}))
            }),
        )
        .route("/api/v1/decisions/{slug}", get(read).post(generate))
        .layer(DefaultBodyLimit::max(1024))
        .layer(
            CorsLayer::new()
                .allow_origin(origins)
                .allow_methods([Method::GET, Method::POST])
                .allow_headers([header::CONTENT_TYPE]),
        )
        .layer(SetResponseHeaderLayer::overriding(
            header::CACHE_CONTROL,
            HeaderValue::from_static("no-store"),
        ))
        .with_state(service)
}

async fn read(
    State(service): State<DecisionService>,
    RoutePath(slug): RoutePath<String>,
) -> (StatusCode, Json<Value>) {
    if !valid_slug(&slug) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error":"invalid_slug"})),
        );
    }
    (StatusCode::OK, Json(service.snapshot(&slug).await))
}

async fn generate(
    State(service): State<DecisionService>,
    RoutePath(slug): RoutePath<String>,
    Json(body): Json<Value>,
) -> (StatusCode, Json<Value>) {
    if !valid_slug(&slug) || body != json!({}) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error":"expected_empty_object_and_valid_slug"})),
        );
    }
    let status = match service.start(slug.clone()).await {
        Ok(()) => StatusCode::ACCEPTED,
        Err(code) => code,
    };
    (status, Json(service.snapshot(&slug).await))
}

//! Standalone, bounded decision API. Public reads; generation only for an explicit
//! operator policy, at most one attempt per market/day and one job at a time.
use std::{collections::BTreeSet, future::Future, path::Path, pin::Pin, sync::Arc};

use crate::{Error, Result};
use axum::{
    extract::{DefaultBodyLimit, Path as RoutePath, State},
    http::{header, HeaderValue, Method, StatusCode},
    routing::get,
    Json, Router,
};
use chrono::{DateTime, Duration, Utc};
use serde_json::{json, Value};
use tower_http::{cors::CorsLayer, set_header::SetResponseHeaderLayer};

pub type Generator =
    Arc<dyn Fn(String) -> Pin<Box<dyn Future<Output = Result<Value>> + Send>> + Send + Sync>;
const MAX_REPORT_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Clone)]
pub struct DecisionService {
    store: crate::decision_store::Store,
    allowed: BTreeSet<String>,
    all_markets: bool,
    daily_limit: u32,
    generator: Generator,
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
    let market = &report["market_snapshot"];
    let market_status = if market["closed"] == true || market["archived"] == true {
        "closed"
    } else if date(&market["end_date"]).is_some_and(|d| d <= now) {
        "expired"
    } else if market["active"] == false || market["accepting_orders"] == false {
        "not_open"
    } else {
        "open_or_unknown"
    };
    let forecast_evaluated = report["forecast"]["evaluation"].is_object();
    let forecast_status = if market_status != "open_or_unknown" {
        "not_applicable"
    } else if forecast_evaluated {
        "evaluated"
    } else if report["forecast"]["error"] == "no_current_relevant_articles" {
        "no_eligible_evidence"
    } else {
        "evaluation_unavailable"
    };
    let exclusions = report["evidence_selection"]["excluded"].as_object();
    let omitted_for_size = exclusions
        .map(|e| e.values().filter(|v| *v == "context_budget").count())
        .unwrap_or(0);
    Ok(json!({
        "slug":slug,"market_id":market_id,"question":report["question"],"generated_at":generated,
        "research_valid_until":generated + Duration::hours(24),"research_stale":now >= generated + Duration::hours(24),
        "valid_until":valid_until,"stale":stale,"action":if stale {"wait"} else {action},"reported_action":action,
        "summary":if stale { "Saved analysis; the priced decision has expired. Do not treat old quotes as a current recommendation." } else { report["decision"]["summary_es"].as_str().unwrap_or("") },
        "predicted_outcome":report["forecast"]["predicted_outcome"],"yes_interval":report["forecast"]["yes_interval"],
        "classifier_confidence":report["forecast"]["classifier_confidence"],"calibrated":false,"experimental":true,
        "model":report["forecast"]["evaluation"]["model"],"reasons":reasons,
        "analysis_version":report["analysis_version"].as_str().unwrap_or("legacy_v1"),
        "market_status":market_status,"forecast_status":forecast_status,
        "forecast_reason":report["forecast"]["reason"],
        "forecast_basis":report["forecast"]["basis"],
        "evidence_used":report["evidence_selection"]["included_ids"].as_array().map(Vec::len).unwrap_or(0),
        "evidence_omitted_for_size":omitted_for_size,
        "stale_sources":report["research"]["coverage"]["stale_or_undated"].as_array().map(Vec::len).unwrap_or(0),
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

impl DecisionService {
    pub async fn connect(
        database_url: &str,
        allowed: BTreeSet<String>,
        generator: Generator,
    ) -> Result<Self> {
        Self::connect_with_generation(database_url, allowed, false, 10, generator).await
    }

    /// Enable app-initiated generation without a per-market operator allowlist.
    pub async fn connect_with_generation(
        database_url: &str,
        allowed: BTreeSet<String>,
        all_markets: bool,
        daily_limit: u32,
        generator: Generator,
    ) -> Result<Self> {
        if !(1..=100).contains(&daily_limit) {
            return Err(invalid("daily generation limit must be between 1 and 100"));
        }
        if allowed.len() > 100 || allowed.iter().any(|s| !valid_slug(s)) {
            return Err(invalid("allowlist requires at most 100 valid slugs"));
        }
        Ok(Self {
            store: crate::decision_store::Store::connect(database_url).await?,
            allowed,
            all_markets,
            daily_limit,
            generator,
        })
    }

    pub async fn import_report(&self, path: &Path) -> Result<()> {
        let report = unwrap_report(read_json(path)?);
        project_report(&report, Utc::now())?;
        self.store.import(&[report], &[]).await
    }

    /// Import the previous JSON store atomically; files remain untouched.
    pub async fn import_legacy_directory(&self, directory: &Path) -> Result<()> {
        if !directory.exists() {
            return Ok(());
        }
        let mut reports = Vec::new();
        for entry in
            std::fs::read_dir(directory).map_err(|_| invalid("cannot list legacy decisions"))?
        {
            let entry = entry.map_err(|_| invalid("cannot read legacy entry"))?;
            if !entry
                .file_type()
                .map_err(|_| invalid("cannot inspect legacy entry"))?
                .is_file()
                || !entry
                    .file_name()
                    .to_string_lossy()
                    .ends_with(".decision.json")
            {
                continue;
            }
            if reports.len() >= 1000 {
                return Err(invalid("legacy import exceeds 1000 reports"));
            }
            let report = unwrap_report(read_json(&entry.path())?);
            project_report(&report, Utc::now())?;
            reports.push(report);
        }
        let ledger = directory.join("job-ledger.json");
        let mut attempts = Vec::new();
        if ledger.exists() {
            let values: std::collections::BTreeMap<String, i64> =
                serde_json::from_value(read_json(&ledger)?)?;
            for (slug, timestamp) in values {
                if !valid_slug(&slug) {
                    return Err(invalid("invalid legacy ledger slug"));
                }
                let started = DateTime::from_timestamp(timestamp, 0)
                    .ok_or_else(|| invalid("invalid legacy timestamp"))?;
                if started > Utc::now() + Duration::seconds(5) {
                    return Err(invalid("future legacy attempt"));
                }
                attempts.push((slug, started));
            }
        }
        self.store.import(&reports, &attempts).await
    }

    async fn snapshot(&self, slug: &str) -> Result<Value> {
        let stored = self.store.snapshot(slug, self.daily_limit).await?;
        let report = stored
            .report
            .as_ref()
            .map(|r| project_report(r, stored.now))
            .transpose()?;
        let research_fresh = report
            .as_ref()
            .is_some_and(|r| r["research_stale"] == false);
        let status = if stored.running {
            "running"
        } else if research_fresh {
            "ready"
        } else if stored.failed {
            "failed"
        } else if report.is_some() {
            "ready"
        } else {
            "missing"
        };
        Ok(
            json!({"schema_version":"polyrover_decision_v1","slug":slug,"status":status,
            "storage":"postgresql","research_ttl_seconds":86400,
            "cache_hit":research_fresh,
            "can_generate":self.generation_enabled(slug) && !stored.busy && stored.cooldown == 0,
            "generation_enabled":self.generation_enabled(slug),
            "daily_generation_limit":self.daily_limit,
            "retry_after_seconds":if stored.running {3} else {stored.cooldown},
            "error_code":if stored.failed && !research_fresh {Some("generation_failed")} else {None},"data":report}),
        )
    }

    async fn start(&self, slug: String) -> std::result::Result<StatusCode, StatusCode> {
        use crate::decision_store::Reservation;
        let id = match self
            .store
            .reserve(&slug, self.generation_enabled(&slug), self.daily_limit)
            .await
            .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?
        {
            Reservation::Cached => return Ok(StatusCode::OK),
            Reservation::Running => return Ok(StatusCode::ACCEPTED),
            Reservation::Limited => return Err(StatusCode::TOO_MANY_REQUESTS),
            Reservation::Forbidden => return Err(StatusCode::FORBIDDEN),
            Reservation::Started(id) => id,
        };
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
            if service.store.complete(id, report.as_ref()).await.is_err() {
                eprintln!("Polyrover: could not persist research completion; daily gate retained");
            }
        });
        Ok(StatusCode::ACCEPTED)
    }

    fn generation_enabled(&self, slug: &str) -> bool {
        self.all_markets || self.allowed.contains(slug)
    }
}

fn unwrap_report(value: Value) -> Value {
    if value["ok"] == true && value["version"] == "1" {
        value["data"].clone()
    } else {
        value
    }
}

async fn response(
    service: &DecisionService,
    slug: &str,
    status: StatusCode,
) -> (StatusCode, Json<Value>) {
    match service.snapshot(slug).await {
        Ok(value) => (status, Json(value)),
        Err(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"error":"decision_database_unavailable"})),
        ),
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
    response(&service, &slug, StatusCode::OK).await
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
        Ok(code) => code,
        Err(code) => code,
    };
    response(&service, &slug, status).await
}

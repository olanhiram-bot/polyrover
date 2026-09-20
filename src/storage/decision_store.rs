//! PostgreSQL is the authoritative cache and spend gate, shared across processes.
use crate::{Error, Result};
use chrono::{DateTime, Duration, Utc};
use postgres_native_tls::MakeTlsConnector;
use serde_json::Value;
use tokio_postgres::{Client, Transaction};

// Transaction-scoped lock protects reservation/import/completion across instances.
const LOCK: i64 = 0x504f4c59524f5645;

#[derive(Clone)]
pub(crate) struct Store {
    config: tokio_postgres::Config,
}

pub(crate) struct Snapshot {
    pub report: Option<Value>,
    pub now: DateTime<Utc>,
    pub cooldown: i64,
    pub running: bool,
    pub busy: bool,
    pub failed: bool,
}

pub(crate) enum Reservation {
    Cached,
    Started(i64),
    Limited,
    Forbidden,
}

fn unavailable(_: impl std::fmt::Display) -> Error {
    // Database errors can contain connection strings, credentials or report data.
    Error::Invalid("decision database unavailable".into())
}

impl Store {
    pub async fn connect(url: &str) -> Result<Self> {
        let mut config: tokio_postgres::Config = url.parse().map_err(unavailable)?;
        // Never silently fall back to plaintext for remote database credentials.
        let local = !config.get_hosts().is_empty()
            && config.get_hosts().iter().all(|h| match h {
                tokio_postgres::config::Host::Tcp(h) => {
                    matches!(h.as_str(), "localhost" | "127.0.0.1" | "::1")
                }
                #[cfg(unix)]
                tokio_postgres::config::Host::Unix(_) => true,
            });
        if !local {
            config.ssl_mode(tokio_postgres::config::SslMode::Require);
        }
        config.connect_timeout(std::time::Duration::from_secs(5));
        let store = Self { config };
        let mut client = store.client().await?;
        let tx = client.transaction().await.map_err(unavailable)?;
        lock(&tx).await?;
        tx.batch_execute(include_str!("../../migrations/001_decisions.sql"))
            .await
            .map_err(unavailable)?;
        tx.commit().await.map_err(unavailable)?;
        Ok(store)
    }

    async fn client(&self) -> Result<Client> {
        let tls = native_tls::TlsConnector::new().map_err(unavailable)?;
        let (client, connection) = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            self.config.connect(MakeTlsConnector::new(tls)),
        )
        .await
        .map_err(unavailable)?
        .map_err(unavailable)?;
        tokio::spawn(async move {
            let _ = connection.await;
        });
        client
            .batch_execute(
                "SET statement_timeout = '5s'; SET lock_timeout = '5s'; SET TIME ZONE 'UTC'",
            )
            .await
            .map_err(unavailable)?;
        Ok(client)
    }

    pub async fn snapshot(&self, slug: &str) -> Result<Snapshot> {
        let client = self.client().await?;
        // One statement gives a coherent MVCC snapshot; DB time is authoritative.
        let row = client.query_one(
            "SELECT clock_timestamp(),
             (SELECT report FROM polyrover_predictions WHERE slug=$1 ORDER BY generated_at DESC LIMIT 1),
             (SELECT research_valid_until FROM polyrover_predictions WHERE slug=$1 ORDER BY generated_at DESC LIMIT 1),
             (SELECT MAX(retry_at) FROM polyrover_research_jobs WHERE slug=$1),
             EXISTS(SELECT 1 FROM polyrover_research_jobs WHERE slug=$1 AND status='running' AND lease_until>clock_timestamp()),
             EXISTS(SELECT 1 FROM polyrover_research_jobs WHERE status='running' AND lease_until>clock_timestamp()),
             (SELECT status FROM polyrover_research_jobs WHERE slug=$1 ORDER BY started_at DESC LIMIT 1)", &[&slug]
        ).await.map_err(unavailable)?;
        let now: DateTime<Utc> = row.get(0);
        let expiry: Option<DateTime<Utc>> = row.get(2);
        let retry: Option<DateTime<Utc>> = row.get(3);
        let running = row.get(4);
        let latest: Option<String> = row.get(6);
        let cooldown = expiry
            .into_iter()
            .chain(retry)
            .map(|t| ((t - now).num_milliseconds().max(0) + 999) / 1000)
            .max()
            .unwrap_or(0);
        Ok(Snapshot {
            report: row.get(1),
            now,
            cooldown,
            running,
            busy: row.get(5),
            failed: !running
                && matches!(
                    latest.as_deref(),
                    Some("failed" | "interrupted" | "running")
                ),
        })
    }

    pub async fn reserve(&self, slug: &str, allowed: bool) -> Result<Reservation> {
        let mut client = self.client().await?;
        let tx = client.transaction().await.map_err(unavailable)?;
        lock(&tx).await?;
        let now: DateTime<Utc> = tx
            .query_one("SELECT clock_timestamp()", &[])
            .await
            .map_err(unavailable)?
            .get(0);
        let cached: bool = tx.query_one(
            "SELECT EXISTS(SELECT 1 FROM polyrover_predictions WHERE slug=$1 AND research_valid_until>$2)", &[&slug, &now]
        ).await.map_err(unavailable)?.get(0);
        if cached {
            return Ok(Reservation::Cached);
        }
        if !allowed {
            return Ok(Reservation::Forbidden);
        }
        let limited: bool = tx
            .query_one(
                "SELECT EXISTS(SELECT 1 FROM polyrover_research_jobs WHERE
             (slug=$1 AND retry_at>$2) OR (status='running' AND lease_until>$2))",
                &[&slug, &now],
            )
            .await
            .map_err(unavailable)?
            .get(0);
        if limited {
            return Ok(Reservation::Limited);
        }
        tx.execute("UPDATE polyrover_research_jobs SET status='interrupted', finished_at=$1 WHERE status='running' AND lease_until<=$1", &[&now])
            .await.map_err(unavailable)?;
        let id = tx
            .query_one(
                "INSERT INTO polyrover_research_jobs (slug,started_at,retry_at,lease_until,status)
             VALUES ($1,$2,$3,$4,'running') RETURNING id",
                &[
                    &slug,
                    &now,
                    &(now + Duration::hours(24)),
                    &(now + Duration::minutes(21)),
                ],
            )
            .await
            .map_err(unavailable)?
            .get(0);
        // Commit BEFORE any provider call: failures and restarts cannot bypass this gate.
        tx.commit().await.map_err(unavailable)?;
        Ok(Reservation::Started(id))
    }

    pub async fn complete(&self, id: i64, report: Option<&Value>) -> Result<()> {
        let mut client = self.client().await?;
        let tx = client.transaction().await.map_err(unavailable)?;
        lock(&tx).await?;
        let job = tx.query_opt("SELECT slug FROM polyrover_research_jobs WHERE id=$1 AND status='running' AND lease_until>clock_timestamp()", &[&id])
            .await.map_err(unavailable)?;
        let Some(job) = job else {
            return Err(unavailable("expired job"));
        };
        if let Some(report) = report {
            if report["slug"].as_str() != Some(job.get::<_, &str>(0)) {
                return Err(unavailable("wrong market"));
            }
            insert_report(&tx, report).await?;
        }
        let status = if report.is_some() {
            "succeeded"
        } else {
            "failed"
        };
        tx.execute("UPDATE polyrover_research_jobs SET status=$2, finished_at=clock_timestamp() WHERE id=$1", &[&id, &status])
            .await.map_err(unavailable)?;
        tx.commit().await.map_err(unavailable)
    }

    /// Additive, idempotent migration; never deletes the source files or history.
    pub async fn import(
        &self,
        reports: &[Value],
        attempts: &[(String, DateTime<Utc>)],
    ) -> Result<()> {
        let mut client = self.client().await?;
        let tx = client.transaction().await.map_err(unavailable)?;
        lock(&tx).await?;
        for report in reports {
            insert_report(&tx, report).await?;
        }
        for (slug, started) in attempts {
            tx.execute(
                "INSERT INTO polyrover_research_jobs (slug,started_at,retry_at,lease_until,status)
                VALUES ($1,$2,$3,$2,'interrupted') ON CONFLICT (slug,started_at) DO NOTHING",
                &[slug, started, &(*started + Duration::hours(24))],
            )
            .await
            .map_err(unavailable)?;
        }
        tx.commit().await.map_err(unavailable)
    }
}

async fn lock(tx: &Transaction<'_>) -> Result<()> {
    tx.query_one("SELECT pg_advisory_xact_lock($1)", &[&LOCK])
        .await
        .map_err(unavailable)?;
    Ok(())
}

async fn insert_report(tx: &Transaction<'_>, report: &Value) -> Result<()> {
    let slug = report["slug"].as_str().ok_or_else(|| unavailable("slug"))?;
    let generated = DateTime::parse_from_rfc3339(report["generated_at"].as_str().unwrap_or(""))
        .map_err(unavailable)?
        .with_timezone(&Utc);
    tx.execute(
        "INSERT INTO polyrover_predictions (slug,generated_at,research_valid_until,report)
        VALUES ($1,$2,$3,$4) ON CONFLICT (slug,generated_at) DO NOTHING",
        &[
            &slug,
            &generated,
            &(generated + Duration::hours(24)),
            report,
        ],
    )
    .await
    .map_err(unavailable)?;
    Ok(())
}

# Standalone decision API

Build with `cargo build --features server`. Polyrover itself is the backend for
the Arenaton event decision panel, independently of the previous Alpha service.

`serve` now requires PostgreSQL. Set `POLYROVER_DATABASE_URL` in the server
environment or ignored `.env`; it is never sent to Flutter or printed in errors.
The server creates its tables on startup using `migrations/001_decisions.sql`.
Remote connections require TLS with certificate/hostname verification; local
loopback/Unix sockets can use local authentication. Native TLS uses the system
certificate store. The Rust server build requires OpenSSL development libraries
on Linux.

For the prepared local workspace:

```sh
bash scripts/database.sh start
mise exec rust@stable -- cargo build --features server
```

The script uses PostgreSQL 17 (installed with `mise install postgres@17`), stores
the cluster in `research/postgres`, and listens only on a Unix socket in
`research/postgres-socket`, port identifier 55432. Peer authentication requires
the same OS user; no password or publicly exposed database port is needed.
The local `.env` has the matching `POLYROVER_DATABASE_URL`. For another checkout,
set `POLYROVER_DATABASE_URL="host=/ABSOLUTE/REPO/research/postgres-socket port=55432 dbname=polyrover user=YOUR_OS_USER"`.
Use `bash scripts/database.sh status` or `stop` as needed. This is a local setup,
not an installed boot-time service or a production deployment.

```sh
./target/debug/polyrover serve \
  --enable-generation \
  --data-dir research/decision-api \
  --allow-origin http://localhost:8080 \
  --import-report research/news-reports/psg-decision-v2.json
```

The import is optional and requires an existing local CLI report. Default bind
is `127.0.0.1:8787`. Flutter sets `POLYROVER_API_BASE_URL` at build time; the old
`SERVER_POLYROVER_BASE_URL` belongs to a different protocol and is not an alias.
Production requires a reachable HTTPS reverse proxy, an exact allowed app
origin and a persistent PostgreSQL deployment. This feature does not deploy a server.

## HTTP contract

- `GET /health`: API identity and schema.
- `GET /api/v1/decisions/{market_slug}`: saved state only; never consumes AI quota.
- `POST /api/v1/decisions/{market_slug}` with `{}`: explicit generation. Returns
  200 with cached data during its 24-hour lifetime; otherwise 202 on acceptance,
  403 when generation is disabled/restricted, 429 for busy/daily limits. Repeated
  requests for an already running market return 202 for the same job. Database failures
  return 503 and never fall back to unrecorded paid generation.
- Envelope `schema_version: polyrover_decision_v1`, exact `slug`,
  `status: missing | running | ready | failed`, `can_generate`,
  `generation_enabled`, `retry_after_seconds`, `error_code`, nullable `data`.
- Additive cache fields: `storage: postgresql`, `research_ttl_seconds: 86400`,
  `cache_hit`; data adds `research_valid_until` and `research_stale`.
- Data includes advisory action, historical action, forecast band, classifier
  confidence, timestamps, costs, coverage, source links, reasons and limitations.
  Responses use `Cache-Control: no-store` and omit article bodies and secrets.

Only lower-case alphanumeric/hyphen slugs of at most 300 characters are accepted.
Flutter requests the selected market, not the parent event.

## Storage and recovery

PostgreSQL is the authoritative store, queried on every API read. These reads
do not contact news providers, market APIs or TypeSafe. No process-local cache can
hide another instance's completed research.

| Location | Contents |
| --- | --- |
| CLI `--output PATH` | One JSON report; refuses overwrite. No file is saved automatically when this flag is omitted. |
| `research/news-reports/` | Locally saved research/decision reports from previous CLI runs; not a database or an automatic crawler destination. |
| PostgreSQL `polyrover_predictions` | Append-only history: market slug, generation time, 24-hour expiry, JSONB report, storage time. Latest report selected by generation time. |
| PostgreSQL `polyrover_research_jobs` | Generation attempts, status, 24-hour retry deadline, job lease and completion time. |
| `research/decision-api/*.decision.json` and `job-ledger.json` | Previous JSON store, automatically imported on startup, never deleted or written by the new API. |

`--data-dir PATH` now selects the **legacy import directory** (default unchanged).
Import is additive, transactional and idempotent by market/generation timestamp;
older reports do not replace newer ones. Importing does not reset research age or
attempt cooldowns. Invalid input aborts startup without deleting files. CLI
`ai decide-market --output` remains an explicit uncached operator command; only
the HTTP API enforces this shared cache. Use `--import-report` to add CLI exports.

Reports contain source metadata, coverage, evaluation results, forecasts and
the prices used, not a full-text article archive. Research expires exactly 24
hours after its report's generation time, not after its last read. Expiry does
not delete history or launch a background job; the next explicit POST can request
new research. GET always returns saved data, marking expired research clearly.

Data, local credentials and PostgreSQL files are excluded from Git and Docker
build contexts. Mount persistent storage in production and schedule backups
(for example `pg_dump`); no automatic backup or history-pruning policy is enabled.

## Paid generation and advisory limits

By default the bare `serve` command is read-only and needs no provider key.
Use `--enable-generation` to let Arenaton generate any selected market without
per-market operator approval. The local workflow is `bash scripts/serve-local.sh`:
it starts PostgreSQL, builds Polyrover, and enables generation on loopback with
the exact localhost Flutter origins. Opening an event performs GET only; the
user must press Generate to start missing/expired research. If the research is
already cached for less than 24 hours, POST returns it without provider calls.
Duplicate clicks during a running job reuse that job.

For intentionally restricted deployments, omit `--enable-generation` and add
repeatable `--allow-market EXACT_SLUG` options (at most 100 markets). Set
`TYPESAFE_API_KEY` only in the server environment or ignored `.env`, never in
Flutter, a URL, source control or browser assets.

Generation is public but bounded: by default **10 new attempts per rolling 24
hours across all markets**, one job per database/schema, one attempt per
market per 24 hours, persisted before provider work, and a 20-minute job timeout.
Successful reports additionally block regeneration for 24 hours from completion.
`--daily-generation-limit 1..100` configures the global cap; the local launcher
also accepts `POLYROVER_DAILY_GENERATION_LIMIT`. Failed attempts count; cached
reads and POSTs do not. This is an attempt cap, not a precise currency budget.
Transaction advisory locks serialize reservations across instances; all instances
must share the same database/schema and cap configuration. A crashed
job's 21-minute lease expires without clearing its 24-hour spend gate. CORS is not
authentication; before exposing all-market generation publicly, add access controls
and rate limiting at their proxy. Allowlisting is not an exact currency budget.

Priced decisions expire within 120 seconds and cannot outlive the selected buy
quote's timestamp plus 120 seconds. Server and Flutter downgrade expired buys to
WAIT while preserving the historical forecast. GET does not refresh prices;
automatic repricing is not implemented. During cooldown the saved report can
therefore remain historical. A manual CLI run/import can replace a report.

Forecasts are experimental and uncalibrated. Classifier confidence is not an
event probability. Incomplete sources remain explicit. No API route submits an
order, signs with a wallet or starts trading.

## Verification

```sh
cargo test --locked --features server --test decision_server
# A dedicated test DB is mandatory for the database integration tests:
POLYROVER_TEST_DATABASE_URL='postgresql://USER:PASSWORD@127.0.0.1:5432/polyrover_test' \
  cargo test --locked --features server --test decision_server -- --include-ignored
cargo clippy --locked --all-targets --all-features -- -D warnings
```

HTTP tests use a deterministic generator and do not spend provider credits or
prove predictive accuracy. The Flutter repository also contains a read-only
`tool/polyrover_decision_probe.dart` for checking this real HTTP contract.

Reference: [PostgreSQL transaction advisory locks](https://www.postgresql.org/docs/current/explicit-locking.html#ADVISORY-LOCKS)
and [the async Rust driver](https://docs.rs/tokio-postgres/latest/tokio_postgres/).

## Directional diagnostics

The additive fields and methodology marker `directional_v2` are documented in
[directional decisions](directional-decisions.md). They distinguish a model
forecast from absent evidence or a closed market, including for legacy cached
reports. The schema envelope, 24-hour cache and spend gates are unchanged.

# Standalone decision API

Build with `cargo build --features server`. Polyrover itself is the backend for
the Arenaton event decision panel, independently of the previous Alpha service.

```sh
./target/debug/polyrover serve \
  --data-dir research/decision-api \
  --allow-origin http://localhost:8080 \
  --import-report research/news-reports/psg-decision-v2.json
```

The import is optional and requires an existing local CLI report. Default bind
is `127.0.0.1:8787`. Flutter sets `POLYROVER_API_BASE_URL` at build time; the old
`SERVER_POLYROVER_BASE_URL` belongs to a different protocol and is not an alias.
Production requires a reachable HTTPS reverse proxy, an exact allowed app
origin and a persistent data directory. This feature does not deploy a server.

## HTTP contract

- `GET /health`: API identity and schema.
- `GET /api/v1/decisions/{market_slug}`: saved state only; never consumes AI quota.
- `POST /api/v1/decisions/{market_slug}` with `{}`: explicit generation. Returns
  202 on acceptance, 403 for non-allowlisted markets, 429 for busy/daily cooldown.
- Envelope `schema_version: polyrover_decision_v1`, exact `slug`,
  `status: missing | running | ready | failed`, `can_generate`,
  `generation_enabled`, `retry_after_seconds`, `error_code`, nullable `data`.
- Data includes advisory action, historical action, forecast band, classifier
  confidence, timestamps, costs, coverage, source links, reasons and limitations.
  Responses use `Cache-Control: no-store` and omit article bodies and secrets.

Only lower-case alphanumeric/hyphen slugs of at most 300 characters are accepted.
Flutter requests the selected market, not the parent event.

## Storage and recovery

Paths are relative to the process working directory unless absolute:

| Path | Contents |
| --- | --- |
| CLI `--output PATH` | One JSON report; refuses overwrite. No file is saved automatically when this flag is omitted. |
| `research/news-reports/` | Locally saved research/decision reports from previous CLI runs; not a database or an automatic crawler destination. |
| `research/decision-api/<market_slug>.decision.json` | Latest complete generated/imported report for each market. Replaced atomically on success. |
| `research/decision-api/job-ledger.json` | Persistent timestamps of generation attempts, including failed attempts. |

`--data-dir PATH` overrides the API directory. Existing reports and daily limits
reload on startup. Current job/failure status is in memory; a restart interrupts
jobs but does not reset their persisted cooldown. Reports contain source metadata,
coverage and evaluation results, not a full-text article archive. There is no SQL,
cloud database, version history or automatic backup. Mount persistent storage and
back it up; ephemeral containers lose data when their filesystem is replaced.
Default research directories are gitignored. If overriding the path, keep data
outside Git and restrict filesystem access to the operator.

## Paid generation and advisory limits

By default the API is read-only and needs no provider key. Add repeatable
`--allow-market EXACT_SLUG` options to enable at most 100 markets. Set
`TYPESAFE_API_KEY` only in the server environment or ignored `.env`, never in
Flutter, a URL, source control or browser assets.

Generation is public but bounded: one job globally, one attempt per market per
24 hours, persisted before provider work, and a 20-minute job timeout. Failures
count toward the limit. Run **one process per data directory**. CORS is not
authentication; operators needing private generation must add access controls
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
cargo clippy --locked --all-targets --all-features -- -D warnings
```

HTTP tests use a deterministic generator and do not spend provider credits or
prove predictive accuracy. The Flutter repository also contains a read-only
`tool/polyrover_decision_probe.dart` for checking this real HTTP contract.

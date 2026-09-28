<p align="center">
  <img src="./assets/readme/hero.svg" width="100%" alt="Polyrover turns public Polymarket APIs into typed Rust data, versioned agent JSON, and local fill estimates without fund-moving code">
</p>

<p align="center">
  <a href="#quick-start">Quick start</a> ·
  <a href="#common-workflows">Workflows</a> ·
  <a href="#cli-reference">CLI</a> ·
  <a href="#rust-library">Rust library</a> ·
  <a href="#safety-and-limitations">Safety</a>
</p>

**Polyrover is a Rust CLI and async library for public Polymarket research.**
Search markets, inspect order books and public accounts, stream events, and
simulate fee-aware fills—without private keys or trading permissions in the
default build.

**Good for:** research agents, dashboards, monitoring, and pre-trade analysis.

**Not for:** signing, placing, or cancelling orders.

## What Polyrover does

| Goal                         | Polyrover surface                                                   |
| ---------------------------- | ------------------------------------------------------------------- |
| Find markets and events      | Gamma search and pagination                                         |
| Inspect prices and liquidity | CLOB prices, single/batch history, spreads, books, and fee metadata |
| Research public accounts     | Positions, trades, activity, holders, value, and leaderboards       |
| Watch market changes         | Typed public market WebSocket events                                |
| Estimate hypothetical fills  | Local book walking with optional taker-fee estimates                |
| Build Rust data pipelines    | One async `Client` over Gamma, CLOB, Data API, and market WSS       |

CLI successes and failures share one versioned JSON envelope for scripts and
agents. The default `public` feature contains no fund-moving path.

Use Polyrover for public-data tooling and local execution research. For API-key
creation, order management, or production trading, follow Polymarket's
[official SDK and API guidance](https://docs.polymarket.com/getting-started/sdks-apis).
The current official documentation says its unified Rust SDK is still in
development.

## Quick start

Install the published v0.1.0 CLI from crates.io:

```bash
cargo install polyrover
```

This checkout is ahead of the released v0.2.0 and includes the Unreleased
changes listed in [`CHANGELOG.md`](CHANGELOG.md). Use the Git dependency shown
below for those SDK-only research APIs until the next release is published.

Find a market, copy one outcome token ID from its `clob_token_ids`, then inspect
and simulate against its current book:

```bash
polyrover gamma search --query bitcoin --limit 3 --json

TOKEN_ID=<OUTCOME_TOKEN_ID>
polyrover clob book --token-id "$TOKEN_ID" --json
polyrover clob simulate --token-id "$TOKEN_ID" --side buy --amount 5 --json
```

For buys, `--amount` is USDC book notional. For sells, it is the number of
shares. Add `--fee-category crypto` when you want the documented crypto taker-fee
formula included.

An abbreviated simulation result looks like this:

```json
{
  "ok": true,
  "version": "1",
  "data": {
    "side": "buy",
    "input_amount": "5",
    "input_amount_type": "usdc",
    "complete": true,
    "filled_size": "9",
    "notional": "5",
    "average_price": "0.555556",
    "unfilled_amount": "0"
  },
  "meta": { "command": "clob simulate" }
}
```

`complete` reports whether eligible book liquidity covered the full input. This
is a snapshot calculation, not a future-fill guarantee.

## Common workflows

### Research a public account

```bash
WALLET=<PUBLIC_WALLET_ADDRESS>
polyrover analytics positions --user "$WALLET" --limit 20 --json
polyrover analytics trades --user "$WALLET" --limit 20 --json
polyrover analytics leaderboard --limit 20 --json
```

### Query one bounded history page

Each command below makes one request/page; see the [endpoint capability matrix](docs/endpoint-capability-matrix.md) for limits and retention semantics.

```bash
polyrover clob price-history --token-id "$TOKEN_ID" --interval 1d --fidelity 5 --json
polyrover gamma event-page --closed true --limit 100 --after-cursor "$CURSOR" --json
polyrover analytics trades --user "$PUBLIC_WALLET" --start 1 --limit 100 --offset 0 --json
```

### Watch bounded market events

```bash
polyrover stream watch \
  --token-id "$TOKEN_ID" \
  --limit 10 \
  --seconds 30 \
  --json
```

### Inspect fees before simulation

```bash
polyrover clob fees --json
polyrover clob fee-rate --token-id "$TOKEN_ID" --json
polyrover clob simulate \
  --token-id "$TOKEN_ID" \
  --side buy \
  --amount 5 \
  --fee-category crypto \
  --json
```

## CLI reference

| Area                | Commands                                                                                                                                                                            | Purpose                                                     |
| ------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------- |
| Health              | `ping`                                                                                                                                                                              | Check public API reachability                               |
| Discovery           | `gamma search`, `gamma markets`, `gamma market-page`, `gamma events`, `gamma event-page`                                                                                            | Find events, markets, and outcome token IDs                 |
| Market data         | `clob book`, `clob price`, `clob price-history`, `clob batch-price-history`                                                                                                         | Read executable liquidity, side prices, and bounded history |
| Fees and simulation | `clob fees`, `clob fee-rate`, `clob simulate`                                                                                                                                       | Inspect costs and estimate hypothetical fills               |
| Account research    | `analytics positions`, `analytics trades`, `analytics closed-positions`, `analytics activity`, `analytics leaderboard`, `analytics builder-leaderboard`, `analytics builder-volume` | Read bounded public portfolio, activity, and ranking data   |
| Streaming           | `stream watch`                                                                                                                                                                      | Collect bounded public market events                        |
| Local paper state   | `sim reset`, `sim buy`, `sim sell`                                                                                                                                                  | Apply local fills without network execution                 |

Add `--json` for the versioned envelope. Run `polyrover help <command>` for all
options and examples.

## Rust library

Polyrover's network API is async-only. Pin the published crate and select the
safe public surface explicitly:

```toml
[dependencies]
polyrover = { version = "0.1.0", default-features = false, features = ["public"] }
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

```rust
use polyrover::{simulation::Request, Client, ClientConfig};

#[tokio::main]
async fn main() -> polyrover::Result<()> {
    let client = Client::new(ClientConfig::default())?;
    let estimate = client
        .simulate_with_fee(
            Request {
                token_id: "TOKEN_ID".into(),
                side: "buy".into(),
                amount: "5".into(),
                limit_price: "0.55".into(),
            },
            "crypto",
        )
        .await?;

    println!(
        "complete={} shares={} fee_usdc={}",
        estimate.complete, estimate.filled_size, estimate.estimated_taker_fee
    );
    Ok(())
}
```

<details>
<summary><strong>Current v0.2.0 source: public historical reads and event streams</strong></summary>

These APIs are newer than published v0.1.0. Use the Git revision until v0.2.0
is published to crates.io:

```toml
[dependencies]
futures-util = "0.3"
polyrover = { git = "https://github.com/TrebuchetDynamics/polyrover", default-features = false, features = ["public"] }
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

Each example below makes one request/page; callers own cursor/offset traversal and persistence. See the [endpoint capability matrix](docs/endpoint-capability-matrix.md).

```rust
use polyrover::{Client, ClientConfig};

let client = Client::new(ClientConfig::default())?;
let prices = client
    .price_history(&polyrover::clob::PriceHistoryParams {
        token_id: "TOKEN_ID".into(),
        start_ts: Some(1_700_000_000),
        end_ts: Some(1_700_086_400),
        fidelity: Some(5),
        ..Default::default()
    })
    .await?;

let events = client
    .event_page(&polyrover::gamma::EventKeysetParams {
        limit: Some(100),
        closed: Some(true),
        after_cursor: previous_next_cursor,
        ..Default::default()
    })
    .await?;

let trades = client
    .trades_with(&polyrover::data::TradeParams {
        user: "PUBLIC_WALLET".into(),
        start: Some(1),
        limit: Some(100),
        ..Default::default()
    })
    .await?;
```

The public SDK also provides one-request `builder_trades`, `rebates`, and
`combo_activity` reads. They remain SDK-only: the CLI exposes the bounded
commands in the [CLI reference](#cli-reference), while callers of these three
methods own cursor traversal and persistence.

Or borrow the resilient market client as a typed stream. Drop the adapter before
using the client directly again:

```rust
use futures_util::{pin_mut, StreamExt};

let events = market_client.events();
pin_mut!(events);
while let Some(event) = events.next().await {
    println!("{:#?}", event?);
}
```

The consuming server owns persistence, scheduling, analytics, and alerts;
Polyrover only supplies typed public data.

</details>

<details>
<summary><strong>Opt-in authenticated history reads</strong></summary>

Authenticated history is an opt-in library surface. Callers pass `&L2Credentials` to each read; Polyrover does not load, store, print, or expose credentials through `ClientConfig` or CLI commands. MegaBot consumers must not enable this feature and continue to compile only `public`.

```rust
use polyrover::{
    auth::L2Credentials,
    authenticated_clob::TradeParams,
};

let credentials = L2Credentials {
    address: account_address,
    api_key,
};
let page = client
    .authenticated_trades_page(
        &credentials,
        &TradeParams {
            after: Some(1_700_000_000),
            next_cursor: previous_next_cursor,
            ..Default::default()
        },
    )
    .await?;
```

`authenticated_clob::Client` additionally exposes one-request `rewards_page`,
`rewards_total`, and `rewards_markets_page` reads. `account_address`, `api_key`,
and `previous_next_cursor` are supplied by the caller and must not be logged.
Each call returns one upstream response/page; no authenticated CLI, key creation,
signing key, order mutation, traversal, or persistence is provided.

</details>

<details>
<summary><strong>Current v0.2.0 source: market-flow context</strong></summary>

Feed typed tracked market events into a bounded per-asset window:

```rust
use polyrover::{market_flow::{MarketFlowConfig, MarketFlowTracker}, Decimal};

let mut flow = MarketFlowTracker::new(MarketFlowConfig {
    window_ms: 15 * 60 * 1_000,
    large_trade_notional: Decimal::new(1_000, 0),
    max_trades_per_asset: 10_000,
})?;
flow.observe_tracked(&tracked_event, observed_at_ms)?;
let context = flow.snapshot("TOKEN_ID", observed_at_ms);
```

The result is descriptive public-flow context; not proof of intent,
coordination, misconduct, or trading edge. The consumer still owns persistence,
scheduling, and alerts.

</details>

Browse the [published v0.1.0 API documentation](https://docs.rs/polyrover/0.1.0/polyrover/)
or the current-main [endpoint capability matrix](docs/endpoint-capability-matrix.md).

## Safety and limitations

Published Polyrover v0.1.0 supports observation, local simulation,
reconciliation, and pre-trade research. The v0.2.0 source adds only public
research context. Neither surface has an order-submission, cancellation,
private-key signing, relayer, or bridge-transfer client.

<details>
<summary><strong>Compile-time feature layers</strong></summary>

<p align="center">
  <img src="./assets/readme/capability-map.svg" width="100%" alt="Polyrover capability layers from the public default through optional authenticated, wallet, execution-model, and bridge-model features">
</p>

- **`public` (default)** — Gamma, CLOB, Data API, market WSS, and resolution.
- **`authenticated`** — public features plus borrowed-credential L2 history reads, HMAC helpers, and user WSS; no authenticated CLI.
- **`wallet`** — local address derivation and readiness helpers.
- **`execution`** — order and cancellation data types only; no submission transport.
- **`bridge`** — bridge data types and local validation only; no transfer transport.
- **`full`** — compiles every surface above; it does not add runtime authority.

</details>

Current limitations:

- “Documented historical-query parity” means Polyrover exposes the documented Prediction Markets history requests and pagination controls. It does not mean Polymarket guarantees permanent retention, and Polyrover does not crawl, download, resume, or persist complete archives.
- A book snapshot cannot predict latency, queue position, or market movement.
- Streaming liquidity/depth and local simulation use Decimal internally;
  upstream wire values remain strings.
- CI uses deterministic provenance-linked fixtures; public API canaries are
  ignored and operator-run only.
- `stream watch` buffers a bounded JSON result rather than emitting JSON Lines.
- Research or backtest output is not evidence of profitability or live readiness.

The machine-readable [`capabilities.json`](capabilities.json) distinguishes
implemented operations, data-types-only surfaces, unsupported behavior, and
planned work.

## Simulation methodology

`clob simulate` walks the current order-book snapshot:

- A **buy** consumes asks from lowest to highest price.
- A **sell** consumes bids from highest to lowest price.
- `--limit-price` stops at worse prices; the boundary price is included.
- `complete: false` means eligible liquidity was insufficient.
- Invalid, non-positive, or non-finite book levels are ignored.
- Results include consumed levels, average and worst price, notional, slippage,
  book hash, and timestamp when available.

It does not model latency, queue position, future book changes, tick rounding,
minimum order size, or stale data. Fill and fee calculations use decimal
arithmetic internally; serialized fill values retain six decimal places and fee
values retain five.

<details>
<summary><strong>Fee schedule and formula</strong></summary>

Polymarket's [fee documentation](https://docs.polymarket.com/trading/fees) says
makers pay no trading fee. Taker fees use:

```text
fee = shares × taker_fee_rate × price × (1 - price)
```

| Market category                                    | Formula coefficient |
| -------------------------------------------------- | ------------------: |
| Crypto                                             |              `0.07` |
| Sports, economics, culture, weather, other/general |              `0.05` |
| Finance, politics, mentions, tech                  |              `0.04` |
| Geopolitics/world events                           |                 `0` |

These are formula coefficients, not percentage labels. Polyrover applies the
formula to each consumed level and rounds the total to five decimal places in
USDC.

`base_fee_bps` from `clob fee-rate` and the category formula coefficient are
different fields. Polyrover does not infer categories, so supply
`--fee-category` explicitly when needed.

</details>

<details>
<summary><strong>Documented Polymarket order types</strong></summary>

Polymarket treats every order as a signed limit order. A “market order” is a
limit order priced to match resting liquidity immediately.

| Type                          | Unfilled amount                                |
| ----------------------------- | ---------------------------------------------- |
| **GTC** — Good Till Cancelled | Rests until filled or cancelled                |
| **GTD** — Good Till Date      | Rests until expiration                         |
| **FOK** — Fill Or Kill        | Cancels unless fully filled immediately        |
| **FAK** — Fill And Kill       | Fills what is available, then cancels the rest |

A post-only order rests as a maker or is rejected if it would match immediately.
See the authoritative
[order lifecycle](https://docs.polymarket.com/concepts/order-lifecycle).
Polyrover documents these types but does not submit them.

</details>

## Development and documentation

```bash
git clone https://github.com/TrebuchetDynamics/polyrover
cd polyrover
cargo test --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo doc --open
```

Tests use local fixtures and require no live credentials.

- [Crate on crates.io](https://crates.io/crates/polyrover)
- [Rust API documentation](https://docs.rs/polyrover/0.1.0/polyrover/)
- [Endpoint and capability matrix](docs/endpoint-capability-matrix.md)
- [Official Polymarket API guide](https://docs.polymarket.com/getting-started/api)
- [ADR-0001: async SDK with safe public default](docs/adr/0001-universal-async-sdk.md)
- [Port and parity roadmap](PORT_PLAN.md)

## License

Licensed under the [MIT License](LICENSE).

### Market stream receipt and reconnect semantics

`MarketRead` and `TrackedMarketRead` expose `received_at`: local UTC time when
the socket yields a frame, before decoding. Raw messages use the same receipt
rounded to milliseconds. The legacy read-method timestamp argument is retained
for compatibility but no longer overrides socket receipt time. This is application
receipt time, not exchange event time or a kernel packet timestamp.

Reconnect clears the tracked book and frame deduplication cache. Replayed events
may therefore be delivered again; durable consumers should remain idempotent.
Tracked snapshots require a full book for each asset on the current connection;
price/trade updates remain available before that seed, while standalone quote
and tick updates carry no reconstructed depth. The standalone `Tracker` remains
an explicitly caller-managed state machine.

Cancelling a market read preserves a pending reconnect notification for the next
returned frame. A replacement socket becomes active only after resubscription
finishes; cancelling a dial or subscription causes the next read to retry the
replacement. Reconnect statistics count fully subscribed replacements.

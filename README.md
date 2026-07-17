<p align="center">
  <img src="./assets/readme/hero.svg" width="100%" alt="Polyrover connects public Polymarket APIs to typed Rust data, stable JSON, and local simulation without live trading">
</p>

<p align="center">
  <a href="#quick-start">Quick start</a> ·
  <a href="#use-it-as-a-library">Rust API</a> ·
  <a href="#capability-layers">Capabilities</a> ·
  <a href="#safety-boundary">Safety</a> ·
  <a href="docs/endpoint-capability-matrix.md">Endpoint matrix</a>
</p>

**Polyrover is an async Rust SDK and CLI for Polymarket.** The default build
reads public Gamma, CLOB, Data API, and market WebSocket data, then turns it
into typed Rust models, stable JSON, and local fill simulations.

> [!IMPORTANT]
> The current release does not submit or cancel orders, sign with private keys,
> call relayers, or execute bridge transfers. Execution and bridge features are
> DTO-only or guarded.

## What you get

- **Gamma** — Search, markets, events, and crypto-window discovery. [`src/gamma.rs`](src/gamma.rs)
- **Public CLOB** — Books, prices, spreads, tick sizes, and market metadata. [`src/clob.rs`](src/clob.rs)
- **Data API** — Positions, trades, activity, holders, volume, and leaderboards. [`src/data.rs`](src/data.rs)
- **Market WSS** — Typed events with heartbeat, reconnect, deduplication, and tracking. [`src/stream_client.rs`](src/stream_client.rs)
- **Research tools** — Book walking, fill estimates, paper state, and generic market resolution. [`src/simulation.rs`](src/simulation.rs) · [`src/market_results.rs`](src/market_results.rs)

The full endpoint, feature, auth, implementation, and test inventory lives in
the [endpoint capability matrix](docs/endpoint-capability-matrix.md).

## Quick start

Install the CLI from Git:

```bash
cargo install --git https://github.com/TrebuchetDynamics/polyrover
```

Run a first public query:

```bash
polyrover gamma markets --limit 3 --json
```

Then inspect a book, estimate a hypothetical fill, or watch the market stream:

```bash
polyrover clob book --token-id <TOKEN_ID> --json

polyrover clob simulate \
  --token <TOKEN_ID> \
  --side buy \
  --amount 100 \
  --limit-price 0.55 \
  --json

polyrover stream watch \
  --token-id <TOKEN_ID> \
  --limit 10 \
  --seconds 30 \
  --json
```

No wallet or private key is needed for the default public build.

## Use it as a library

Polyrover is pre-1.0 and its network API is async-only.

```toml
[dependencies]
polyrover = { git = "https://github.com/TrebuchetDynamics/polyrover", default-features = false, features = ["public"] }
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

```rust
use polyrover::{simulation::Request, Client, ClientConfig};

#[tokio::main]
async fn main() -> polyrover::Result<()> {
    let client = Client::new(ClientConfig::default())?;

    let price = client.price("TOKEN_ID", "buy").await?;
    let estimate = client
        .simulate(Request {
            token_id: "TOKEN_ID".into(),
            side: "buy".into(),
            amount: "100".into(),
            limit_price: "0.55".into(),
        })
        .await?;

    println!("price={price} estimated_fill={}", estimate.average_price);
    Ok(())
}
```

Paginated wallet research uses typed Data API parameters while the original
limit-only helpers remain available:

```rust
use polyrover::{
    data::{ClosedPositionParams, LeaderboardParams},
    Client, ClientConfig,
};

# async fn research() -> polyrover::Result<()> {
let client = Client::new(ClientConfig::default())?;
let leaders = client
    .trader_leaderboard_with(&LeaderboardParams {
        category: "POLITICS".into(),
        time_period: "MONTH".into(),
        order_by: "PNL".into(),
        limit: Some(50),
        offset: Some(0),
        ..Default::default()
    })
    .await?;
if let Some(leader) = leaders.first() {
    let closed = client
        .closed_positions_with(&ClosedPositionParams {
            user: leader.proxy_wallet.clone(),
            limit: Some(50),
            offset: Some(0),
            ..Default::default()
        })
        .await?;
    println!("closed positions: {}", closed.len());
}
# Ok(())
# }
```

Network clients use async `reqwest` and `tokio-tungstenite`. DTO parsing, book
math, simulation, HMAC helpers, and address derivation remain synchronous.

## Stable JSON for tools and agents

Every CLI command uses the same versioned envelope. Errors keep the same shape
with `ok: false`, so consumers need one parser.

```json
{
  "ok": true,
  "version": "1",
  "data": {},
  "meta": {
    "command": "clob price"
  }
}
```

`version` is the output-contract version, not the Cargo package version.

## Capability layers

<p align="center">
  <img src="./assets/readme/capability-map.svg" width="100%" alt="Polyrover capability layers from the public default through optional authenticated, wallet, execution DTO, and bridge DTO features">
</p>

- **`public` (default)** — Gamma, public CLOB/Data reads, market WSS, and resolution. Implemented.
- **`authenticated`** — `public`, L2 HMAC helpers, and user WSS. Implemented.
- **`wallet`** — Pure address derivation and readiness helpers. Implemented.
- **`execution`** — `authenticated` + `wallet` and order/cancel models. DTO-only; no submission.
- **`bridge`** — Bridge metadata, quote/status models, and guards. DTO-only / unsupported guards.
- **`full`** — Every compiled surface above. Does not grant runtime authority.

Cargo features control compilation and dependency exposure—not authorization.
Core market and outcome identities are generic; crypto Up/Down window discovery
is a specialized helper rather than a constraint on the SDK.

### Capability taxonomy

[`capabilities.json`](capabilities.json) is the machine-readable operation
catalog shared with Polydart and named against Polymarket CLI commit `9b18b5f`.
Its statuses are `implemented`, `dtoOnly`, `unsupported`, and `planned`.
Taxonomy parity does not imply implementation parity.

## Safety boundary

Polyrover is designed for observation, analysis, simulation, reconciliation,
and pre-trade research. The current codebase has:

- no live order-placement path;
- no live cancellation path;
- no private-key import, storage, or signing path;
- no relayer invocation;
- no bridge execution;
- no wallet wizard that moves funds or silently prepares live trading.

Authenticated stream foundations, redacted auth helpers, wallet readiness
helpers, and execution/bridge DTOs remain explicit opt-ins. If you need current
production trading, approvals, CTF operations, or transfers, use an
execution-capable boundary such as Polygolem or the official `polymarket-cli`.

Simulation and paper fills are estimates. Real execution can differ because of
latency, fees, slippage, liquidity changes, and market movement.

<details>
<summary><strong>CLI command reference</strong></summary>

```text
polyrover async Polymarket CLI

ping --json
gamma search --query <text> [--limit n] --json
gamma markets [--limit n] --json
clob book --token-id <id> --json
clob price --token-id <id> --side buy|sell --json
clob simulate --token <id> --side buy|sell --amount <n> [--limit-price p] --json
analytics positions --user <wallet> [--limit n] --json
analytics trades --user <wallet> [--limit n] --json
analytics leaderboard [--limit n] --json
stream watch --token-id <id> [--token-id <id> ...] [--url ws://...] [--limit n] [--seconds s] --json
sim reset [--cash n] --json
sim buy --token-id <id> --price <p> --size <n> --json
sim sell --token-id <id> --price <p> --size <n> --json
```

</details>

## Build and verify

```bash
git clone https://github.com/TrebuchetDynamics/polyrover
cd polyrover
cargo test --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo doc --open
```

Tests use local fixtures; they do not require live credentials.

## Project references

- [Endpoint and capability matrix](docs/endpoint-capability-matrix.md)
- [ADR-0001: Universal async SDK with safe public default](docs/adr/0001-universal-async-sdk.md)
- [Port and parity roadmap](PORT_PLAN.md)

## License

Licensed under the [MIT License](LICENSE).

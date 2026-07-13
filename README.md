# polyrover

**The safer Polymarket CLI for research, agents, and automation.**

`polyrover` is a Rust command-line tool and library for exploring Polymarket data without putting a wallet in the blast radius. It speaks Gamma, CLOB, public Data APIs, analytics, local simulation, and stream foundations — but it does not place live orders.

If the official [`polymarket-cli`](https://github.com/Polymarket/polymarket-cli) is a trading terminal, polyrover is the scout you send ahead first.

## Why use polyrover?

- **Read-only by default** — browse markets, inspect books, fetch analytics, and simulate fills without configuring a private key.
- **Agent-friendly JSON** — responses use the same `{ ok, version, data | error, meta }` shape for scripts and LLM tools.
- **Built for pre-trade research** — search markets, check CLOB books/prices, inspect wallet analytics, estimate slippage, and run local paper fills.
- **No accidental live trading path** — order, bridge, auth, and wallet surfaces are modeled carefully, but live signing, private-key import/storage, relayers, and order submission are deliberately excluded for now.
- **Simple commands** — no shell wizard needed before your first query.

## polyrover vs. `polymarket-cli`

| Need | Use polyrover | Use `polymarket-cli` |
| --- | --- | --- |
| Research markets without a wallet | ✅ | ✅ |
| JSON for bots/agents | ✅ consistent envelope | ✅ `-o json` |
| Inspect CLOB prices/books | ✅ | ✅ |
| Simulate fills before touching funds | ✅ built in | Not the main focus |
| Paper/local ledger experiments | ✅ built in | Not the main focus |
| Avoid private-key configuration entirely | ✅ | Partial: trading setup uses a private key/config |
| Place/cancel live orders | ❌ intentionally absent | ✅ |
| Wallet setup, approvals, CTF, bridge | ❌ dry-run/DTO/readiness only | ✅ |

**Short version:** choose polyrover when the job is *observe, analyze, simulate, automate safely*. Choose `polymarket-cli` when you intentionally need a full trading wallet workflow.

## Library layout

- HTTP clients: `gamma` (market/event discovery), `clob` (order books,
  prices), `data` (positions, trades, activity), unified behind `Client`.
- Streaming: `stream` (market WSS decoding), `stream_client` (subscription
  lifecycle, ping, reconnect, dedup), `user_stream` (user WSS shapes).
- Domain: `types`, `market_resolver` (crypto window discovery, up/down token
  resolution), `market_data` (book state, top-of-book, liquidity, depth),
  `market_results` (authoritative outcomes).
- Local research: `paper` (paper-trading state), `simulation` (book fill
  simulation).
- Support: `auth`, `wallet`, `capabilities`, `config`, `error`, `jsonx`,
  `output`, `transport`.

Each module carries a `//!` doc; `cargo doc --open` renders the full map.

## Install

```bash
cargo install --git https://github.com/TrebuchetDynamics/polyrover
```

Or build from source:

```bash
git clone https://github.com/TrebuchetDynamics/polyrover
cd polyrover
cargo build --release
./target/release/polyrover --help
```

## Quick start

```bash
# Ping Gamma + CLOB
polyrover ping --json

# Search Gamma
polyrover gamma search --query "bitcoin" --limit 5 --json

# List markets
polyrover gamma markets --limit 10 --json

# Inspect a CLOB order book
polyrover clob book --token-id <TOKEN_ID> --json

# Get a buy/sell price
polyrover clob price --token-id <TOKEN_ID> --side buy --json

# Walk the book and estimate a fill
polyrover clob simulate \
  --token <TOKEN_ID> \
  --side buy \
  --amount 100 \
  --limit-price 0.55 \
  --json

# Public wallet analytics
polyrover analytics positions --user <WALLET> --limit 20 --json
polyrover analytics trades --user <WALLET> --limit 20 --json
polyrover analytics leaderboard --limit 20 --json

# Watch the public market WebSocket (bounded by --limit and --seconds)
polyrover stream watch --token-id <TOKEN_ID> --limit 10 --seconds 30 --json

# Local paper state examples
polyrover sim reset --cash 10000 --json
polyrover sim buy --token-id <TOKEN_ID> --price 0.42 --size 10 --json
polyrover sim sell --token-id <TOKEN_ID> --price 0.48 --size 10 --json
```

## Output shape

polyrover is meant to be boring to parse. `version` is the JSON contract version, not the Cargo package version:

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

Errors use the same envelope with `ok: false`, so scripts do not need a second parser for failure cases.

Simulation and paper fills are estimates for research. Live execution can diverge because of latency, fees, slippage, liquidity changes, and market movement.

## Safety model

polyrover is intentionally conservative:

- no live order placement;
- no live cancel path;
- no private-key import, storage, or live signing path;
- no credential export;
- no relayer or bridge execution;
- no wallet setup wizard that can silently move you toward trading.

The project includes typed DTOs, redacted auth helpers, dry-run bridge/order surfaces, wallet readiness helpers, and stream foundations so research systems can model Polymarket correctly without receiving permission to spend funds.

## Current command surface

```text
polyrover read-only Polymarket CLI

Commands:
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

## Library modules

`polyrover` also exposes source modules for:

- `Client` and `ClientConfig` as the unified read-only entry point for common Gamma, CLOB, Data, health, and fill-simulation operations;
- `gamma` and `clob` read clients;
- `data` public Data API access;
- `simulation` CLOB book walking and fill estimates;
- `paper` local paper state;
- `stream` and `stream_client` raw and typed market streams, including lifecycle events;
- `market_resolver`, `market_results`, and `market_data` helpers;
- `capabilities` and `intel` scoring metadata;
- `user_stream` payload parsing;
- `bridge`, `clob_orders`, and `wallet` DTO/readiness helpers.

## Who this is for

Use polyrover if you are building:

- research notebooks;
- market scanners;
- agent tools;
- read-only dashboards;
- backtest inputs;
- pre-trade risk checks;
- scripts that should never be able to place an order.

For production trading, approvals, CTF operations, bridge execution, wallet management, or order submission, use the official `polymarket-cli` and keep the usual wallet safety discipline.

## License

No license has been declared yet. Add one before publishing releases.

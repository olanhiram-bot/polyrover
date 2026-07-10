# polyrover port plan

Source: `../polygolem` (Go 1.25 module `github.com/TrebuchetDynamics/polygolem`).

## Port order

1. Read-only SDK foundation: transport, config, JSON envelope, Gamma read client, CLOB read client, resilient JSON types, book math. ✅
2. Add public Data API. ✅
3. Port local simulation/paper state. ✅
4. Port read-only CLOB fill simulation. ✅
5. Add WebSocket market stream DTO/dedup/stats/subscription foundation. ✅
6. Port crypto market resolver/discovery helpers. ✅
7. Port market-data tracker snapshots. ✅
8. Port pure capabilities/intel scoring metadata. ✅
9. Add minimal public WebSocket IO client. ✅
10. Add L2 HMAC auth helpers and authenticated user-stream payload/client foundation. ✅
11. Add public WebSocket ping/reconnect helpers. ✅
12. Add bridge dry-run/DTOs and CLOB order/cancel DTOs. ✅
13. Add offline wallet address derivation/readiness helpers. ✅
14. Add typed public market events, including market lifecycle events. ✅
15. Expand full async WebSocket callback/reconnect parity.
16. Only after parity tests: authenticated CLOB reads.
17. Last, with explicit owner approval: signing, wallet, relayer, bridge, and live order paths.

## First slice shipped here

- `src/gamma.rs`: `HealthCheck`, `Markets`, `Events`, `Search` equivalents.
- `src/clob.rs`: health, time, markets, market-by-token, order book, price, midpoint, spread, tick-size, neg-risk.
- `src/types.rs`: tolerant Gamma/CLOB DTOs, `StringOrArray`, `NormalizedTime`, CLOB best bid/ask/ask-size math.
- `src/output.rs`: Polygolem-style `{ok, version, data|error, meta}` JSON envelope.
- `src/main.rs`: tiny read-only CLI.
- `src/data.rs`: public Data API positions/trades/activity/holders/value/open-interest/leaderboard/live-volume.
- `src/paper.rs`: local no-risk paper ledger buy/sell/reset state.
- `src/simulation.rs`: read-only CLOB book walk/fill/slippage estimator.
- `src/stream.rs`: public market stream DTOs, subscription JSON, bounded dedup, split-array, raw-message and stats foundation.
- `src/market_resolver.rs`: crypto up/down outcome, slug/window, token parsing, query, and candidate filtering helpers.
- `src/market_data.rs`: stream event to latest per-token snapshot tracker.
- `src/capabilities.rs`: surface metadata and read-only/secret gate classification.
- `src/intel.rs`: pure wallet shrinkage/ROI/scoring helpers.
- `src/stream_client.rs`: public market WebSocket connect/subscribe/raw-or-typed read wrapper using stream DTOs, plus ping/reconnect helpers.
- `src/auth.rs`: CLOB L2 HMAC signing/header helpers with redaction support.
- `src/user_stream.rs`: authenticated user stream DTOs, payload builder, parser, and minimal client wrapper.
- `src/bridge.rs`: Bridge API DTOs plus unsupported withdrawal dry-run safety guard.
- `src/clob_orders.rs`: CLOB order/cancel/record DTOs for authenticated order surfaces, without submit methods.
- `src/wallet.rs`: offline wallet readiness and deterministic proxy/safe/deposit-wallet address derivation helpers.
- `src/stream.rs`: typed decoding for book, price, trade, tick, top-of-book, new-market, and resolved-market events.

## Deliberately not ported yet

Live signing, wallet onboarding, relayer, bridge, order placement/cancel, private-key handling, and credential export remain excluded because they can move funds or expose secrets.

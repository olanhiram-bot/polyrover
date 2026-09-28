# Changelog

## Unreleased

### Added

- Linked CLI help and current project documentation to Polymarket's official API and SDK guidance.
- Documented public historical-query parity with atomic Gamma event pages, builder trade/leaderboard/volume history, rebates, combo activity, and bounded CLOB/Gamma/Data CLI commands.
- Opt-in, library-only L2-authenticated trade, order, and reward history reads with per-call borrowed credentials.

### Safety

- Authenticated reads add no credential loading/storage, private-key signing, CLI secret path, API-key creation, order mutation, traversal, or persistence.

## 0.2.0 - 2026-07-28

### Added

- Atomic batch CLOB prices, midpoints, spreads, and last-trade reads.
- Gamma screening filters plus tags, series, sports metadata, market types, and teams.
- Transparent public wallet dossiers with explicit source/coverage fields.
- Bounded provenance-aware market-flow summaries over public WSS events.

### Changed

- Streaming liquidity, depth, midpoint, spread, ordering, and zero-size calculations use exact decimal arithmetic.
- `market_data::Liquidity` and `market_data::Depth` numeric fields changed from `f64` to `rust_decimal::Decimal`.

### Safety

- No signing, order submission, cancellation, wallet connection, relayer, bridge transfer, alert delivery, prediction, or execution capability was added.

## 0.1.0 - 2026-07-27

### Added

- Gamma keyset pagination through `market_page`, including opaque `after_cursor`/`next_cursor` handling for complete catalogs beyond the offset limit.
- Paginated and filterable Data API queries for closed positions, trades, activity, and trader leaderboards.
- Complete public wallet, trade, activity, and leaderboard DTO fields needed for reproducible wallet research.

### Changed

| Previous API                    | Replacement                       | Reason                           |
| ------------------------------- | --------------------------------- | -------------------------------- |
| `capabilities::all()`           | `CapabilityCatalog::all()`        | Use the operation-level catalog. |
| `capabilities::read_only_ids()` | Filter `CapabilityCatalog::all()` | Remove the coarse helper.        |

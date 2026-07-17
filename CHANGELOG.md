# Changelog

## Unreleased

### Added

- Paginated and filterable Data API queries for closed positions, trades, activity, and trader leaderboards.
- Complete public wallet, trade, activity, and leaderboard DTO fields needed for reproducible wallet research.

### Changed

| Previous API | Replacement | Reason |
|---|---|---|
| `capabilities::all()` | `CapabilityCatalog::all()` | Use the operation-level catalog. |
| `capabilities::read_only_ids()` | Filter `CapabilityCatalog::all()` | Remove the coarse helper. |

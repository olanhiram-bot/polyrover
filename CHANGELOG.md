# Changelog

## Unreleased

### Changed

| Previous API | Replacement | Reason |
|---|---|---|
| `capabilities::all()` | `CapabilityCatalog::all()` | Use the operation-level catalog. |
| `capabilities::read_only_ids()` | Filter `CapabilityCatalog::all()` | Remove the coarse helper. |

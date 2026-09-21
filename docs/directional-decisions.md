# Directional decisions (`directional_v2`)

## Diagnosis and change

The previous seven local reports contained five with no final TypeSafe call:
all eligible evidence was missing, stale or excluded by the 24 KB synthesis
budget. One recent source was excluded solely for its size. The final rubric
also demanded numeric forecasts/base rates even for a qualitative conclusion.

The decision pipeline now:

1. Checks market closure, archive, activity, order acceptance and known elapsed
   deadline before news collection or any paid evaluation. Ineligible markets
   produce an auditable non-prediction, not an invented winner. A missing deadline
   still blocks a buy recommendation later.
2. Keeps the exact-question Google News snapshot. If it contains no extracted,
   dated source within `max-age-days`, it also tries the same query with
   `when:Nd`. The supplemental URL (or its failure) is recorded. Results are
   merged with stable unique IDs and text-hash deduplication. This remains a
   bounded provider snapshot, not all news on the web.
3. Assesses all extracted article text with the actual market rules and deadline,
   not just the abbreviated question. Relevant, fully assessed,
   current articles are split losslessly into Unicode-safe blocks and packed
   into <=24,000-byte states. Long articles are no longer excluded
   from the final assessment. Oversized metadata fails explicitly.
4. Asks independent TypeSafe `Choice` questions for direction and its reason,
   separately from optional numeric odds/basis. Qualitative YES/NO does not
   require a percentage. Conflicting evidence, neutral context and genuinely
   insufficient evidence remain valid answers. Direction confidence <0.65
   downgrades YES/NO to uncertain; confidence is not event probability.
5. Multiple batches are reduced using typed model assessments with source IDs,
   preserving contrary assessments rather than counting favorable articles.
   A failed batch fails the forecast instead of pretending all evidence was
   considered. At most 64 synthesis calls are allowed and POSTs are not retried.
   Additional batch responses and usage are retained; article text is not.
6. Reduced forecasts are qualitative only. Numeric intervals are kept only for
   direct full-context assessments with quantitative basis, sufficient classifier
   confidence and a compatible direction. No probability is inferred from a
   YES/NO classification or from counts. Trading gates are unchanged.

The API adds `analysis_version`, `market_status`, `forecast_status`,
`forecast_reason`, `forecast_basis`, `evidence_used`, `stale_sources`, and
`evidence_omitted_for_size`. Legacy reports receive derived diagnostics without
changing their stored forecast or spending quota. `forecast_status` distinguishes
not applicable, no eligible evidence, unavailable evaluation, and model evaluated.

## Cache and rollout

The existing report envelope remains `decision_v1` / `polyrover_decision_v1`.
`analysis_version` is an additive methodology marker. No cache invalidation,
history deletion or bypass of the per-market/global spend gates occurs. Cached
research remains reusable for 24 hours and the updated method applies to the
next permitted generation. Quote expiry (120 seconds) is independent.

Arenaton defaults to an eligible child market when no explicit selector exists.
Explicit historical selections remain available. Its UI differentiates the
TypeSafe outlook from a buy recommendation and explains absent model evaluation
and closed markets rather than presenting both as model uncertainty.

## Verification and limits

Offline tests cover qualitative directions, no fabricated odds, lossless long
articles, map/reduce HTTP wiring, closure preflight, old-report diagnostics,
cache/rate gates, explicit selectors and UI rendering. Live predictive accuracy
and calibration are not established by these tests. A useful forecast is not
guaranteed for every market, especially when news lacks event-specific evidence.

Local verification: 128 Flutter tests; Rust library/decision/API/TypeSafe tests;
seven PostgreSQL integration cases in a separate test database. A live synthetic
contract probe (not a real market prediction) returned qualitative YES, no numeric
interval, using `jev-1.13.0`. An earlier probe with inconsistent synthetic coverage
returned uncertainty; after supplying consistent rules and coverage it passed.
No cached market was regenerated for verification. Live Gamma preflight confirmed
the Iran March 13 market is closed and returns a non-prediction with zero evaluated
articles. The live API retained its prior cached report and projected `not_applicable`.

TypeSafe recommends atomic independent questions and combining their results
in code: [introduction](https://docs.typesafe.ai/introduction).
Its [confidence statistic](https://docs.typesafe.ai/confidence) describes the
classification, not a calibrated probability of the external event.

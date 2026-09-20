# TypeSafe research integration

Polyrover combines public Gamma discovery, CLOB books, Data API history, local
fill simulation, wallet scores, and a versioned JSON CLI. TypeSafe adds a useful
semantic operation: judging the meaning and completeness of market text.

The existing decimal liquidity calculations, fee formulas, wallet statistics,
and authoritative outcome resolver remain the right tools for their tasks.
The evaluation integration lives in `src/research/typesafe.rs` and uses the
existing HTTP dependencies. News collection adds optional HTML/RSS parsers. It is a separate client so a
TypeSafe bearer token never enters a Polymarket request.

## Implemented workflow

`ai review-market` obtains one market by Gamma slug, or reads one raw market
object from a local JSON file. Files must contain the market itself, not a CLI
envelope or an array. The example in `examples/typesafe-market.json` is synthetic.

The request sends three independent questions together, using the
[documented System One endpoint](https://docs.typesafe.ai/api):

| Question | Primitive | Application |
| --- | --- | --- |
| Primary subject | Choice | Categorize research queues, with an `other` fallback |
| Resolution clarity | Score, levels 0–3 | Flag rules that require interpretation |
| Explicit resolution source | Noul | Identify missing evidence about who or what determines resolution |

State contains only market ID, slug, question, description, resolution source,
end date, and outcomes. Other Gamma fields and arbitrary `Market.extra` fields
are excluded. The model receives instructions to treat supplied text as evidence.
It does not fetch the named resolution source or verify that source's reliability.

Local code routes to `research_ready` only if all of these hold:

- A nonempty description is present.
- The category is not `other` and its confidence meets `min_confidence`.
- Resolution clarity is at least 2.5/3 and its confidence meets `min_confidence`.
- The probability that an explicit source is identified is at least 0.8.

Otherwise the result is `manual_review`, with machine-readable reasons. The
default confidence floor is 0.8; `--min-confidence` changes the Choice/Score floor
only. Noul is a probability and has no separate confidence field, as explained in
the [TypeSafe confidence documentation](https://docs.typesafe.ai/confidence).
These initial thresholds are heuristics and need evaluation against labeled
markets before relying on the routing at scale. `research_ready` is a text-quality
assessment, not permission to trade, an outcome forecast, or a guarantee that a
market will resolve cleanly.

Results include the actual model, full distributions, usage, evaluation time,
market identity, threshold, and rubric version `market_rules_v1`. Store the
source market snapshot alongside the result when reproducibility matters;
`jev-latest` is a moving alias, and `--model` accepts an explicit model version.

## CLI

The CLI reads `TYPESAFE_API_KEY` from the process environment. For local use,
copy `.env.example` to `.env`, set the key there, and load it before evaluating:

```bash
set -a
. ./.env
set +a
```

`.env` is ignored by Git. The CLI does not automatically load files from the
working directory. Never add real credentials to `.env.example`.

```bash
# Local inspection; no key and no external calls.
cargo run --features typesafe -- ai review-market \
  --market-file examples/typesafe-market.json --dry-run --json

# Fetch Gamma data and preview the request; no TypeSafe usage.
cargo run --features typesafe -- ai review-market \
  --slug MARKET_SLUG --dry-run --json

# Set TYPESAFE_API_KEY in the process environment before evaluating.
cargo run --features typesafe -- ai review-market \
  --slug MARKET_SLUG --min-confidence 0.9 --json

cargo run --features typesafe -- help ai review-market
```

Evaluations transmit the selected fields to TypeSafe and consume API quota.
No credentials are needed by the default build, or by dry runs. The `full`
feature continues to mean the Polymarket feature bundle; add `typesafe`
explicitly for this external service. The existing capability manifest catalogs
Polymarket operations; this external research adapter is documented here.

## Rust library

Enable `features = ["typesafe"]` on the Polyrover dependency from this checkout.

```rust,no_run
use polyrover::{typesafe, Client, ClientConfig};

#[tokio::main]
async fn main() -> polyrover::Result<()> {
    let public = Client::new(ClientConfig::default())?;
    let market = public.market_by_slug("MARKET_SLUG").await?;
    let evaluator = typesafe::Client::from_env(typesafe::Config::default())?;
    let review = evaluator.review_market(&market, 0.8).await?;
    println!("{}", serde_json::to_string_pretty(&review)?);
    Ok(())
}
```

`Client::new(api_key, config)` supports explicitly supplied credentials instead
of environment loading. `market_review_request` builds an inspectable request;
`Client::evaluate(&Request)` also accepts custom state and batches of typed
Choice, Score, and Noul questions. This initial Rust adapter supports text
instructions and text criteria; advanced structured rubrics and custom Noul
criteria are not exposed.

## Failure behavior and validation

Requests have a 30-second timeout, reject unsafe remote HTTP endpoints, and do
not follow redirects. Credentials use a sensitive authorization header and are
not exposed through `Debug`. Non-success response bodies are omitted from errors
because an upstream server could echo sensitive input.

HTTP 429 maps to `Error::RateLimited` and preserves numeric `Retry-After` seconds.
Other non-success responses map to `Error::Api`, including overload status 529.
POSTs are not automatically retried, avoiding duplicate evaluation charges on
ambiguous failures. Callers own backoff and retry budgets. Failed or malformed
responses return an error, never a fabricated assessment or a successful route.

Response validation checks IDs, primitive types, requested options/levels,
confidence bounds, and probability distributions. Offline HTTP tests cover the
wire format, authorization, redaction, redirect refusal, rate limits, routing,
malformed responses, and CLI validation. CI tests the standalone feature as
well as all features. Live model accuracy and billing are not established by
these contract tests.

## News research

`ai research-market` searches Google News with the market question. It accepts
exactly one of `--slug`, `--market-file`, `--question`, or `--news-url`. A URL must
be a Google News `/search` URL; `q`, `hl`, `gl`, and `ceid` are retained in its RSS
equivalent. New searches default to `hl=en-US&gl=US&ceid=US:en`.

```bash
cargo run --features typesafe -- ai research-market \
  --news-url 'https://news.google.com/search?q=Will+Paris+Saint-Germain+win+the+2026-27+UEFA+Champions+League+Championship?&hl=en-US&gl=US&ceid=US:en' \
  --output psg-news.json --json
```

The collector attempts every RSS item, including older or apparently irrelevant
items, with three concurrent article reads. Google controls this finite snapshot:
it may differ from the interactive page, has no implemented pagination contract,
and does not establish that every matching article on the web was found. At 100
items the report flags a possible provider cap. Google RSS/redirect behavior is
not a supported API contract and can change.

Google article links are followed to publishers using redirects or Google's
public link-resolution metadata/RPC. Publisher pages are parsed for JSON-LD
`articleBody` or article/main HTML paragraphs. RSS descriptions and headlines
are never substituted for article text. Detected paywalls, access challenges,
unsupported encodings, extraction failures, timeouts, and oversized pages are
reported as unavailable. Each response is limited to 8 MiB; oversized content
is rejected, never silently truncated. HTTPS public addresses are checked and
pinned at every redirect. Publisher requests carry no TypeSafe key or cookies.

All extracted text is sent to TypeSafe in lossless Unicode chunks of at most
12,000 UTF-8 bytes, without splitting a character. Every chunk asks about exact-event relevance, the direction
of the article's claims, and whether those claims are attributed facts or
forecasts/opinions. These are assessments of source claims, not verified truth
or calibrated probabilities of a future result. Text is treated as untrusted
data. Long articles may require multiple calls; every chunk's result or error
is retained. `--collect-only` makes no TypeSafe requests. API usage scales with
the number and length of accessible articles.

The report records titles, original links, dates, extraction methods, character
counts, SHA-256 content hashes, duplicates, chunk coverage, model answers, and
token usage. Full article text is only kept in memory for assessment, not written
to the report. Exact normalized text duplicates point to the first article's
assessment; syndication with edited text may still appear more than once.
Distinct publisher hosts measure diversity, not proof of independent reporting.

Articles older than `--max-age-days` (default 30), undated articles, and dates
more than one day in the future are read and assessed but excluded from signal
counts. Only fully assessed articles with relevance at least 0.8 and direction
confidence at least `--min-confidence` (default 0.8) enter those counts. Mixed
evidence is preserved. Low confidence is never treated as evidence for “No”.

The route remains `manual_review`: HTML extraction cannot prove that a publisher
served its entire article, and aggregating news opinions does not establish an
outcome probability. `publisher_completeness_verified` is explicitly false.
The report distinguishes extracted text from unavailable articles and evaluated
text from failed or skipped evaluations. It never claims to have read all
original articles when a page was blocked or only partial content was served.

`--output` creates a new JSON report and refuses to overwrite existing files.
The command also prints the normal versioned JSON envelope. If the process fails
before finishing, a reserved output file may be empty; an empty file is not a
completed research report. Generated research reports should stay local.

## Possible extensions

The generic evaluator can later support user-defined relevance screening or
summarized wallet-dossier review. Those would need their own rubrics, bounded
state, and labeled examples. They are not part of the current market workflow.
No order submission, alert delivery, or scheduler is added.
